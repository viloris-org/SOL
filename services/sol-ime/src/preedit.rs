//! The IME preedit model.
//!
//! The preedit is the "live" composition text shown in the focused text field
//! while the user is still choosing a candidate — e.g. the pinyin syllable
//! `shan` before it is committed as 山. Like the candidate window, its
//! visual parameters come from `sol-design` tokens (PRD §21.1 / §19.1).

use sol_design::color::Color;

/// The preedit display attributes carried by the native SCP text-input model.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Preedit {
    /// Raw UTF-8 composition text.
    pub text: String,
    /// Byte offset of the cursor within `text` (for underlining).
    pub cursor: usize,
    /// Whether the preedit is being actively composed.
    pub active: bool,
}

impl Preedit {
    /// Insert `ch` at the cursor and advance it.
    pub fn insert(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor].chars().next_back().unwrap();
            let start = self.cursor - prev.len_utf8();
            self.text.replace_range(start..self.cursor, "");
            self.cursor = start;
        }
    }

    /// Clear the composition entirely.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// The preedit text color (a `sol-design` token).
    pub fn text_color(&self) -> Color {
        Color::TextPrimary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_advances_cursor() {
        let mut p = Preedit::default();
        p.insert('s');
        p.insert('h');
        assert_eq!(p.text, "sh");
        assert_eq!(p.cursor, 2);
    }

    #[test]
    fn backspace_removes_cjk_char() {
        let mut p = Preedit::default();
        p.text.push('山');
        p.text.push('西');
        p.cursor = 6; // byte length of 山(3)+西(3)
        p.backspace();
        assert_eq!(p.text, "山");
        assert_eq!(p.cursor, 3);
    }
}
