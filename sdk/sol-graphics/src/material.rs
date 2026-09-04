//! Renderer-facing composition plan for SOL Liquid Glass.
//!
//! This module deliberately produces commands rather than backdrop pixels.
//! Sampling remains backend/compositor-owned, so requesting glass never grants
//! a client screen-capture capability.

use sol_design::{
    accessibility::TokenMode,
    color::{Color, Rgba},
    material::{Material, MaterialNesting, MaterialSpec, ShadowSpec},
};

/// Optical features a renderer can perform within its frame budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialCapabilities {
    /// The backend can sample the pixels already composed behind the surface.
    pub backdrop_sampling: bool,
    /// The backend can blur a renderer-private backdrop texture.
    pub blur: bool,
    /// The backend can adjust saturation while filtering that texture.
    pub saturation: bool,
    /// The backend can displace and split samples near the material rim.
    pub refraction: bool,
    /// The backend can apply a low-cost material grain pass.
    pub grain: bool,
}

impl MaterialCapabilities {
    /// Capable GPU/compositor path.
    pub const fn full() -> Self {
        Self {
            backdrop_sampling: true,
            blur: true,
            saturation: true,
            refraction: true,
            grain: true,
        }
    }

    /// Conservative software/remote-session path.
    pub const fn solid() -> Self {
        Self {
            backdrop_sampling: false,
            blur: false,
            saturation: false,
            refraction: false,
            grain: false,
        }
    }
}

impl Default for MaterialCapabilities {
    fn default() -> Self {
        Self::solid()
    }
}

/// Who may observe a sampled backdrop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackdropAccess {
    /// Pixels stay inside the trusted renderer/compositor pipeline.
    RendererOnly,
}

/// Why a requested optical treatment was simplified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialFallback {
    /// Every requested pass is supported.
    None,
    /// Lower-cost optical details were omitted, preserving blur and tint.
    ReducedEffects,
    /// Backdrop sampling/blur was unavailable, so the material became solid.
    SolidSurface,
}

/// One ordered operation in a backend-independent material render plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MaterialPass {
    /// Capture an internal backdrop group. The texture is never client-visible.
    SampleBackdrop { access: BackdropAccess },
    /// Apply a Gaussian-style blur in logical pixels.
    Blur { radius: f32 },
    /// Adjust backdrop chroma before tinting.
    Saturate { amount: f32 },
    /// Displace the backdrop near curved boundaries.
    Refract {
        strength: f32,
        chromatic_aberration: f32,
    },
    /// Mix a theme-adaptive tint over the filtered backdrop.
    Tint { color: Rgba, opacity: f32 },
    /// Add subtle texture after tinting.
    Grain { opacity: f32 },
    /// Shade the inside edge away from the virtual light source.
    InnerShadow { color: Rgba, opacity: f32 },
    /// Draw the bright optical rim.
    EdgeHighlight {
        color: Rgba,
        opacity: f32,
        width: f32,
    },
    /// Separate this functional layer from content below it.
    DropShadow { color: Rgba, spec: ShadowSpec },
    /// Draw a deterministic boundary for high-contrast mode.
    Boundary { color: Rgba, width: f32 },
}

/// Ordered Liquid Glass composition commands ready for a graphics backend.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialRenderPlan {
    /// Semantic role that produced this plan.
    pub material: Material,
    /// Accessibility and nesting-resolved material values.
    pub spec: MaterialSpec,
    /// Ordered passes; backends must preserve this order.
    pub passes: Vec<MaterialPass>,
    /// Whether and why the renderer simplified the requested appearance.
    pub fallback: MaterialFallback,
}

