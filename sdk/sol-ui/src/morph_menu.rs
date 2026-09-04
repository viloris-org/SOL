//! Trigger-anchored Liquid Glass container morph.
//!
//! One shared surface changes shape from a circular trigger into a menu. The
//! controller owns live spring state so open/close reversals never restart from
//! a logical endpoint, and direct gesture takeover remains one-to-one.

use sol_animation::SpringValue;
use sol_design::{
    accessibility::{MotionPreference, TokenMode},
    color::{Color, Rgba},
    material::{Material, MaterialNesting},
    metrics::{ControlMetric, MorphMenuMetricSpec},
    motion::{Motion, MotionSpec},
    radius::Radius,
};

use crate::{
    AccessibilityNode, AccessibilityState, LiquidGlassFrame, LiquidGlassSurface,
    MaterialTokenContract, MaterializedComponent, SemanticId, SemanticRole,
};

/// One action revealed inside a morphing glass menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlassMenuItem {
    /// Stable application-local identifier.
    pub id: String,
    /// User-visible label.
    pub label: String,
    /// Whether this action can be activated.
    pub enabled: bool,
}

impl GlassMenuItem {
    /// Create an enabled menu action.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }

    /// Keep an action visible while disabling activation.
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Stable phase of a shared-container transformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MorphMenuState {
    /// Only the compact trigger is visible.
    #[default]
    Closed,
    /// The surface is expanding toward the menu state.
    Opening,
    /// Menu geometry and content are fully presented.
    Open,
    /// The surface is returning to its trigger.
    Closing,
    /// A pointer/gesture directly owns the morph progress.
    Tracking,
}

/// Smooth-union geometry consumed by a capable Liquid Glass renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlassMorphShapeFrame {
    /// Circular trigger/lobe extent after direct press feedback.
    pub lobe_extent: f32,
    /// Width of the body growing below the lobe.
    pub body_width: f32,
    /// Height of the body growing below the lobe.
    pub body_height: f32,
    /// Offset from the top-center origin to the body.
    pub body_offset_y: f32,
    /// Current body corner radius.
    pub body_corner_radius: f32,
    /// Smooth-union radius joining the circle and rounded body.
    pub union_radius: f32,
    /// Whole-shape scale used by immediate press/release feedback.
    pub scale: f32,
    /// Rotation from a plus/menu glyph toward a close glyph.
    pub trigger_rotation_degrees: f32,
}

/// Resolved content projection for one menu action.
#[derive(Debug, Clone, PartialEq)]
pub struct GlassMenuItemFrame {
    /// Stable application-local identifier.
    pub id: String,
    /// User-visible label.
    pub label: String,
    /// Whether activation is available.
    pub enabled: bool,
    /// Theme-resolved foreground color.
    pub foreground: Rgba,
    /// Shared reveal opacity; menu items never stagger and block interaction.
    pub opacity: f32,
    /// Scale-only reveal that avoids layout movement.
    pub scale: f32,
}

/// Complete renderer-neutral frame for a morphing glass menu.
#[derive(Debug, Clone, PartialEq)]
pub struct GlassMorphMenuFrame {
    /// Accessible group label.
    pub label: String,
    /// Current semantic phase.
    pub state: MorphMenuState,
    /// Shared material surface across compact and expanded states.
    pub surface: LiquidGlassFrame,
    /// Smooth-union shape at the live presentation progress.
    pub shape: GlassMorphShapeFrame,
    /// Menu action projections.
    pub items: Vec<GlassMenuItemFrame>,
    /// Current morph progress. Momentum may briefly overshoot `0..=1`.
    pub progress: f32,
    /// Content reveal progress clamped to `0..=1`.
    pub content_opacity: f32,
    /// Motion selected after accessibility resolution.
    pub motion: MotionSpec,
    /// False means the renderer must snap geometry and cross-fade content.
    pub spatial_motion: bool,
}

/// Semantic shared-container menu matching the circular-trigger reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlassMorphMenu {
    /// Accessible group label.
    pub label: String,
    /// Accessible label for the compact trigger.
    pub trigger_label: String,
    /// Ordered actions revealed by the expansion.
    pub items: Vec<GlassMenuItem>,
}

