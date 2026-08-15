//! Typography tokens.
//!
//! Named styles instead of bare point sizes. Rendering maps these to a
//! concrete family/size/weight at draw time so type stays consistent across
//! the shell and first-party apps.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Regular,
    Medium,
    SemiBold,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    /// Body / UI copy.
    Body,
    /// Panel & window titles.
    Title,
    /// Dock / top-bar labels.
    Label,
    /// Launcher / overview search.
    Display,
}

/// Font metrics for a given style at the given output scale.
#[derive(Debug, Clone, Copy)]
pub struct FontSpec {
    pub style: FontStyle,
    pub pixels: f32,
    pub weight: FontWeight,
}

impl FontStyle {
    pub fn spec(self, scale: f32) -> FontSpec {
        let (px, weight) = match self {
            FontStyle::Body => (14.0, FontWeight::Regular),
            FontStyle::Title => (17.0, FontWeight::SemiBold),
            FontStyle::Label => (13.0, FontWeight::Medium),
            FontStyle::Display => (22.0, FontWeight::Bold),
        };
        FontSpec {
            style: self,
            pixels: px * scale,
            weight,
        }
    }
}
