#![cfg_attr(target_os = "uefi", no_main)]
#![cfg_attr(target_os = "uefi", no_std)]

#[cfg(not(target_os = "uefi"))]
fn main() {
    eprintln!("sol-boot is a UEFI application; build it for x86_64-unknown-uefi");
}

#[cfg(target_os = "uefi")]
mod firmware {
    extern crate alloc;

    use alloc::vec::Vec;
    use core::fmt;
    use sol_boot::{
        BootManager, BootPixel, BootStorage, GraphicsDecision, GraphicsMode, render_boot_frame,
        select_graphics_mode,
    };
    use sol_boot_core::BootAction;
    use uefi::CString16;
    use uefi::boot::{self, LoadImageSource};
    use uefi::fs::{Error as FileError, FileSystem};
    use uefi::prelude::*;
    use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};

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

    fn run() -> Result<(), ()> {
        uefi::println!("SOL boot 0.1");
        let fs = boot::get_image_file_system(boot::image_handle()).map_err(|error| {
            uefi::println!("storage: {error}");
        })?;
        let mut storage = UefiStorage {
            fs: FileSystem::new(fs),
        };
        let recovery_requested = storage
            .read(RECOVERY_REQUEST)
            .map_err(|error| uefi::println!("recovery request: {error}"))?
            .is_some();
        if recovery_requested {
            let _ = storage.remove(RECOVERY_REQUEST);
        }
        let mut manager = BootManager::new(storage, RELEASE_KEY).map_err(|error| {
            uefi::println!("configuration: {error}");
        })?;

        let selection = match manager.select(recovery_requested) {
            Ok(selection) => selection,
            Err(error) => {
                uefi::println!("boot policy failed closed: {error}");
                return start_first_available(manager.storage_mut(), &[RECOVERY_A, RECOVERY_B]);
            }
        };
        match selection.action() {
            BootAction::BootKnownGood(deployment) => {
                uefi::println!(
                    "booting known-good slot {:?} generation {}",
                    deployment.slot(),
                    deployment.generation()
                );
            }
            BootAction::BootTrial {
                deployment,
                attempt,
            } => {
                uefi::println!(
                    "booting trial slot {:?} generation {} attempt {}",
                    deployment.slot(),
                    deployment.generation(),
                    attempt.get()
                );
            }
            BootAction::Recovery(reason) => {
                uefi::println!("entering recovery: {reason:?}");
                return start_first_available(manager.storage_mut(), &[RECOVERY_A, RECOVERY_B]);
            }
        }

        render_splash();
        let uki = manager.load_selected_uki(selection).map_err(|error| {
            uefi::println!("selected UKI changed: {error}");
        })?;
        start_image_buffer(&uki, selection.uki_path().ok_or(())?)
    }

    fn start_first_available(storage: &mut UefiStorage, paths: &[&str]) -> Result<(), ()> {
        for path in paths {
            let Ok(Some(image)) = storage.read(path) else {
                continue;
            };
            if start_image_buffer(&image, path).is_ok() {
                return Ok(());
            }
        }
        uefi::println!("no bootable image remained");
        Err(())
    }

    fn start_image_buffer(image: &[u8], path: &str) -> Result<(), ()> {
        let handle = boot::load_image(
            boot::image_handle(),
            LoadImageSource::FromBuffer {
                buffer: image,
                file_path: None,
            },
        )
        .map_err(|error| uefi::println!("image {path} rejected: {error}"))?;
        boot::start_image(handle).map_err(|error| uefi::println!("image {path} returned: {error}"))
    }

    fn render_splash() {
        let Ok(handle) = boot::get_handle_for_protocol::<GraphicsOutput>() else {
            return;
        };
        let Ok(mut gop) = boot::open_protocol_exclusive::<GraphicsOutput>(handle) else {
            return;
        };
        let current_info = gop.current_mode_info();
        let modes = gop.modes().collect::<Vec<_>>();
        let descriptions = modes
            .iter()
            .zip(0_u32..)
            .map(|(mode, index)| {
                let (width, height) = mode.info().resolution();
                GraphicsMode {
                    index,
                    width,
                    height,
                    stride: mode.info().stride(),
                }
            })
            .collect::<Vec<_>>();
        let current_index = modes
            .iter()
            .position(|mode| *mode.info() == current_info)
            .and_then(|index| u32::try_from(index).ok());
        let Some(current_index) = current_index else {
            return;
        };
        let chosen = match select_graphics_mode(&descriptions, current_index, None) {
            GraphicsDecision::Preserve(mode) => mode,
            GraphicsDecision::SetOnce(mode) => {
                let Some(firmware_mode) = modes.get(mode.index as usize) else {
                    return;
                };
                if gop.set_mode(firmware_mode).is_err() {
                    return;
                }
                mode
            }
            GraphicsDecision::Unavailable => return,
        };
        let Some(frame) = render_boot_frame(chosen.width, chosen.height) else {
            return;
        };
        let frame = frame
            .iter()
            .map(|pixel: &BootPixel| BltPixel::new(pixel.red, pixel.green, pixel.blue))
            .collect::<Vec<_>>();
        let _ = gop.blt(BltOp::BufferToVideo {
            buffer: &frame,
            src: BltRegion::Full,
            dest: (0, 0),
            dims: (chosen.width, chosen.height),
        });
    }
}
