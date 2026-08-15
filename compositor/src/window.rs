//! Phase 1 window management.
//!
//! A minimal window model + hit-testing + focus for the SOL compositor. This
//! replaces the Phase 0 placeholder where "any pointer motion focuses the first
//! toplevel" with a real model:
//!
//! - each toplevel is tracked with a logical [`Rectangle`] on the screen;
//! - pointer hit-testing picks the topmost window under the cursor;
//! - keyboard focus follows the topmost hit window;
//! - z-ordering is maintained so `Activated` / `Unactivated` are delivered to
//!   the right clients.
//!
//! This intentionally stays small: no full scene-graph, no Output abstraction
//! yet. Those belong to the multi-monitor / workspace milestones. New windows
//! are placed with a simple cascade (a floating/snapping baseline).
//!
//! PRD §12 (window management) is the source of the interaction model; this
//! module is the engine underneath it.

use smithay::{
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Rectangle, Size},
    wayland::shell::xdg::ToplevelSurface,
};

/// A toplevel window managed by the compositor.
#[derive(Debug, Clone)]
pub struct Window {
    pub surface: ToplevelSurface,
    pub rect: Rectangle<i32, Logical>,
    pub z_index: usize,
}

impl Window {
    pub fn wl_surface(&self) -> &WlSurface {
        self.surface.wl_surface()
    }
}

/// The window manager: owns the set of open windows plus their layout/focus.
#[derive(Debug, Default)]
pub struct WindowManager {
    windows: Vec<Window>,
    /// Index of the currently keyboard-focused window (topmost / highest z).
    focused: Option<usize>,
}

/// Default gap between cascade-placed windows, in logical pixels.
const CASCADE_STEP: i32 = 32;

/// Default starting size of a new toplevel that hasn't reported one yet.
fn default_window_size() -> Size<i32, Logical> {
    Size::new(800, 600)
}

impl WindowManager {
    /// Register a freshly created toplevel surface and give it an initial
    /// position. The new window becomes the focused, topmost window.
    pub fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.ensure_alive();
        let rect = self.next_placeholder_rect();
        self.windows.push(Window {
            surface,
            rect,
            z_index: self.windows.len(),
        });
        self.focused = Some(self.windows.len() - 1);
    }

    /// Find the topmost window whose rectangle contains `pos`.
    ///
    /// `pos` is in logical compositor space (scale 1.0). We iterate topmost
    /// (highest z) first, so an overlapping window wins over one beneath it.
    pub fn surface_under(&self, pos: smithay::utils::Point<f64, Logical>) -> Option<WlSurface> {
        self.hit_test_order()
            .find(|w| w.rect.to_f64().contains(pos))
            .map(|w| w.wl_surface().clone())
    }

    /// The toplevels in hit-test order (topmost / highest z first).
    fn hit_test_order(&self) -> impl Iterator<Item = &Window> {
        let mut order: Vec<&Window> = self.windows.iter().collect();
        order.sort_by_key(|w| std::cmp::Reverse(w.z_index));
        order.into_iter()
    }

    /// The surface that currently has keyboard focus, if any.
    pub fn focused_surface(&self) -> Option<WlSurface> {
        self.focused
            .and_then(|i| self.windows.get(i))
            .map(|w| w.wl_surface().clone())
    }

    /// Bring `surface` to the top of the z-order and focus it.
    pub fn set_focus(&mut self, surface: &WlSurface) {
        self.ensure_alive();
        let Some(idx) = self.windows.iter().position(|w| w.wl_surface() == surface) else {
            return;
        };
        if self.focused == Some(idx) {
            return;
        }
        let max_z = self.windows.iter().map(|w| w.z_index).max().unwrap_or(0);
        self.windows[idx].z_index = max_z + 1;
        self.focused = Some(idx);
    }

    /// Cycle focus/raise through all windows for Alt+Tab-style switching.
    /// Returns the newly focused surface's principal WlSurface, if any.
    pub fn cycle_focus(&mut self) -> Option<WlSurface> {
        self.ensure_alive();
        if self.windows.is_empty() {
            self.focused = None;
            return None;
        }
        // Bring the window after the currently focused one to the front.
        let next = match self.focused {
            Some(f) => (f + 1) % self.windows.len(),
            None => 0,
        };
        self.promote(next);
        self.focused_surface()
    }

    /// Move a window to the front of the z-order and focus it.
    fn promote(&mut self, idx: usize) {
        let max_z = self.windows.iter().map(|w| w.z_index).max().unwrap_or(0);
        self.windows[idx].z_index = max_z + 1;
        self.focused = Some(idx);
    }

    /// Drop windows whose surfaces died, and repair focus if it pointed to one.
    pub fn ensure_alive(&mut self) {
        self.windows.retain(|w| w.surface.alive());
        self.focused = match self.focused {
            Some(f) if f < self.windows.len() => Some(f),
            _ if self.windows.is_empty() => None,
            _ => Some(self.windows.len() - 1),
        };
    }

    /// A window's rectangle may be resized by the client (a configure ack).
    /// Keep the position, update the size.
    pub fn update_size(&mut self, surface: &WlSurface, size: Size<i32, Logical>) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.wl_surface() == surface) {
            w.rect.size = size;
        }
    }

    /// The list of toplevel surfaces (for rendering), in creation order.
    pub fn toplevel_surfaces(&self) -> impl Iterator<Item = &ToplevelSurface> {
        self.windows.iter().map(|w| &w.surface)
    }

    /// Simple cascade placement: stagger each new window down-right so several
    /// windows stay visually distinct before any real tiling/snapping lands.
    fn next_placeholder_rect(&self) -> Rectangle<i32, Logical> {
        let n = self.windows.len() as i32;
        let x = CASCADE_STEP * (n % 8);
        let y = CASCADE_STEP * (n % 8);
        Rectangle::new((x, y).into(), default_window_size())
    }
}
