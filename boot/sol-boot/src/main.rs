#![cfg_attr(target_os = "uefi", no_main)]
#![cfg_attr(target_os = "uefi", no_std)]

#[cfg(not(target_os = "uefi"))]
fn main() {
    eprintln!("sol-boot is a UEFI application; build it for x86_64-unknown-uefi");
}

#[cfg(target_os = "uefi")]
mod firmware {
    extern crate alloc;

    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use core::fmt;
    use core::time::Duration;
    use sol_boot::{
        BootManager, BootMenu, BootPixel, BootStorage, Firmware, GraphicsDecision, GraphicsMode,
        KeyInput, MenuAction, MenuEntry, MenuResult, SplashProgress, ease_out_cubic,
        redraw_boot_frame, select_graphics_mode,
    };
    use sol_boot_core::BootAction;
    use uefi::CString16;
    use uefi::boot::{self, LoadImageSource};
    use uefi::fs::{Error as FileError, FileSystem};
    use uefi::prelude::*;
    use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
    use uefi::system;

    include!(concat!(env!("OUT_DIR"), "/release_key.rs"));

    const RECOVERY_A: &str = "\\EFI\\SOL\\recovery\\recovery-a.efi";
    const RECOVERY_B: &str = "\\EFI\\SOL\\recovery\\recovery-b.efi";
    const RECOVERY_REQUEST: &str = "\\EFI\\SOL\\state\\recovery.request";
    const MENU_TIMEOUT_SECONDS: u64 = 5;
    /// Mac-style silent launch: extended key-listen window over the brand mark;
    /// interruption keys open the picker, everything else boots straight.
    const STARTUP_LISTEN_MILLIS: u64 = 3000;
    const LISTEN_TICK_MILLIS: u64 = 100;
    /// Progress reached once the signed deployment selection is authenticated.
    const MILESTONE_SELECTED: f32 = 0.25;
    /// Progress reached once the selected UKI is read and hash-verified.
    const MILESTONE_UKI_LOADED: f32 = 0.85;

    /// Emits one structured status line through [`BootLog`].
    macro_rules! boot_log {
        ($sink:expr, $($argument:tt)+) => {
            $sink.record(alloc::format!($($argument)+))
        };
    }

    /// Dual-mode diagnostics: plain console text on machines without a
    /// graphical panel (headless units, test harnesses mirroring the text
    /// console onto UART) and fully buffered silence behind a real panel
    /// unless the operator explicitly requested a verbose boot.
    ///
    /// Raw Serial protocol access is deliberately avoided: an exclusive claim
    /// made by this image — even one released before transfer — derails child
    /// image execution on reference firmware, so all output rides ConOut.
    struct BootLog {
        verbose: bool,
        /// Emit every line immediately instead of buffering for replay.
        open_text_mode: bool,
        buffered: Vec<String>,
    }

    impl BootLog {
        fn new(open_text_mode: bool, verbose: bool) -> Self {
            let mut log = Self {
                verbose,
                open_text_mode,
                buffered: Vec::new(),
            };
            if verbose {
                // Verbose prints run live; replay anything captured earlier.
                for message in log.buffered.drain(..) {
                    uefi::println!("{message}");
                }
            }
            log
        }

        fn record(&mut self, message: String) {
            if self.open_text_mode || self.verbose {
                uefi::println!("{message}");
            } else {
                self.buffered.push(message);
            }
        }
    }

    #[derive(Debug, Default)]
    struct StartupIntent {
        show_menu: bool,
        verbose: bool,
    }

