use sol_design::{
    color::Color,
    material::{Material, MaterialMode},
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
    assert!(d(Motion::Panel) < d(Motion::Window));
    assert!(d(Motion::Window) < d(Motion::Workspace));
}

#[test]
fn fluid_materials_have_solid_accessibility_fallbacks() {
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
        let fluid = material.spec(MaterialMode::Fluid);
        let reduced = material.spec(MaterialMode::ReducedTransparency);
        assert!((0.0..=1.0).contains(&fluid.tint_opacity));
        assert_eq!(reduced.backdrop_blur, 0.0);
        assert_eq!(reduced.refraction, 0.0);
        assert_eq!(reduced.tint_opacity, 1.0);
    }
}
