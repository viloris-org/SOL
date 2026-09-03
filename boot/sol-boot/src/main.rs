#![cfg_attr(target_os = "uefi", no_main)]
#![cfg_attr(target_os = "uefi", no_std)]

#[cfg(target_os = "uefi")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    // Best-effort panic diagnostics: try to persist to ESP before reset.
    // Failure to log is not fatal - we must reset regardless.
    let _ = firmware::persist_panic_diagnostics(info);

    uefi::runtime::reset(uefi::runtime::ResetType::COLD, uefi::Status::ABORTED, None)
}

#[cfg(not(target_os = "uefi"))]
fn main() {
    eprintln!("sol-boot is a UEFI application; build it for x86_64-unknown-uefi");
}

#[cfg(target_os = "uefi")]
mod firmware {
    extern crate alloc;

    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::fmt;
    use core::fmt::Write;
    use sol_boot::{BootLog, BootManager, BootStorage, draw_optional_boot_frame};
    use sol_boot_core::BootAction;
    use uefi::CString16;
    use uefi::boot::{self, LoadImageSource};
    use uefi::fs::{Error as FileError, FileSystem};
    use uefi::prelude::*;
    use uefi::proto::console::gop::GraphicsOutput;

    include!(concat!(env!("OUT_DIR"), "/release_key.rs"));

    const RECOVERY_A: &str = "\\EFI\\SOL\\recovery\\recovery-a.efi";
    const RECOVERY_B: &str = "\\EFI\\SOL\\recovery\\recovery-b.efi";
    const RECOVERY_REQUEST: &str = "\\EFI\\SOL\\state\\recovery.request";

    struct UefiStorage {
        fs: FileSystem,
    }

    #[derive(Debug)]
    enum UefiStorageError {
        InvalidPath,
        File(FileError),
    }

