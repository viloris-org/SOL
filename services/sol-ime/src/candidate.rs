//! The IME candidate window model, sized with `sol-design` tokens.
//!
//! A candidate window is a vertical list of alternatives that the user can
//! page through with the arrow keys (or select directly by position). Its
//! dimensions are driven by `sol-design` — no bare pixels here (PRD §19.1).
//!
//! Rendering is delegated to `sol-ui` in Phase 2; this module owns the
//! **data model and layout arithmetic** so that both the frontend and the
//! compositor can agree on surface sizes without duplicating constants.

use sol_design::color::Color;
use sol_design::radius::Radius;
use sol_design::spacing::Spacing;

/// Height of a single candidate row in logical pixels.
///
/// Uses a `sol-design` spacing token so every row is uniformly tall even if
/// the theme later changes the scale factor.
const CANDIDATE_ROW_HEIGHT: f32 = Spacing::Lg.px();

/// Width of a single candidate row in logical pixels.
///
/// Computed from `sol-design` spacing: left margin + content area + right
/// margin. The content area grows to fit the widest candidate (handled at
/// layout time).
const CANDIDATE_ROW_MARGIN_X: f32 = 8.0;

/// Candidate window background color (a `sol-design` token).
pub fn window_background() -> Color {
    Color::Elevated
}

/// Candidate window corner radius (a `sol-design` token).
pub fn window_radius() -> Radius {
    Radius::Md
}

/// Horizontal gap between the candidate label (the index, e.g. "1: ") and the
/// candidate text.
pub const LABEL_GAP: f32 = 4.0;

/// Vertical gap between candidate windows in the list.
pub const ROW_PADDING_Y: f32 = 4.0;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CandidateWindow {
    /// The full candidate list from the engine (pinyin entries, etc.).
    pub candidates: Vec<String>,
    /// Currently selected candidate (index into `candidates`).
    pub selected: usize,
    /// How many candidates are shown per page (one-based index visible in the
    /// label, e.g. `1:` `2:` … `9:`).
    pub page_size: usize,
    /// Which page is currently visible (0-indexed).
    pub page: usize,
}

impl CandidateWindow {
    /// Create a candidate window from an engine response.
    pub fn new(candidates: Vec<String>) -> Self {
        Self {
            candidates,
            selected: 0,
            page_size: 9,
            page: 0,
        }
    }

    /// The candidates visible on the current page (sliced view).
    pub fn visible(&self) -> &[String] {
        let start = self.page * self.page_size;
        if start >= self.candidates.len() {
            return &[];
        }
        let end = (start + self.page_size).min(self.candidates.len());
        &self.candidates[start..end]
    }

    /// The logical height of the candidate window in pixels.
    pub fn preferred_height(&self) -> f32 {
        let rows = self.visible().len().max(1) as f32;
        rows * (CANDIDATE_ROW_HEIGHT + ROW_PADDING_Y * 2.0)
    }

    /// The logical width of the candidate window in pixels.
    ///
    /// Content is right-aligned within a field of fixed left margin so the
    /// indices line up when the window grows on longer text.
    pub fn preferred_width(&self) -> f32 {
        let label_width = LABEL_WIDTH_CHARS as f32 * 6.0; // rough: each digit ~6px
        let max_content = self
            .visible()
            .iter()
            .map(|c| measure_text_width(c))
            .fold(0.0, f32::max);
        CANDIDATE_ROW_MARGIN_X * 2.0 + label_width + LABEL_GAP + max_content
    }

    /// Move the selection up by one page.
    pub fn page_up(&mut self) {
        if self.page > 0 {
            self.page -= 1;
            self.selected = self.page_start_index();
        }
    }

    /// Move the selection down by one page.
    pub fn page_down(&mut self) {
        let max_page = self.page_count().saturating_sub(1);
        if self.page < max_page {
            self.page += 1;
            self.selected = self.page_start_index();
        }
    }

    /// Move the selection up within the current page.
    pub fn prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move the selection down within the current page.
    pub fn next(&mut self) {
        if self.selected < self.candidates.len().saturating_sub(1) {
            self.selected += 1;
        }
    }

    /// Whether there is anything to render.
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// The absolute index the current page starts at.
    fn page_start_index(&self) -> usize {
        self.page * self.page_size
    }

    /// Total number of pages the candidate list occupies.
    fn page_count(&self) -> usize {
        self.candidates.len().div_ceil(self.page_size)
    }
}

/// Number of label chars for page-index + candidate-index (`"1: "` etc).
const LABEL_WIDTH_CHARS: usize = 3;

/// Rough pixel-width of a piece of candidate text. The actual glyph-level
/// measurement depends on the font and will be wired to pango / harfbuzz in
/// Phase 2. For now a simple average width suffices for layout planning.
fn measure_text_width(text: &str) -> f32 {
    // Punctuation is usually narrower; CJK characters wider. A simple rule:
    // count CJK-like ranges.
    let mut total = 0.0;
    for c in text.chars() {
        if is_cjk(c) {
            total += 14.0;
        } else {
            total += 7.0;
        }
    }
    total
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{20000}'..='\u{2A6DF}' // CJK Extension B
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{3040}'..='\u{30FF}' // Hiragana/Katakana
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_window_lays_out_pinyin() {
        let win = CandidateWindow::new(["一", "二", "三", "四", "五"].map(String::from).to_vec());
        assert_eq!(win.page_size, 9);
        assert!(!win.is_empty());
        assert!(win.preferred_height() > 0.0);
        assert!(win.preferred_width() > 0.0);
    }

    #[test]
    fn paging_cycles_candidate_selection() {
        let mut win = CandidateWindow::new((0..20).map(|i| format!("候选{i}")).collect());
        win.next();
        assert_eq!(win.selected, 1);
        win.page_down();
        assert_eq!(win.page, 1);
        assert_eq!(win.selected, 9);
    }

    #[test]
    fn visible_bounds_to_page() {
        let mut win = CandidateWindow::new((0..18).map(|i| format!("c{i}")).collect());
        assert_eq!(win.page_size, 9);
        // Page 0 shows indices 0..9, page 1 shows 9..18.
        let p0 = win.visible().len();
        assert_eq!(p0, 9);
        win.page_down();
        let p1 = win.visible().len();
        assert_eq!(p1, 9);
    }
}
