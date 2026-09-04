//! Liquid Glass material and elevation tokens.
//!
//! Components select a semantic [`Material`] role. Only this module owns the
//! concrete optical values; renderers decide how to realize backdrop sampling,
//! blur, saturation and refraction without exposing captured pixels to apps.

/// Surface ordering used to resolve depth shadows.
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowSpec {
    /// Gaussian shadow blur radius in logical pixels.
    pub blur: f32,
    /// Vertical shadow offset in logical pixels.
    pub offset_y: f32,
    /// Normalized shadow opacity.
    pub opacity: f32,
}

impl Elevation {
    /// Resolve the elevation into renderer-neutral shadow parameters.
    pub const fn shadow(self) -> ShadowSpec {
        match self {
            Self::Base => ShadowSpec {
                blur: 0.0,
                offset_y: 0.0,
                opacity: 0.0,
            },
            Self::Panel => ShadowSpec {
                blur: 12.0,
                offset_y: 2.0,
                opacity: 0.18,
            },
            Self::Floating => ShadowSpec {
                blur: 22.0,
                offset_y: 6.0,
                opacity: 0.22,
            },
        }
    }
}

/// Semantic Liquid Glass surface requested by a SolUI component.
///
/// Dense document content remains solid. Glass is reserved for controls,
/// navigation and surfaces whose depth carries functional meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    /// Opaque application content or a root window surface.
    Content,
    /// Persistent navigation bars and application toolbars.
    Chrome,
    /// Sheets and inspectors spanning a substantial area.
    Panel,
    /// Menus, popovers and transient surfaces above the current task.
    Floating,
    /// Small buttons, thumbs and selection indicators over solid content.
    Control,
    /// Persistent navigation beside a solid document or content area.
    Sidebar,
    /// Bottom application dock and its anchored folders or stacks.
    Dock,
    /// Compact live-activity surface in trusted system chrome.
    Capsule,
}

/// Accessibility-aware rendering mode for a material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialMode {
    /// Full adaptive translucency and renderer-owned backdrop response.
    Liquid,
    /// Solid surfaces for users who reduce transparency or weak backends.
    ReducedTransparency,
    /// Solid surfaces with an explicit, high-contrast boundary.
    HighContrast,
}

/// Placement of one material relative to its parent surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaterialNesting {
    /// The surface sits on solid content and may sample one backdrop group.
    #[default]
    Independent,
    /// The surface sits on glass and must share the parent's backdrop group.
    Consolidated,
}

/// Renderer-neutral optical description of a Liquid Glass material.
///
/// These values are a design contract, not permission to read a backdrop.
/// The renderer/compositor owns sampling and must preserve the solid fallback
/// when an effect is unavailable or disallowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSpec {
    /// Whether this surface requests renderer-owned backdrop sampling.
    pub samples_backdrop: bool,
    /// Backdrop blur radius in logical pixels.
    pub backdrop_blur: f32,
    /// Backdrop color saturation multiplier.
    pub saturation: f32,
    /// Opacity of the theme-provided material tint.
    pub tint_opacity: f32,
    /// Opacity of the outer specular rim.
    pub edge_highlight_opacity: f32,
    /// Width of the optical rim in logical pixels.
    pub edge_width: f32,
    /// Opacity of the shallow inner shade opposite the highlight.
    pub inner_shadow_opacity: f32,
    /// Opacity of the depth shadow resolved for this material.
    pub shadow_opacity: f32,
    /// Normalized lens displacement for capable renderers.
    pub refraction: f32,
    /// Normalized RGB fringe at strongly refracted edges.
    pub chromatic_aberration: f32,
    /// Subtle normalized grain that prevents a flat synthetic surface.
    pub grain_opacity: f32,
    /// Whether the material requires an explicit contrast boundary.
    pub explicit_boundary: bool,
}

impl MaterialSpec {
    /// Return whether the plan needs a sampled backdrop.
    pub const fn is_translucent(self) -> bool {
        self.samples_backdrop && self.tint_opacity < 1.0
    }

    /// Consolidate a child surface into an existing glass backdrop group.
    ///
    /// The child keeps a restrained edge and interaction tint, but never
    /// recursively samples, blurs or refracts the already-filtered parent.
    pub const fn consolidated(self) -> Self {
        if !self.samples_backdrop {
            return self;
        }
        Self {
            samples_backdrop: false,
            backdrop_blur: 0.0,
            saturation: 1.0,
            tint_opacity: 0.12,
            edge_highlight_opacity: self.edge_highlight_opacity,
            edge_width: self.edge_width,
            inner_shadow_opacity: self.inner_shadow_opacity,
            shadow_opacity: 0.0,
            refraction: 0.0,
            chromatic_aberration: 0.0,
            grain_opacity: 0.0,
            explicit_boundary: self.explicit_boundary,
        }
    }
}

impl Material {
    /// Resolve the semantic material into optical tokens.
    pub const fn spec(self, mode: MaterialMode) -> MaterialSpec {
        match mode {
            MaterialMode::Liquid => self.liquid_spec(),
            MaterialMode::ReducedTransparency => self.solid_spec(false),
            MaterialMode::HighContrast => self.solid_spec(true),
        }
    }

    /// Resolve a surface and enforce the one-backdrop-group nesting rule.
    pub const fn spec_for(self, mode: MaterialMode, nesting: MaterialNesting) -> MaterialSpec {
        let spec = self.spec(mode);
        match nesting {
            MaterialNesting::Independent => spec,
            MaterialNesting::Consolidated => spec.consolidated(),
        }
    }

