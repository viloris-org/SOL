//! Input abstraction layer for SOL compositor.
//!
//! This module decouples input handling from the Smithay seat/keyboard/pointer
//! protocol handlers. It translates backend events (winit, libinput) into SCP
//! input messages without depending on Wayland protocol types.

mod coordinator;
mod keyboard;
mod pointer;
mod touch;
mod types;

pub use coordinator::InputCoordinator;
pub use types::{
    InputEvent, KeyEvent, KeyState, Modifiers, PointerAxis, PointerButton, PointerEvent,
    TouchEvent, TouchPhase,
};

use crate::scp::protocol::{SessionId, SurfaceId};

/// Input target resolution result.
#[derive(Debug, Clone)]
pub struct InputTarget {
    pub session_id: SessionId,
    pub surface_id: SurfaceId,
    /// Surface-local coordinates.
    pub surface_x: f64,
    pub surface_y: f64,
}

/// Input focus state.
#[derive(Debug, Clone)]
pub struct FocusState {
    pub keyboard_focus: Option<(SessionId, SurfaceId)>,
    pub pointer_focus: Option<InputTarget>,
}

impl Default for FocusState {
    fn default() -> Self {
        Self {
            keyboard_focus: None,
            pointer_focus: None,
        }
    }
}
