//! SCP input event handling and dispatch.

use crate::scp::protocol::{
    AxisSource, ButtonState, InputEvent, KeyState, Orientation, SessionId, SurfaceId,
};
use std::collections::HashSet;
use std::time::Instant;

/// Input state tracker for the compositor.
#[derive(Debug)]
pub struct InputState {
    keyboard_focus: Option<(SessionId, SurfaceId)>,
    pointer_focus: Option<(SessionId, SurfaceId)>,
    touch_points: Vec<TouchPoint>,
    serial_counter: u32,
    last_input_time: Instant,

    // Keyboard state
    pressed_keys: HashSet<u32>,
    modifiers: ModifierState,

    // Pointer state
    pointer_position: (f64, f64),
    pressed_buttons: HashSet<u32>,
}

#[derive(Debug, Clone)]
struct TouchPoint {
    id: i32,
    #[allow(dead_code)]
    surface: (SessionId, SurfaceId),
    x: f64,
    y: f64,
}

/// Keyboard modifier state (XKB compatible).
#[derive(Debug, Default, Clone, Copy)]
pub struct ModifierState {
    pub mods_depressed: u32, // Currently pressed modifiers
    pub mods_latched: u32,   // Latched modifiers (sticky)
    pub mods_locked: u32,    // Locked modifiers (caps lock, etc.)
    pub group: u32,          // Keyboard layout group
}

