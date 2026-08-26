//! Minimal UEFI child image used only by the OVMF end-to-end harness.

#![cfg_attr(target_os = "uefi", no_main)]
#![cfg_attr(target_os = "uefi", no_std)]

#[cfg(not(target_os = "uefi"))]
fn main() {
    eprintln!("sol-boot-test-payload is only used by the OVMF integration test");
}

#[cfg(target_os = "uefi")]
mod firmware {
    use uefi::prelude::*;
    use uefi::runtime::{self, ResetType};

    #[entry]
    fn main() -> Status {
        if uefi::helpers::init().is_err() {
            return Status::ABORTED;
        }
        uefi::println!("SOL_BOOT_TEST_PAYLOAD_STARTED");
        runtime::reset(ResetType::SHUTDOWN, Status::SUCCESS, None)
    }
}
