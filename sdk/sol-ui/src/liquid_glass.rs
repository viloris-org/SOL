//! Semantic Liquid Glass components.
//!
//! Applications choose component intent and content. Optical values are always
//! resolved from `sol-design`; no public constructor accepts raw blur, tint,
//! refraction, radius, shadow or animation parameters.

use sol_design::{
    accessibility::TokenMode,
    color::{Color, ColorRamp, GradientStop, Rgba},
    material::{Elevation, Material, MaterialNesting, MaterialSpec, ShadowSpec},
    metrics::{ControlMetric, MetricSpec},
    motion::{Motion, MotionSpec},
    radius::Radius,
    spacing::Spacing,
    typography::FontStyle,
};

use crate::ButtonState;

/// The resolved renderer-neutral appearance of one Liquid Glass surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquidGlassFrame {
    /// Semantic material role retained for renderer diagnostics.
    pub material: Material,
    /// Whether this surface owns or joins a backdrop group.
    pub nesting: MaterialNesting,
    /// Accessibility-aware optical parameters.
    pub spec: MaterialSpec,
    /// Theme-adaptive glass tint.
    pub tint: Rgba,
    /// Specular edge color.
    pub highlight: Rgba,
    /// Inner/depth shadow color.
    pub shadow_color: Rgba,
    /// High-contrast fallback boundary color.
    pub boundary: Rgba,
    /// Token-resolved corner radius.
    pub corner_radius: f32,
    /// Token-resolved depth shadow geometry.
    pub shadow: ShadowSpec,
    /// Accessibility-aware materialization motion.
    pub motion: MotionSpec,
}

/// Reusable semantic surface for bars, panels, overlays and controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiquidGlassSurface {
    material: Material,
    nesting: MaterialNesting,
    radius: Radius,
    elevation: Elevation,
}

impl LiquidGlassSurface {
    /// Construct a surface using the canonical geometry for its material role.
    pub const fn new(material: Material) -> Self {
        let (radius, elevation) = match material {
            Material::Content => (Radius::None, Elevation::Base),
            Material::Chrome => (Radius::Full, Elevation::Panel),
            Material::Panel => (Radius::Xl, Elevation::Floating),
            Material::Floating => (Radius::Lg, Elevation::Floating),
            Material::Control => (Radius::Full, Elevation::Panel),
            Material::Sidebar => (Radius::Lg, Elevation::Panel),
            Material::Dock => (Radius::Lg, Elevation::Floating),
            Material::Capsule => (Radius::Full, Elevation::Floating),
        };
        Self {
            material,
            nesting: MaterialNesting::Independent,
            radius,
            elevation,
        }
    }

    /// Join the nearest parent glass backdrop group instead of stacking blur.
    pub const fn consolidated(mut self) -> Self {
        self.nesting = MaterialNesting::Consolidated;
        self
    }

    /// Return the semantic material role.
    pub const fn material(self) -> Material {
        self.material
    }

    /// Return the one-backdrop-group placement contract.
    pub const fn nesting(self) -> MaterialNesting {
        self.nesting
    }

    /// Return the semantic radius token.
    pub const fn radius(self) -> Radius {
        self.radius
    }

    /// Resolve this surface at the semantic-to-renderer boundary.
    pub fn frame_for(self, mode: TokenMode) -> LiquidGlassFrame {
        let spec = mode.material_spec_for(self.material, self.nesting);
        let mut shadow = self.elevation.shadow();
        shadow.opacity = spec.shadow_opacity;
        LiquidGlassFrame {
            material: self.material,
            nesting: self.nesting,
            spec,
            tint: mode.color(Color::MaterialTint),
            highlight: mode.color(Color::MaterialHighlight),
            shadow_color: mode.color(Color::MaterialShadow),
            boundary: mode.color(Color::Border),
            corner_radius: self.radius.px(),
            shadow,
            motion: mode.motion_spec(Motion::Material),
        }
    }
}

