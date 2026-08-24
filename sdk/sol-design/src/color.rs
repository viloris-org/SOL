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
            Color::Error => Rgba(0.85, 0.25, 0.25, 1.0),
        }
    }
}
