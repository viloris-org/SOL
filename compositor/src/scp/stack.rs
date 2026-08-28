//! Z-ordered window stack and pointer hit-testing.
//!
//! Input routing needs one question answered cheaply and unambiguously: given a
//! point on an output, which surface owns it? That requires every window's
//! *absolute* geometry and a total order over them, neither of which the
//! per-role maps in [`crate::scp::surface`] provide on their own.
//!
//! [`WindowStack`] is that flattened view, ordered topmost-first:
//!
//! ```text
//! lock surfaces               ← above everything when the session is locked
//! overlay layer surfaces      ← critical alerts
//! top layer surfaces          ← panels, docks
//! popups (innermost first)    ← menus, tooltips; above their parent
//! toplevels (focus order)     ← application windows
//! bottom layer surfaces       ← wallpaper-adjacent chrome
//! background layer surfaces
//! ```
//!
//! The stack is rebuilt from authoritative state rather than maintained
//! incrementally, so it cannot drift out of sync with surface lifetimes.

use crate::scp::protocol::{
    LayerSurfaceId, LockSurfaceId, PopupId, Rect, SessionId, SurfaceId, ToplevelId,
};

/// Which role a stack entry was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackKind {
    LayerSurface(LayerSurfaceId),
    Toplevel(ToplevelId),
    Popup(PopupId),
    LockSurface(LockSurfaceId),
}

/// One window in the Z order, in absolute output coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackEntry {
    pub session_id: SessionId,
    pub surface_id: SurfaceId,
    pub kind: StackKind,
    /// Absolute position and size on the output layout.
    pub rect: Rect,
    /// Whether this window can take keyboard focus when clicked.
    pub accepts_keyboard: bool,
}

impl StackEntry {
    /// Translate absolute coordinates into this window's surface-local frame.
    pub fn to_local(&self, x: f64, y: f64) -> (f64, f64) {
        (x - f64::from(self.rect.x), y - f64::from(self.rect.y))
    }

    /// Whether an absolute point falls within this window's bounds.
    ///
    /// This is only the coarse bounds test. A surface may additionally restrict
    /// input to a sub-region, which the caller applies in surface-local space.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let left = f64::from(self.rect.x);
        let top = f64::from(self.rect.y);
        let right = left + f64::from(self.rect.width);
        let bottom = top + f64::from(self.rect.height);
        x >= left && x < right && y >= top && y < bottom
    }
}

/// Windows ordered topmost-first.
#[derive(Debug, Default, Clone)]
pub struct WindowStack {
    entries: Vec<StackEntry>,
}

impl WindowStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an entry below everything already pushed.
    ///
    /// Callers must push in descending Z order; [`WindowStack`] does not sort.
    pub fn push(&mut self, entry: StackEntry) {
        self.entries.push(entry);
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Iterate topmost-first.
    pub fn iter(&self) -> impl Iterator<Item = &StackEntry> {
        self.entries.iter()
    }

    /// Iterate bottommost-first, the order a renderer would paint in.
    pub fn iter_bottom_up(&self) -> impl Iterator<Item = &StackEntry> {
        self.entries.iter().rev()
    }

    pub fn entries(&self) -> &[StackEntry] {
        &self.entries
    }

    /// Topmost window whose bounds contain the point.
    ///
    /// `accepts` refines the coarse bounds test with the surface's input region
    /// and is given surface-local coordinates. A window that rejects the point
    /// is transparent to input: the search continues beneath it.
    pub fn hit_test(
        &self,
        x: f64,
        y: f64,
        mut accepts: impl FnMut(&StackEntry, f64, f64) -> bool,
    ) -> Option<StackEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.contains(x, y))
            .find(|entry| {
                let (local_x, local_y) = entry.to_local(x, y);
                accepts(entry, local_x, local_y)
            })
            .copied()
    }

    /// Look up an entry by the surface that backs it.
    pub fn find_surface(&self, session_id: SessionId, surface_id: SurfaceId) -> Option<StackEntry> {
        self.entries
            .iter()
            .find(|entry| entry.session_id == session_id && entry.surface_id == surface_id)
            .copied()
    }

    /// Topmost entry belonging to `session_id`.
    pub fn topmost_for_session(&self, session_id: SessionId) -> Option<StackEntry> {
        self.entries
            .iter()
            .find(|entry| entry.session_id == session_id)
            .copied()
    }
}

/// Horizontal and vertical step between successive cascaded windows.
const CASCADE_STEP: i32 = 32;
/// Cascade offsets wrap after this many windows so placement stays on-screen.
const CASCADE_WRAP: i32 = 8;

