use sol_design::accessibility::TokenMode;
use sol_ui::{
    GlassButton, GlassComponentFrame, GlassMenuItem, GlassMorphMenu, GlassRenderer, GlassSegment,
    GlassSegmentedControl, GlassSlider, GlassToolbar, GlassToolbarItem, RecordingGlassRenderer,
};

#[test]
fn reference_components_compose_through_the_public_api() {
    let selector = GlassSegmentedControl::new("Capture type")
        .segment(GlassSegment::new("video", "Video"))
        .segment(GlassSegment::new("photo", "Photo"));
    let toolbar = GlassToolbar::new()
        .item(GlassToolbarItem::Button(GlassButton::new("Previous")))
        .item(GlassToolbarItem::Button(GlassButton::new("Play")));
    let slider = GlassSlider::new("Hue", 62).hue_track();

    let mut renderer = RecordingGlassRenderer::default();
    renderer.render_glass(&GlassComponentFrame::SegmentedControl(
        selector.frame_for(TokenMode::light()),
    ));
    renderer.render_glass(&GlassComponentFrame::Toolbar(
        toolbar.frame_for(TokenMode::light()),
    ));
    renderer.render_glass(&GlassComponentFrame::Slider(
        slider.frame_for(TokenMode::light()),
    ));

    assert_eq!(renderer.frames.len(), 3);
}

#[test]
fn public_frames_preserve_layout_when_transparency_is_reduced() {
    let button = GlassButton::new("Next");
    let liquid = button.frame_for(TokenMode::dark());
    let reduced = button.frame_for(TokenMode::dark().reduced_transparency());

    assert_eq!(liquid.metric, reduced.metric);
    assert!(liquid.surface.spec.samples_backdrop);
    assert!(!reduced.surface.spec.samples_backdrop);
}

#[test]
fn morph_menu_reverses_and_renders_through_the_public_api() {
    let menu = GlassMorphMenu::new("Account", "Open account menu")
        .item(GlassMenuItem::new("profile", "My Profile"))
        .item(GlassMenuItem::new("settings", "Settings"))
        .item(GlassMenuItem::new("logout", "Log Out"));
    let mut controller = menu.controller();
    controller.pointer_down();
    controller.pointer_up_and_toggle(1.0);
    let frame = controller.tick(1.0 / 60.0, TokenMode::light());

    let mut renderer = RecordingGlassRenderer::default();
    renderer.render_glass(&GlassComponentFrame::MorphMenu(Box::new(frame)));
    assert_eq!(renderer.frames.len(), 1);
    assert!(controller.target_open());
}
