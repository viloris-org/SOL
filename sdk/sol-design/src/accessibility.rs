//! Theme and accessibility-aware token resolution.
//!
//! Components keep semantic roles such as [`Color`] and [`Motion`]. A
//! [`TokenMode`] is the single place that resolves those roles for the active
//! theme and accessibility preferences.

use crate::{
    color::{Color, Rgba},
    material::{Material, MaterialMode, MaterialSpec},
    motion::{Motion, MotionSpec},
    typography::{FontSpec, FontStyle},
};

/// The application's requested color theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Bright surfaces with dark foregrounds.
    #[default]
    Light,
    /// Dark surfaces with light foregrounds.
    Dark,
}

/// Contrast preference selected by the user or platform accessibility API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Contrast {
    /// Normal contrast palette.
    #[default]
    Standard,
    /// Palette with deliberately stronger foreground and boundary contrast.
    High,
}

/// Motion preference selected by the user or platform accessibility API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionPreference {
    /// Use the semantic motion tier's normal timing and spring.
    #[default]
    Full,
    /// Remove non-essential transition time and spring motion.
    Reduced,
}

/// Transparency preference selected by the user or accessibility service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransparencyPreference {
    /// Use adaptive SOL fluid materials where the semantic role permits them.
    #[default]
    Fluid,
    /// Replace translucent materials with solid surfaces.
    Reduced,
}

/// Named text scale preference.
///
/// This avoids allowing application code to set arbitrary font multipliers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextScale {
    /// Design baseline text size.
    #[default]
    Default,
    /// A comfortably larger reading size.
    Large,
    /// The largest standard SOL reading size.
    ExtraLarge,
}

impl TextScale {
    /// Resolve the named preference to a typography scale.
    pub const fn factor(self) -> f32 {
        match self {
            Self::Default => 1.0,
            Self::Large => 1.25,
            Self::ExtraLarge => 1.5,
        }
    }
}

/// The complete design-token mode for one SolUI surface.
///
/// Theme switching and accessibility preferences enter SolUI only through
/// this value. Components retain semantic token roles and never carry their
/// own alternate palette, duration, or text-scale literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenMode {
    /// Requested light or dark appearance.
    pub theme: Theme,
    /// Requested normal or high contrast palette.
    pub contrast: Contrast,
    /// Requested full or reduced motion.
    pub motion: MotionPreference,
    /// Requested fluid or reduced-transparency materials.
    pub transparency: TransparencyPreference,
    /// Requested named text scale.
    pub text_scale: TextScale,
}

impl TokenMode {
    /// Construct standard light tokens.
    pub const fn light() -> Self {
        Self {
            theme: Theme::Light,
            contrast: Contrast::Standard,
            motion: MotionPreference::Full,
            transparency: TransparencyPreference::Fluid,
            text_scale: TextScale::Default,
        }
    }

    /// Construct standard dark tokens.
    pub const fn dark() -> Self {
        Self {
            theme: Theme::Dark,
            ..Self::light()
        }
    }

    /// Turn on high contrast without changing the selected theme.
    pub const fn high_contrast(mut self) -> Self {
        self.contrast = Contrast::High;
        self
    }

    /// Turn on reduced motion without changing other preferences.
    pub const fn reduced_motion(mut self) -> Self {
        self.motion = MotionPreference::Reduced;
        self
    }

    /// Replace translucent materials with solid, non-refractive surfaces.
    pub const fn reduced_transparency(mut self) -> Self {
        self.transparency = TransparencyPreference::Reduced;
        self
    }

    /// Select a named text size without allowing a raw multiplier.
    pub const fn with_text_scale(mut self, text_scale: TextScale) -> Self {
        self.text_scale = text_scale;
        self
    }