/// Role-only visual contract used by golden/token tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialTokenContract {
    /// Semantic material instead of concrete optical parameters.
    pub material: Material,
    /// Whether a component owns or joins a backdrop group.
    pub nesting: MaterialNesting,
    /// Named corner geometry.
    pub radius: Radius,
    /// Named component geometry.
    pub metric: ControlMetric,
    /// Named materialization motion.
    pub motion: Motion,
}

impl MaterialTokenContract {
    /// Produce a stable token-name-only snapshot.
    pub fn snapshot(self) -> String {
        format!(
            "material={:?};nesting={:?};radius={:?};metric={:?};motion={:?}",
            self.material, self.nesting, self.radius, self.metric, self.motion,
        )
    }
}

/// Components whose material appearance comes only from design-token roles.
pub trait MaterializedComponent {
    /// Return the full role-only material contract.
    fn material_tokens(&self) -> MaterialTokenContract;
}

/// A pill or circular direct-action control made from Liquid Glass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlassButton {
    /// User-visible or icon-equivalent accessible label.
    pub label: String,
    /// Whether the button accepts input.
    pub enabled: bool,
    /// Immediate hover/press feedback state.
    pub state: ButtonState,
    surface: LiquidGlassSurface,
}

/// Resolved visual projection for a [`GlassButton`].
#[derive(Debug, Clone, PartialEq)]
pub struct GlassButtonFrame {
    /// User-visible label.
    pub label: String,
    /// Resolved surface appearance.
    pub surface: LiquidGlassFrame,
    /// Theme-resolved content color.
    pub foreground: Rgba,
    /// Token-resolved minimum geometry.
    pub metric: MetricSpec,
    /// Token-resolved content padding.
    pub padding: f32,
    /// Token-resolved label size.
    pub font_size: f32,
    /// Immediate interaction state.
    pub state: ButtonState,
    /// Whether activation is available.
    pub enabled: bool,
}

impl GlassButton {
    /// Create a standalone glass action.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            state: ButtonState::Normal,
            surface: LiquidGlassSurface::new(Material::Control),
        }
    }

    /// Consolidate the button when it is placed inside another glass surface.
    pub const fn consolidated(mut self) -> Self {
        self.surface = self.surface.consolidated();
        self
    }

    /// Enable or disable activation.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        if !enabled {
            self.state = ButtonState::Disabled;
        }
        self
    }

    /// Update immediate pointer feedback.
    pub const fn state(mut self, state: ButtonState) -> Self {
        self.state = state;
        self
    }

    /// Resolve all design tokens for the active surface mode.
    pub fn frame_for(&self, mode: TokenMode) -> GlassButtonFrame {
        GlassButtonFrame {
            label: self.label.clone(),
            surface: self.surface.frame_for(mode),
            foreground: mode.color(if self.enabled {
                Color::TextPrimary
            } else {
                Color::TextSecondary
            }),
            metric: ControlMetric::GlassControl.spec(),
            padding: Spacing::Md.px(),
            font_size: mode.typography(FontStyle::Label).pixels,
            state: self.state,
            enabled: self.enabled,
        }
    }
}

impl MaterializedComponent for GlassButton {
    fn material_tokens(&self) -> MaterialTokenContract {
        MaterialTokenContract {
            material: Material::Control,
            nesting: self.surface.nesting(),
            radius: Radius::Full,
            metric: ControlMetric::GlassControl,
            motion: Motion::Material,
        }
    }
}

/// One mutually exclusive choice in a glass segmented control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlassSegment {
    /// Stable application-local identifier.
    pub id: String,
    /// User-visible label.
    pub label: String,
    /// Whether this choice accepts selection.
    pub enabled: bool,
}

impl GlassSegment {
    /// Create an enabled segment.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }

    /// Disable this choice while keeping it visible.
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Resolved visual state for one segment.
#[derive(Debug, Clone, PartialEq)]
pub struct GlassSegmentFrame {
    /// Stable application-local identifier.
    pub id: String,
    /// User-visible label.
    pub label: String,
    /// Whether this segment is selected.
    pub selected: bool,
    /// Whether this segment accepts selection.
    pub enabled: bool,
    /// Theme-resolved label color.
    pub foreground: Rgba,
    /// Selected indicator; absent for unselected segments.
    pub selection_surface: Option<LiquidGlassFrame>,
}

/// Complete renderer-neutral frame for a segmented selector.
#[derive(Debug, Clone, PartialEq)]
pub struct GlassSegmentedControlFrame {
    /// Accessible group label.
    pub label: String,
    /// Shared outer material.
    pub surface: LiquidGlassFrame,
    /// Ordered segment frames.
    pub segments: Vec<GlassSegmentFrame>,
    /// Token-resolved group geometry.
    pub metric: MetricSpec,
    /// Token-resolved spacing between choices.
    pub spacing: f32,
    /// Token-resolved label size.
    pub font_size: f32,
}

/// A pill-shaped, mutually exclusive Liquid Glass selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlassSegmentedControl {
    /// Accessible group label.
    pub label: String,
    /// Ordered choices.
    pub segments: Vec<GlassSegment>,
    /// Selected item, if the group contains an enabled segment.
    pub selected_index: Option<usize>,
    surface: LiquidGlassSurface,
}

impl GlassSegmentedControl {
    /// Create an empty selector.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            segments: Vec::new(),
            selected_index: None,
            surface: LiquidGlassSurface::new(Material::Chrome),
        }
    }

    /// Append a choice. The first enabled choice becomes selected.
    pub fn segment(mut self, segment: GlassSegment) -> Self {
        if self.selected_index.is_none() && segment.enabled {
            self.selected_index = Some(self.segments.len());
        }
        self.segments.push(segment);
        self
    }

    /// Select an enabled choice by stable ID. Returns whether selection changed.
    pub fn select(&mut self, id: &str) -> bool {
        let Some(index) = self
            .segments
            .iter()
            .position(|segment| segment.id == id && segment.enabled)
        else {
            return false;
        };
        let changed = self.selected_index != Some(index);
        self.selected_index = Some(index);
        changed
    }

    /// Select the previous enabled choice, wrapping at the edge.
    pub fn select_previous(&mut self) -> Option<&str> {
        self.select_adjacent(true)
    }

    /// Select the next enabled choice, wrapping at the edge.
    pub fn select_next(&mut self) -> Option<&str> {
        self.select_adjacent(false)
    }

    /// Return the currently selected stable ID.
    pub fn selected_id(&self) -> Option<&str> {
        self.selected_index
            .and_then(|index| self.segments.get(index))
            .map(|segment| segment.id.as_str())
    }

    /// Resolve the group surface.
    pub fn surface_frame_for(&self, mode: TokenMode) -> LiquidGlassFrame {
        self.surface.frame_for(mode)
    }

    /// Resolve each segment and its consolidated selected indicator.
    pub fn segment_frames_for(&self, mode: TokenMode) -> Vec<GlassSegmentFrame> {
        self.segments
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                let selected = self.selected_index == Some(index);
                GlassSegmentFrame {
                    id: segment.id.clone(),
                    label: segment.label.clone(),
                    selected,
                    enabled: segment.enabled,
                    foreground: mode.color(if !segment.enabled {
                        Color::TextSecondary
                    } else if selected {
                        Color::Accent
                    } else {
                        Color::TextPrimary
                    }),
                    selection_surface: selected.then(|| {
                        LiquidGlassSurface::new(Material::Control)
                            .consolidated()
                            .frame_for(mode)
                    }),
                }
            })
            .collect()
    }

    /// Resolve the complete selector tree in one renderer-neutral frame.
    pub fn frame_for(&self, mode: TokenMode) -> GlassSegmentedControlFrame {
        GlassSegmentedControlFrame {
            label: self.label.clone(),
            surface: self.surface_frame_for(mode),
            segments: self.segment_frames_for(mode),
            metric: ControlMetric::SegmentedControl.spec(),
            spacing: Spacing::Xs.px(),
            font_size: mode.typography(FontStyle::Label).pixels,
        }
    }

    fn select_adjacent(&mut self, reverse: bool) -> Option<&str> {
        let enabled: Vec<usize> = self
            .segments
            .iter()
            .enumerate()
            .filter_map(|(index, segment)| segment.enabled.then_some(index))
            .collect();
        if enabled.is_empty() {
            self.selected_index = None;
            return None;
        }
        let position = self
            .selected_index
            .and_then(|current| enabled.iter().position(|index| *index == current))
            .unwrap_or(0);
        let next = if reverse {
            (position + enabled.len() - 1) % enabled.len()
        } else {
            (position + 1) % enabled.len()
        };
        self.selected_index = Some(enabled[next]);
        self.selected_id()
    }
}

