//! IME engine and fcitx5 transport boundary.
//!
//! The SOL frontend owns presentation.  Fcitx owns conversion and candidate
//! ranking.  [`Fcitx5Transport`] deliberately translates fcitx's D-Bus signal
//! vocabulary into small frontend events, which keeps D-Bus types out of the
//! candidate-window and Wayland protocol code.

use crate::candidate::CandidateWindow;
use crate::preedit::Preedit;
use std::error::Error;
use std::fmt;

/// The seam between the SOL IME frontend and an engine backend.
pub trait EngineBridge {
    /// Send composition text to the engine and return the resulting frontend
    /// state.  A composed candidate is returned in `committed_text` only once
    /// the engine commits it.
    fn commit(&mut self, text: &str) -> EngineResponse;

    /// Ask the engine to commit its selected candidate.
    fn confirm(&mut self, candidate_index: Option<usize>) -> EngineResponse;

    /// Cancel the active composition.
    fn reset(&mut self) -> EngineResponse;
}

/// Result of an engine operation.
pub type EngineResponse = Result<EngineResult, EngineError>;

/// An engine or transport failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The fcitx transport could not submit or receive an operation.
    Transport(String),
    /// The transport supplied a response that cannot be represented safely.
    InvalidEvent(&'static str),
}

impl EngineError {
    /// Construct an error reported by an engine transport.
    #[must_use]
    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport(message.into())
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(message) => write!(formatter, "IME transport error: {message}"),
            Self::InvalidEvent(message) => {
                write!(formatter, "invalid IME transport event: {message}")
            }
        }
    }
}

impl Error for EngineError {}

/// The engine's frontend-facing response to a composition update.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineResult {
    /// The live preedit (uncommitted) text, if any.
    pub preedit: Preedit,
    /// The candidate window, empty when there are no candidates.
    pub candidates: CandidateWindow,
    /// Text committed by the engine into the focused application.
    pub committed_text: Option<String>,
}

/// A typed request from the frontend to the fcitx transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fcitx5Request {
    /// Feed composition text as key presses to the active fcitx input context.
    TypeText(String),
    /// Select a candidate by zero-based index.
    SelectCandidate(usize),
    /// Cancel the active composition.
    Reset,
}

/// A normalized event emitted by an fcitx input context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fcitx5Event {
    /// Replace the frontend preedit.
    Preedit(Preedit),
    /// Replace the frontend candidate list and selection.
    Candidates {
        /// Candidate display strings in fcitx order.
        values: Vec<String>,
        /// The selected candidate, if fcitx selected one.
        selected: Option<usize>,
    },
    /// Commit final text to the focused client and clear composition UI.
    Commit(String),
    /// Clear preedit and candidates without committing text.
    Clear,
}

/// Concrete transport used by [`Fcitx5Bridge`].
///
/// Implementations own connection setup, request serialization, and event
/// delivery.  The contract is intentionally synchronous at this boundary: one
/// request returns the ordered events it caused, making it directly testable
/// with a fake while a D-Bus implementation may wait briefly for signals.
pub trait Fcitx5Transport {
    /// Submit `request` and return every frontend event observed for it.
    fn request(&mut self, request: Fcitx5Request) -> Result<Vec<Fcitx5Event>, EngineError>;
}

/// A bridge from a typed fcitx transport to the SOL frontend model.
#[derive(Debug)]
pub struct Fcitx5Bridge<T> {
    transport: T,
    state: EngineResult,
}

