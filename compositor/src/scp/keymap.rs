//! Keyboard keymap management for SCP.
//!
//! Handles XKB keymap generation and modifier tracking.

use std::os::unix::io::RawFd;
use std::collections::HashSet;

/// Keyboard keymap state.
#[derive(Debug)]
pub struct KeymapState {
    /// XKB keymap format
    format: KeymapFormat,
    /// Keymap data (mmap-able file descriptor in production)
    keymap_data: Vec<u8>,
    /// Keymap size in bytes
    size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapFormat {
    NoKeymap = 0,
    XkbV1 = 1,
}

impl Default for KeymapState {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapState {
    pub fn new() -> Self {
        // Default to a minimal XKB keymap
        let default_keymap = Self::generate_default_xkb_keymap();
        Self {
            format: KeymapFormat::XkbV1,
            keymap_data: default_keymap.into_bytes(),
            size: 0, // Will be set when data is available
        }
    }

    /// Generate a minimal XKB keymap for testing/fallback.
    fn generate_default_xkb_keymap() -> String {
        // Minimal XKB keymap with basic US layout
        r#"xkb_keymap {
    xkb_keycodes "evdev+aliases(qwerty)" {
        minimum = 8;
        maximum = 255;
        <ESC> = 9;
        <AE01> = 10;
        <AE02> = 11;
        <AE03> = 12;
        <AE04> = 13;
        <AE05> = 14;
        <AE06> = 15;
        <AE07> = 16;
        <AE08> = 17;
        <AE09> = 18;
        <AE10> = 19;
        <BKSP> = 22;
        <TAB> = 23;
        <RTRN> = 36;
        <LCTL> = 37;
        <LSHF> = 50;
        <RSHF> = 62;
        <LALT> = 64;
        <SPCE> = 65;
        <CAPS> = 66;
    };

    xkb_types "complete" {
        virtual_modifiers NumLock,Alt,LevelThree,LevelFive,Meta,Super,Hyper,ScrollLock;
        type "ONE_LEVEL" {
            modifiers= none;
            level_name[Level1]= "Any";
        };
        type "TWO_LEVEL" {
            modifiers= Shift;
            map[Shift]= Level2;
            level_name[Level1]= "Base";
            level_name[Level2]= "Shift";
        };
    };

    xkb_compatibility "complete" {
        interpret Any+AnyOf(all) {
            action= SetMods(modifiers=modMapMods,clearLocks);
        };
    };

    xkb_symbols "pc+us+inet(evdev)" {
        name[group1]="English (US)";
        key <ESC>  { [ Escape ] };
        key <AE01> { [ 1, exclam ] };
        key <AE02> { [ 2, at ] };
        key <BKSP> { [ BackSpace ] };
        key <TAB>  { [ Tab, ISO_Left_Tab ] };
        key <RTRN> { [ Return ] };
        key <LCTL> { [ Control_L ] };
        key <LSHF> { [ Shift_L ] };
        key <RSHF> { [ Shift_R ] };
        key <LALT> { [ Alt_L, Meta_L ] };
        key <SPCE> { [ space ] };
        key <CAPS> { [ Caps_Lock ] };
    };

    xkb_geometry "pc(pc104)" {
        width= 470;
        height= 180;
    };
};
"#
        .to_string()
    }

    pub fn format(&self) -> KeymapFormat {
        self.format
    }

    pub fn size(&self) -> u32 {
        self.keymap_data.len() as u32
    }

    pub fn data(&self) -> &[u8] {
        &self.keymap_data
    }

    /// Create a sealed memfd for the keymap (Phase 1+).
    #[cfg(target_os = "linux")]
    pub fn create_memfd(&self) -> Result<RawFd, std::io::Error> {
        use std::io::Write;
        use std::os::unix::io::FromRawFd;
        use crate::scp::memfd;

        // Create anonymous sealed memfd
        let fd = memfd::create("sol-keymap", true)?;

        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(&self.keymap_data)?;
        file.sync_all()?;

        // Get raw fd back before sealing
        let fd = std::os::unix::io::IntoRawFd::into_raw_fd(file);

        // Seal the memfd (make it read-only)
        memfd::seal_readonly(fd)?;

        Ok(fd)
    }

