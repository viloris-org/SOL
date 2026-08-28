//! Keycode decoding for the login screen.
//!
//! SCP delivers raw XKB keycodes — evdev codes offset by 8, the same space
//! `sol-compositor`'s keymap module speaks. Turning one into a character needs a
//! layout, and the compositor does send a keymap (`CompositorMessage::KeymapFormat`),
//! but today that keymap is a hand-written stub covering about twenty keys, and
//! reading it properly would mean pulling in libxkbcommon.
//!
//! So the greeter carries its own US-QWERTY table. It is deliberately small and
//! self-contained: a password prompt needs printable characters, backspace, and
//! Enter, and nothing here should depend on a system library being present.
//!
//! TODO: decode against the keymap the compositor delivers once it ships a real
//! one, so non-US layouts can log in.

/// Modifier state relevant to producing a character.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub caps_lock: bool,
}

impl Modifiers {
    /// Read the modifier masks SCP reports.
    ///
    /// Masks match `sol_compositor::scp::keymap::modifiers`; latched and locked
    /// are folded in alongside depressed so that Caps Lock and a latched Shift
    /// both count.
    pub const fn from_masks(depressed: u32, latched: u32, locked: u32) -> Self {
        const SHIFT: u32 = 1 << 0;
        const CAPS_LOCK: u32 = 1 << 1;

        let active = depressed | latched | locked;
        Self {
            shift: active & SHIFT != 0,
            caps_lock: active & CAPS_LOCK != 0,
        }
    }
}

/// What a keypress means to the login UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyInput {
    /// A printable character to append to the password.
    Char(char),
    /// Delete the character before the caret.
    Backspace,
    /// Submit the password.
    Enter,
    /// Move to the next user.
    NextUser,
    /// Move to the previous user.
    PreviousUser,
    /// Abandon what has been typed.
    Escape,
}

/// XKB keycodes for the keys that are not printable characters.
mod code {
    pub const ESCAPE: u32 = 9;
    pub const BACKSPACE: u32 = 22;
    pub const TAB: u32 = 23;
    pub const ENTER: u32 = 36;
    pub const KEYPAD_ENTER: u32 = 104;
    pub const LEFT: u32 = 113;
    pub const RIGHT: u32 = 114;
    pub const UP: u32 = 111;
    pub const DOWN: u32 = 116;
}

/// Printable US-QWERTY keys as `(keycode, unshifted, shifted)`.
///
/// Keycodes are evdev + 8, laid out row by row from the number row down.
const PRINTABLE: &[(u32, char, char)] = &[
    // Number row
    (10, '1', '!'),
    (11, '2', '@'),
    (12, '3', '#'),
    (13, '4', '$'),
    (14, '5', '%'),
    (15, '6', '^'),
    (16, '7', '&'),
    (17, '8', '*'),
    (18, '9', '('),
    (19, '0', ')'),
    (20, '-', '_'),
    (21, '=', '+'),
    // Top letter row
    (24, 'q', 'Q'),
    (25, 'w', 'W'),
    (26, 'e', 'E'),
    (27, 'r', 'R'),
    (28, 't', 'T'),
    (29, 'y', 'Y'),
    (30, 'u', 'U'),
    (31, 'i', 'I'),
    (32, 'o', 'O'),
    (33, 'p', 'P'),
    (34, '[', '{'),
    (35, ']', '}'),
    // Home row
    (38, 'a', 'A'),
    (39, 's', 'S'),
    (40, 'd', 'D'),
    (41, 'f', 'F'),
    (42, 'g', 'G'),
    (43, 'h', 'H'),
    (44, 'j', 'J'),
    (45, 'k', 'K'),
    (46, 'l', 'L'),
    (47, ';', ':'),
    (48, '\'', '"'),
    (49, '`', '~'),
    (51, '\\', '|'),
    // Bottom letter row
    (52, 'z', 'Z'),
    (53, 'x', 'X'),
    (54, 'c', 'C'),
    (55, 'v', 'V'),
    (56, 'b', 'B'),
    (57, 'n', 'N'),
    (58, 'm', 'M'),
    (59, ',', '<'),
    (60, '.', '>'),
    (61, '/', '?'),
    // Space
    (65, ' ', ' '),
];

