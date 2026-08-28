//! sol-ime — SOL system input method bridge.
//!
//! SOL's first-party IME **frontend** (candidate window / preedit) lives here,
//! rendered with `sol-ui` + `sol-design` so it is unmistakably SOL (PRD §21.1,
//! ADR-0007). The **engine** backend is reused from fcitx5
//! (`fcitx5-chinese-addons` for pinyin first) — SOL does not self-host a pinyin
//! engine.
//!
//! ## Phase 1 M1 scaffold
//!
//! This milestone ships the candidate-window **data model and layout** driven by
//! `sol-design` tokens. The SCP text-input client wiring and the fcitx5 bridge
//! land as the protocol glue is exercised end-to-end.
//!
//! The compositor-side SCP text-input capability is not implemented yet. This
//! crate deliberately exposes no legacy compositor-protocol client while that
//! native capability is being designed.

pub mod candidate;
pub mod engine;
pub mod fcitx5_dbus;
pub mod preedit;

use sol_design::color::Color;

/// A compositor-independent handle to the IME frontend.
///
/// In Phase 1 M1 this is a scaffold; the client connection to the compositor's
/// SCP text-input capability and candidate layer surface will attach here for
/// `sol-ui` to render into.
#[derive(Default)]
pub struct SolIme {
    /// Current composition state, mirroring what the engine produced.
    pub preedit: preedit::Preedit,
    /// Current candidate list (the candidate window renders this).
    pub candidates: candidate::CandidateWindow,
    /// Whether the input method is currently active (has keyboard focus in a
    /// text field).
    pub active: bool,
}

impl SolIme {
    /// Apply an engine result to the frontend and return text to commit to the
    /// focused SCP client, if the engine settled a composition.
    pub fn apply_engine_result(&mut self, result: &engine::EngineResult) -> Option<String> {
        self.preedit = result.preedit.clone();
        self.candidates = result.candidates.clone();
        result.committed_text.clone()
    }

    /// Whether anything needs to be presented (preedit text or candidates).
    pub fn has_content(&self) -> bool {
        !self.preedit.text.is_empty() || !self.candidates.is_empty()
    }

    /// The surface background the candidate window uses (a `sol-design` token).
    pub fn candidate_window_background(&self) -> Color {
        Color::Elevated
    }
}

/// An empty preedit when nothing is being composed.
pub const EMPTY_PREEDIT: &str = "";
