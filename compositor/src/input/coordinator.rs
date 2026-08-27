//! Input coordinator - translates backend events to SCP messages.

use super::{FocusState, InputEvent, InputTarget, KeyEvent, Modifiers, PointerEvent};
use crate::scp::protocol::{CompositorMessage, SessionId, SurfaceId};
use std::collections::HashMap;

/// Coordinates input routing between backend and SCP clients.
pub struct InputCoordinator {
    focus: FocusState,
    pointer_position: (f64, f64),
    #[allow(dead_code)] // TODO: Use for key repeat in Phase 2
    pressed_keys: HashMap<u32, KeyEvent>,
    current_modifiers: Modifiers,
    next_serial: u32,
}

impl InputCoordinator {
    pub fn new() -> Self {
        Self {
            focus: FocusState::default(),
            pointer_position: (0.0, 0.0),
            pressed_keys: HashMap::new(),
            current_modifiers: Modifiers::empty(),
            next_serial: 1,
        }
    }

    fn next_serial(&mut self) -> u32 {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);
        serial
    }

    pub fn focus(&self) -> &FocusState {
        &self.focus
    }

    pub fn set_keyboard_focus(&mut self, target: Option<(SessionId, SurfaceId)>) {
        self.focus.keyboard_focus = target;
    }

    pub fn set_pointer_focus(&mut self, target: Option<InputTarget>) {
        self.focus.pointer_focus = target;
    }

    pub fn pointer_position(&self) -> (f64, f64) {
        self.pointer_position
    }

    /// Process a backend input event and produce SCP messages for clients.
    pub fn handle_event(&mut self, event: InputEvent) -> Vec<(SessionId, CompositorMessage)> {
        match event {
            InputEvent::Keyboard(key_event) => self.handle_keyboard(key_event),
            InputEvent::Pointer(pointer_event) => self.handle_pointer(pointer_event),
            InputEvent::Touch(touch_event) => self.handle_touch(touch_event),
        }
    }

    fn handle_keyboard(&mut self, event: KeyEvent) -> Vec<(SessionId, CompositorMessage)> {
        let Some((session_id, surface_id)) = self.focus.keyboard_focus else {
            return vec![];
        };

        self.current_modifiers = event.modifiers;

        vec![(
            session_id,
            CompositorMessage::InputEvent {
                surface_id,
                event: crate::scp::protocol::InputEvent::KeyboardKey {
                    serial: self.next_serial(),
                    key: event.key,
                    state: match event.state {
                        super::KeyState::Pressed => crate::scp::protocol::KeyState::Pressed,
                        super::KeyState::Released => crate::scp::protocol::KeyState::Released,
                    },
                    time_ms: event.time_msec,
                },
            },
        )]
    }

    fn handle_pointer(&mut self, event: PointerEvent) -> Vec<(SessionId, CompositorMessage)> {
        match event {
            PointerEvent::Motion {
                time_msec,
                absolute_x,
                absolute_y,
            } => {
                self.pointer_position = (absolute_x, absolute_y);

                let Some(target) = &self.focus.pointer_focus else {
                    return vec![];
                };

                vec![(
                    target.session_id,
                    CompositorMessage::InputEvent {
                        surface_id: target.surface_id,
                        event: crate::scp::protocol::InputEvent::PointerMotion {
                            x: target.surface_x,
                            y: target.surface_y,
                            time_ms: time_msec,
                        },
                    },
                )]
            }
            PointerEvent::Button {
                time_msec,
                button,
                state,
            } => {
                let Some(target) = &self.focus.pointer_focus else {
                    return vec![];
                };

                let button_code = match button {
                    super::PointerButton::Left => 0x110,
                    super::PointerButton::Right => 0x111,
                    super::PointerButton::Middle => 0x112,
                    super::PointerButton::Other(code) => code,
                };

                vec![(
                    target.session_id,
                    CompositorMessage::InputEvent {
                        surface_id: target.surface_id,
                        event: crate::scp::protocol::InputEvent::PointerButton {
                            serial: self.next_serial(),
                            button: button_code,
                            state: match state {
                                super::KeyState::Pressed => {
                                    crate::scp::protocol::ButtonState::Pressed
                                }
                                super::KeyState::Released => {
                                    crate::scp::protocol::ButtonState::Released
                                }
                            },
                            time_ms: time_msec,
                        },
                    },
                )]
            }
            PointerEvent::Axis { .. } => {
                // TODO: implement axis events
                vec![]
            }
            PointerEvent::Frame => vec![],
        }
    }

    fn handle_touch(&mut self, _event: super::TouchEvent) -> Vec<(SessionId, CompositorMessage)> {
        // TODO: implement touch events
        vec![]
    }
}

impl Default for InputCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