    /// Fallback for non-Linux or testing: create temp file.
    #[cfg(not(target_os = "linux"))]
    pub fn create_memfd(&self) -> Result<RawFd, std::io::Error> {
        use std::io::Write;
        use std::os::unix::io::IntoRawFd;

        let mut tmpfile = tempfile::tempfile()?;
        tmpfile.write_all(&self.keymap_data)?;
        tmpfile.sync_all()?;
        Ok(tmpfile.into_raw_fd())
    }

    /// Update keymap from external source (e.g., system XKB config).
    pub fn update_from_xkb(&mut self, keymap_string: String) {
        self.keymap_data = keymap_string.into_bytes();
        self.size = self.keymap_data.len() as u32;
    }
}

/// Keyboard repeat rate configuration.
#[derive(Debug, Clone, Copy)]
pub struct RepeatInfo {
    /// Characters per second
    pub rate: i32,
    /// Delay before repeat starts (ms)
    pub delay: i32,
}

impl Default for RepeatInfo {
    fn default() -> Self {
        Self {
            rate: 25,    // 25 chars/sec
            delay: 600,  // 600ms delay
        }
    }
}

impl RepeatInfo {
    pub fn new(rate: i32, delay: i32) -> Self {
        Self { rate, delay }
    }

    pub fn disabled() -> Self {
        Self { rate: 0, delay: 0 }
    }

    pub fn is_enabled(&self) -> bool {
        self.rate > 0
    }
}

/// Keyboard modifier state tracking.
#[derive(Debug, Clone, Default)]
pub struct ModifierState {
    /// Currently pressed modifier keys (by keycode)
    pressed_keys: HashSet<u32>,
    /// Depressed modifiers (currently held down)
    pub mods_depressed: u32,
    /// Latched modifiers (locked until next non-modifier key)
    pub mods_latched: u32,
    /// Locked modifiers (e.g., Caps Lock, Num Lock)
    pub mods_locked: u32,
    /// Active keyboard layout group
    pub group: u32,
}

/// Standard modifier masks (XKB-compatible)
pub mod modifiers {
    pub const SHIFT: u32 = 1 << 0;
    pub const CAPS_LOCK: u32 = 1 << 1;
    pub const CTRL: u32 = 1 << 2;
    pub const ALT: u32 = 1 << 3;
    pub const NUM_LOCK: u32 = 1 << 4;
    pub const MOD3: u32 = 1 << 5;
    pub const SUPER: u32 = 1 << 6;
    pub const MOD5: u32 = 1 << 7;
}

impl ModifierState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update modifier state when a key is pressed.
    pub fn key_pressed(&mut self, keycode: u32) {
        self.pressed_keys.insert(keycode);
        self.update_modifiers(keycode, true);
    }

    /// Update modifier state when a key is released.
    pub fn key_released(&mut self, keycode: u32) {
        self.pressed_keys.remove(&keycode);
        self.update_modifiers(keycode, false);
    }

    /// Check if a specific modifier is active.
    pub fn is_modifier_active(&self, modifier_mask: u32) -> bool {
        (self.mods_depressed | self.mods_latched | self.mods_locked) & modifier_mask != 0
    }

    /// Check if Shift is active.
    pub fn is_shift(&self) -> bool {
        self.is_modifier_active(modifiers::SHIFT)
    }

    /// Check if Ctrl is active.
    pub fn is_ctrl(&self) -> bool {
        self.is_modifier_active(modifiers::CTRL)
    }

