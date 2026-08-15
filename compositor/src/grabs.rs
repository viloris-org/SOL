//! Interactive move/resize grabs for Phase 1 M1.
//!
//! These implement the compositor side of `xdg_toplevel.move` and
//! `xdg_toplevel.resize`. A client requests an interactive move or resize with
//! a button grab serial; we install a [`PointerGrab`] that tracks pointer
//! motion and updates the window's rectangle (position for move, size for
//! resize) until the initiating button is released.
//!
//! Modelled on the Smithay examples' shell grabs, but kept minimal for the SOL
//! window model (no SSD, no scene graph — just a [`WindowManager`] rect update).

use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerGrab,
        PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle, Size},
};
use wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;

use crate::state::SolState;

/// The surface currently being moved, and the pointer offset into it.
pub struct MoveSurfaceGrab {
    start_data: GrabStartData<SolState>,
    surface: WlSurface,
    /// Offset from the pointer to the window's top-left at grab start.
    offset: Point<i32, Logical>,
}

impl MoveSurfaceGrab {
    pub fn new(
        start_data: GrabStartData<SolState>,
        surface: WlSurface,
        offset: Point<i32, Logical>,
    ) -> Self {
        MoveSurfaceGrab {
            start_data,
            surface,
            offset,
        }
    }
}

impl PointerGrab<SolState> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // Keep the moved window at the top of the z-order for the duration and
        // reposition it so the pointer stays at the same spot in the window.
        let pointer = event.location;
        let new_top_left = Point::from((
            pointer.x as i32 - self.offset.x,
            pointer.y as i32 - self.offset.y,
        ));
        state
            .window_manager
            .move_window(&self.surface, new_top_left);

        // During an interactive move we don't want the pointer to grab focus on
        // the moved window's interactive widgets; forward a neutral None focus.
        handle.motion(state, None, event);
    }

    fn relative_motion(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(state, focus, event);
    }

    fn button(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &ButtonEvent,
    ) {
        handle.button(state, event);
        // Release the grab when the active button lift ends and no button is
        // held any more.
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, state, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        details: AxisFrame,
    ) {
        handle.axis(state, details);
    }

    fn frame(&mut self, state: &mut SolState, handle: &mut PointerInnerHandle<'_, SolState>) {
        handle.frame(state);
    }

    fn gesture_swipe_begin(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(state, event);
    }

    fn gesture_swipe_update(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(state, event);
    }

    fn gesture_swipe_end(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(state, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(state, event);
    }

    fn gesture_pinch_update(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(state, event);
    }

    fn gesture_pinch_end(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(state, event);
    }

    fn gesture_hold_begin(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(state, event);
    }

    fn gesture_hold_end(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(state, event);
    }

    fn start_data(&self) -> &GrabStartData<SolState> {
        &self.start_data
    }

    fn unset(&mut self, _state: &mut SolState) {}
}

/// The resize grab tracks pointer motion against a fixed window rect and
/// updates the size along the requested edge.
pub struct ResizeSurfaceGrab {
    start_data: GrabStartData<SolState>,
    surface: WlSurface,
    edges: ResizeEdge,
    initial_rect: Rectangle<i32, Logical>,
    last_size: Point<i32, Logical>,
}

impl ResizeSurfaceGrab {
    pub fn new(
        start_data: GrabStartData<SolState>,
        surface: WlSurface,
        edges: ResizeEdge,
        initial_rect: Rectangle<i32, Logical>,
    ) -> Self {
        let last_size = initial_rect.size.to_point();
        ResizeSurfaceGrab {
            start_data,
            surface,
            edges,
            initial_rect,
            last_size,
        }
    }
}

impl PointerGrab<SolState> for ResizeSurfaceGrab {
    fn motion(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        let pointer = event.location;
        let mut w = self.last_size.x;
        let mut h = self.last_size.y;

        match self.edges {
            ResizeEdge::Top
            | ResizeEdge::TopLeft
            | ResizeEdge::TopRight
            | ResizeEdge::Bottom
            | ResizeEdge::BottomLeft
            | ResizeEdge::BottomRight => {
                // Vertical edge: grow/shrink height.
                let dy: f64 = match self.edges {
                    ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight => {
                        self.initial_rect.loc.y as f64 - pointer.y
                    }
                    _ => pointer.y - self.initial_rect.loc.y as f64,
                };
                h += dy as i32;
                // Horizontal edge: grow/shrink width (corner resizes do both).
                let dx: f64 = match self.edges {
                    ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft => {
                        self.initial_rect.loc.x as f64 - pointer.x
                    }
                    ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight => {
                        pointer.x - self.initial_rect.loc.x as f64
                    }
                    _ => 0.0,
                };
                w += dx as i32;
            }
            _ => {}
        }

        let w = w.max(1);
        let h = h.max(1);
        self.last_size = Point::from((w, h));
        state
            .window_manager
            .resize_window(&self.surface, Size::from((w, h)));

        handle.motion(state, None, event);
    }

    fn relative_motion(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(state, focus, event);
    }

    fn button(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &ButtonEvent,
    ) {
        handle.button(state, event);
        // Release the grab when the active button lifts and nothing remains pressed.
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, state, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        details: AxisFrame,
    ) {
        handle.axis(state, details);
    }

    fn frame(&mut self, state: &mut SolState, handle: &mut PointerInnerHandle<'_, SolState>) {
        handle.frame(state);
    }

    fn gesture_swipe_begin(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(state, event);
    }

    fn gesture_swipe_update(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(state, event);
    }

    fn gesture_swipe_end(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(state, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(state, event);
    }

    fn gesture_pinch_update(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(state, event);
    }

    fn gesture_pinch_end(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(state, event);
    }

    fn gesture_hold_begin(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(state, event);
    }

    fn gesture_hold_end(
        &mut self,
        state: &mut SolState,
        handle: &mut PointerInnerHandle<'_, SolState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(state, event);
    }

    fn start_data(&self) -> &GrabStartData<SolState> {
        &self.start_data
    }

    fn unset(&mut self, _state: &mut SolState) {}
}