impl MaterializedComponent for GlassSegmentedControl {
    fn material_tokens(&self) -> MaterialTokenContract {
        MaterialTokenContract {
            material: Material::Chrome,
            nesting: MaterialNesting::Independent,
            radius: Radius::Full,
            metric: ControlMetric::SegmentedControl,
            motion: Motion::Material,
        }
    }
}

/// An item hosted by a [`GlassToolbar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlassToolbarItem {
    /// Direct semantic action.
    Button(GlassButton),
    /// Visual separator supplied by the component renderer.
    Separator,
    /// Flexible gap between action groups.
    FlexibleSpace,
}

/// One resolved item in a floating toolbar.
#[derive(Debug, Clone, PartialEq)]
pub enum GlassToolbarItemFrame {
    /// Consolidated button frame.
    Button(Box<GlassButtonFrame>),
    /// Visual separator supplied by the renderer.
    Separator,
    /// Flexible gap between action groups.
    FlexibleSpace,
}

/// Complete renderer-neutral frame for a floating toolbar.
#[derive(Debug, Clone, PartialEq)]
pub struct GlassToolbarFrame {
    /// Shared outer material.
    pub surface: LiquidGlassFrame,
    /// Ordered, resolved children.
    pub items: Vec<GlassToolbarItemFrame>,
    /// Token-resolved toolbar geometry.
    pub metric: MetricSpec,
    /// Token-resolved item spacing.
    pub spacing: f32,
}

/// Floating toolbar matching the persistent glass navigation pattern.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlassToolbar {
    /// Ordered toolbar items.
    pub items: Vec<GlassToolbarItem>,
}

impl GlassToolbar {
    /// Create an empty toolbar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a toolbar item, consolidating nested glass buttons at render time.
    pub fn item(mut self, item: GlassToolbarItem) -> Self {
        self.items.push(item);
        self
    }

    /// Resolve the toolbar's shared backdrop surface.
    pub fn surface_frame_for(&self, mode: TokenMode) -> LiquidGlassFrame {
        LiquidGlassSurface::new(Material::Chrome).frame_for(mode)
    }

    /// Resolve one contained button without creating nested backdrop blur.
    pub fn button_frame_for(&self, button: &GlassButton, mode: TokenMode) -> GlassButtonFrame {
        button.clone().consolidated().frame_for(mode)
    }

    /// Resolve the toolbar and every child without nested backdrop sampling.
    pub fn frame_for(&self, mode: TokenMode) -> GlassToolbarFrame {
        GlassToolbarFrame {
            surface: self.surface_frame_for(mode),
            items: self
                .items
                .iter()
                .map(|item| match item {
                    GlassToolbarItem::Button(button) => {
                        GlassToolbarItemFrame::Button(Box::new(self.button_frame_for(button, mode)))
                    }
                    GlassToolbarItem::Separator => GlassToolbarItemFrame::Separator,
                    GlassToolbarItem::FlexibleSpace => GlassToolbarItemFrame::FlexibleSpace,
                })
                .collect(),
            metric: ControlMetric::FloatingToolbar.spec(),
            spacing: Spacing::Sm.px(),
        }
    }
}