/// Decode a pressed key, or `None` when it means nothing to the login screen.
///
/// Modifier keys themselves decode to `None`: their effect arrives separately in
/// `CompositorMessage::Modifiers`.
pub fn decode(keycode: u32, modifiers: Modifiers) -> Option<KeyInput> {
    match keycode {
        code::BACKSPACE => return Some(KeyInput::Backspace),
        code::ENTER | code::KEYPAD_ENTER => return Some(KeyInput::Enter),
        code::ESCAPE => return Some(KeyInput::Escape),
        code::TAB | code::DOWN | code::RIGHT if !modifiers.shift => {
            return Some(KeyInput::NextUser);
        }
        code::TAB => return Some(KeyInput::PreviousUser),
        code::UP | code::LEFT => return Some(KeyInput::PreviousUser),
        _ => {}
    }

    let (_, unshifted, shifted) = PRINTABLE.iter().find(|(code, _, _)| *code == keycode)?;

    // Caps Lock only applies to letters, and it inverts rather than reinforces
    // Shift: Shift+`a` with Caps Lock on is a lowercase `a`.
    let upper = if unshifted.is_ascii_alphabetic() {
        modifiers.shift != modifiers.caps_lock
    } else {
        modifiers.shift
    };

    Some(KeyInput::Char(if upper { *shifted } else { *unshifted }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers {
        shift: false,
        caps_lock: false,
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        caps_lock: false,
    };
    const CAPS: Modifiers = Modifiers {
        shift: false,
        caps_lock: true,
    };
    const SHIFT_CAPS: Modifiers = Modifiers {
        shift: true,
        caps_lock: true,
    };

    fn character(keycode: u32, modifiers: Modifiers) -> char {
        match decode(keycode, modifiers) {
            Some(KeyInput::Char(character)) => character,
            other => panic!("keycode {keycode} decoded to {other:?}, expected a character"),
        }
    }

    #[test]
    fn letters_follow_shift() {
        assert_eq!(character(38, NONE), 'a');
        assert_eq!(character(38, SHIFT), 'A');
    }

    #[test]
    fn caps_lock_inverts_shift_for_letters_only() {
        assert_eq!(character(38, CAPS), 'A');
        assert_eq!(character(38, SHIFT_CAPS), 'a');

        // Digits ignore Caps Lock entirely.
        assert_eq!(character(10, CAPS), '1');
        assert_eq!(character(10, SHIFT_CAPS), '!');
    }

    #[test]
    fn digits_and_symbols_shift_to_their_upper_glyph() {
        assert_eq!(character(10, NONE), '1');
        assert_eq!(character(10, SHIFT), '!');
        assert_eq!(character(61, NONE), '/');
        assert_eq!(character(61, SHIFT), '?');
        assert_eq!(character(65, SHIFT), ' ');
    }

    #[test]
    fn editing_and_submit_keys_decode() {
        assert_eq!(decode(22, NONE), Some(KeyInput::Backspace));
        assert_eq!(decode(36, NONE), Some(KeyInput::Enter));
        assert_eq!(decode(104, NONE), Some(KeyInput::Enter));
        assert_eq!(decode(9, NONE), Some(KeyInput::Escape));
    }

    #[test]
    fn tab_and_arrows_move_between_users() {
        assert_eq!(decode(23, NONE), Some(KeyInput::NextUser));
        assert_eq!(decode(23, SHIFT), Some(KeyInput::PreviousUser));
        assert_eq!(decode(116, NONE), Some(KeyInput::NextUser));
        assert_eq!(decode(111, NONE), Some(KeyInput::PreviousUser));
    }

    #[test]
    fn modifier_and_unknown_keycodes_decode_to_nothing() {
        // Left Shift, Left Ctrl, Left Alt, Caps Lock: state, not input.
        for keycode in [50, 37, 64, 66] {
            assert_eq!(decode(keycode, NONE), None, "keycode {keycode}");
        }
        assert_eq!(decode(0, NONE), None);
        assert_eq!(decode(250, NONE), None);
    }

    #[test]
    fn modifier_masks_fold_depressed_latched_and_locked() {
        assert_eq!(Modifiers::from_masks(0, 0, 0), NONE);
        assert_eq!(Modifiers::from_masks(1, 0, 0), SHIFT);
        // A latched Shift counts the same as a held one.
        assert_eq!(Modifiers::from_masks(0, 1, 0), SHIFT);
        // Caps Lock arrives locked.
        assert_eq!(Modifiers::from_masks(0, 0, 2), CAPS);
        assert_eq!(Modifiers::from_masks(1, 0, 2), SHIFT_CAPS);
    }

    #[test]
    fn every_printable_keycode_is_listed_once() {
        let mut codes: Vec<u32> = PRINTABLE.iter().map(|(code, _, _)| *code).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "a keycode is mapped twice");
    }
}