impl<T: Fcitx5Transport> Fcitx5Bridge<T> {
    /// Construct a bridge over an established fcitx transport.
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            state: EngineResult::default(),
        }
    }

    /// Return the transport for service setup or diagnostics.
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    fn apply(&mut self, request: Fcitx5Request) -> EngineResponse {
        let events = self.transport.request(request)?;
        self.state.committed_text = None;

        for event in events {
            match event {
                Fcitx5Event::Preedit(preedit) => self.state.preedit = preedit,
                Fcitx5Event::Candidates { values, selected } => {
                    let mut candidates = CandidateWindow::new(values);
                    if let Some(selected) = selected {
                        candidates.selected =
                            selected.min(candidates.candidates.len().saturating_sub(1));
                    }
                    self.state.candidates = candidates;
                }
                Fcitx5Event::Commit(text) => {
                    self.state.committed_text = Some(text);
                    self.clear_composition();
                }
                Fcitx5Event::Clear => self.clear_composition(),
            }
        }

        Ok(self.state.clone())
    }

    fn clear_composition(&mut self) {
        self.state.preedit.clear();
        self.state.preedit.active = false;
        self.state.candidates = CandidateWindow::default();
    }
}

impl<T: Fcitx5Transport> EngineBridge for Fcitx5Bridge<T> {
    fn commit(&mut self, text: &str) -> EngineResponse {
        self.apply(Fcitx5Request::TypeText(text.to_owned()))
    }

    fn confirm(&mut self, candidate_index: Option<usize>) -> EngineResponse {
        let candidate_index = candidate_index.unwrap_or(self.state.candidates.selected);
        self.apply(Fcitx5Request::SelectCandidate(candidate_index))
    }

    fn reset(&mut self) -> EngineResponse {
        self.apply(Fcitx5Request::Reset)
    }
}

/// A no-op engine used when no backend is available (e.g. headless CI).
///
/// It deliberately forwards raw input as committed text rather than silently
/// dropping it, so disabling fcitx cannot make a text field unusable.
#[derive(Default)]
pub struct NoopEngine;

impl EngineBridge for NoopEngine {
    fn commit(&mut self, text: &str) -> EngineResponse {
        Ok(EngineResult {
            committed_text: Some(text.to_owned()),
            ..EngineResult::default()
        })
    }

    fn confirm(&mut self, _candidate_index: Option<usize>) -> EngineResponse {
        Ok(EngineResult::default())
    }

    fn reset(&mut self) -> EngineResponse {
        Ok(EngineResult::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{EngineBridge, Fcitx5Bridge, Fcitx5Event, Fcitx5Request, Fcitx5Transport, Preedit};
    use crate::SolIme;

    #[derive(Default, Debug)]
    struct PinyinHarness {
        requests: Vec<Fcitx5Request>,
    }

    impl Fcitx5Transport for PinyinHarness {
        fn request(
            &mut self,
            request: Fcitx5Request,
        ) -> Result<Vec<Fcitx5Event>, super::EngineError> {
            self.requests.push(request.clone());
            match request {
                Fcitx5Request::TypeText(text) if text == "shan" => Ok(vec![
                    Fcitx5Event::Preedit(Preedit {
                        text,
                        cursor: 4,
                        active: true,
                    }),
                    Fcitx5Event::Candidates {
                        values: vec!["山".to_owned(), "闪".to_owned(), "善".to_owned()],
                        selected: Some(0),
                    },
                ]),
                Fcitx5Request::SelectCandidate(0) => Ok(vec![Fcitx5Event::Commit("山".to_owned())]),
                Fcitx5Request::Reset => Ok(vec![Fcitx5Event::Clear]),
                _ => Ok(Vec::new()),
            }
        }
    }

    #[test]
    fn fake_fcitx_pinyin_round_trip_reaches_preedit_candidates_and_commit() {
        let mut ime = SolIme::default();
        let mut engine = Fcitx5Bridge::new(PinyinHarness::default());

        let composing = engine
            .commit("shan")
            .expect("fake composition should succeed");
        assert_eq!(ime.apply_engine_result(&composing), None);
        assert_eq!(ime.preedit.text, "shan");
        assert_eq!(ime.preedit.cursor, 4);
        assert_eq!(ime.candidates.candidates, ["山", "闪", "善"]);

        let committed = engine
            .confirm(Some(0))
            .expect("fake selection should succeed");
        assert_eq!(ime.apply_engine_result(&committed), Some("山".to_owned()));
        assert_eq!(ime.preedit.text, "");
        assert!(ime.candidates.is_empty());
        assert_eq!(engine.transport().requests.len(), 2);
    }
}