    /// Samples keyboard input briefly while the brand mark is on screen.
    ///
    /// Navigation keys open the interactive picker; `V` requests a verbose
    /// textual boot. Unattended machines see neither and stay silent.
    fn capture_startup_intent() -> StartupIntent {
        let ticks = (STARTUP_LISTEN_MILLIS / LISTEN_TICK_MILLIS).max(1);
        let mut intent = StartupIntent::default();
        for _ in 0..ticks {
            let pressed = system::with_stdin(|stdin| stdin.read_key()).ok().flatten();
            if let Some(key) = pressed {
                let navigation =
                    KeyInput::is_up(&key) || KeyInput::is_down(&key) || KeyInput::is_escape(&key);
                let letter = KeyInput::to_char(&key).map(|ch| ch.to_ascii_lowercase());
                match (navigation, letter) {
                    (true, _) => {
                        intent.show_menu = true;
                        break;
                    }
                    // Enter commits whatever the picker would already choose,
                    // so treat it as an intentional pause rather than noise.
                    (false, Some('f' | 'p' | 'r')) => {
                        intent.show_menu = true;
                        break;
                    }
                    (false, Some('v')) => intent.verbose = true,
                    _ => {}
                }
            }
            boot::stall(Duration::from_millis(LISTEN_TICK_MILLIS));
        }
        intent
    }

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

    /// Translates a policy selection into the deployable menu equivalent.
    fn default_menu_action(action: BootAction) -> MenuAction {
        match action {
            BootAction::BootKnownGood(deployment) | BootAction::BootTrial { deployment, .. } => {
                MenuAction::BootDeployment {
                    slot: deployment.slot(),
                    generation: deployment.generation(),
                }
            }
            BootAction::Recovery(_) => MenuAction::Recovery,
        }
    }

    /// Builds the picker's default row, preserving historical entry wording.
    fn default_menu_entry(action: BootAction) -> MenuEntry {
        let title = match action {
            BootAction::BootKnownGood(deployment) => alloc::format!(
                "SOL {:?} generation {} (known-good)",
                deployment.slot(),
                deployment.generation()
            ),
            BootAction::BootTrial {
                deployment,
                attempt,
            } => alloc::format!(
                "SOL {:?} generation {} (trial attempt {})",
                deployment.slot(),
                deployment.generation(),
                attempt.get()
            ),
            BootAction::Recovery(reason) => alloc::format!("Recovery Mode ({:?})", reason),
        };
        MenuEntry {
            title,
            action: default_menu_action(action),
            is_default: true,
        }
    }

