use sol_design::{
    color::Color,
    material::{Material, MaterialMode, MaterialNesting},
    motion::Motion,
    spacing::Spacing,
};

#[test]
fn spacing_scale_is_monotonic() {
    assert!(Spacing::Xs.px() < Spacing::Sm.px());
    assert!(Spacing::Sm.px() < Spacing::Md.px());
    assert!(Spacing::Md.px() < Spacing::Lg.px());
    assert!(Spacing::Lg.px() < Spacing::Xl.px());
}

#[test]
fn semantic_colors_resolve_with_alpha() {
    // Every role must resolve to a fully-opaque fill or an overlay.
    let colors = [
        Color::Surface.rgba(),
        Color::Elevated.rgba(),
        Color::Accent.rgba(),
        Color::TextPrimary.rgba(),
        Color::TextOnAccent.rgba(),
        Color::TextSecondary.rgba(),
        Color::Border.rgba(),
        Color::HoverOverlay.rgba(),
        Color::MaterialTint.rgba(),
        Color::MaterialHighlight.rgba(),
        Color::MaterialShadow.rgba(),
        Color::Error.rgba(),
    ];
    // Components are never allowed to pick colors outside the token table,
    // so all resolved colors are in the 0.0–1.0 component range.
    for c in colors {
        assert!((0.0..=1.0).contains(&c.0));
        assert!((0.0..=1.0).contains(&c.1));
        assert!((0.0..=1.0).contains(&c.2));
        assert!((0.0..=1.0).contains(&c.3));
    }
}

#[test]
fn motion_has_progressive_duration() {
    let d = |m: Motion| m.spec().duration_ms;
    assert!(d(Motion::None) <= d(Motion::Fast));
    assert!(d(Motion::Fast) < d(Motion::Panel));
    assert!(d(Motion::Panel) < d(Motion::Material));
    assert!(d(Motion::Rebound) <= d(Motion::Material));
    assert!(d(Motion::Material) < d(Motion::Morph));
    assert!(d(Motion::Morph) < d(Motion::Window));
    assert!(d(Motion::Window) < d(Motion::Workspace));
    assert!(d(Motion::Window) <= d(Motion::SessionHandoff));
}

#[test]
fn liquid_materials_have_solid_accessibility_fallbacks() {
    for material in [
        Material::Content,
        Material::Chrome,
        Material::Panel,
        Material::Floating,
        Material::Control,
        Material::Sidebar,
        Material::Dock,
        Material::Capsule,
    ] {
        let liquid = material.spec(MaterialMode::Liquid);
        let reduced = material.spec(MaterialMode::ReducedTransparency);
        assert!((0.0..=1.0).contains(&liquid.tint_opacity));
        assert!(!reduced.samples_backdrop);
        assert_eq!(reduced.backdrop_blur, 0.0);
        assert_eq!(reduced.refraction, 0.0);
        assert_eq!(reduced.tint_opacity, 1.0);
    }
}

#[test]
fn nested_liquid_materials_share_one_backdrop_group() {
    let independent =
        Material::Control.spec_for(MaterialMode::Liquid, MaterialNesting::Independent);
    let nested = Material::Control.spec_for(MaterialMode::Liquid, MaterialNesting::Consolidated);
    assert!(independent.samples_backdrop);
    assert!(!nested.samples_backdrop);
    assert_eq!(nested.backdrop_blur, 0.0);
}