/// Choose an initial position for a new toplevel.
///
/// SOL has no window manager yet, so new windows are centered on the output and
/// then cascaded by `index` to keep successive windows from stacking exactly on
/// top of one another. The result is clamped so a window larger than the output
/// still starts at the output origin rather than at a negative coordinate.
pub fn place_toplevel(output: &Rect, width: i32, height: i32, index: u32) -> (i32, i32) {
    let step = (i32::try_from(index % u32::try_from(CASCADE_WRAP).unwrap_or(1)).unwrap_or(0))
        .saturating_mul(CASCADE_STEP);

    let centered_x = output.x + (output.width - width) / 2;
    let centered_y = output.y + (output.height - height) / 2;

    let max_x = output.x + (output.width - width).max(0);
    let max_y = output.y + (output.height - height).max(0);

    let x = centered_x
        .saturating_add(step)
        .clamp(output.x, max_x.max(output.x));
    let y = centered_y
        .saturating_add(step)
        .clamp(output.y, max_y.max(output.y));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        kind: StackKind,
        session_id: SessionId,
        surface_id: SurfaceId,
        rect: Rect,
    ) -> StackEntry {
        StackEntry {
            session_id,
            surface_id,
            kind,
            rect,
            accepts_keyboard: true,
        }
    }

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn accept_all(_: &StackEntry, _: f64, _: f64) -> bool {
        true
    }

    #[test]
    fn hit_test_returns_the_topmost_overlapping_window() {
        let mut stack = WindowStack::new();
        stack.push(entry(StackKind::Popup(1), 1, 10, rect(50, 50, 100, 100)));
        stack.push(entry(StackKind::Toplevel(1), 1, 11, rect(0, 0, 400, 400)));

        let hit = stack
            .hit_test(60.0, 60.0, accept_all)
            .expect("point is covered");
        assert_eq!(hit.kind, StackKind::Popup(1));

        // Outside the popup but still inside the toplevel.
        let hit = stack
            .hit_test(300.0, 300.0, accept_all)
            .expect("point is covered");
        assert_eq!(hit.kind, StackKind::Toplevel(1));
    }

    #[test]
    fn hit_test_skips_windows_that_reject_the_point() {
        let mut stack = WindowStack::new();
        stack.push(entry(StackKind::Popup(1), 1, 10, rect(0, 0, 100, 100)));
        stack.push(entry(StackKind::Toplevel(1), 1, 11, rect(0, 0, 400, 400)));

        // The popup covers the point but declines it, so input falls through.
        let hit = stack
            .hit_test(10.0, 10.0, |candidate, _, _| {
                candidate.kind != StackKind::Popup(1)
            })
            .expect("point falls through to the toplevel");
        assert_eq!(hit.kind, StackKind::Toplevel(1));
    }

    #[test]
    fn hit_test_returns_none_outside_every_window() {
        let mut stack = WindowStack::new();
        stack.push(entry(StackKind::Toplevel(1), 1, 11, rect(0, 0, 100, 100)));

        assert!(stack.hit_test(500.0, 500.0, accept_all).is_none());
    }

    #[test]
    fn bounds_are_half_open() {
        let single = entry(StackKind::Toplevel(1), 1, 11, rect(10, 10, 20, 20));

        assert!(single.contains(10.0, 10.0));
        assert!(single.contains(29.9, 29.9));
        // The far edge belongs to the next window, not this one.
        assert!(!single.contains(30.0, 20.0));
        assert!(!single.contains(20.0, 30.0));
        assert!(!single.contains(9.9, 20.0));
    }

    #[test]
    fn local_coordinates_are_relative_to_the_window_origin() {
        let single = entry(StackKind::Toplevel(1), 1, 11, rect(100, 200, 50, 50));
        let (x, y) = single.to_local(120.0, 250.0);
        assert!((x - 20.0).abs() < f64::EPSILON);
        assert!((y - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn iterates_bottom_up_for_rendering() {
        let mut stack = WindowStack::new();
        stack.push(entry(StackKind::Popup(1), 1, 10, rect(0, 0, 10, 10)));
        stack.push(entry(StackKind::Toplevel(1), 1, 11, rect(0, 0, 10, 10)));

        let painted: Vec<_> = stack.iter_bottom_up().map(|entry| entry.kind).collect();
        assert_eq!(painted, vec![StackKind::Toplevel(1), StackKind::Popup(1)]);
    }

    #[test]
    fn places_the_first_toplevel_centered() {
        let output = rect(0, 0, 1920, 1080);
        let (x, y) = place_toplevel(&output, 800, 600, 0);
        assert_eq!((x, y), (560, 240));
    }

    #[test]
    fn cascades_successive_toplevels() {
        let output = rect(0, 0, 1920, 1080);
        let (first_x, first_y) = place_toplevel(&output, 800, 600, 0);
        let (second_x, second_y) = place_toplevel(&output, 800, 600, 1);
        assert_eq!(second_x - first_x, CASCADE_STEP);
        assert_eq!(second_y - first_y, CASCADE_STEP);

        // Offsets wrap so a long-lived session cannot walk windows off-screen.
        let (wrapped_x, wrapped_y) = place_toplevel(&output, 800, 600, CASCADE_WRAP as u32);
        assert_eq!((wrapped_x, wrapped_y), (first_x, first_y));
    }

    #[test]
    fn clamps_windows_larger_than_the_output() {
        let output = rect(0, 0, 800, 600);
        let (x, y) = place_toplevel(&output, 1600, 1200, 3);
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn respects_output_origin_in_a_multi_output_layout() {
        let secondary = rect(1920, 0, 1920, 1080);
        let (x, y) = place_toplevel(&secondary, 800, 600, 0);
        assert_eq!((x, y), (1920 + 560, 240));
    }
}