    fn run() -> Result<(), ()> {
        // NOTE: deliberately no text-console manipulation here — clearing or
        // re-configuring ConOut before handoff wedges child images on some
        // firmware. The graphical splash refresh overwrites the panel instead.

        // The mark leads everything: policy work, verification, and I/O all
        // happen underneath an already-present graphical presentation.
        let mut splash = SplashSession::open();
        if let Some(session) = splash.as_mut() {
            session.draw(SplashProgress::Hidden);
        }

        // Headless machines (no graphics output) get live console text so
        // harnesses and operators keep machine-readable status; panels stay
        // graphical-silent unless verbose boot was requested.
        let intent = capture_startup_intent();
        let mut log = BootLog::new(splash.is_none(), intent.verbose);
        boot_log!(log, "SOL boot {}", env!("CARGO_PKG_VERSION"));
        let vendor = Firmware::firmware_vendor();
        boot_log!(
            log,
            "platform: firmware '{}' revision {}, secure boot {}",
            vendor.as_deref().unwrap_or("unknown"),
            Firmware::firmware_revision(),
            if Firmware::is_secure_boot_enabled() {
                "on"
            } else {
                "off"
            }
        );

        let fs = boot::get_image_file_system(boot::image_handle()).map_err(|error| {
            Firmware::show_error_and_wait(&alloc::format!("Storage error: {}", error));
        })?;
        let mut storage = UefiStorage {
            fs: FileSystem::new(fs),
        };

        let recovery_requested = storage
            .read(RECOVERY_REQUEST)
            .map_err(|error| {
                boot_log!(log, "recovery request check failed: {error}");
            })?
            .is_some();

        if recovery_requested {
            boot_log!(log, "honoring one-shot recovery.request flag");
            let _ = storage.remove(RECOVERY_REQUEST);
        }

        let mut manager = BootManager::new(storage, RELEASE_KEY).map_err(|error| {
            Firmware::show_error_and_wait(&alloc::format!("Configuration error: {}", error));
        })?;

        let selection = match manager.select(recovery_requested) {
            Ok(selection) => selection,
            Err(error) => {
                boot_log!(log, "boot policy failed closed: {error}");
                Firmware::show_error_and_wait(&alloc::format!("Boot policy failed: {error}"));
                return start_first_available(
                    manager.storage_mut(),
                    &[RECOVERY_A, RECOVERY_B],
                    &mut log,
                );
            }
        };

        // Compose the launch status before the picker runs: the trial-attempt
        // detail lives on the selection, not on generic menu actions.
        let launch_line = match selection.action() {
            BootAction::BootKnownGood(deployment) => alloc::format!(
                "booting known-good slot {:?} generation {}",
                deployment.slot(),
                deployment.generation()
            ),
            BootAction::BootTrial {
                deployment,
                attempt,
            } => alloc::format!(
                "booting trial slot {:?} generation {} attempt {}",
                deployment.slot(),
                deployment.generation(),
                attempt.get()
            ),
            BootAction::Recovery(reason) => {
                alloc::format!("entering recovery mode ({reason:?})")
            }
        };

        let mut chosen = Some(default_menu_action(selection.action()));
        if intent.show_menu {
            let default_index = 0_usize;
            let mut entries = Vec::new();
            entries.push(default_menu_entry(selection.action()));
            entries.push(MenuEntry {
                title: "Recovery Mode".to_string(),
                action: MenuAction::Recovery,
                is_default: false,
            });
            entries.push(MenuEntry {
                title: "Firmware Setup".to_string(),
                action: MenuAction::FirmwareSetup,
                is_default: false,
            });
            entries.push(MenuEntry {
                title: "Power Off".to_string(),
                action: MenuAction::PowerOff,
                is_default: false,
            });

            let menu = BootMenu::new(entries.clone(), default_index, MENU_TIMEOUT_SECONDS);
            chosen = match menu.run() {
                Ok(MenuResult::Selected(index)) => {
                    if index >= entries.len() {
                        return Err(());
                    }
                    Some(entries[index].action)
                }
                Ok(MenuResult::FirmwareSetup) => Some(MenuAction::FirmwareSetup),
                Ok(MenuResult::PowerOff) => Some(MenuAction::PowerOff),
                Err(error) => {
                    boot_log!(log, "menu unavailable: {error}");
                    chosen
                }
            };
        }

        match chosen {
            Some(MenuAction::BootDeployment { .. }) => {
                boot_log!(log, "{launch_line}");
                if let Some(session) = splash.as_mut() {
                    session.ease_towards(MILESTONE_SELECTED);
                }
                let uki = manager.load_selected_uki(selection).map_err(|error| {
                    Firmware::show_error_and_wait(&alloc::format!("UKI load failed: {error}"));
                })?;
                if let Some(session) = splash.as_mut() {
                    session.ease_towards(MILESTONE_UKI_LOADED);
                }
                if let Some(session) = splash.as_mut() {
                    // Complete the capsule exactly as control transfers away.
                    session.ease_towards(1.0);
                }
                start_image_buffer(&uki, selection.uki_path().ok_or(())?, &mut log)
            }
            Some(MenuAction::Recovery) => {
                boot_log!(log, "{launch_line}");
                start_first_available(manager.storage_mut(), &[RECOVERY_A, RECOVERY_B], &mut log)
            }
            Some(MenuAction::FirmwareSetup) => {
                let _ = Firmware::reboot_to_firmware_setup().ok();
                Err(())
            }
            Some(MenuAction::PowerOff) => Firmware::power_off(),
            None => Err(()),
        }
    }

    fn start_first_available(
        storage: &mut UefiStorage,
        paths: &[&str],
        log: &mut BootLog,
    ) -> Result<(), ()> {
        for path in paths {
            boot_log!(log, "attempting recovery image {path}");
            let Ok(Some(image)) = storage.read(path) else {
                continue;
            };
            if start_image_buffer(&image, path, log).is_ok() {
                return Ok(());
            }
        }
        boot_log!(log, "no bootable image remained");
        Firmware::show_error_and_wait("No bootable image found");
        Err(())
    }

