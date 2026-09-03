#![no_std]
//! Firmware adapters and host-testable execution policy for `sol-boot`.

extern crate alloc;

mod logging;
mod manager;

pub use logging::{BootLog, LogLevel};
pub use manager::{BootManager, BootManagerError, BootStorage, SelectedBoot, SlotFiles, StateFile};

#[cfg(target_os = "uefi")]
mod graphics;
#[cfg(target_os = "uefi")]
pub use graphics::draw_optional_boot_frame;
