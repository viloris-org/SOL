//! Input event types.

/// Top-level input event from backend.
#[derive(Debug, Clone)]
pub enum InputEvent {
    Keyboard(KeyEvent),
    Pointer(PointerEvent),
    Touch(TouchEvent),
}

/// Keyboard event.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub time_msec: u32,
    pub key: u32,
    pub state: KeyState,
    pub modifiers: Modifiers,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
}

/// Pointer (mouse) event.
#[derive(Debug, Clone)]
pub enum PointerEvent {
    Motion {
        time_msec: u32,
        absolute_x: f64,
        absolute_y: f64,
    },
    Button {
        time_msec: u32,
        button: PointerButton,
        state: KeyState,
    },
    Axis {
        time_msec: u32,
        axis: PointerAxis,
        value: f64,
    },
    Frame,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Other(u32),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PointerAxis {
    Vertical,
    Horizontal,
}

/// Touch event.
#[derive(Debug, Clone)]
pub struct TouchEvent {
    pub time_msec: u32,
    pub touch_id: i32,
    pub phase: TouchPhase,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Up,
    Motion,
    Cancel,
}

/// Keyboard modifier state.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl Modifiers {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.logo
    }
}