/// Resolve a semantic material into a secure, capability-aware render plan.
///
/// Missing refraction, grain or saturation removes only those embellishments.
/// Missing either sampling or blur switches the entire backdrop-dependent
/// portion to the material's solid accessibility fallback, preserving
/// legibility and hierarchy.
pub fn plan_material(
    material: Material,
    nesting: MaterialNesting,
    mode: TokenMode,
    capabilities: MaterialCapabilities,
) -> MaterialRenderPlan {
    let requested = mode.material_spec_for(material, nesting);
    let missing_foundation =
        requested.samples_backdrop && (!capabilities.backdrop_sampling || !capabilities.blur);
    let spec = if missing_foundation {
        mode.reduced_transparency()
            .material_spec_for(material, nesting)
    } else {
        requested
    };

    let tint = mode.color(Color::MaterialTint);
    let highlight = mode.color(Color::MaterialHighlight);
    let shadow = mode.color(Color::MaterialShadow);
    let boundary = mode.color(Color::Border);
    let mut passes = Vec::new();

    if spec.samples_backdrop {
        passes.push(MaterialPass::SampleBackdrop {
            access: BackdropAccess::RendererOnly,
        });
        if spec.backdrop_blur > 0.0 {
            passes.push(MaterialPass::Blur {
                radius: spec.backdrop_blur,
            });
        }
        if capabilities.saturation && spec.saturation > 1.0 {
            passes.push(MaterialPass::Saturate {
                amount: spec.saturation,
            });
        }
        if capabilities.refraction && spec.refraction > 0.0 {
            passes.push(MaterialPass::Refract {
                strength: spec.refraction,
                chromatic_aberration: spec.chromatic_aberration,
            });
        }
    }

    passes.push(MaterialPass::Tint {
        color: tint,
        opacity: spec.tint_opacity,
    });
    if capabilities.grain && spec.grain_opacity > 0.0 {
        passes.push(MaterialPass::Grain {
            opacity: spec.grain_opacity,
        });
    }
    if spec.inner_shadow_opacity > 0.0 {
        passes.push(MaterialPass::InnerShadow {
            color: shadow,
            opacity: spec.inner_shadow_opacity,
        });
    }
    if spec.edge_highlight_opacity > 0.0 {
        passes.push(MaterialPass::EdgeHighlight {
            color: highlight,
            opacity: spec.edge_highlight_opacity,
            width: spec.edge_width,
        });
    }
    if spec.shadow_opacity > 0.0 {
        let mut shadow_spec = material_elevation(material).shadow();
        shadow_spec.opacity = spec.shadow_opacity;
        passes.push(MaterialPass::DropShadow {
            color: shadow,
            spec: shadow_spec,
        });
    }
    if spec.explicit_boundary {
        passes.push(MaterialPass::Boundary {
            color: boundary,
            width: spec.edge_width,
        });
    }

    let omitted_detail = requested.samples_backdrop
        && ((!capabilities.saturation && requested.saturation > 1.0)
            || (!capabilities.refraction && requested.refraction > 0.0)
            || (!capabilities.grain && requested.grain_opacity > 0.0));
    let fallback = if missing_foundation {
        MaterialFallback::SolidSurface
    } else if omitted_detail {
        MaterialFallback::ReducedEffects
    } else {
        MaterialFallback::None
    };

    MaterialRenderPlan {
        material,
        spec,
        passes,
        fallback,
    }
}

const fn material_elevation(material: Material) -> sol_design::material::Elevation {
    use sol_design::material::Elevation;
    match material {
        Material::Content => Elevation::Base,
        Material::Chrome | Material::Control | Material::Sidebar => Elevation::Panel,
        Material::Panel | Material::Floating | Material::Dock | Material::Capsule => {
            Elevation::Floating
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_plan_keeps_backdrop_private_and_orders_tint_after_refraction() {
        let plan = plan_material(
            Material::Control,
            MaterialNesting::Independent,
            TokenMode::light(),
            MaterialCapabilities::full(),
        );
        assert_eq!(plan.fallback, MaterialFallback::None);
        assert_eq!(
            plan.passes.first(),
            Some(&MaterialPass::SampleBackdrop {
                access: BackdropAccess::RendererOnly,
            })
        );
        let refraction = plan
            .passes
            .iter()
            .position(|pass| matches!(pass, MaterialPass::Refract { .. }))
            .expect("control glass should refract");
        let tint = plan
            .passes
            .iter()
            .position(|pass| matches!(pass, MaterialPass::Tint { .. }))
            .expect("every material should tint");
        assert!(refraction < tint);
    }

    #[test]
    fn software_path_uses_a_solid_surface_without_sampling() {
        let plan = plan_material(
            Material::Panel,
            MaterialNesting::Independent,
            TokenMode::dark(),
            MaterialCapabilities::solid(),
        );
        assert_eq!(plan.fallback, MaterialFallback::SolidSurface);
        assert!(!plan.spec.samples_backdrop);
        assert!(
            plan.passes
                .iter()
                .all(|pass| !matches!(pass, MaterialPass::SampleBackdrop { .. }))
        );
    }

    #[test]
    fn nested_control_does_not_create_a_second_backdrop_group() {
        let plan = plan_material(
            Material::Control,
            MaterialNesting::Consolidated,
            TokenMode::light(),
            MaterialCapabilities::full(),
        );
        assert!(!plan.spec.samples_backdrop);
        assert!(
            plan.passes
                .iter()
                .all(|pass| !matches!(pass, MaterialPass::SampleBackdrop { .. }))
        );
    }

    #[test]
    fn high_contrast_plan_is_solid_and_bounded() {
        let plan = plan_material(
            Material::Floating,
            MaterialNesting::Independent,
            TokenMode::dark().high_contrast(),
            MaterialCapabilities::full(),
        );
        assert!(!plan.spec.samples_backdrop);
        assert!(
            plan.passes
                .iter()
                .any(|pass| matches!(pass, MaterialPass::Boundary { .. }))
        );
    }
}
