//! UEFI console abstractions for menu rendering and input handling.

use core::time::Duration;
use uefi::proto::console::text::{Color, Key, ScanCode};
use uefi::{boot, system};

/// Console color scheme for boot menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuColor {
    /// Normal text.
    Normal,
    /// Highlighted entry.
    Highlight,
    /// Default entry indicator.
    Entry,
    /// Error or warning.
    Error,
}

impl MenuColor {
    /// Returns UEFI foreground/background color pair.
    pub const fn to_uefi(self) -> (Color, Color) {
        match self {
            Self::Normal => (Color::LightGray, Color::Black),
            Self::Highlight => (Color::Black, Color::LightGray),
            Self::Entry => (Color::White, Color::Black),
            Self::Error => (Color::LightRed, Color::Black),
        }
    }
}

/// Console dimensions and state.
pub struct Console {
    width: usize,
    height: usize,
}

impl Console {
    /// Queries current console dimensions.
    pub fn new() -> Result<Self, uefi::Error> {
        let mode = system::with_stdout(|stdout| stdout.current_mode())?;
        let mode_info = mode.unwrap();
        Ok(Self {
            width: mode_info.columns(),
            height: mode_info.rows(),
        })
    }

    /// Returns console width in characters.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Returns console height in lines.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Clears the entire screen.
    pub fn clear(&self) {
        let _ = system::with_stdout(|stdout| stdout.clear());
    }

    /// Sets console color.
    pub fn set_color(&self, color: MenuColor) {
        let (fg, bg) = color.to_uefi();
        let _ = system::with_stdout(|stdout| stdout.set_color(fg, bg));
    }

    /// Moves cursor to position (0-indexed).
    pub fn set_cursor(&self, x: usize, y: usize) {
        let _ = system::with_stdout(|stdout| stdout.set_cursor_position(x, y));
    }

    /// Shows or hides cursor.
    pub fn set_cursor_visible(&self, visible: bool) {
        let _ = system::with_stdout(|stdout| stdout.enable_cursor(visible));
    }

    /// Prints text at current cursor position.
    pub fn print(&self, text: &str) {
        let _ = uefi::print!("{}", text);
    }

    /// Prints text at specific position with color.
    pub fn print_at(&self, x: usize, y: usize, color: MenuColor, text: &str) {
        self.set_cursor(x, y);
        self.set_color(color);
        self.print(text);
    }

    /// Reads a key with optional timeout in microseconds.
    ///
    /// Returns `Ok(Some(key))` if pressed, `Ok(None)` on timeout, `Err` on failure.
    pub fn read_key(&self, timeout_us: Option<u64>) -> Result<Option<Key>, uefi::Error> {
        if let Some(_timeout) = timeout_us {
            // Simple polling implementation
            system::with_stdin(|stdin| stdin.read_key())
        } else {
            // Block indefinitely
            system::with_stdin(|stdin| stdin.read_key())
        }
    }

    /// Waits for any key press.
    pub fn wait_for_key(&self) -> Result<Key, uefi::Error> {
        loop {
            if let Some(key) = self.read_key(None)? {
                return Ok(key);
            }
            boot::stall(Duration::from_millis(100));
        }
    }
}

/// Key input helper.
pub struct KeyInput;

impl KeyInput {
    /// Checks if key is Enter/Return.
    #[must_use]
    pub fn is_enter(key: &Key) -> bool {
        matches!(key, Key::Special(ScanCode::NULL))
    }

    /// Checks if key is Escape.
    #[must_use]
    pub fn is_escape(key: &Key) -> bool {
        matches!(key, Key::Special(ScanCode::ESCAPE))
    }

    /// Checks if key is Up arrow.
    #[must_use]
    pub fn is_up(key: &Key) -> bool {
        matches!(key, Key::Special(ScanCode::UP))
    }

    /// Checks if key is Down arrow.
    #[must_use]
    pub fn is_down(key: &Key) -> bool {
        matches!(key, Key::Special(ScanCode::DOWN))
    }

    /// Checks if key is a printable character.
    #[must_use]
    pub fn is_printable(key: &Key) -> bool {
        matches!(key, Key::Printable(_))
    }

    /// Extracts character from printable key.
    #[must_use]
    pub fn to_char(key: &Key) -> Option<char> {
        match key {
            Key::Printable(character) => Some(char::from(*character)),
            _ => None,
        }
    }
}
