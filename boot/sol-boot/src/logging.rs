//! Best-effort boot diagnostics logging to persistent ESP storage.
//!
//! Logs are written to `\EFI\SOL\logs\boot-{timestamp}.log` and help diagnose
//! production boot failures. Logging failures are never fatal - boot continues
//! regardless of log write success.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// Maximum number of boot logs retained on ESP.
const MAX_RETAINED_LOGS: usize = 10;

/// Boot event severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

/// In-memory boot log builder.
pub struct BootLog {
    entries: Vec<(LogLevel, String)>,
}

impl BootLog {
    /// Creates a new empty boot log.
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(32),
        }
    }

    /// Appends an informational entry.
    pub fn info(&mut self, message: impl Into<String>) {
        self.entries.push((LogLevel::Info, message.into()));
    }

    /// Appends a warning entry.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.entries.push((LogLevel::Warning, message.into()));
    }

    /// Appends an error entry.
    pub fn error(&mut self, message: impl Into<String>) {
        self.entries.push((LogLevel::Error, message.into()));
    }

    /// Formats the complete log as a UTF-8 string.
    pub fn format(&self) -> String {
        let mut output = String::new();
        let _ = writeln!(output, "SOL Boot Log");
        let _ = writeln!(output, "============");
        let _ = writeln!(output);

        for (level, message) in &self.entries {
            let prefix = match level {
                LogLevel::Info => "[INFO]",
                LogLevel::Warning => "[WARN]",
                LogLevel::Error => "[ERROR]",
            };
            let _ = writeln!(output, "{} {}", prefix, message);
        }

        output
    }

    /// Returns the log as UTF-8 bytes for ESP write.
    pub fn as_bytes(&self) -> Vec<u8> {
        self.format().into_bytes()
    }
}

impl Default for BootLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_formatting_works() {
        let mut log = BootLog::new();
        log.info("System started");
        log.warn("Trial exhausted");
        log.error("Verification failed");

        let formatted = log.format();
        assert!(formatted.contains("[INFO] System started"));
        assert!(formatted.contains("[WARN] Trial exhausted"));
        assert!(formatted.contains("[ERROR] Verification failed"));
    }
}
