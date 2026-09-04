//! Named component geometry tokens.
//!
//! SolUI selects one of these roles instead of embedding a visual metric.

/// Semantic control geometry role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMetric {
    /// The standard button minimum size.
    Button,
    /// The standard editable-field height.
    TextField,
    /// The standard tab height.
    Tab,
    /// The standard toolbar height.
    Toolbar,
    /// A direct Liquid Glass button or circular control.
    GlassControl,
    /// A pill-shaped group of mutually exclusive choices.
    SegmentedControl,
    /// A floating Liquid Glass action bar.
    FloatingToolbar,
    /// A slider with a glass thumb.
    GlassSlider,
    /// An anchored trigger that morphs into a compact menu.
    MorphMenu,
}

/// Resolved logical component dimensions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricSpec {
    /// Minimum logical width.
    pub min_width: f32,
    /// Logical height.
    pub height: f32,
}

impl ControlMetric {
    /// Resolve the named role to its design-controlled geometry.
    pub const fn spec(self) -> MetricSpec {
        match self {
            Self::Button => MetricSpec {
                min_width: 100.0,
                height: 32.0,
            },
            Self::TextField => MetricSpec {
                min_width: 100.0,
                height: 32.0,
            },
            Self::Tab => MetricSpec {
                min_width: 0.0,
                height: 32.0,
            },
            Self::Toolbar => MetricSpec {
                min_width: 0.0,
                height: 40.0,
            },
            Self::GlassControl => MetricSpec {
                min_width: 44.0,
                height: 44.0,
            },
            Self::SegmentedControl => MetricSpec {
                min_width: 120.0,
                height: 52.0,
            },
            Self::FloatingToolbar => MetricSpec {
                min_width: 0.0,
                height: 56.0,
            },
            Self::GlassSlider => MetricSpec {
                min_width: 160.0,
                height: 44.0,
            },
            Self::MorphMenu => MetricSpec {
                min_width: 168.0,
                height: 190.0,
            },
        }
    }
}

/// Design-owned geometry for the compact-trigger-to-menu transformation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MorphMenuMetricSpec {
    /// Diameter of the circular trigger/lobe.
    pub trigger_extent: f32,
    /// Expanded body width.
    pub expanded_width: f32,
    /// Expanded body height below the trigger origin.
    pub expanded_height: f32,
    /// Vertical offset where the body joins the trigger lobe.
    pub body_offset_y: f32,
    /// Expanded panel corner radius.
    pub body_corner_radius: f32,
    /// Smooth-union radius between lobe and body.
    pub union_radius: f32,
    /// Pressed scale used only during direct pointer contact.
    pub pressed_scale: f32,
    /// Morph progress after which menu content can become legible.
    pub content_reveal_start: f32,
    /// Rotation from the compact glyph toward the expanded close glyph.
    pub trigger_rotation_degrees: f32,
}

impl MorphMenuMetricSpec {
    /// Canonical compact Liquid Glass morph geometry.
    pub const fn compact() -> Self {
        Self {
            trigger_extent: 52.0,
            expanded_width: 168.0,
            expanded_height: 190.0,
            body_offset_y: 28.0,
            body_corner_radius: 24.0,
            union_radius: 18.0,
            pressed_scale: 0.97,
            content_reveal_start: 0.35,
            trigger_rotation_degrees: 45.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_metric_has_a_non_negative_geometry() {
        for metric in [
            ControlMetric::Button,
            ControlMetric::TextField,
            ControlMetric::Tab,
            ControlMetric::Toolbar,
            ControlMetric::GlassControl,
            ControlMetric::SegmentedControl,
            ControlMetric::FloatingToolbar,
            ControlMetric::GlassSlider,
            ControlMetric::MorphMenu,
        ] {
            let spec = metric.spec();
            assert!(spec.min_width >= 0.0);
            assert!(spec.height > 0.0);
        }
    }

    #[test]
    fn morph_menu_geometry_preserves_a_tappable_trigger_and_bounded_reveal() {
        let spec = MorphMenuMetricSpec::compact();
        assert!(spec.trigger_extent >= 44.0);
        assert!(spec.expanded_width > spec.trigger_extent);
        assert!(spec.expanded_height > spec.trigger_extent);
        assert!((0.0..=1.0).contains(&spec.content_reveal_start));
        assert!((0.9..1.0).contains(&spec.pressed_scale));
    }
}
