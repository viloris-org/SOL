//! Design tokens module.
//!
//! ## Single source of truth
//!
//! **sol-design** is the *only* crate allowed to define concrete visual
//! parameters (colors, spacing, radii, durations, motion curves, …). UI
//! components in `sol-ui` and first-party applications must reference these
//! tokens by named value — never hand-write a bare `#RRGGBB`, `8.0`, or
//! `217ms`. Type-safe wrapper types turn "wrong" usage into a compile error
//! instead of a style drift, so consistency is enforced by the type system
//! rather than by convention.
//!
//! (PRD §19 Design Tokens / §4.1 Consistency First.)
//!
//! This phase ships a *minimal* token set — enough to build the first shell
//! screen and dogfood surfaces. It is intentionally NOT exhaustive:
//! categories and named values are added per-component in Design Review.

pub mod color;
pub mod material;
pub mod motion;
pub mod radius;
pub mod spacing;
pub mod typography;

/// Wallpaper / window-clear fallback background.
/// Temporary stand-in so real v0.1 surfaces have a defined root fill.
pub const DEFAULT_BACKGROUND: color::Rgba = color::Rgba(0.11, 0.10, 0.13, 1.0);

#[cfg(test)]
mod consistency_tests {
    use super::*;

    /// Test that all color variants resolve to valid RGBA values.
    #[test]
    fn color_tokens_resolve_to_valid_rgba() {
        let colors = [
            color::Color::Surface,
            color::Color::Elevated,
            color::Color::Accent,
            color::Color::TextPrimary,
            color::Color::TextSecondary,
            color::Color::Border,
            color::Color::HoverOverlay,
            color::Color::Error,
        ];
        
        for c in colors {
            let rgba = c.rgba();
            assert!(rgba.0 >= 0.0 && rgba.0 <= 1.0);
            assert!(rgba.1 >= 0.0 && rgba.1 <= 1.0);
            assert!(rgba.2 >= 0.0 && rgba.2 <= 1.0);
            assert!(rgba.3 >= 0.0 && rgba.3 <= 1.0);
        }
    }

    /// Test that all spacing values are positive.
    #[test]
    fn spacing_tokens_are_positive() {
        let spacings = [
            spacing::Spacing::Xs,
            spacing::Spacing::Sm,
            spacing::Spacing::Md,
            spacing::Spacing::Lg,
            spacing::Spacing::Xl,
        ];
        
        for s in spacings {
            assert!(s.px() > 0.0);
        }
    }

    /// Test that all radius values are non-negative.
    #[test]
    fn radius_tokens_are_non_negative() {
        let radii = [
            radius::Radius::None,
            radius::Radius::Sm,
            radius::Radius::Md,
            radius::Radius::Full,
        ];
        
        for r in radii {
            assert!(r.px() >= 0.0);
        }
    }

    /// Test that motion specifications are valid.
    #[test]
    fn motion_tokens_have_valid_specs() {
        let motions = [
            motion::Motion::None,
            motion::Motion::Fast,
            motion::Motion::Panel,
            motion::Motion::Window,
            motion::Motion::Workspace,
        ];
        
        for m in motions {
            let spec = m.spec();
            assert!(spec.duration_ms <= 1000);
            if let Some((freq, damp)) = spec.spring {
                assert!(freq > 0.0);
                assert!(damp > 0.0 && damp <= 1.0);
            }
        }
    }

    /// Test that elevation shadows are valid.
    #[test]
    fn elevation_tokens_have_valid_shadows() {
        let elevations = [
            material::Elevation::Base,
            material::Elevation::Panel,
            material::Elevation::Floating,
        ];
        
        for e in elevations {
            let shadow = e.shadow();
            assert!(shadow.blur >= 0.0);
            assert!(shadow.offset_y >= 0.0);
            assert!(shadow.opacity >= 0.0 && shadow.opacity <= 1.0);
        }
    }

    /// Test that typography specs are valid.
    #[test]
    fn typography_tokens_have_valid_specs() {
        let styles = [
            typography::FontStyle::Body,
            typography::FontStyle::Title,
            typography::FontStyle::Label,
            typography::FontStyle::Display,
        ];
        
        for s in styles {
            let spec = s.spec(1.0);
            assert!(spec.pixels > 0.0);
            assert!(spec.weight == typography::FontWeight::Regular ||
                    spec.weight == typography::FontWeight::Medium ||
                    spec.weight == typography::FontWeight::SemiBold ||
                    spec.weight == typography::FontWeight::Bold);
        }
    }
}
