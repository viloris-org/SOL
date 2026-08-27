#![no_std]
//! Firmware adapters and host-testable execution policy for `sol-boot`.

extern crate alloc;

mod graphics;
mod manager;

#[cfg(target_os = "uefi")]
mod console;
#[cfg(target_os = "uefi")]
mod firmware;
#[cfg(target_os = "uefi")]
mod menu;

pub use graphics::{
    BootPixel, EdidError, GraphicsDecision, GraphicsMode, MAX_FRAME_EDGE, PreferredResolution,
    SplashProgress, ease_out_cubic, edid_preferred_mode, redraw_boot_frame, render_boot_frame,
    select_graphics_mode,
};
pub use manager::{BootManager, BootManagerError, BootStorage, SelectedBoot, SlotFiles, StateFile};

#[cfg(target_os = "uefi")]
pub use console::{Console, KeyInput, MenuColor};
#[cfg(target_os = "uefi")]
pub use firmware::{Firmware, FirmwareError, FirmwareResult};
#[cfg(target_os = "uefi")]
pub use menu::{BootMenu, MenuAction, MenuEntry, MenuError, MenuResult};
