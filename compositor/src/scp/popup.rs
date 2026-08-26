//! Popup window positioning and constraint resolution.

use crate::scp::protocol::{
    ConstraintAdjustment, Edge, Gravity, PopupPositioner, Rect, SurfaceId,
};

/// Resolved popup geometry after constraint adjustment.
#[derive(Debug, Clone)]
pub struct PopupGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Popup window state.
#[derive(Debug)]
pub struct Popup {
    pub surface_id: SurfaceId,
    pub parent_id: SurfaceId,
    pub geometry: PopupGeometry,
    pub grab: bool,
}

impl Popup {
    pub fn new(surface_id: SurfaceId, parent_id: SurfaceId, grab: bool) -> Self {
        Self {
            surface_id,
            parent_id,
            geometry: PopupGeometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            grab,
        }
    }
}

/// Position a popup relative to its parent, applying constraint adjustments.
pub fn position_popup(
    positioner: &PopupPositioner,
    parent_geometry: &Rect,
    output_bounds: &Rect,
) -> PopupGeometry {
    let (mut x, mut y) = calculate_initial_position(positioner, parent_geometry);

    // Apply constraint adjustments if popup would be off-screen
    let popup_rect = Rect {
        x,
        y,
        width: positioner.size.0,
        height: positioner.size.1,
    };

    if !is_fully_visible(&popup_rect, output_bounds)
        && let Some(adjusted) = apply_constraints(positioner, &popup_rect, output_bounds, parent_geometry)
    {
        x = adjusted.x;
        y = adjusted.y;
    }

    PopupGeometry {
        x,
        y,
        width: positioner.size.0,
        height: positioner.size.1,
    }
}

fn calculate_initial_position(positioner: &PopupPositioner, _parent: &Rect) -> (i32, i32) {
    // Anchor rect is in parent-local coordinates, so use it directly
    let anchor_x = positioner.anchor_rect.x;
    let anchor_y = positioner.anchor_rect.y;

    // Calculate anchor point based on anchor edge
    let (anchor_px, anchor_py) = match positioner.anchor_edge {
        Edge::Top => (
            anchor_x + positioner.anchor_rect.width / 2,
            anchor_y,
        ),
        Edge::Bottom => (
            anchor_x + positioner.anchor_rect.width / 2,
            anchor_y + positioner.anchor_rect.height,
        ),
        Edge::Left => (
            anchor_x,
            anchor_y + positioner.anchor_rect.height / 2,
        ),
        Edge::Right => (
            anchor_x + positioner.anchor_rect.width,
            anchor_y + positioner.anchor_rect.height / 2,
        ),
    };

    // Apply gravity to determine popup position relative to anchor
    let (popup_x, popup_y) = match positioner.gravity {
        Gravity::None => (anchor_px, anchor_py),
        Gravity::Top => (anchor_px - positioner.size.0 / 2, anchor_py - positioner.size.1),
        Gravity::Bottom => (anchor_px - positioner.size.0 / 2, anchor_py),
        Gravity::Left => (anchor_px - positioner.size.0, anchor_py - positioner.size.1 / 2),
        Gravity::Right => (anchor_px, anchor_py - positioner.size.1 / 2),
        Gravity::TopLeft => (anchor_px - positioner.size.0, anchor_py - positioner.size.1),
        Gravity::TopRight => (anchor_px, anchor_py - positioner.size.1),
        Gravity::BottomLeft => (anchor_px - positioner.size.0, anchor_py),
        Gravity::BottomRight => (anchor_px, anchor_py),
    };

    (
        popup_x + positioner.offset.0,
        popup_y + positioner.offset.1,
    )
}

fn is_fully_visible(rect: &Rect, bounds: &Rect) -> bool {
    rect.x >= bounds.x
        && rect.y >= bounds.y
        && rect.x + rect.width <= bounds.x + bounds.width
        && rect.y + rect.height <= bounds.y + bounds.height
}

fn apply_constraints(
    positioner: &PopupPositioner,
    popup: &Rect,
    bounds: &Rect,
    parent: &Rect,
) -> Option<Rect> {
    let mut result = popup.clone();
    let constraint = &positioner.constraint;

    // Try flip adjustments first (most common)
    if constraint.flip_x && (popup.x < bounds.x || popup.x + popup.width > bounds.x + bounds.width) {
        result.x = flip_horizontal(positioner, parent);
    }

    if constraint.flip_y && (popup.y < bounds.y || popup.y + popup.height > bounds.y + bounds.height) {
        result.y = flip_vertical(positioner, parent);
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

fn flip_horizontal(positioner: &PopupPositioner, parent: &Rect) -> i32 {
    let anchor_x = parent.x + positioner.anchor_rect.x;
    match positioner.anchor_edge {
        Edge::Left => anchor_x + positioner.anchor_rect.width + positioner.offset.0,
        Edge::Right => anchor_x - positioner.size.0 - positioner.offset.0,
        _ => anchor_x + positioner.anchor_rect.width / 2 - positioner.size.0 / 2,
    }
}

fn flip_vertical(positioner: &PopupPositioner, parent: &Rect) -> i32 {
    let anchor_y = parent.y + positioner.anchor_rect.y;
    match positioner.anchor_edge {
        Edge::Top => anchor_y + positioner.anchor_rect.height + positioner.offset.1,
        Edge::Bottom => anchor_y - positioner.size.1 - positioner.offset.1,
        _ => anchor_y + positioner.anchor_rect.height / 2 - positioner.size.1 / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_popup_below_parent() {
        let positioner = PopupPositioner {
            anchor_rect: Rect { x: 10, y: 10, width: 100, height: 30 },
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

        let parent = Rect { x: 0, y: 0, width: 400, height: 300 };
        let output = Rect { x: 0, y: 0, width: 1920, height: 1080 };

        let geometry = position_popup(&positioner, &parent, &output);

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
            anchor_rect: Rect { x: 0, y: 0, width: 100, height: 30 },
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

        let parent = Rect { x: 0, y: 950, width: 400, height: 100 };
        let output = Rect { x: 0, y: 0, width: 1920, height: 1080 };

        let geometry = position_popup(&positioner, &parent, &output);

        // Should flip to above the parent since it would go off-screen below
        assert!(geometry.y < parent.y);
    }
}