impl ModifierState {
    pub fn is_empty(&self) -> bool {
        self.mods_depressed == 0
            && self.mods_latched == 0
            && self.mods_locked == 0
            && self.group == 0
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keyboard_focus: None,
            pointer_focus: None,
            touch_points: Vec::new(),
            serial_counter: 1,
            last_input_time: Instant::now(),
            pressed_keys: HashSet::new(),
            modifiers: ModifierState::default(),
            pointer_position: (0.0, 0.0),
            pressed_buttons: HashSet::new(),
        }
    }

    pub fn next_serial(&mut self) -> u32 {
        let serial = self.serial_counter;
        self.serial_counter = self.serial_counter.wrapping_add(1);
        self.last_input_time = Instant::now();
        serial
    }

    pub fn last_input_time(&self) -> Instant {
        self.last_input_time
    }

    // ===== Keyboard =====

    pub fn set_keyboard_focus(&mut self, target: Option<(SessionId, SurfaceId)>) {
        self.keyboard_focus = target;
    }

    pub fn keyboard_focus(&self) -> Option<(SessionId, SurfaceId)> {
        self.keyboard_focus
    }

    pub fn pressed_keys(&self) -> Vec<u32> {
        self.pressed_keys.iter().copied().collect()
    }

    pub fn modifiers(&self) -> ModifierState {
        self.modifiers
    }

    pub fn set_modifiers(&mut self, modifiers: ModifierState) {
        self.modifiers = modifiers;
    }

    pub fn dispatch_keyboard_enter(
        &mut self,
        surface: (SessionId, SurfaceId),
        pressed_keys: Vec<u32>,
    ) -> InputEvent {
        self.keyboard_focus = Some(surface);
        self.pressed_keys = pressed_keys.iter().copied().collect();
        let serial = self.next_serial();
        InputEvent::KeyboardEnter {
            serial,
            keys: pressed_keys,
        }
    }

    pub fn dispatch_keyboard_leave(&mut self) -> Option<InputEvent> {
        if self.keyboard_focus.is_some() {
            self.keyboard_focus = None;
            self.pressed_keys.clear();
            let serial = self.next_serial();
            Some(InputEvent::KeyboardLeave { serial })
        } else {
            None
        }
    }

    pub fn dispatch_keyboard_key(
        &mut self,
        key: u32,
        state: KeyState,
        time_ms: u32,
    ) -> Option<InputEvent> {
        if self.keyboard_focus.is_some() {
            match state {
                KeyState::Pressed => {
                    self.pressed_keys.insert(key);
                }
                KeyState::Released => {
                    self.pressed_keys.remove(&key);
                }
            }
            let serial = self.next_serial();
            Some(InputEvent::KeyboardKey {
                serial,
                key,
                state,
                time_ms,
            })
        } else {
            None
        }
    }

    pub fn dispatch_modifiers(&self, modifiers: ModifierState, serial: u32) -> InputEvent {
        InputEvent::Modifiers {
            serial,
            mods_depressed: modifiers.mods_depressed,
            mods_latched: modifiers.mods_latched,
            mods_locked: modifiers.mods_locked,
            group: modifiers.group,
        }
    }

    // ===== Pointer =====

    pub fn set_pointer_focus(&mut self, target: Option<(SessionId, SurfaceId)>) {
        self.pointer_focus = target;
    }

    pub fn pointer_focus(&self) -> Option<(SessionId, SurfaceId)> {
        self.pointer_focus
    }

    pub fn pointer_position(&self) -> (f64, f64) {
        self.pointer_position
    }

    pub fn pressed_buttons(&self) -> Vec<u32> {
        self.pressed_buttons.iter().copied().collect()
    }

    pub fn dispatch_pointer_enter(
        &mut self,
        surface: (SessionId, SurfaceId),
        x: f64,
        y: f64,
    ) -> InputEvent {
        self.pointer_focus = Some(surface);
        self.pointer_position = (x, y);
        let serial = self.next_serial();
        InputEvent::PointerEnter { serial, x, y }
    }

    pub fn dispatch_pointer_leave(&mut self) -> Option<InputEvent> {
        if self.pointer_focus.is_some() {
            self.pointer_focus = None;
            let serial = self.next_serial();
            Some(InputEvent::PointerLeave { serial })
        } else {
            None
        }
    }

    pub fn dispatch_pointer_motion(&mut self, x: f64, y: f64, time_ms: u32) -> Option<InputEvent> {
        if self.pointer_focus.is_some() {
            self.pointer_position = (x, y);
            Some(InputEvent::PointerMotion { x, y, time_ms })
        } else {
            None
        }
    }

    pub fn dispatch_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
        time_ms: u32,
    ) -> Option<InputEvent> {
        if self.pointer_focus.is_some() {
            match state {
                ButtonState::Pressed => {
                    self.pressed_buttons.insert(button);
                }
                ButtonState::Released => {
                    self.pressed_buttons.remove(&button);
                }
            }
            let serial = self.next_serial();
            Some(InputEvent::PointerButton {
                serial,
                button,
                state,
                time_ms,
            })
        } else {
            None
        }
    }

    pub fn dispatch_pointer_axis(
        &mut self,
        axis_source: AxisSource,
        orientation: Orientation,
        value: f64,
        discrete: i32,
        time_ms: u32,
    ) -> Option<InputEvent> {
        if self.pointer_focus.is_some() {
            Some(InputEvent::PointerAxis {
                time_ms,
                axis_source,
                orientation,
                value,
                discrete,
            })
        } else {
            None
        }
    }

    pub fn dispatch_pointer_frame(&self) -> Option<InputEvent> {
        if self.pointer_focus.is_some() {
            Some(InputEvent::PointerFrame)
        } else {
            None
        }
    }

    // ===== Touch =====

    pub fn dispatch_touch_down(
        &mut self,
        surface: (SessionId, SurfaceId),
        touch_id: i32,
        x: f64,
        y: f64,
        time_ms: u32,
    ) -> InputEvent {
        self.touch_points.push(TouchPoint {
            id: touch_id,
            surface,
            x,
            y,
        });
        let serial = self.next_serial();
        InputEvent::TouchDown {
            serial,
            touch_id,
            x,
            y,
            time_ms,
        }
    }

    pub fn dispatch_touch_up(&mut self, touch_id: i32, time_ms: u32) -> Option<InputEvent> {
        self.touch_points.retain(|tp| tp.id != touch_id);
        let serial = self.next_serial();
        Some(InputEvent::TouchUp {
            serial,
            touch_id,
            time_ms,
        })
    }

    pub fn dispatch_touch_motion(
        &mut self,
        touch_id: i32,
        x: f64,
        y: f64,
        time_ms: u32,
    ) -> Option<InputEvent> {
        if let Some(tp) = self.touch_points.iter_mut().find(|tp| tp.id == touch_id) {
            tp.x = x;
            tp.y = y;
            Some(InputEvent::TouchMotion {
                touch_id,
                x,
                y,
                time_ms,
            })
        } else {
            None
        }
    }

    pub fn dispatch_touch_cancel(&mut self) -> InputEvent {
        self.touch_points.clear();
        InputEvent::TouchCancel
    }

    pub fn dispatch_touch_frame(&self) -> InputEvent {
        InputEvent::TouchFrame
    }

    pub fn dispatch_touch_shape(&self, touch_id: i32, major: f64, minor: f64) -> InputEvent {
        InputEvent::TouchShape {
            touch_id,
            major,
            minor,
        }
    }

    pub fn dispatch_touch_orientation(&self, touch_id: i32, orientation: f64) -> InputEvent {
        InputEvent::TouchOrientation {
            touch_id,
            orientation,
        }
    }
}