    /// Resolve a semantic color role under this theme and contrast mode.
    pub const fn color(self, color: Color) -> Rgba {
        match (self.theme, self.contrast, color) {
            (Theme::Light, Contrast::Standard, Color::Surface) => Rgba(0.95, 0.95, 0.96, 1.0),
            (Theme::Light, Contrast::Standard, Color::Elevated) => Rgba(1.0, 1.0, 1.0, 1.0),
            (Theme::Light, Contrast::Standard, Color::Accent) => Rgba(0.40, 0.55, 0.95, 1.0),
            (Theme::Light, Contrast::Standard, Color::TextPrimary) => Rgba(0.12, 0.12, 0.14, 1.0),
            (Theme::Light, Contrast::Standard, Color::TextSecondary) => Rgba(0.42, 0.42, 0.47, 1.0),
            (Theme::Light, Contrast::Standard, Color::Border) => Rgba(0.80, 0.80, 0.83, 1.0),
            (Theme::Light, Contrast::Standard, Color::HoverOverlay) => Rgba(0.0, 0.0, 0.0, 0.06),
            (Theme::Light, Contrast::Standard, Color::Error) => Rgba(0.85, 0.25, 0.25, 1.0),
            (Theme::Dark, Contrast::Standard, Color::Surface) => Rgba(0.10, 0.11, 0.13, 1.0),
            (Theme::Dark, Contrast::Standard, Color::Elevated) => Rgba(0.16, 0.17, 0.20, 1.0),
            (Theme::Dark, Contrast::Standard, Color::Accent) => Rgba(0.50, 0.66, 1.0, 1.0),
            (Theme::Dark, Contrast::Standard, Color::TextPrimary) => Rgba(0.95, 0.96, 0.98, 1.0),
            (Theme::Dark, Contrast::Standard, Color::TextSecondary) => Rgba(0.70, 0.72, 0.76, 1.0),
            (Theme::Dark, Contrast::Standard, Color::Border) => Rgba(0.34, 0.36, 0.40, 1.0),
            (Theme::Dark, Contrast::Standard, Color::HoverOverlay) => Rgba(1.0, 1.0, 1.0, 0.10),
            (Theme::Dark, Contrast::Standard, Color::Error) => Rgba(1.0, 0.45, 0.45, 1.0),
            (Theme::Light, Contrast::High, Color::Surface) => Rgba::WHITE,
            (Theme::Light, Contrast::High, Color::Elevated) => Rgba::WHITE,
            (Theme::Light, Contrast::High, Color::Accent) => Rgba(0.0, 0.20, 0.75, 1.0),
            (Theme::Light, Contrast::High, Color::TextPrimary) => Rgba::BLACK,
            (Theme::Light, Contrast::High, Color::TextSecondary) => Rgba::BLACK,
            (Theme::Light, Contrast::High, Color::Border) => Rgba::BLACK,
            (Theme::Light, Contrast::High, Color::HoverOverlay) => Rgba(0.0, 0.0, 0.0, 0.18),
            (Theme::Light, Contrast::High, Color::Error) => Rgba(0.70, 0.0, 0.0, 1.0),
            (Theme::Dark, Contrast::High, Color::Surface) => Rgba::BLACK,
            (Theme::Dark, Contrast::High, Color::Elevated) => Rgba::BLACK,
            (Theme::Dark, Contrast::High, Color::Accent) => Rgba(1.0, 0.90, 0.0, 1.0),
            (Theme::Dark, Contrast::High, Color::TextPrimary) => Rgba::WHITE,
            (Theme::Dark, Contrast::High, Color::TextSecondary) => Rgba::WHITE,
            (Theme::Dark, Contrast::High, Color::Border) => Rgba::WHITE,
            (Theme::Dark, Contrast::High, Color::HoverOverlay) => Rgba(1.0, 1.0, 1.0, 0.22),
            (Theme::Dark, Contrast::High, Color::Error) => Rgba(1.0, 0.55, 0.55, 1.0),
        }
    }

    /// Resolve a semantic motion tier under the selected motion preference.
    pub fn motion_spec(self, motion: Motion) -> MotionSpec {
        if matches!(self.motion, MotionPreference::Reduced) {
            MotionSpec {
                duration_ms: 0,
                spring: None,
            }
        } else {
            motion.spec()
        }
    }

    /// Resolve a semantic material under transparency and contrast settings.
    pub const fn material_spec(self, material: Material) -> MaterialSpec {
        let mode = match (self.contrast, self.transparency) {
            (Contrast::High, _) => MaterialMode::HighContrast,
            (_, TransparencyPreference::Reduced) => MaterialMode::ReducedTransparency,
            _ => MaterialMode::Fluid,
        };
        material.spec(mode)
    }

    /// Resolve named typography under the selected text-scale preference.
    pub fn typography(self, style: FontStyle) -> FontSpec {
        style.spec(self.text_scale.factor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_changes_surface_without_changing_component_roles() {
        assert_ne!(
            TokenMode::light().color(Color::Surface),
            TokenMode::dark().color(Color::Surface)
        );
    }

    #[test]
    fn high_contrast_has_strong_foreground_and_boundary_tokens() {
        let mode = TokenMode::dark().high_contrast();
        assert_eq!(mode.color(Color::Surface), Rgba::BLACK);
        assert_eq!(mode.color(Color::TextPrimary), Rgba::WHITE);
        assert_eq!(mode.color(Color::Border), Rgba::WHITE);
    }

    #[test]
    fn reduced_motion_removes_duration_and_spring() {
        let spec = TokenMode::light()
            .reduced_motion()
            .motion_spec(Motion::Workspace);
        assert_eq!(spec.duration_ms, 0);
        assert!(spec.spring.is_none());
    }

    #[test]
    fn named_text_scale_changes_typography_only_through_the_mode() {
        let normal = TokenMode::light().typography(FontStyle::Body);
        let enlarged = TokenMode::light()
            .with_text_scale(TextScale::Large)
            .typography(FontStyle::Body);
        assert!(enlarged.pixels > normal.pixels);
    }

    #[test]
    fn reduced_transparency_removes_blur_and_refraction() {
        let spec = TokenMode::light()
            .reduced_transparency()
            .material_spec(Material::Panel);
        assert_eq!(spec.backdrop_blur, 0.0);
        assert_eq!(spec.refraction, 0.0);
        assert_eq!(spec.tint_opacity, 1.0);
    }

    #[test]
    fn high_contrast_forces_a_solid_bounded_material() {
        let spec = TokenMode::dark()
            .high_contrast()
            .material_spec(Material::Floating);
        assert_eq!(spec.backdrop_blur, 0.0);
        assert!(spec.explicit_boundary);
    }
}