impl GlassMorphMenu {
    /// Create an empty morph menu.
    pub fn new(label: impl Into<String>, trigger_label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            trigger_label: trigger_label.into(),
            items: Vec::new(),
        }
    }

    /// Append one menu action.
    pub fn item(mut self, item: GlassMenuItem) -> Self {
        self.items.push(item);
        self
    }

    /// Retain this component in an interruptible animation controller.
    pub fn controller(self) -> GlassMorphMenuController {
        GlassMorphMenuController::new(self)
    }

    fn frame_for(
        &self,
        mode: TokenMode,
        state: MorphMenuState,
        progress: f32,
        press_scale: f32,
    ) -> GlassMorphMenuFrame {
        let metrics = MorphMenuMetricSpec::compact();
        let spatial_motion = matches!(mode.motion, MotionPreference::Full);
        let visual_progress = progress.clamp(-0.05, 1.05);
        let extent_progress = visual_progress.max(0.0);
        let geometry_progress = smoothstep(visual_progress.clamp(0.0, 1.0));
        let reveal = if geometry_progress <= metrics.content_reveal_start {
            0.0
        } else {
            smoothstep(
                (geometry_progress - metrics.content_reveal_start)
                    / (1.0 - metrics.content_reveal_start),
            )
        };
        let content_scale = lerp(metrics.pressed_scale, 1.0, reveal);
        let mut surface = LiquidGlassSurface::new(Material::Floating).frame_for(mode);
        surface.motion = mode.motion_spec(Motion::Morph);

        GlassMorphMenuFrame {
            label: self.label.clone(),
            state,
            surface,
            shape: GlassMorphShapeFrame {
                lobe_extent: metrics.trigger_extent,
                body_width: lerp(
                    metrics.trigger_extent,
                    metrics.expanded_width,
                    extent_progress,
                ),
                body_height: lerp(0.0, metrics.expanded_height, extent_progress),
                body_offset_y: metrics.body_offset_y,
                body_corner_radius: lerp(
                    metrics.trigger_extent / 2.0,
                    metrics.body_corner_radius,
                    geometry_progress,
                ),
                union_radius: metrics.union_radius * geometry_progress,
                scale: press_scale,
                trigger_rotation_degrees: metrics.trigger_rotation_degrees * geometry_progress,
            },
            items: self
                .items
                .iter()
                .map(|item| GlassMenuItemFrame {
                    id: item.id.clone(),
                    label: item.label.clone(),
                    enabled: item.enabled,
                    foreground: mode.color(if item.enabled {
                        Color::TextPrimary
                    } else {
                        Color::TextSecondary
                    }),
                    opacity: reveal,
                    scale: content_scale,
                })
                .collect(),
            progress: visual_progress,
            content_opacity: reveal,
            motion: mode.motion_spec(Motion::Morph),
            spatial_motion,
        }
    }
}

impl MaterializedComponent for GlassMorphMenu {
    fn material_tokens(&self) -> MaterialTokenContract {
        MaterialTokenContract {
            material: Material::Floating,
            nesting: MaterialNesting::Independent,
            radius: Radius::Lg,
            metric: ControlMetric::MorphMenu,
            motion: Motion::Morph,
        }
    }
}

/// Retained, interruptible interaction state for [`GlassMorphMenu`].
pub struct GlassMorphMenuController {
    menu: GlassMorphMenu,
    morph: SpringValue,
    press_scale: SpringValue,
    state: MorphMenuState,
    target_open: bool,
}

impl GlassMorphMenuController {
    /// Retain a closed menu with stable spring presentation values.
    pub fn new(menu: GlassMorphMenu) -> Self {
        Self {
            menu,
            morph: SpringValue::new(Motion::Morph, 0.0),
            press_scale: SpringValue::new(Motion::Rebound, 1.0),
            state: MorphMenuState::Closed,
            target_open: false,
        }
    }

    /// Return whether the current destination is the expanded state.
    pub const fn target_open(&self) -> bool {
        self.target_open
    }

    /// Return the live semantic phase.
    pub const fn state(&self) -> MorphMenuState {
        self.state
    }

    /// Apply immediate physical compression on pointer-down.
    pub fn pointer_down(&mut self) {
        self.press_scale
            .snap_to(MorphMenuMetricSpec::compact().pressed_scale);
    }

    /// Release the compressed trigger and toggle the menu.
    ///
    /// `normalized_velocity` is the pointer-release scale velocity per second.
    /// It is handed directly into the rebound spring.
    pub fn pointer_up_and_toggle(&mut self, normalized_velocity: f32) {
        self.press_scale.set_motion(Motion::Rebound);
        self.press_scale
            .retarget_with_velocity(1.0, normalized_velocity);
        self.toggle();
    }

    /// Cancel a pointer press without changing menu state.
    pub fn pointer_cancel(&mut self, normalized_velocity: f32) {
        self.press_scale.set_motion(Motion::Rebound);
        self.press_scale
            .retarget_with_velocity(1.0, normalized_velocity);
    }

    /// Toggle without adding pointer bounce, suitable for keyboard activation.
    pub fn toggle(&mut self) {
        self.target_open = !self.target_open;
        self.morph.set_motion(Motion::Morph);
        self.morph
            .retarget(if self.target_open { 1.0 } else { 0.0 });
        self.state = if self.target_open {
            MorphMenuState::Opening
        } else {
            MorphMenuState::Closing
        };
    }

    /// Give a drag direct one-to-one ownership of the morph.
    pub fn take_over_with_progress(&mut self, progress: f32, velocity: f32) {
        self.morph.take_over(progress.clamp(0.0, 1.0), velocity);
        self.state = MorphMenuState::Tracking;
    }

    /// Settle a released drag using its direction before its resting position.
    pub fn settle_from_gesture(&mut self, velocity: f32) {
        self.target_open = if velocity > 0.0 {
            true
        } else if velocity < 0.0 {
            false
        } else {
            self.morph.value() >= 0.5
        };
        self.morph.set_motion(Motion::Rebound);
        self.morph
            .retarget_with_velocity(if self.target_open { 1.0 } else { 0.0 }, velocity);
        self.state = if self.target_open {
            MorphMenuState::Opening
        } else {
            MorphMenuState::Closing
        };
    }

