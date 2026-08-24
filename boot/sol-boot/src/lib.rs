#![no_std]
//! Firmware adapters and host-testable execution policy for `sol-boot`.

extern crate alloc;

mod graphics;
mod manager;

pub use graphics::{
    BootPixel, EdidError, GraphicsDecision, GraphicsMode, PreferredResolution, edid_preferred_mode,
    render_boot_frame, select_graphics_mode,
};
pub use manager::{BootManager, BootManagerError, BootStorage, SelectedBoot, SlotFiles, StateFile};
