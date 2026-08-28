//! Popup window positioning, constraint resolution, and grab lifetime.

use crate::scp::protocol::{Edge, Gravity, PopupId, PopupPositioner, Rect, SessionId, SurfaceId};

/// Resolved popup geometry after constraint adjustment.
///
/// Coordinates are relative to the popup's parent surface, matching the frame
/// the positioner's anchor rectangle is expressed in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PopupGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Popup window state.
#[derive(Debug, Clone)]
pub struct Popup {
    pub id: PopupId,
    /// Owning session. Surface IDs are client-local, so a popup is only
    /// identified by the pair — without this, two clients that both use surface
    /// ID 1 would alias each other's popups.
    pub session_id: SessionId,
    pub surface_id: SurfaceId,
    /// Parent surface, either a toplevel or an outer popup in the same session.
    pub parent_id: SurfaceId,
    /// Parent popup, when this popup is nested inside another.
    pub parent_popup: Option<PopupId>,
    pub geometry: PopupGeometry,
    /// Whether the popup took an input grab, making outside clicks dismiss it.
    pub grab: bool,
}

impl Popup {
    pub fn new(
        id: PopupId,
        session_id: SessionId,
        surface_id: SurfaceId,
        parent_id: SurfaceId,
        grab: bool,
    ) -> Self {
        Self {
            id,
            session_id,
            surface_id,
            parent_id,
            parent_popup: None,
            geometry: PopupGeometry::default(),
            grab,
        }
    }

    /// The surface pair that identifies this popup.
    pub const fn key(&self) -> (SessionId, SurfaceId) {
        (self.session_id, self.surface_id)
    }
}

/// Owns every live popup and the chain of popups holding an input grab.
///
/// Popups form trees rooted at a toplevel: a menu bar item opens a menu, which
/// opens a submenu. Two invariants make dismissal correct:
///
/// 1. **Cascade** — closing a popup closes everything nested inside it, so no
///    orphaned submenu can outlive its parent.
/// 2. **Grab chain** — grabbing popups form an ordered chain, outermost first.
///    A click outside the chain dismisses all of it; a click on a member
///    dismisses only what is nested deeper.
#[derive(Debug, Default)]
pub struct PopupManager {
    popups: std::collections::HashMap<PopupId, Popup>,
    grab_chain: Vec<PopupId>,
    next_id: PopupId,
}

impl PopupManager {
    pub fn new() -> Self {
        Self {
            popups: std::collections::HashMap::new(),
            grab_chain: Vec::new(),
            next_id: 1,
        }
    }

    /// Register a popup and return its compositor-assigned ID.
    ///
    /// `parent_popup` is `None` when the parent surface is a toplevel or layer
    /// surface, and `Some` when this popup nests inside another.
    pub fn create(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
        parent_id: SurfaceId,
        parent_popup: Option<PopupId>,
        grab: bool,
    ) -> Result<PopupId, String> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("Popup ID space exhausted")?;

        let mut popup = Popup::new(id, session_id, surface_id, parent_id, grab);
        popup.parent_popup = parent_popup;
        self.popups.insert(id, popup);