    /// Check if Alt is active.
    pub fn is_alt(&self) -> bool {
        self.is_modifier_active(modifiers::ALT)
    }

    /// Check if Super/Meta is active.
    pub fn is_super(&self) -> bool {
        self.is_modifier_active(modifiers::SUPER)
    }

    /// Reset modifier state (e.g., on focus loss).
    pub fn reset(&mut self) {
        self.pressed_keys.clear();
        self.mods_depressed = 0;
        self.mods_latched = 0;
        // Keep mods_locked (Caps Lock, Num Lock persist)
    }

    /// Get currently pressed keys.
    pub fn pressed_keys(&self) -> Vec<u32> {
        self.pressed_keys.iter().copied().collect()
    }

    /// Internal: update modifier masks based on keycode.
    fn update_modifiers(&mut self, keycode: u32, pressed: bool) {
        let modifier_mask = self.keycode_to_modifier(keycode);

        if modifier_mask == 0 {
            return; // Not a modifier key
        }

        if pressed {
            self.mods_depressed |= modifier_mask;
        } else {
            self.mods_depressed &= !modifier_mask;
        }
    }

    /// Map keycode to modifier mask (simplified, real implementation uses XKB).
    fn keycode_to_modifier(&self, keycode: u32) -> u32 {
        // Linux evdev keycodes (offset by 8 from XKB)
        match keycode {
            50 | 62 => modifiers::SHIFT,    // Left/Right Shift
            37 | 105 => modifiers::CTRL,    // Left/Right Ctrl
            64 | 108 => modifiers::ALT,     // Left/Right Alt
            133 | 134 => modifiers::SUPER,  // Left/Right Super
            66 => modifiers::CAPS_LOCK,     // Caps Lock
            77 => modifiers::NUM_LOCK,      // Num Lock
            _ => 0,
        }
    }

    /// Toggle lock modifier (e.g., Caps Lock).
    pub fn toggle_lock(&mut self, modifier_mask: u32) {
        self.mods_locked ^= modifier_mask;
    }

    /// Set layout group.
    pub fn set_group(&mut self, group: u32) {
        self.group = group;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_keymap() {
        let keymap = KeymapState::new();
        assert_eq!(keymap.format(), KeymapFormat::XkbV1);
        assert!(keymap.size() > 0);
        assert!(keymap.data().len() > 0);
    }

    #[test]
    fn test_repeat_info() {
        let repeat = RepeatInfo::default();
        assert!(repeat.is_enabled());

        let disabled = RepeatInfo::disabled();
        assert!(!disabled.is_enabled());
    }

    #[test]
    fn test_modifier_state() {
        let mut mods = ModifierState::new();

        // Press Shift
        mods.key_pressed(50);
        assert!(mods.is_shift());
        assert!(!mods.is_ctrl());

        // Press Ctrl
        mods.key_pressed(37);
        assert!(mods.is_shift());
        assert!(mods.is_ctrl());

        // Release Shift
        mods.key_released(50);
        assert!(!mods.is_shift());
        assert!(mods.is_ctrl());

        // Release Ctrl
        mods.key_released(37);
        assert!(!mods.is_ctrl());
    }

    #[test]
    fn test_modifier_reset() {
        let mut mods = ModifierState::new();
        mods.key_pressed(50); // Shift
        mods.key_pressed(37); // Ctrl

        assert!(mods.is_shift());
        assert!(mods.is_ctrl());

        mods.reset();
        assert!(!mods.is_shift());
        assert!(!mods.is_ctrl());
    }

    #[test]
    fn test_caps_lock_toggle() {
        let mut mods = ModifierState::new();

        mods.toggle_lock(modifiers::CAPS_LOCK);
        assert!(mods.is_modifier_active(modifiers::CAPS_LOCK));

        mods.toggle_lock(modifiers::CAPS_LOCK);
        assert!(!mods.is_modifier_active(modifiers::CAPS_LOCK));
    }
}