    /// Advance retained springs and resolve the next component frame.
    pub fn tick(&mut self, elapsed_seconds: f32, mode: TokenMode) -> GlassMorphMenuFrame {
        if matches!(mode.motion, MotionPreference::Reduced) {
            self.morph.snap_to(if self.target_open { 1.0 } else { 0.0 });
            self.press_scale.snap_to(1.0);
        } else {
            self.morph.step(elapsed_seconds);
            self.press_scale.step(elapsed_seconds);
        }
        if self.morph.is_settled() {
            self.state = if self.target_open {
                MorphMenuState::Open
            } else {
                MorphMenuState::Closed
            };
        }
        self.menu.frame_for(
            mode,
            self.state,
            self.morph.value(),
            self.press_scale.value(),
        )
    }

    /// Build the currently exposed accessibility tree.
    ///
    /// Menu actions enter the tree only after expansion. The trigger remains
    /// present so assistive technology always has a stable close action.
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        let mut children = vec![AccessibilityNode {
            id: SemanticId::new("trigger"),
            role: SemanticRole::Button,
            label: self.menu.trigger_label.clone(),
            value: Some(if self.target_open { "open" } else { "closed" }.to_owned()),
            state: AccessibilityState::default(),
            children: Vec::new(),
        }];
        if self.target_open {
            children.extend(self.menu.items.iter().map(|item| AccessibilityNode {
                id: SemanticId::new(item.id.clone()),
                role: SemanticRole::Button,
                label: item.label.clone(),
                value: None,
                state: AccessibilityState {
                    disabled: !item.enabled,
                    ..AccessibilityState::default()
                },
                children: Vec::new(),
            }));
        }
        AccessibilityNode {
            id: SemanticId::new("morph-menu"),
            role: SemanticRole::Group,
            label: self.menu.label.clone(),
            value: None,
            state: AccessibilityState::default(),
            children,
        }
    }
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn smoothstep(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> GlassMorphMenu {
        GlassMorphMenu::new("Account", "Open account menu")
            .item(GlassMenuItem::new("profile", "My Profile"))
            .item(GlassMenuItem::new("settings", "Settings"))
            .item(GlassMenuItem::new("logout", "Log Out"))
    }

    #[test]
    fn pointer_down_is_immediate_and_release_rebounds() {
        let metrics = MorphMenuMetricSpec::compact();
        let mut controller = menu().controller();
        controller.pointer_down();
        let pressed = controller.tick(0.0, TokenMode::light());
        assert_eq!(pressed.shape.scale, metrics.pressed_scale);

        controller.pointer_up_and_toggle(1.4);
        let mut overshot = false;
        for _ in 0..30 {
            overshot |= controller.tick(1.0 / 60.0, TokenMode::light()).shape.scale > 1.0;
        }
        assert!(overshot);
        assert!(controller.target_open());
    }

    #[test]
    fn rapid_reversal_continues_from_the_live_presentation() {
        let mut controller = menu().controller();
        controller.toggle();
        for _ in 0..5 {
            controller.tick(1.0 / 60.0, TokenMode::light());
        }
        let before = controller.tick(0.0, TokenMode::light()).progress;
        controller.toggle();
        let reversed = controller.tick(0.0, TokenMode::light());
        assert_eq!(reversed.progress, before);
        assert_eq!(reversed.state, MorphMenuState::Closing);
    }

    #[test]
    fn gesture_release_uses_velocity_direction_and_can_overshoot() {
        let mut controller = menu().controller();
        controller.take_over_with_progress(0.2, 1.0);
        controller.settle_from_gesture(1.0);
        assert!(controller.target_open());

        let mut overshot = false;
        for _ in 0..90 {
            overshot |= controller.tick(1.0 / 60.0, TokenMode::light()).progress > 1.0;
        }
        assert!(overshot);
        assert_eq!(controller.state(), MorphMenuState::Open);
    }

    #[test]
    fn reduced_motion_snaps_geometry_and_requests_a_crossfade() {
        let mut controller = menu().controller();
        controller.toggle();
        let frame = controller.tick(0.0, TokenMode::light().reduced_motion());
        assert_eq!(frame.state, MorphMenuState::Open);
        assert_eq!(frame.progress, 1.0);
        assert!(!frame.spatial_motion);
        assert_eq!(frame.motion.duration_ms, 160);
        assert!(frame.motion.spring.is_none());
    }

    #[test]
    fn menu_actions_are_exposed_only_while_open() {
        let mut controller = menu().controller();
        assert_eq!(controller.accessibility_tree().children.len(), 1);
        controller.toggle();
        assert_eq!(controller.accessibility_tree().children.len(), 4);
    }

    #[test]
    fn material_contract_uses_only_named_roles() {
        assert_eq!(
            menu().material_tokens().snapshot(),
            "material=Floating;nesting=Independent;radius=Lg;metric=MorphMenu;motion=Morph"
        );
    }
}