impl MaterializedComponent for GlassToolbar {
    fn material_tokens(&self) -> MaterialTokenContract {
        MaterialTokenContract {
            material: Material::Chrome,
            nesting: MaterialNesting::Independent,
            radius: Radius::Full,
            metric: ControlMetric::FloatingToolbar,
            motion: Motion::Material,
        }
    }
}

/// Slider with a solid/accent track and a refractive glass thumb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlassSlider {
    /// Accessible label.
    pub label: String,
    /// Current percentage in the inclusive `0..=100` range.
    pub value: u8,
    /// Keyboard adjustment step in percentage points.
    pub step: u8,
    /// Whether the value accepts input.
    pub enabled: bool,
    /// Semantic track treatment.
    pub track: GlassSliderTrack,
}

/// Semantic slider track treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlassSliderTrack {
    /// Standard theme accent progress over a neutral remainder.
    #[default]
    Accent,
    /// Full design-owned hue spectrum used by color controls.
    Hue,
}

/// Resolved slider track fill.
#[derive(Debug, Clone, PartialEq)]
pub enum GlassSliderTrackFrame {
    /// Theme accent progress over the supplied neutral remainder.
    Accent { active: Rgba, inactive: Rgba },
    /// Design-owned linear color ramp.
    Ramp(&'static [GradientStop]),
}

/// Resolved renderer-neutral slider appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct GlassSliderFrame {
    /// Accessible label.
    pub label: String,
    /// Current normalized progress.
    pub progress: f32,
    /// Token-resolved track treatment.
    pub track: GlassSliderTrackFrame,
    /// Refractive thumb surface.
    pub thumb: LiquidGlassFrame,
    /// Token-resolved component geometry.
    pub metric: MetricSpec,
    /// Whether adjustment is available.
    pub enabled: bool,
}

impl GlassSlider {
    /// Create a slider with a clamped initial percentage.
    pub fn new(label: impl Into<String>, value: u8) -> Self {
        Self {
            label: label.into(),
            value: value.min(100),
            step: 5,
            enabled: true,
            track: GlassSliderTrack::Accent,
        }
    }

    /// Configure a non-zero keyboard adjustment step.
    pub fn with_step(mut self, step: u8) -> Self {
        self.step = step.clamp(1, 100);
        self
    }

    /// Enable or disable adjustment.
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Use the design-owned full hue ramp shown by color controls.
    pub const fn hue_track(mut self) -> Self {
        self.track = GlassSliderTrack::Hue;
        self
    }

    /// Set and clamp the current percentage.
    pub fn set_value(&mut self, value: u8) {
        self.value = value.min(100);
    }

    /// Move one keyboard step toward zero.
    pub fn decrement(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        let next = self.value.saturating_sub(self.step);
        let changed = next != self.value;
        self.value = next;
        changed
    }

    /// Move one keyboard step toward one hundred.
    pub fn increment(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        let next = self.value.saturating_add(self.step).min(100);
        let changed = next != self.value;
        self.value = next;
        changed
    }

    /// Resolve track, thumb and geometry tokens for the active surface mode.
    pub fn frame_for(&self, mode: TokenMode) -> GlassSliderFrame {
        GlassSliderFrame {
            label: self.label.clone(),
            progress: f32::from(self.value) / 100.0,
            track: match self.track {
                GlassSliderTrack::Accent => GlassSliderTrackFrame::Accent {
                    active: mode.color(Color::Accent),
                    inactive: mode.color(Color::Border),
                },
                GlassSliderTrack::Hue => GlassSliderTrackFrame::Ramp(ColorRamp::Hue.stops()),
            },
            thumb: LiquidGlassSurface::new(Material::Control).frame_for(mode),
            metric: ControlMetric::GlassSlider.spec(),
            enabled: self.enabled,
        }
    }
}

impl MaterializedComponent for GlassSlider {
    fn material_tokens(&self) -> MaterialTokenContract {
        MaterialTokenContract {
            material: Material::Control,
            nesting: MaterialNesting::Independent,
            radius: Radius::Full,
            metric: ControlMetric::GlassSlider,
            motion: Motion::Material,
        }
    }
}