    impl fmt::Display for UefiStorageError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidPath => formatter.write_str("invalid UCS-2 ESP path"),
                Self::File(error) => error.fmt(formatter),
            }
        }
    }

    impl BootStorage for UefiStorage {
        type Error = UefiStorageError;

        fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>, Self::Error> {
            let path = CString16::try_from(path).map_err(|_| UefiStorageError::InvalidPath)?;
            if !self
                .fs
                .try_exists(path.as_ref())
                .map_err(UefiStorageError::File)?
            {
                return Ok(None);
            }
            self.fs
                .read(path.as_ref())
                .map(Some)
                .map_err(UefiStorageError::File)
        }

        fn write_durable(&mut self, path: &str, bytes: &[u8]) -> Result<(), Self::Error> {
            let path = CString16::try_from(path).map_err(|_| UefiStorageError::InvalidPath)?;
            self.fs
                .write(path.as_ref(), bytes)
                .map_err(UefiStorageError::File)
        }

        fn remove(&mut self, path: &str) -> Result<(), Self::Error> {
            let path = CString16::try_from(path).map_err(|_| UefiStorageError::InvalidPath)?;
            if self
                .fs
                .try_exists(path.as_ref())
                .map_err(UefiStorageError::File)?
            {
                self.fs
                    .remove_file(path.as_ref())
                    .map_err(UefiStorageError::File)?;
            }
            Ok(())
        }
    }

    #[entry]
    fn main() -> Status {
        if uefi::helpers::init().is_err() {
            return Status::ABORTED;
        }
        match run() {
            Ok(()) => Status::SUCCESS,
            Err(()) => Status::LOAD_ERROR,
        }
    }

    /// Executes trust and fallback policy. May optionally draw one static frame
    /// in the current GOP mode (ADR-0026 Section 6). Graphics are best-effort and
    /// never affect verification, retry, fallback, or recovery.
    fn run() -> Result<(), ()> {
        let mut log = BootLog::new();
        log.info("SOL boot manager started");

        // Optional: draw one static brand mark in current GOP mode (best-effort).
        // Missing GOP or rendering failure is silently ignored.
        if let Ok(gop_handle) = boot::get_handle_for_protocol::<GraphicsOutput>() {
            if let Ok(mut gop) = boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle) {
                draw_optional_boot_frame(&mut gop);
                log.info("Boot frame displayed");
            }
        }

        let fs = boot::get_image_file_system(boot::image_handle()).map_err(|_| ())?;
        let mut storage = UefiStorage {
            fs: FileSystem::new(fs),
        };

        let recovery_requested = storage.read(RECOVERY_REQUEST).ok().flatten().is_some();
        if recovery_requested {
            log.info("Recovery explicitly requested");
            let _ = storage.remove(RECOVERY_REQUEST);
        }

        let mut manager = BootManager::new(storage, RELEASE_KEY).map_err(|e| {
            log.error(format!("Manager initialization failed: {}", e));
            ()
        })?;

        let Ok(selection) = manager.select(recovery_requested) else {
            log.error("Selection failed, entering recovery");
            let _ = persist_boot_log(manager.storage_mut(), &log);
            return start_recovery(manager.storage_mut());
        };

        log.info(format!("Selected action: {:?}", selection.action()));

        if matches!(selection.action(), BootAction::Recovery(_)) {
            log.warn("Recovery selected by policy");
            let _ = persist_boot_log(manager.storage_mut(), &log);
            return start_recovery(manager.storage_mut());
        }

        if let Ok(uki) = manager.load_selected_uki(selection) {
            log.info("Starting selected deployment UKI");
            let _ = persist_boot_log(manager.storage_mut(), &log);
            // A booting OS does not return. Any LoadImage/StartImage error or
            // child return continues immediately to the retained fallback.
            let _ = start_image_buffer(&uki);
            log.error("Selected UKI returned unexpectedly");
        } else {
            log.error("Failed to load selected UKI");
        }

        if let Ok(Some(uki)) = manager.load_fallback_uki(selection) {
            log.warn("Starting fallback deployment UKI");
            let _ = persist_boot_log(manager.storage_mut(), &log);
            let _ = start_image_buffer(&uki);
            log.error("Fallback UKI returned unexpectedly");
        }

        log.error("All deployment options exhausted, entering recovery");
        let _ = persist_boot_log(manager.storage_mut(), &log);
        start_recovery(manager.storage_mut())
    }

    fn start_recovery(storage: &mut UefiStorage) -> Result<(), ()> {
        for path in [RECOVERY_A, RECOVERY_B] {
            if let Ok(Some(image)) = storage.read(path) {
                let _ = start_image_buffer(&image);
            }
        }
        Err(())
    }

    /// Starts an in-memory EFI image without a device path or display work.
    /// Returning from an OS/recovery image is treated as failure so the next
    /// authorized fallback is attempted.
    fn start_image_buffer(image: &[u8]) -> Result<(), ()> {
        let handle = boot::load_image(
            boot::image_handle(),
            LoadImageSource::FromBuffer {
                buffer: image,
                file_path: None,
            },
        )
        .map_err(|_| ())?;
        let _ = boot::start_image(handle);
        Err(())
    }

    /// Best-effort panic diagnostics persistence.
    pub(crate) fn persist_panic_diagnostics(info: &core::panic::PanicInfo<'_>) -> Result<(), ()> {
        // Attempt to get filesystem access
        let fs = boot::get_image_file_system(boot::image_handle()).map_err(|_| ())?;
        let mut fs = FileSystem::new(fs);

        // Format panic message
        let mut message = String::new();
        let _ = writeln!(message, "SOL Boot Panic");
        let _ = writeln!(message, "===============");
        let _ = writeln!(message, "{}", info);

        if let Some(location) = info.location() {
            let _ = writeln!(message, "Location: {}:{}:{}",
                location.file(), location.line(), location.column());
        }

        // Write to persistent log (ignore failure - we're already panicking)
        let path = CString16::try_from("\\EFI\\SOL\\logs\\panic.txt").map_err(|_| ())?;
        let _ = fs.write(path.as_ref(), message.as_bytes());

        Ok(())
    }

    /// Persists boot log to ESP for post-mortem diagnostics.
    fn persist_boot_log(storage: &mut UefiStorage, log: &BootLog) -> Result<(), ()> {
        // Best-effort: log write failures don't affect boot flow
        let path = CString16::try_from("\\EFI\\SOL\\logs\\boot.txt").map_err(|_| ())?;
        storage.fs.write(path.as_ref(), &log.as_bytes()).map_err(|_| ())?;
        Ok(())
    }
}