        if grab {
            self.grab_chain.push(id);
        }
        Ok(id)
    }

    pub fn get(&self, id: PopupId) -> Option<&Popup> {
        self.popups.get(&id)
    }

    pub fn get_mut(&mut self, id: PopupId) -> Option<&mut Popup> {
        self.popups.get_mut(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Popup> {
        self.popups.values()
    }

    pub fn len(&self) -> usize {
        self.popups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.popups.is_empty()
    }

    /// Find the popup backed by a surface. Surface IDs are client-local, so both
    /// halves of the key are required.
    pub fn find_by_surface(&self, session_id: SessionId, surface_id: SurfaceId) -> Option<PopupId> {
        self.popups
            .values()
            .find(|popup| popup.session_id == session_id && popup.surface_id == surface_id)
            .map(|popup| popup.id)
    }

    /// Popups holding a grab, outermost first.
    pub fn grab_chain(&self) -> &[PopupId] {
        &self.grab_chain
    }

    /// The deepest popup currently holding a grab.
    pub fn innermost_grab(&self) -> Option<PopupId> {
        self.grab_chain.last().copied()
    }

    pub fn has_grab(&self) -> bool {
        !self.grab_chain.is_empty()
    }

    /// Remove a popup and everything nested inside it, innermost first.
    ///
    /// The ordering matters to clients: a submenu should learn it is gone before
    /// the menu that owned it.
    pub fn dismiss_subtree(&mut self, id: PopupId) -> Vec<Popup> {
        if !self.popups.contains_key(&id) {
            return Vec::new();
        }

        // Collect the subtree breadth-first, then reverse so deeper popups —
        // which are discovered later — are dismissed first.
        let mut ordered = vec![id];
        let mut cursor = 0;
        while cursor < ordered.len() {
            let parent = ordered[cursor];
            cursor += 1;
            for popup in self.popups.values() {
                if popup.parent_popup == Some(parent) {
                    ordered.push(popup.id);
                }
            }
        }
        ordered.reverse();

        self.remove_ids(&ordered)
    }

    /// Dismiss every popup parented to a surface that is going away, along with
    /// their descendants.
    pub fn dismiss_children_of_surface(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Vec<Popup> {
        let roots: Vec<PopupId> = self
            .popups
            .values()
            .filter(|popup| popup.session_id == session_id && popup.parent_id == surface_id)
            .map(|popup| popup.id)
            .collect();

        roots
            .into_iter()
            .flat_map(|root| self.dismiss_subtree(root))
            .collect()
    }

    /// Dismiss the whole grab chain, innermost first.
    pub fn dismiss_grab_chain(&mut self) -> Vec<Popup> {
        match self.grab_chain.first().copied() {
            Some(outermost) => self.dismiss_subtree(outermost),
            None => Vec::new(),
        }
    }

    /// Dismiss the popups nested inside `id`, keeping `id` itself.
    ///
    /// This is what a click on an outer menu should do: collapse the submenus it
    /// opened without closing the menu the user is still pointing at.
    pub fn dismiss_descendants(&mut self, id: PopupId) -> Vec<Popup> {
        let children: Vec<PopupId> = self
            .popups
            .values()
            .filter(|popup| popup.parent_popup == Some(id))
            .map(|popup| popup.id)
            .collect();

        children
            .into_iter()
            .flat_map(|child| self.dismiss_subtree(child))
            .collect()
    }

    /// Resolve what an input event outside a popup should dismiss.
    ///
    /// `hit` is the surface the event landed on, or `None` if it landed on empty
    /// space. Returns the popups to dismiss, innermost first.
    pub fn dismiss_for_outside_input(&mut self, hit: Option<(SessionId, SurfaceId)>) -> Vec<Popup> {
        if self.grab_chain.is_empty() {
            return Vec::new();
        }

        let hit_popup = hit.and_then(|(session_id, surface_id)| {
            self.find_by_surface(session_id, surface_id)
                .filter(|id| self.grab_chain.contains(id))
        });

        match hit_popup {
            // The click landed inside the chain: collapse only what is deeper.
            Some(id) => self.dismiss_descendants(id),
            // The click landed outside every grabbing popup: collapse it all.
            None => self.dismiss_grab_chain(),
        }
    }

    /// Remove every popup owned by a disconnecting session.
    pub fn remove_session(&mut self, session_id: SessionId) -> Vec<Popup> {
        let owned: Vec<PopupId> = self
            .popups
            .values()
            .filter(|popup| popup.session_id == session_id)
            .map(|popup| popup.id)
            .collect();

        self.remove_ids(&owned)
    }

    fn remove_ids(&mut self, ids: &[PopupId]) -> Vec<Popup> {
        let removed: Vec<Popup> = ids.iter().filter_map(|id| self.popups.remove(id)).collect();
        self.grab_chain.retain(|id| self.popups.contains_key(id));
        removed
    }
}

/// Position a popup relative to its parent, applying constraint adjustments.
///
/// Everything here works in one frame: parent-local coordinates, the same frame
/// the positioner's anchor rectangle is expressed in. `output_bounds` must
/// therefore be the output rectangle translated into that frame, which is what
/// lets a constraint compare a popup against a screen edge without either side
/// needing to know where the parent sits on the layout.
pub fn position_popup(positioner: &PopupPositioner, output_bounds: &Rect) -> PopupGeometry {
    let (mut x, mut y) = calculate_initial_position(positioner);

    // Apply constraint adjustments if popup would be off-screen
    let popup_rect = Rect {
        x,
        y,
        width: positioner.size.0,
        height: positioner.size.1,
    };

    let mut width = positioner.size.0;
    let mut height = positioner.size.1;

    if !is_fully_visible(&popup_rect, output_bounds)
        && let Some(adjusted) = apply_constraints(positioner, &popup_rect, output_bounds)
    {
        x = adjusted.x;
        y = adjusted.y;
        width = adjusted.width;
        height = adjusted.height;
    }

    PopupGeometry {
        x,
        y,
        width,
        height,
    }
}

fn calculate_initial_position(positioner: &PopupPositioner) -> (i32, i32) {
    // Anchor rect is in parent-local coordinates, so use it directly
    let anchor_x = positioner.anchor_rect.x;
    let anchor_y = positioner.anchor_rect.y;

    // Calculate anchor point based on anchor edge
    let (anchor_px, anchor_py) = match positioner.anchor_edge {
        Edge::Top => (anchor_x + positioner.anchor_rect.width / 2, anchor_y),
        Edge::Bottom => (
            anchor_x + positioner.anchor_rect.width / 2,
            anchor_y + positioner.anchor_rect.height,
        ),
        Edge::Left => (anchor_x, anchor_y + positioner.anchor_rect.height / 2),
        Edge::Right => (
            anchor_x + positioner.anchor_rect.width,
            anchor_y + positioner.anchor_rect.height / 2,
        ),
    };

    // Apply gravity to determine popup position relative to anchor
    let (popup_x, popup_y) = match positioner.gravity {
        Gravity::None => (anchor_px, anchor_py),
        Gravity::Top => (
            anchor_px - positioner.size.0 / 2,
            anchor_py - positioner.size.1,
        ),
        Gravity::Bottom => (anchor_px - positioner.size.0 / 2, anchor_py),
        Gravity::Left => (
            anchor_px - positioner.size.0,
            anchor_py - positioner.size.1 / 2,
        ),
        Gravity::Right => (anchor_px, anchor_py - positioner.size.1 / 2),
        Gravity::TopLeft => (anchor_px - positioner.size.0, anchor_py - positioner.size.1),
        Gravity::TopRight => (anchor_px, anchor_py - positioner.size.1),
        Gravity::BottomLeft => (anchor_px - positioner.size.0, anchor_py),
        Gravity::BottomRight => (anchor_px, anchor_py),
    };

    (popup_x + positioner.offset.0, popup_y + positioner.offset.1)
}

fn is_fully_visible(rect: &Rect, bounds: &Rect) -> bool {
    rect.x >= bounds.x
        && rect.y >= bounds.y
        && rect.x + rect.width <= bounds.x + bounds.width
        && rect.y + rect.height <= bounds.y + bounds.height
}

fn apply_constraints(positioner: &PopupPositioner, popup: &Rect, bounds: &Rect) -> Option<Rect> {
    let mut result = *popup;
    let constraint = &positioner.constraint;

    // Try flip adjustments first (most common)
    if constraint.flip_x && (popup.x < bounds.x || popup.x + popup.width > bounds.x + bounds.width)
    {
        result.x = flip_horizontal(positioner);
    }

    if constraint.flip_y
        && (popup.y < bounds.y || popup.y + popup.height > bounds.y + bounds.height)
    {
        result.y = flip_vertical(positioner);
    }

    // Try slide adjustments
    if constraint.slide_x {
        if result.x < bounds.x {
            result.x = bounds.x;
        } else if result.x + result.width > bounds.x + bounds.width {
            result.x = bounds.x + bounds.width - result.width;
        }
    }

    if constraint.slide_y {
        if result.y < bounds.y {
            result.y = bounds.y;
        } else if result.y + result.height > bounds.y + bounds.height {
            result.y = bounds.y + bounds.height - result.height;
        }
    }

    // Try resize adjustments (last resort)
    if constraint.resize_x {
        if result.x < bounds.x {
            result.width -= bounds.x - result.x;
            result.x = bounds.x;
        }
        if result.x + result.width > bounds.x + bounds.width {
            result.width = bounds.x + bounds.width - result.x;
        }
    }

    if constraint.resize_y {
        if result.y < bounds.y {
            result.height -= bounds.y - result.y;
            result.y = bounds.y;
        }
        if result.y + result.height > bounds.y + bounds.height {
            result.height = bounds.y + bounds.height - result.y;
        }
    }

    Some(result)
}

fn flip_horizontal(positioner: &PopupPositioner) -> i32 {
    let anchor_x = positioner.anchor_rect.x;
    match positioner.anchor_edge {
        Edge::Left => anchor_x + positioner.anchor_rect.width + positioner.offset.0,
        Edge::Right => anchor_x - positioner.size.0 - positioner.offset.0,
        _ => anchor_x + positioner.anchor_rect.width / 2 - positioner.size.0 / 2,
    }
}

fn flip_vertical(positioner: &PopupPositioner) -> i32 {
    let anchor_y = positioner.anchor_rect.y;
    match positioner.anchor_edge {
        Edge::Top => anchor_y + positioner.anchor_rect.height + positioner.offset.1,
        Edge::Bottom => anchor_y - positioner.size.1 - positioner.offset.1,
        _ => anchor_y + positioner.anchor_rect.height / 2 - positioner.size.1 / 2,
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;

    /// A menu on surface 10, with two nested submenus, all grabbing.
    fn nested_chain() -> (PopupManager, PopupId, PopupId, PopupId) {
        let mut manager = PopupManager::new();
        let outer = manager
            .create(1, 10, 1, None, true)
            .expect("create outer popup");
        let middle = manager
            .create(1, 11, 10, Some(outer), true)
            .expect("create middle popup");
        let inner = manager
            .create(1, 12, 11, Some(middle), true)
            .expect("create inner popup");
        (manager, outer, middle, inner)
    }

    #[test]
    fn grab_chain_is_ordered_outermost_first() {
        let (manager, outer, middle, inner) = nested_chain();
        assert_eq!(manager.grab_chain(), &[outer, middle, inner]);
        assert_eq!(manager.innermost_grab(), Some(inner));
    }

    #[test]
    fn grabless_popups_stay_out_of_the_chain() {
        let mut manager = PopupManager::new();
        let tooltip = manager
            .create(1, 10, 1, None, false)
            .expect("create tooltip");

        assert!(!manager.has_grab());
        assert!(manager.grab_chain().is_empty());
        assert!(
            manager.get(tooltip).is_some(),
            "a grabless popup still exists, it just does not hold input"
        );
    }

    #[test]
    fn dismissing_a_subtree_reports_innermost_first() {
        let (mut manager, outer, middle, inner) = nested_chain();

        let dismissed: Vec<PopupId> = manager
            .dismiss_subtree(outer)
            .iter()
            .map(|popup| popup.id)
            .collect();

        // A submenu must learn it is gone before the menu that owned it.
        assert_eq!(dismissed, vec![inner, middle, outer]);
        assert!(manager.is_empty());
        assert!(manager.grab_chain().is_empty());
    }

    #[test]
    fn dismissing_descendants_keeps_the_popup_itself() {
        let (mut manager, outer, middle, inner) = nested_chain();

        let dismissed: Vec<PopupId> = manager
            .dismiss_descendants(outer)
            .iter()
            .map(|popup| popup.id)
            .collect();

        assert_eq!(dismissed, vec![inner, middle]);
        assert!(manager.get(outer).is_some());
        assert_eq!(manager.grab_chain(), &[outer]);
    }

    #[test]
    fn input_outside_the_chain_collapses_all_of_it() {
        let (mut manager, outer, middle, inner) = nested_chain();

        // A click on an unrelated surface.
        let dismissed: Vec<PopupId> = manager
            .dismiss_for_outside_input(Some((1, 99)))
            .iter()
            .map(|popup| popup.id)
            .collect();

        assert_eq!(dismissed, vec![inner, middle, outer]);
        assert!(manager.is_empty());
    }

    #[test]
    fn input_on_empty_space_collapses_the_chain() {
        let (mut manager, ..) = nested_chain();
        assert_eq!(manager.dismiss_for_outside_input(None).len(), 3);
        assert!(manager.is_empty());
    }

    #[test]
    fn input_on_a_chain_member_collapses_only_deeper_popups() {
        let (mut manager, outer, middle, inner) = nested_chain();

        // Clicking the middle menu closes its submenu but not itself.
        let dismissed: Vec<PopupId> = manager
            .dismiss_for_outside_input(Some((1, 11)))
            .iter()
            .map(|popup| popup.id)
            .collect();

        assert_eq!(dismissed, vec![inner]);
        assert_eq!(manager.grab_chain(), &[outer, middle]);
    }

    #[test]
    fn input_on_the_innermost_popup_dismisses_nothing() {
        let (mut manager, ..) = nested_chain();
        assert!(manager.dismiss_for_outside_input(Some((1, 12))).is_empty());
        assert_eq!(manager.len(), 3);
    }

    #[test]
    fn a_surface_going_away_takes_its_popup_tree_with_it() {
        let (mut manager, outer, middle, inner) = nested_chain();

        // Surface 1 is the toplevel the outer menu hangs off.
        let dismissed: Vec<PopupId> = manager
            .dismiss_children_of_surface(1, 1)
            .iter()
            .map(|popup| popup.id)
            .collect();

        assert_eq!(dismissed, vec![inner, middle, outer]);
        assert!(manager.is_empty());
    }

    #[test]
    fn popups_are_keyed_by_session_and_surface_together() {
        let mut manager = PopupManager::new();
        // Two clients both using client-local surface ID 10.
        let first = manager.create(1, 10, 1, None, true).expect("first popup");
        let second = manager.create(2, 10, 1, None, true).expect("second popup");

        assert_ne!(first, second);
        assert_eq!(manager.find_by_surface(1, 10), Some(first));
        assert_eq!(manager.find_by_surface(2, 10), Some(second));

        // Dropping one client leaves the other's popups untouched.
        let removed = manager.remove_session(1);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, first);
        assert_eq!(manager.find_by_surface(2, 10), Some(second));
    }

    #[test]
    fn dismissing_an_unknown_popup_is_a_no_op() {
        let (mut manager, ..) = nested_chain();
        assert!(manager.dismiss_subtree(9999).is_empty());
        assert_eq!(manager.len(), 3);
    }

    #[test]
    fn removing_a_middle_popup_takes_its_submenus_but_not_its_parent() {
        let (mut manager, outer, middle, inner) = nested_chain();

        let dismissed: Vec<PopupId> = manager
            .dismiss_subtree(middle)
            .iter()
            .map(|popup| popup.id)
            .collect();

        assert_eq!(dismissed, vec![inner, middle]);
        assert_eq!(manager.grab_chain(), &[outer]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::protocol::ConstraintAdjustment;

    /// Translate an output rectangle into the parent's coordinate frame, the
    /// same way `ScpState` does before positioning a popup.
    fn output_in_parent_frame(output: &Rect, parent: &Rect) -> Rect {
        Rect {
            x: output.x - parent.x,
            y: output.y - parent.y,
            width: output.width,
            height: output.height,
        }
    }

    #[test]
    fn positions_popup_below_parent() {
        let positioner = PopupPositioner {
            anchor_rect: Rect {
                x: 10,
                y: 10,
                width: 100,
                height: 30,
            },
            anchor_edge: Edge::Bottom,
            gravity: Gravity::Bottom,
            constraint: ConstraintAdjustment {
                flip_x: false,
                flip_y: false,
                slide_x: false,
                slide_y: false,
                resize_x: false,
                resize_y: false,
            },
            offset: (0, 0),
            size: (120, 200),
        };

        let parent = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        let output = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let local_output = output_in_parent_frame(&output, &parent);

        let geometry = position_popup(&positioner, &local_output);

        // anchor_rect is at (10, 10) with size (100, 30)
        // anchor_edge = Bottom means anchor point is at bottom edge center: (10 + 100/2, 10 + 30) = (60, 40)
        // gravity = Bottom means popup is positioned below anchor: (60 - 120/2, 40) = (0, 40)
        assert_eq!(geometry.x, 0);
        assert_eq!(geometry.y, 40);
        assert_eq!(geometry.width, 120);
        assert_eq!(geometry.height, 200);
    }

    #[test]
    fn flips_popup_when_off_screen() {
        let positioner = PopupPositioner {
            anchor_rect: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 30,
            },
            anchor_edge: Edge::Bottom,
            gravity: Gravity::Bottom,
            constraint: ConstraintAdjustment {
                flip_x: false,
                flip_y: true,
                slide_x: false,
                slide_y: false,
                resize_x: false,
                resize_y: false,
            },
            offset: (0, 0),
            size: (100, 500),
        };

        let parent = Rect {
            x: 0,
            y: 950,
            width: 400,
            height: 100,
        };
        let output = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let local_output = output_in_parent_frame(&output, &parent);

        let geometry = position_popup(&positioner, &local_output);

        // The popup is anchored below the parent's top edge and is taller than
        // the space left on screen, so flip_y moves it above the anchor —
        // a negative y in the parent's own frame.
        assert!(geometry.y < 0);
    }
}