/// Any complete Liquid Glass component frame accepted by a renderer adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum GlassComponentFrame {
    /// Direct glass action.
    Button(GlassButtonFrame),
    /// Mutually exclusive pill selector.
    SegmentedControl(GlassSegmentedControlFrame),
    /// Floating navigation/action bar.
    Toolbar(GlassToolbarFrame),
    /// Bounded adjustable value.
    Slider(GlassSliderFrame),
    /// Trigger-anchored shared-container transformation.
    MorphMenu(Box<crate::GlassMorphMenuFrame>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segmented_control_selects_enabled_choices_and_wraps() {
        let mut control = GlassSegmentedControl::new("Capture type")
            .segment(GlassSegment::new("video", "Video"))
            .segment(GlassSegment::new("disabled", "Disabled").disabled())
            .segment(GlassSegment::new("photo", "Photo"));
        assert_eq!(control.selected_id(), Some("video"));
        assert_eq!(control.select_next(), Some("photo"));
        assert_eq!(control.select_next(), Some("video"));
        assert!(!control.select("disabled"));
    }

    #[test]
    fn selected_segment_consolidates_its_glass() {
        let control = GlassSegmentedControl::new("Mode")
            .segment(GlassSegment::new("video", "Video"))
            .segment(GlassSegment::new("photo", "Photo"));
        let frames = control.segment_frames_for(TokenMode::light());
        let selected = frames[0]
            .selection_surface
            .expect("first enabled segment should be selected");
        assert_eq!(selected.nesting, MaterialNesting::Consolidated);
        assert!(!selected.spec.samples_backdrop);
    }

    #[test]
    fn reduced_transparency_keeps_component_geometry() {
        let button = GlassButton::new("Play");
        let liquid = button.frame_for(TokenMode::light());
        let reduced = button.frame_for(TokenMode::light().reduced_transparency());
        assert_eq!(liquid.metric, reduced.metric);
        assert!(liquid.surface.spec.samples_backdrop);
        assert!(!reduced.surface.spec.samples_backdrop);
    }

    #[test]
    fn toolbar_buttons_share_the_toolbar_backdrop() {
        let toolbar = GlassToolbar::new();
        let frame = toolbar.button_frame_for(&GlassButton::new("Previous"), TokenMode::light());
        assert_eq!(frame.surface.nesting, MaterialNesting::Consolidated);
        assert!(!frame.surface.spec.samples_backdrop);
    }

    #[test]
    fn complete_toolbar_frame_resolves_every_child() {
        let toolbar = GlassToolbar::new()
            .item(GlassToolbarItem::Button(GlassButton::new("Previous")))
            .item(GlassToolbarItem::Separator)
            .item(GlassToolbarItem::Button(GlassButton::new("Play")));
        let frame = toolbar.frame_for(TokenMode::dark());
        assert_eq!(frame.items.len(), 3);
        assert!(matches!(
            &frame.items[0],
            GlassToolbarItemFrame::Button(button) if !button.surface.spec.samples_backdrop
        ));
    }

    #[test]
    fn slider_clamps_and_respects_disabled_state() {
        let mut slider = GlassSlider::new("Hue", 250).with_step(20);
        assert_eq!(slider.value, 100);
        assert!(!slider.increment());
        assert!(slider.decrement());
        assert_eq!(slider.value, 80);
        slider.enabled = false;
        assert!(!slider.decrement());
    }

    #[test]
    fn hue_slider_uses_the_design_owned_spectrum() {
        let frame = GlassSlider::new("Hue", 50)
            .hue_track()
            .frame_for(TokenMode::light());
        assert!(matches!(
            frame.track,
            GlassSliderTrackFrame::Ramp(stops) if stops.len() == 7
        ));
    }

    #[test]
    fn component_snapshot_contains_only_semantic_tokens() {
        assert_eq!(
            GlassSegmentedControl::new("Mode")
                .material_tokens()
                .snapshot(),
            "material=Chrome;nesting=Independent;radius=Full;metric=SegmentedControl;motion=Material"
        );
    }
}
