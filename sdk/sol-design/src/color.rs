//! Color tokens.
//!
//! Colors are expressed as RGBA components so they expose zero information
//! about the concrete encoding — the renderer decides how to turn an
//! `Rgba` into pixels. Consumers name a semantic role (`Color::Surface`),
//! never a bare hex/`u32` literal.
//!
//! ## Palette (v0.1 draft)
//!
//! The exact hues are placeholder until a visual design pass, but the *shape*
//! is real: a semantic `Color` enum plus an accessibility-correct token
//! scale. Brightness values are luminance-weighted.

/// Generic RGBA color in linear-ish float components, 0.0–1.0 each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    pub const TRANSPARENT: Self = Self(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Self = Self(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Self = Self(1.0, 1.0, 1.0, 1.0);
}

/// One color stop in a design-owned linear ramp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    /// Normalized position along the ramp.
    pub position: f32,
    /// Resolved stop color.
    pub color: Rgba,
}

/// Reusable semantic color ramps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRamp {
    /// Full hue spectrum for color controls.
    Hue,
}

const HUE_STOPS: [GradientStop; 7] = [
    GradientStop {
        position: 0.0,
        color: Rgba(1.0, 0.0, 0.0, 1.0),
    },
    GradientStop {
        position: 0.167,
        color: Rgba(1.0, 1.0, 0.0, 1.0),
    },
    GradientStop {
        position: 0.333,
        color: Rgba(0.0, 1.0, 0.0, 1.0),
    },
    GradientStop {
        position: 0.5,
        color: Rgba(0.0, 1.0, 1.0, 1.0),
    },
    GradientStop {
        position: 0.667,
        color: Rgba(0.0, 0.0, 1.0, 1.0),
    },
    GradientStop {
        position: 0.833,
        color: Rgba(1.0, 0.0, 1.0, 1.0),
    },
    GradientStop {
        position: 1.0,
        color: Rgba(1.0, 0.0, 0.0, 1.0),
    },
];

impl ColorRamp {
    /// Resolve the ramp to design-owned positions and colors.
    pub const fn stops(self) -> &'static [GradientStop] {
        match self {
            Self::Hue => &HUE_STOPS,
        }
    }
}

/// Semantic surface colors. Components and apps use these by *role*, so a
/// global theme switch (light / dark / high-contrast) re-resolves the same
/// enum without touching component code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Base window / surface fill.
    Surface,
    /// Raised surfaces: menus, popovers, dock, top bar.
    Elevated,
    /// The app/shell accent color for active controls.
    Accent,
    /// Foreground text on `Surface`.
    TextPrimary,
    /// Foreground text and symbols placed on `Accent`.
    TextOnAccent,
    /// Secondary / muted text and metadata.
    TextSecondary,
    /// Fine separators and outlines.
    Border,
    /// Hover / press / focus overlays (translucent).
    HoverOverlay,
    /// Theme-adaptive neutral tint mixed into Liquid Glass.
    MaterialTint,
    /// Specular rim color for Liquid Glass edges.
    MaterialHighlight,
    /// Depth shadow color for translucent material layers.
    MaterialShadow,
    /// Error semantic color.
    Error,
}

impl Color {
    /// Resolve to `Rgba` for light mode.
    pub fn rgba(self) -> Rgba {
        match self {
            Color::Surface => Rgba(0.95, 0.95, 0.96, 1.0),
            Color::Elevated => Rgba(1.0, 1.0, 1.0, 1.0),
            Color::Accent => Rgba(0.40, 0.55, 0.95, 1.0),
            Color::TextPrimary => Rgba(0.12, 0.12, 0.14, 1.0),
            Color::TextOnAccent => Rgba(0.12, 0.12, 0.14, 1.0),
            Color::TextSecondary => Rgba(0.42, 0.42, 0.47, 1.0),
            Color::Border => Rgba(0.80, 0.80, 0.83, 1.0),
            Color::HoverOverlay => Rgba(0.0, 0.0, 0.0, 0.06),
            Color::MaterialTint => Rgba::WHITE,
            Color::MaterialHighlight => Rgba::WHITE,
            Color::MaterialShadow => Rgba::BLACK,
            Color::Error => Rgba(0.85, 0.25, 0.25, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_ramps_are_ordered_and_bounded() {
        let stops = ColorRamp::Hue.stops();
        assert_eq!(stops.first().map(|stop| stop.position), Some(0.0));
        assert_eq!(stops.last().map(|stop| stop.position), Some(1.0));
        for pair in stops.windows(2) {
            assert!(pair[0].position < pair[1].position);
        }
        for stop in stops {
            assert!((0.0..=1.0).contains(&stop.color.0));
            assert!((0.0..=1.0).contains(&stop.color.1));
            assert!((0.0..=1.0).contains(&stop.color.2));
            assert!((0.0..=1.0).contains(&stop.color.3));
        }
    }
}