    /// Transfers control to the verified image bytes.
    ///
    /// Deliberately loaded without extra load context today: attaching a
    /// crafted media path crashed mid-start on reference OVMF builds, and
    /// anonymous buffer transfer is the long-standing exercised route.
    /// Restoring richer load context belongs to the seamless-handoff work.
    fn start_image_buffer(image: &[u8], path: &str, log: &mut BootLog) -> Result<(), ()> {
        boot_log!(log, "handoff: starting image {path}");
        let handle = match boot::load_image(
            boot::image_handle(),
            LoadImageSource::FromBuffer {
                buffer: image,
                file_path: None,
            },
        ) {
            Ok(handle) => handle,
            Err(error) => {
                boot_log!(log, "image {path} rejected: {error}");
                return Err(());
            }
        };
        match boot::start_image(handle) {
            Ok(()) => {
                boot_log!(log, "handoff: image {path} exited cleanly");
                Ok(())
            }
            Err(error) => {
                boot_log!(log, "image {path} returned: {error}");
                Err(())
            }
        }
    }

    /// Owns the selected GOP mode plus reusable buffers for splash draws so
    /// animation frames never reallocate mid-flight.
    struct SplashSession {
        gop: uefi::boot::ScopedProtocol<GraphicsOutput>,
        width: usize,
        height: usize,
        composition: Vec<BootPixel>,
        video: Vec<BltPixel>,
        /// Latest presented fill fraction, anchoring milestone easing.
        progress: f32,
    }

    impl SplashSession {
        /// Selects a usable graphics mode without disturbing the console on
        /// failure; every error degrades to silently skipping splash visuals.
        fn open() -> Option<Self> {
            let handle = boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
            let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;
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
                .and_then(|index| u32::try_from(index).ok())?;
            match select_graphics_mode(&descriptions, current_index, None) {
                GraphicsDecision::Preserve(_) => {}
                GraphicsDecision::SetOnce(mode) => {
                    let firmware_mode = modes.get(mode.index as usize)?;
                    gop.set_mode(firmware_mode).ok()?;
                }
                GraphicsDecision::Unavailable => return None,
            }
            let (width, height) = gop.current_mode_info().resolution();
            Some(Self {
                gop,
                width,
                height,
                composition: alloc::vec![BootPixel::rgb(0, 0, 0); width.checked_mul(height)?],
                video: alloc::vec![BltPixel::new(0, 0, 0); width.checked_mul(height)?],
                progress: 0.0,
            })
        }

        /// Composes and presents one splash frame, remembering the fill level.
        fn draw(&mut self, progress: SplashProgress) {
            if redraw_boot_frame(&mut self.composition, self.width, self.height, progress).is_none()
            {
                return;
            }
            for (target, pixel) in self.video.iter_mut().zip(&self.composition) {
                *target = BltPixel::new(pixel.red, pixel.green, pixel.blue);
            }
            let _ = self.gop.blt(BltOp::BufferToVideo {
                buffer: &self.video,
                src: BltRegion::Full,
                dest: (0, 0),
                dims: (self.width, self.height),
            });
            if let SplashProgress::Fraction(fraction) = progress {
                self.progress = fraction.clamp(0.0, 1.0);
            }
        }

        /// Animates the capsule from its current fill towards `target` with
        /// decelerating motion; each call bridges one real boot milestone, so
        /// the bar tracks genuine work instead of a fixed countdown.
        fn ease_towards(&mut self, target: f32) {
            const SEGMENT_FRAMES: u64 = 18;
            const FRAME_PACING_MICROS: u64 = 16_000;

            let goal = target.clamp(self.progress, 1.0);
            if goal <= self.progress {
                return;
            }
            let start = self.progress;
            for step in 1..=SEGMENT_FRAMES {
                let eased = ease_out_cubic(step as f32 / SEGMENT_FRAMES as f32);
                let fill = start + (goal - start) * eased;
                self.draw(SplashProgress::Fraction(fill));
                boot::stall(Duration::from_micros(FRAME_PACING_MICROS));
            }
            self.progress = goal;
        }
    }
}
