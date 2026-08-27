//! Interactive boot menu with timeout and keyboard navigation.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use sol_boot_core::DeploymentSlot;

use crate::console::{Console, KeyInput, MenuColor};

/// Boot menu entry display information.
#[derive(Debug, Clone)]
pub struct MenuEntry {
    /// Entry title (e.g., "SOL A generation 5").
    pub title: String,
    /// Entry action.
    pub action: MenuAction,
    /// Whether this is the default entry.
    pub is_default: bool,
}

/// Actions available from the boot menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Boot a specific slot/generation.
    BootDeployment {
        slot: DeploymentSlot,
        generation: u64,
    },
    /// Enter recovery mode.
    Recovery,
    /// Reboot to firmware setup.
    FirmwareSetup,
    /// Power off.
    PowerOff,
}

/// Menu user interaction result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuResult {
    /// User selected an entry (or timeout expired).
    Selected(usize),
    /// User requested firmware setup.
    FirmwareSetup,
    /// User requested power off.
    PowerOff,
}

/// Interactive boot menu configuration.
pub struct BootMenu {
    entries: Vec<MenuEntry>,
    default_index: usize,
    timeout_seconds: u64,
}

impl BootMenu {
    /// Creates a new boot menu with entries and timeout.
    pub fn new(entries: Vec<MenuEntry>, default_index: usize, timeout_seconds: u64) -> Self {
        Self {
            entries,
            default_index,
            timeout_seconds,
        }
    }

    /// Runs the interactive menu and returns the selected entry index.
    pub fn run(&self) -> Result<MenuResult, MenuError> {
        let console = Console::new().map_err(|_| MenuError::ConsoleInit)?;
        console.set_cursor_visible(false);
        console.clear();

        let mut selected = self.default_index;
        let mut timeout_remaining = self.timeout_seconds;
        let mut needs_redraw = true;
        let mut tick_count = 0u32;

        loop {
            if needs_redraw {
                self.draw_menu(&console, selected, timeout_remaining)?;
                needs_redraw = false;
            }

            // Read key with short timeout for countdown tick
            let key_result = console.read_key(Some(100_000)); // 100ms

            match key_result {
                Ok(Some(key)) => {
                    // Key pressed, cancel timeout
                    timeout_remaining = 0;

                    if KeyInput::is_up(&key) {
                        if selected > 0 {
                            selected -= 1;
                            needs_redraw = true;
                        }
                    } else if KeyInput::is_down(&key) {
                        if selected + 1 < self.entries.len() {
                            selected += 1;
                            needs_redraw = true;
                        }
                    } else if KeyInput::is_enter(&key) {
                        // User confirmed selection
                        return Ok(self.handle_entry_action(selected));
                    } else if KeyInput::is_escape(&key) {
                        // ESC: select default and boot
                        return Ok(MenuResult::Selected(self.default_index));
                    } else if let Some(ch) = KeyInput::to_char(&key) {
                        // Check for hotkeys
                        if ch == 'f' || ch == 'F' {
                            return Ok(MenuResult::FirmwareSetup);
                        } else if ch == 'p' || ch == 'P' {
                            return Ok(MenuResult::PowerOff);
                        }
                    }
                }
                Ok(None) => {
                    // No key, count ticks for timeout
                    if timeout_remaining > 0 {
                        tick_count += 1;
                        // Approximately 10 ticks = 1 second
                        if tick_count >= 10 {
                            tick_count = 0;
                            timeout_remaining -= 1;
                            needs_redraw = true;

                            if timeout_remaining == 0 {
                                // Auto-boot default entry
                                return Ok(MenuResult::Selected(self.default_index));
                            }
                        }
                    }
                }
                Err(_) => {
                    // Input error, retry
                    continue;
                }
            }
        }
    }

    /// Handles the action associated with a menu entry.
    fn handle_entry_action(&self, index: usize) -> MenuResult {
        if index >= self.entries.len() {
            return MenuResult::Selected(self.default_index);
        }

        match self.entries[index].action {
            MenuAction::BootDeployment { .. } => MenuResult::Selected(index),
            MenuAction::Recovery => MenuResult::Selected(index),
            MenuAction::FirmwareSetup => MenuResult::FirmwareSetup,
            MenuAction::PowerOff => MenuResult::PowerOff,
        }
    }

    /// Draws the complete menu interface.
    fn draw_menu(&self, console: &Console, selected: usize, timeout: u64) -> Result<(), MenuError> {
        console.clear();

        let height = console.height();
        let width = console.width();

        // Calculate layout
        let visible_entries = (height.saturating_sub(5)).min(self.entries.len());
        let start_y = 2;
        let mut first_visible = selected.saturating_sub(visible_entries / 2);
        if first_visible + visible_entries > self.entries.len() {
            first_visible = self.entries.len().saturating_sub(visible_entries);
        }

        // Draw title
        let title = "SOL Boot Menu";
        let title_x = (width.saturating_sub(title.len())) / 2;
        console.print_at(title_x, 0, MenuColor::Entry, title);

        // Draw entries
        for (i, entry_index) in (first_visible..first_visible + visible_entries).enumerate() {
            if entry_index >= self.entries.len() {
                break;
            }

            let entry = &self.entries[entry_index];
            let y = start_y + i;
            let is_selected = entry_index == selected;
            let color = if is_selected {
                MenuColor::Highlight
            } else {
                MenuColor::Normal
            };

            // Format entry line
            let prefix = if entry.is_default { " ► " } else { "   " };
            let entry_text = alloc::format!("{}{}", prefix, entry.title);
            let entry_x = (width.saturating_sub(entry_text.len())) / 2;

            console.print_at(entry_x, y, color, &entry_text);
        }

        // Draw status line
        let status_y = height.saturating_sub(2);
        if timeout > 0 {
            let status = alloc::format!(
                "Boot in {} seconds (↑/↓: navigate, Enter: select, Esc: default)",
                timeout
            );
            let status_x = (width.saturating_sub(status.len().min(width))) / 2;
            console.print_at(status_x, status_y, MenuColor::Normal, &status);
        } else {
            let status = "↑/↓: navigate, Enter: select, F: firmware setup, P: power off";
            let status_x = (width.saturating_sub(status.len().min(width))) / 2;
            console.print_at(status_x, status_y, MenuColor::Normal, status);
        }

        Ok(())
    }
}

/// Menu-related errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuError {
    /// Failed to initialize console.
    ConsoleInit,
    /// Invalid entry selection.
    InvalidSelection,
}

impl fmt::Display for MenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConsoleInit => f.write_str("failed to initialize console"),
            Self::InvalidSelection => f.write_str("invalid menu selection"),
        }
    }
}
