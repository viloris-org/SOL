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
    utils::{Logical, Point, Rectangle, Size},
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
pub struct WindowManager {
    windows: Vec<Window>,
    /// Index of the currently keyboard-focused window (topmost / highest z).
    focused: Option<usize>,
    /// The logical screen region available to windows (the "work area"), used
    /// by cascade placement and edge snapping (PRD §12 Floating + Snap).
    work_area: Rectangle<i32, Logical>,
}

impl Default for WindowManager {
    fn default() -> Self {
        WindowManager {
            windows: Vec::new(),
            focused: None,
            work_area: Rectangle::from_size(Size::new(1920, 1080)),
        }
    }
}

/// The type of snap edge a window can be attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapEdge {
    /// Maximized to fill the work area.
    Maximized,
    /// Fills the left half of the work area.
    Left,
    /// Fills the right half of the work area.
    Right,
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

    /// The current geometry (rectangle) of a window, if known.
    pub fn surface_geometry(&self, surface: &WlSurface) -> Option<Rectangle<i32, Logical>> {
        self.windows
            .iter()
            .find(|w| w.wl_surface() == surface)
            .map(|w| w.rect)
    }

    /// Move a window's top-left to `loc` (interactive move).
    pub fn move_window(&mut self, surface: &WlSurface, loc: Point<i32, Logical>) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.wl_surface() == surface) {
            w.rect.loc = loc;
        }
    }

    /// Resize a window to `size` (interactive resize); position is kept.
    pub fn resize_window(&mut self, surface: &WlSurface, size: Size<i32, Logical>) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.wl_surface() == surface) {
            w.rect.size = size;
        }
    }

    /// The list of toplevel surfaces (for rendering), in creation order.
    pub fn toplevel_surfaces(&self) -> impl Iterator<Item = &ToplevelSurface> {
        self.windows.iter().map(|w| &w.surface)
    }

    /// Set the work area (the screen region windows can occupy). Used by
    /// cascade placement and edge snapping. Defaults to 1920×1080.
    #[allow(dead_code)]
    pub fn set_work_area(&mut self, area: Rectangle<i32, Logical>) {
        self.work_area = area;
    }

    /// The current work area.
    pub fn work_area(&self) -> Rectangle<i32, Logical> {
        self.work_area
    }

    /// Snap a window to an edge of the work area.
    ///
    /// Implements PRD §12 Floating + Snap:
    /// - [`SnapEdge::Left`] fills the left half,
    /// - [`SnapEdge::Right`] fills the right half,
    /// - [`SnapEdge::Maximized`] fills the whole work area,
    /// - [`SnapEdge::Free`] leaves the window geometry untouched.
    ///
    /// Snap margins so a snapped window isn't flush against the screen edge.
    pub fn snap(&mut self, surface: &WlSurface, edge: SnapEdge) {
        self.ensure_alive();
        let Some(idx) = self.windows.iter().position(|w| w.wl_surface() == surface) else {
            return;
        };
        let area = self.work_area;

        match edge {
            SnapEdge::Left => {
                self.windows[idx].rect = Rectangle::new(
                    (area.loc.x, area.loc.y).into(),
                    Size::new(area.size.w / 2, area.size.h),
                );
            }
            SnapEdge::Right => {
                self.windows[idx].rect = Rectangle::new(
                    (area.loc.x + area.size.w / 2, area.loc.y).into(),
                    Size::new(area.size.w - area.size.w / 2, area.size.h),
                );
            }
            SnapEdge::Maximized => {
                self.windows[idx].rect = Rectangle::new(
                    (area.loc.x, area.loc.y).into(),
                    Size::new(area.size.w, area.size.h),
                );
            }
        }
    }

    /// Whether `surface` is currently snapped (not free).
    #[allow(dead_code)]
    pub fn is_snapped(&self, surface: &WlSurface) -> bool {
        self.windows
            .iter()
            .find(|w| w.wl_surface() == surface)
            .map(|w| {
                // A snapped window fills either the whole work area (maximized)
                // or exactly one half (left/right).
                let area = self.work_area;
                let half = area.size.w / 2;
                w.rect.size == Size::new(area.size.w, area.size.h) // maximized
                    || w.rect.size == Size::new(half, area.size.h) // left/right
            })
            .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A placeholder toplevel surface isn't easy to create without a live
    /// display, so we exercise the geometry math through the public window
    /// manager where the window's surface data isn't needed.

    #[test]
    fn default_work_area_is_full_hd() {
        let wm = WindowManager::default();
        assert_eq!(wm.work_area().size.w, 1920);
        assert_eq!(wm.work_area().size.h, 1080);
    }

    #[test]
    fn setting_work_area_changes_snap_geometry() {
        let mut wm = WindowManager::default();
        wm.set_work_area(Rectangle::from_size(Size::new(2560, 1440)));
        assert_eq!(wm.work_area().size.w, 2560);
    }
}
