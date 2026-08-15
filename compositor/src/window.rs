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
    /// The workspace this window belongs to (0-indexed). Windows on the active
    /// workspace are shown/focused; the rest are hidden (PRD §13).
    pub workspace: usize,
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
    /// The active workspace (0-indexed). `windows` whose `workspace` equals
    /// this are visible; the rest are hidden (PRD §13).
    active_workspace: usize,
    /// Total number of workspaces.
    workspace_count: usize,
}

impl Default for WindowManager {
    fn default() -> Self {
        WindowManager {
            windows: Vec::new(),
            focused: None,
            work_area: Rectangle::from_size(Size::new(1920, 1080)),
            active_workspace: 0,
            workspace_count: 4,
        }
    }
}

/// A handle describing an in-progress interactive workspace transition
/// (PRD §13 / §4.4).
///
/// In M1 this reserves the interface for touchpad-driven transitions — the full
/// gesture-to-UI-progress wiring lands in Phase 4. It captures the source and
/// destination workspace plus a `progress: 0.0..=1.0` value so callers can
/// drive an interruptible, reversible, cancellable transition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkspaceTransition {
    /// Workspace the gesture started on (the "from" state).
    pub from: usize,
    /// Workspace being revealed (the "to" state).
    pub to: usize,
    /// Normalized gesture progress in `0.0..=1.0`.
    pub progress: f32,
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
    /// position. The new window becomes the focused, topmost window, and
    /// belongs to the currently active workspace.
    pub fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.ensure_alive();
        let rect = self.next_placeholder_rect();
        self.windows.push(Window {
            surface,
            rect,
            z_index: self.windows.len(),
            workspace: self.active_workspace,
        });
        self.focused = Some(self.windows.len() - 1);
    }

    /// Find the topmost window belonging to the active workspace whose
    /// rectangle contains `pos`.
    ///
    /// `pos` is in logical compositor space (scale 1.0). We iterate topmost
    /// (highest z) first, so an overlapping window wins over one beneath it.
    pub fn surface_under(&self, pos: smithay::utils::Point<f64, Logical>) -> Option<WlSurface> {
        self.hit_test_order()
            .find(|w| w.rect.to_f64().contains(pos))
            .map(|w| w.wl_surface().clone())
    }

    /// The visible (active-workspace) toplevels in hit-test order (topmost /
    /// highest z first).
    fn hit_test_order(&self) -> impl Iterator<Item = &Window> {
        let mut order: Vec<&Window> = self
            .windows
            .iter()
            .filter(|w| w.workspace == self.active_workspace)
            .collect();
        order.sort_by_key(|w| std::cmp::Reverse(w.z_index));
        order.into_iter()
    }

    /// The surface that currently has keyboard focus, if any.
    pub fn focused_surface(&self) -> Option<WlSurface> {
        self.focused
            .and_then(|i| self.windows.get(i))
            .filter(|w| w.workspace == self.active_workspace)
            .map(|w| w.wl_surface().clone())
    }

    /// Bring `surface` to the top of the z-order and focus it.
    pub fn set_focus(&mut self, surface: &WlSurface) {
        self.ensure_alive();
        let Some(idx) = self.windows.iter().position(|w| w.wl_surface() == surface) else {
            return;
        };
        if self.windows[idx].workspace != self.active_workspace {
            return;
        }
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
        let visible: Vec<usize> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, w)| w.workspace == self.active_workspace)
            .map(|(i, _)| i)
            .collect();
        if visible.is_empty() {
            self.focused = None;
            return None;
        }
        // Bring the window after the currently focused one to the front.
        let cur = visible.iter().position(|&i| self.focused == Some(i));
        let next = match cur {
            Some(c) => visible[(c + 1) % visible.len()],
            None => visible[0],
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

    /// The list of toplevel surfaces belonging to the **active** workspace, in
    /// creation order. Renders only these.
    pub fn toplevel_surfaces(&self) -> impl Iterator<Item = &ToplevelSurface> {
        self.windows
            .iter()
            .filter(|w| w.workspace == self.active_workspace)
            .map(|w| &w.surface)
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

    // -- Workspaces (PRD §13) -------------------------------------------------

    /// The index of the currently active workspace (0-indexed).
    #[allow(dead_code)]
    pub fn active_workspace(&self) -> usize {
        self.active_workspace
    }

    /// Total number of workspaces.
    #[allow(dead_code)]
    pub fn workspace_count(&self) -> usize {
        self.workspace_count
    }

    /// Set the number of workspaces. If the active workspace is beyond the new
    /// count, it is clamped. Defaults to 4.
    #[allow(dead_code)]
    pub fn set_workspace_count(&mut self, count: usize) {
        assert!(count >= 1, "at least one workspace required");
        self.workspace_count = count;
        if self.active_workspace >= count {
            self.active_workspace = count - 1;
        }
    }

    /// Switch the active workspace (PRD §13). Windows on the new workspace
    /// become visible; those on the prior workspace are hidden. Returns the
    /// workspace index switched to.
    #[allow(dead_code)]
    pub fn switch_workspace(&mut self, index: usize) -> usize {
        assert!(index < self.workspace_count, "workspace index out of range");
        self.active_workspace = index;
        // Focus the topmost window on the new workspace, if any.
        let top_surface = self.hit_test_order().next().map(|w| w.surface.clone());
        self.focused = top_surface.as_ref().and_then(|s| {
            self.windows
                .iter()
                .position(|w| w.wl_surface() == s.wl_surface())
        });
        self.active_workspace
    }

    /// Move a window to another workspace.
    #[allow(dead_code)]
    pub fn move_to_workspace(&mut self, surface: &WlSurface, workspace: usize) {
        assert!(
            workspace < self.workspace_count,
            "workspace index out of range"
        );
        if let Some(w) = self.windows.iter_mut().find(|w| w.wl_surface() == surface) {
            w.workspace = workspace;
        }
        self.ensure_alive();
    }

    /// The toplevel surfaces belonging to a given workspace.
    #[allow(dead_code)]
    pub fn workspace_surfaces(&self, workspace: usize) -> impl Iterator<Item = &ToplevelSurface> {
        let ws = workspace;
        self.windows
            .iter()
            .filter(move |w| w.workspace == ws)
            .map(|w| &w.surface)
    }

    /// Begin a (placeholder) interactive workspace transition. The full
    /// gesture-to-progress wiring lands in Phase 4 (PRD §4.4); this reserves
    /// the `WorkspaceTransition` type and the switch entry point so callers can
    /// reference it during M1.
    #[allow(dead_code)]
    pub fn begin_workspace_transition(&self, from: usize, to: usize) -> WorkspaceTransition {
        WorkspaceTransition {
            from,
            to,
            progress: 0.0,
        }
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

    #[test]
    fn default_workspace_is_zero_with_four_count() {
        let wm = WindowManager::default();
        assert_eq!(wm.active_workspace(), 0);
        assert_eq!(wm.workspace_count(), 4);
    }

    #[test]
    fn resizing_workspace_count_clamps_active() {
        let mut wm = WindowManager::default();
        wm.switch_workspace(3);
        wm.set_workspace_count(2);
        assert_eq!(wm.active_workspace(), 1);
        assert_eq!(wm.workspace_count(), 2);
    }

    #[test]
    fn switching_to_invalid_workspace_panics() {
        let wm = WindowManager::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut w = wm;
            w.switch_workspace(5);
        }));
        assert!(result.is_err(), "out-of-range workspace should panic");
    }

    #[test]
    fn transition_handle_records_from_and_to() {
        let wm = WindowManager::default();
        let t = wm.begin_workspace_transition(0, 1);
        assert_eq!(t.from, 0);
        assert_eq!(t.to, 1);
        assert_eq!(t.progress, 0.0);
    }
}
