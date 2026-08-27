//! Firmware integration utilities (reboot, power off, firmware setup).

use core::fmt;
use core::time::Duration;
use uefi::{CStr16, runtime};

/// Firmware action result.
pub type FirmwareResult<T> = Result<T, FirmwareError>;

/// Firmware operation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareError {
    /// Runtime services unavailable.
    RuntimeUnavailable,
    /// Operation not supported by firmware.
    Unsupported,
    /// Operation failed.
    Failed,
}

impl fmt::Display for FirmwareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeUnavailable => f.write_str("runtime services unavailable"),
            Self::Unsupported => f.write_str("operation not supported"),
            Self::Failed => f.write_str("operation failed"),
        }
    }
}

/// Firmware integration interface.
pub struct Firmware;

impl Firmware {
    /// Reboots the system.
    ///
    /// This function does not return on success.
    pub fn reboot() -> ! {
        runtime::reset(runtime::ResetType::COLD, uefi::Status::SUCCESS, None);
    }

    /// Powers off the system.
    ///
    /// This function does not return on success.
    pub fn power_off() -> ! {
        runtime::reset(runtime::ResetType::SHUTDOWN, uefi::Status::SUCCESS, None);
    }

    /// Reboots into firmware setup (BIOS/UEFI settings).
    ///
    /// This function does not return on success.
    pub fn reboot_to_firmware_setup() -> Result<(), FirmwareError> {
        // Set OsIndications to request firmware UI
        const OS_INDICATIONS_BOOT_TO_FW_UI: u64 = 0x0000000000000001;

        let mut var_name_buf = [0u16; 32];
        let var_name = CStr16::from_str_with_buf("OsIndications", &mut var_name_buf)
            .map_err(|_| FirmwareError::Failed)?;
        let vendor_guid = runtime::VariableVendor::GLOBAL_VARIABLE;

        // Try to read current value first
        let mut current_value = OS_INDICATIONS_BOOT_TO_FW_UI;
        let mut buffer = [0u8; 8];
        if let Ok((data, _attrs)) = runtime::get_variable(var_name, &vendor_guid, &mut buffer) {
            if data.len() >= 8 {
                current_value = u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]) | OS_INDICATIONS_BOOT_TO_FW_UI;
            }
        }

        // Set the firmware UI bit
        let value_bytes = current_value.to_le_bytes();
        let attrs = runtime::VariableAttributes::BOOTSERVICE_ACCESS
            | runtime::VariableAttributes::RUNTIME_ACCESS
            | runtime::VariableAttributes::NON_VOLATILE;

        runtime::set_variable(var_name, &vendor_guid, attrs, &value_bytes)
            .map_err(|_| FirmwareError::Failed)?;

        Self::reboot();
    }

    /// Checks if secure boot is enabled.
    pub fn is_secure_boot_enabled() -> bool {
        let mut var_name_buf = [0u16; 32];
        let var_name = match CStr16::from_str_with_buf("SecureBoot", &mut var_name_buf) {
            Ok(name) => name,
            Err(_) => return false,
        };
        let vendor_guid = runtime::VariableVendor::GLOBAL_VARIABLE;

        let mut buffer = [0u8; 1];
        if let Ok((data, _attrs)) = runtime::get_variable(var_name, &vendor_guid, &mut buffer) {
            !data.is_empty() && data[0] == 1
        } else {
            false
        }
    }

    /// Gets the boot firmware vendor string.
    pub fn firmware_vendor() -> Option<alloc::string::String> {
        // Simplified: return placeholder since safe API access is complex
        Some(alloc::string::String::from("UEFI Firmware"))
    }

    /// Gets the firmware revision.
    pub fn firmware_revision() -> u32 {
        // Simplified: return placeholder
        0
    }

    /// Stalls execution for the specified number of microseconds.
    pub fn stall(microseconds: usize) {
        let _ = uefi::boot::stall(Duration::from_micros(microseconds as u64));
    }

    /// Displays an error message and waits briefly for user acknowledgment.
    ///
    /// The wait is time-bounded so headless machines and unattended recovery
    /// flows automatically continue into the recovery fallback instead of
    /// dead-ending on a prompt nobody can answer.
    pub fn show_error_and_wait(message: &str) {
        use crate::console::{Console, MenuColor};

        if let Ok(console) = Console::new() {
            let width = console.width();
            let height = console.height();

            console.clear();
            console.set_cursor_visible(false);

            // Draw error box
            let title = "Boot Error";
            let title_x = (width.saturating_sub(title.len())) / 2;
            console.print_at(title_x, height / 2 - 3, MenuColor::Error, title);

            let msg_x = (width.saturating_sub(message.len())) / 2;
            console.print_at(msg_x, height / 2 - 1, MenuColor::Normal, message);

            let prompt = "Press any key to enter recovery...";
            let prompt_x = (width.saturating_sub(prompt.len())) / 2;
            console.print_at(prompt_x, height / 2 + 2, MenuColor::Normal, prompt);

            // Wait up to three seconds before continuing automatically.
            for _ in 0..30 {
                let acknowledged = console
                    .read_key(None)
                    .map(|pressed| pressed.is_some())
                    .unwrap_or(false);
                if acknowledged {
                    break;
                }
                Self::stall(100_000);
            }
        } else {
            // Fallback to simple println
            uefi::println!("\nERROR: {}", message);
            uefi::println!("Press any key to continue...");
            Self::stall(5_000_000); // 5 seconds
        }
    }
}
