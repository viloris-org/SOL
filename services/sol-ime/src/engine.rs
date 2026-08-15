//! IME engine bridge.
//!
//! Per ADR-0007, SOL ships a first-party IME **frontend** (`sol-ime`) and
//! reuses **fcitx5** as the engine backend — it does not self-host a pinyin /
//! segmentation engine. The frontend and the engine must talk over a stable
//! bridge so we can swap or version the engine without touching the candidate
//! window or preedit UI.
//!
//! `EngineBridge` is that seam. It is deliberately thin:
//!
//! - the frontend **sends** what the user typed / committed into the engine,
//! - the engine **returns** a [`crate::candidate::CandidateWindow`] and
//!   preedit text.
//!
//! ## fcitx5 backend
//!
//! The concrete `Fcitx5Bridge` connects to a running fcitx5 over its DBus
//! service (`org.fcitx.Fcitx5.InputContext`) or its in-process library.
//! Fcitx5 is Arch-native (`fcitx5-ime`, `fcitx5-chinese-addons` are in the
//! package list per ADR-0007). The full transport is developed on an Arch dev
//! machine — the trait and the default `NoopEngine` compile everywhere so the
//! rest of the workspace is CI-green.

use crate::candidate::CandidateWindow;
use crate::preedit::Preedit;

/// The seam between the SOL IME frontend and an engine backend.
pub trait EngineBridge {
    /// Send the raw text (what the user typed after composition, e.g. the
    /// pinyin syllables) to the engine and get back a fresh candidate list +
    /// preedit.
    fn commit(&mut self, text: &str) -> EngineResult;

    /// The engine is asked to settle the current composition (commit the
    /// selected candidate into the application).
    fn confirm(&mut self, candidate_index: Option<usize>);

    /// Reset the engine (e.g. the user pressed Escape to cancel composition).
    fn reset(&mut self);
}

/// The engine's response to a composition update.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineResult {
    /// The live preedit (uncommitted) text, if any.
    pub preedit: Preedit,
    /// The candidate window (usually empty if there are no candidates).
    pub candidates: CandidateWindow,
}

/// A no-op engine used when no backend is available (e.g. headless CI /
/// tests). It always returns an empty composition, so the frontend just shows
/// nothing and forwarding raw text straight through.
#[derive(Default)]
pub struct NoopEngine;

impl EngineBridge for NoopEngine {
    fn commit(&mut self, _text: &str) -> EngineResult {
        EngineResult::default()
    }

    fn confirm(&mut self, _candidate_index: Option<usize>) {}

    fn reset(&mut self) {}
}

/// Scaffold for the fcitx5 backend.
///
/// The real implementation talks to fcitx5 over its DBus input-context
/// interface (`org.fcitx.Fcitx5.InputContext` / the `CreateInputContext`
/// method), feeding it key events and pinyin syllables and reading back the
/// candidate list from the serialized candidate / cursor messages.
///
/// That wiring is developed against a local fcitx5 on the Arch dev machine;
/// this type holds the bridge state so the concrete transport can be added
/// without reshaping `EngineBridge`.
#[derive(Default)]
pub struct Fcitx5Bridge {
    /// DBus input-context object path, populated on connect.
    _input_context: Option<String>,
    /// Serial counter for request round-trips.
    _serial: u64,
}

impl EngineBridge for Fcitx5Bridge {
    fn commit(&mut self, _text: &str) -> EngineResult {
        // Not yet connected to a real fcitx5; return empty for now.
        // The transport (DBus InputContext::ProcessKey / CommitString) is
        // implemented on the Arch dev host, where fcitx5 is present.
        EngineResult::default()
    }

    fn confirm(&mut self, _candidate_index: Option<usize>) {}

    fn reset(&mut self) {}
}