    const fn liquid_spec(self) -> MaterialSpec {
        match self {
            Self::Content => Self::solid_content(),
            Self::Chrome => MaterialSpec {
                samples_backdrop: true,
                backdrop_blur: 20.0,
                saturation: 1.35,
                tint_opacity: 0.56,
                edge_highlight_opacity: 0.30,
                edge_width: 1.0,
                inner_shadow_opacity: 0.08,
                shadow_opacity: 0.12,
                refraction: 0.08,
                chromatic_aberration: 0.012,
                grain_opacity: 0.010,
                explicit_boundary: false,
            },
            Self::Panel => MaterialSpec {
                samples_backdrop: true,
                backdrop_blur: 30.0,
                saturation: 1.22,
                tint_opacity: 0.70,
                edge_highlight_opacity: 0.24,
                edge_width: 1.0,
                inner_shadow_opacity: 0.10,
                shadow_opacity: 0.18,
                refraction: 0.05,
                chromatic_aberration: 0.008,
                grain_opacity: 0.012,
                explicit_boundary: false,
            },
            Self::Floating => MaterialSpec {
                samples_backdrop: true,
                backdrop_blur: 24.0,
                saturation: 1.20,
                tint_opacity: 0.76,
                edge_highlight_opacity: 0.28,
                edge_width: 1.0,
                inner_shadow_opacity: 0.12,
                shadow_opacity: 0.24,
                refraction: 0.05,
                chromatic_aberration: 0.010,
                grain_opacity: 0.010,
                explicit_boundary: false,
            },
            Self::Control => MaterialSpec {
                samples_backdrop: true,
                backdrop_blur: 12.0,
                saturation: 1.32,
                tint_opacity: 0.46,
                edge_highlight_opacity: 0.42,
                edge_width: 1.0,
                inner_shadow_opacity: 0.14,
                shadow_opacity: 0.12,
                refraction: 0.14,
                chromatic_aberration: 0.020,
                grain_opacity: 0.008,
                explicit_boundary: false,
            },
            Self::Sidebar => MaterialSpec {
                samples_backdrop: true,
                backdrop_blur: 30.0,
                saturation: 1.18,
                tint_opacity: 0.72,
                edge_highlight_opacity: 0.18,
                edge_width: 1.0,
                inner_shadow_opacity: 0.08,
                shadow_opacity: 0.14,
                refraction: 0.04,
                chromatic_aberration: 0.006,
                grain_opacity: 0.012,
                explicit_boundary: false,
            },
            Self::Dock => MaterialSpec {
                samples_backdrop: true,
                backdrop_blur: 32.0,
                saturation: 1.40,
                tint_opacity: 0.60,
                edge_highlight_opacity: 0.34,
                edge_width: 1.0,
                inner_shadow_opacity: 0.10,
                shadow_opacity: 0.24,
                refraction: 0.08,
                chromatic_aberration: 0.014,
                grain_opacity: 0.012,
                explicit_boundary: false,
            },
            Self::Capsule => MaterialSpec {
                samples_backdrop: true,
                backdrop_blur: 16.0,
                saturation: 1.30,
                tint_opacity: 0.70,
                edge_highlight_opacity: 0.38,
                edge_width: 1.0,
                inner_shadow_opacity: 0.12,
                shadow_opacity: 0.18,
                refraction: 0.10,
                chromatic_aberration: 0.016,
                grain_opacity: 0.008,
                explicit_boundary: false,
            },
        }
    }

    const fn solid_content() -> MaterialSpec {
        MaterialSpec {
            samples_backdrop: false,
            backdrop_blur: 0.0,
            saturation: 1.0,
            tint_opacity: 1.0,
            edge_highlight_opacity: 0.0,
            edge_width: 0.0,
            inner_shadow_opacity: 0.0,
            shadow_opacity: 0.0,
            refraction: 0.0,
            chromatic_aberration: 0.0,
            grain_opacity: 0.0,
            explicit_boundary: false,
        }
    }

    const fn solid_spec(self, high_contrast: bool) -> MaterialSpec {
        let mut spec = Self::solid_content();
        spec.edge_highlight_opacity = if high_contrast { 0.0 } else { 0.10 };
        spec.edge_width = if matches!(self, Self::Content) {
            0.0
        } else {
            1.0
        };
        spec.shadow_opacity = match self {
            Self::Content => 0.0,
            Self::Chrome | Self::Control => 0.10,
            Self::Sidebar => 0.14,
            Self::Panel => 0.16,
            Self::Capsule => 0.18,
            Self::Dock | Self::Floating => 0.22,
        };
        spec.explicit_boundary = high_contrast && !matches!(self, Self::Content);
        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_refract_more_than_large_surfaces() {
        let control = Material::Control.spec(MaterialMode::Liquid);
        let panel = Material::Panel.spec(MaterialMode::Liquid);
        assert!(control.refraction > panel.refraction);
        assert!(control.edge_highlight_opacity > panel.edge_highlight_opacity);
    }

    #[test]
    fn consolidated_children_never_resample_glass() {
        let nested =
            Material::Control.spec_for(MaterialMode::Liquid, MaterialNesting::Consolidated);
        assert!(!nested.samples_backdrop);
        assert_eq!(nested.backdrop_blur, 0.0);
        assert_eq!(nested.refraction, 0.0);
        assert_eq!(nested.chromatic_aberration, 0.0);
    }
}
