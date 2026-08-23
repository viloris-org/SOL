//! Material / elevation tokens.
//!
//! Elevation controls shadow + surface ordering so a panel vs a window vs a
//! menu get a consistent depth language. Components request an elevation
//! level; the renderer maps it to blur + shadow + surface-color mixing.
//!
//! [`Material`] defines SOL's fluid-glass material language. Renderers own the
//! actual backdrop sampling/refraction implementation; applications select a
//! semantic role and cannot tune raw blur or opacity values.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Elevation {
    /// Default window surface / app content.
    Base,
    /// Dock, top bar, sidebar — floats above content.
    Panel,
    /// Popover, menu, tooltip — floats above panels.
    Floating,
}

/// Render description resolved by `sol-graphics`.
#[derive(Debug, Clone, Copy)]
pub struct ShadowSpec {
    pub blur: f32,
    pub offset_y: f32,
    pub opacity: f32,
}

impl Elevation {
    pub fn shadow(self) -> ShadowSpec {
        match self {
            Elevation::Base => ShadowSpec {
                blur: 0.0,
                offset_y: 0.0,
                opacity: 0.0,
            },
            Elevation::Panel => ShadowSpec {
                blur: 12.0,
                offset_y: 2.0,
                opacity: 0.18,
            },
            Elevation::Floating => ShadowSpec {
                blur: 22.0,
                offset_y: 6.0,
                opacity: 0.22,
            },
        }
    }
}

/// Semantic surface material requested by a component.
///
/// Glass belongs to system chrome and transient functional layers. App content
/// remains solid by default so text and dense information retain contrast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    /// Opaque application content or a root window surface.
    Content,
    /// Navigation bars, toolbars, and persistent system chrome.
    Chrome,
    /// Sidebars, inspectors, and sheets spanning a substantial area.
    Panel,
    /// Menus, popovers, and other transient surfaces above the current task.
    Floating,
    /// Small controls that sit directly over non-glass content.
    Control,
    /// Persistent application navigation beside solid document/content areas.
    Sidebar,
    /// Bottom application dock and its anchored folders/stacks.
    Dock,
    /// Compact live-activity surface in trusted system chrome.
    Capsule,
}

/// Accessibility-aware rendering mode for a material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialMode {
    /// Full adaptive translucency and renderer-provided backdrop response.
    Fluid,
    /// Near-solid surfaces for users who reduce transparency.
    ReducedTransparency,
    /// Fully solid surfaces with an explicit boundary for high contrast.
    HighContrast,
}

/// Renderer-neutral description of a SOL material.
///
/// Values are design tokens, not a promise that every backend can implement
/// physical refraction immediately. A backend must preserve hierarchy and
/// contrast when it falls back to a simpler composition path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSpec {
    /// Backdrop blur radius in logical pixels.
    pub backdrop_blur: f32,
    /// Backdrop color saturation multiplier.
    pub saturation: f32,
    /// Opacity of the theme-provided material tint.
    pub tint_opacity: f32,
    /// Opacity of the edge/specular highlight.
    pub edge_highlight_opacity: f32,
    /// Opacity of the depth shadow resolved for this material.
    pub shadow_opacity: f32,
    /// Normalized distortion/refraction strength for capable renderers.
    pub refraction: f32,
    /// Subtle normalized grain used to prevent flat, synthetic gradients.
    pub grain_opacity: f32,
    /// Whether the material needs an explicit high-contrast boundary.
    pub explicit_boundary: bool,
}

impl Material {
    /// Resolve the semantic material into renderer-neutral tokens.
    pub const fn spec(self, mode: MaterialMode) -> MaterialSpec {
        match mode {
            MaterialMode::Fluid => self.fluid_spec(),
            MaterialMode::ReducedTransparency => self.solid_spec(false),
            MaterialMode::HighContrast => self.solid_spec(true),
        }
    }

    const fn fluid_spec(self) -> MaterialSpec {
        match self {
            Material::Content => MaterialSpec {
                backdrop_blur: 0.0,
                saturation: 1.0,
                tint_opacity: 1.0,
                edge_highlight_opacity: 0.0,
                shadow_opacity: 0.0,
                refraction: 0.0,
                grain_opacity: 0.0,
                explicit_boundary: false,
            },
            Material::Chrome => MaterialSpec {
                backdrop_blur: 20.0,
                saturation: 1.35,
                tint_opacity: 0.58,
                edge_highlight_opacity: 0.28,
                shadow_opacity: 0.12,
                refraction: 0.08,
                grain_opacity: 0.012,
                explicit_boundary: false,
            },
            Material::Panel => MaterialSpec {
                backdrop_blur: 28.0,
                saturation: 1.25,
                tint_opacity: 0.68,
                edge_highlight_opacity: 0.22,
                shadow_opacity: 0.18,
                refraction: 0.06,
                grain_opacity: 0.014,
                explicit_boundary: false,
            },
            Material::Floating => MaterialSpec {
                backdrop_blur: 24.0,
                saturation: 1.20,
                tint_opacity: 0.78,
                edge_highlight_opacity: 0.25,
                shadow_opacity: 0.24,
                refraction: 0.04,
                grain_opacity: 0.012,
                explicit_boundary: false,
            },
            Material::Control => MaterialSpec {
                backdrop_blur: 12.0,
                saturation: 1.30,
                tint_opacity: 0.52,
                edge_highlight_opacity: 0.32,
                shadow_opacity: 0.10,
                refraction: 0.10,
                grain_opacity: 0.010,
                explicit_boundary: false,
            },
            Material::Sidebar => MaterialSpec {
                backdrop_blur: 28.0,
                saturation: 1.20,
                tint_opacity: 0.72,
                edge_highlight_opacity: 0.18,
                shadow_opacity: 0.14,
                refraction: 0.04,
                grain_opacity: 0.014,
                explicit_boundary: false,
            },
            Material::Dock => MaterialSpec {
                backdrop_blur: 30.0,
                saturation: 1.40,
                tint_opacity: 0.62,
                edge_highlight_opacity: 0.30,
                shadow_opacity: 0.24,
                refraction: 0.07,
                grain_opacity: 0.014,
                explicit_boundary: false,
            },
            Material::Capsule => MaterialSpec {
                backdrop_blur: 16.0,
                saturation: 1.30,
                tint_opacity: 0.74,
                edge_highlight_opacity: 0.34,
                shadow_opacity: 0.18,
                refraction: 0.08,
                grain_opacity: 0.010,
                explicit_boundary: false,
            },
        }
    }

    const fn solid_spec(self, high_contrast: bool) -> MaterialSpec {
        MaterialSpec {
            backdrop_blur: 0.0,
            saturation: 1.0,
            tint_opacity: 1.0,
            edge_highlight_opacity: if high_contrast { 0.0 } else { 0.10 },
            shadow_opacity: match self {
                Material::Content => 0.0,
                Material::Chrome | Material::Control => 0.10,
                Material::Sidebar => 0.14,
                Material::Panel => 0.16,
                Material::Capsule => 0.18,
                Material::Dock => 0.24,
                Material::Floating => 0.22,
            },
            refraction: 0.0,
            grain_opacity: 0.0,
            explicit_boundary: high_contrast,
        }
    }
}
