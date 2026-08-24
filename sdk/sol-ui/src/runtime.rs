//! Retained semantic state and renderer contracts for SolUI.
//!
//! The public types in this module deliberately describe SOL concepts only.
//! A renderer adapter may retain native widget state, but applications neither
//! construct nor receive a renderer-specific object.

use sol_animation::InterruptibleAnimation;
use sol_design::{
    accessibility::TokenMode,
    color::Rgba,
    motion::{Motion, MotionSpec},
    typography::FontStyle,
};

use crate::Button;

/// The logical size offered by a host surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalSize {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
}

impl LogicalSize {
    /// Create a logical size.
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Convert to physical pixels using the host's current scale factor.
    pub fn physical_pixels(self, scale_factor: f32) -> (u32, u32) {
        (
            (self.width * scale_factor).round() as u32,
            (self.height * scale_factor).round() as u32,
        )
    }
}

/// The platform surface that hosts a SolUI frame.
///
/// `sol-shell` owns its Wayland layer-shell surface. Application windows use
/// an xdg toplevel host. Both expose this small, renderer-neutral contract to
/// SolUI, so layer-shell protocol types cannot leak into app code.
pub trait SurfaceHost {
    /// Logical extent negotiated with the platform surface.
    fn logical_size(&self) -> LogicalSize;

    /// Fractional output scale advertised by the platform surface.
    fn scale_factor(&self) -> f32;

    /// Request a frame after retained state changes.
    fn request_frame(&mut self);
}

/// A fully resolved button frame. This is the semantic-to-renderer boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct ButtonFrame {
    /// The user-visible label.
    pub label: String,
    /// Semantic fill resolved through `sol-design`.
    pub background: Rgba,
    /// Semantic foreground resolved through `sol-design`.
    pub foreground: Rgba,
    /// Semantic corner radius resolved through `sol-design`.
    pub corner_radius: f32,
    /// Token-resolved label font size for the active text-scale preference.
    pub font_size: f32,
    /// External animation progress supplied by SolAnimation.
    pub progress: f32,
}

/// Retained state for one semantic button.
///
/// SolUI is reactive/declarative at the application boundary: callers mutate
/// semantic state and ask the runtime for the next frame. This state is
/// retained so accessibility, focus, and interruption have a stable owner;
/// it is not an immediate-mode draw list.
pub struct ButtonController {
    button: Button,
    animation: InterruptibleAnimation,
    progress: f32,
}

impl ButtonController {
    /// Retain a semantic button and initialize its motion ownership.
    pub fn new(button: Button) -> Self {
        let animation = InterruptibleAnimation::new(button.motion());
        Self {
            button,
            animation,
            progress: 0.0,
        }
    }

    /// Replace the current animation with gesture progress.
    ///
    /// This is intentionally an external assignment: Slint's own animation
    /// system is not the authority for an interactive SOL gesture. A gesture
    /// can interrupt an in-flight transition at any point.
    pub fn take_over_with_progress(&mut self, progress: f32) {
        let progress = progress.clamp(0.0, 1.0);
        self.animation.update_with_progress(progress);
        self.progress = progress;
    }

    /// Preserve velocity supplied by an interrupted gesture.
    pub fn set_velocity(&mut self, velocity: f32) {
        self.animation.set_velocity(velocity);
    }

    /// The current semantic motion tier.
    pub fn motion(&self) -> Motion {
        self.button.motion()
    }

    /// The velocity carried into the next animation step.
    pub fn velocity(&self) -> f32 {
        self.animation.velocity
    }

    /// Resolve this component's semantic motion under accessibility preferences.
    pub fn motion_spec(&self, mode: TokenMode) -> MotionSpec {
        mode.motion_spec(self.motion())
    }

    /// Resolve retained state into a renderer-neutral frame.
    pub fn frame(&self) -> ButtonFrame {
        self.frame_for(TokenMode::default())
    }

    /// Resolve retained state using the active theme and accessibility tokens.
    pub fn frame_for(&self, mode: TokenMode) -> ButtonFrame {
        ButtonFrame {
            label: self.button.label.to_owned(),
            background: mode.color(self.button.background()),
            foreground: mode.color(self.button.foreground()),
            corner_radius: self.button.corner_radius().px(),
            font_size: mode.typography(FontStyle::Label).pixels,
            progress: self.progress,
        }
    }
}

/// A renderer that accepts SolUI's retained semantic frames.
pub trait Renderer {
    /// Apply a resolved frame to the backend's retained widget tree.
    fn render_button(&mut self, frame: &ButtonFrame);
}

/// A deterministic renderer used as the repeatable architecture fixture.
///
/// It records exactly what a native adapter would consume, which lets CI
/// verify tokens, fractional scaling, and external animation ownership without
/// a compositor, GPU, or desktop session.
#[derive(Debug, Default)]
pub struct RecordingRenderer {
    /// Frames submitted in order.
    pub frames: Vec<ButtonFrame>,
}

impl Renderer for RecordingRenderer {
    fn render_button(&mut self, frame: &ButtonFrame) {
        self.frames.push(frame.clone());
    }
}

/// Drive a retained button through a renderer and schedule one host frame.
pub fn present_button(
    host: &mut impl SurfaceHost,
    renderer: &mut impl Renderer,
    button: &ButtonController,
) {
    present_button_for(host, renderer, button, TokenMode::default());
}

/// Drive a retained button through a renderer using the surface's active
/// design-token mode, then schedule one host frame.
///
/// A shell or application host supplies this mode at the semantic boundary;
/// renderers never need a second theme, contrast, motion, or text-scale API.
pub fn present_button_for(
    host: &mut impl SurfaceHost,
    renderer: &mut impl Renderer,
    button: &ButtonController,
    mode: TokenMode,
) {
    let _physical_size = host.logical_size().physical_pixels(host.scale_factor());
    renderer.render_button(&button.frame_for(mode));
    host.request_frame();
}

/// A tiny fixture host for platform-independent tests and examples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixtureSurfaceHost {
    size: LogicalSize,
    scale: f32,
    /// Number of render requests submitted to this host.
    pub requested_frames: u32,
}

impl FixtureSurfaceHost {
    /// Create a fixture with a logical size and an arbitrary (including
    /// fractional) scale factor.
    pub const fn new(size: LogicalSize, scale: f32) -> Self {
        Self {
            size,
            scale,
            requested_frames: 0,
        }
    }
}

impl SurfaceHost for FixtureSurfaceHost {
    fn logical_size(&self) -> LogicalSize {
        self.size
    }

    fn scale_factor(&self) -> f32 {
        self.scale
    }

    fn request_frame(&mut self) {
        self.requested_frames += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ButtonState;
    use sol_design::{color::Color, radius::Radius};

    #[test]
    fn retained_button_uses_tokens_and_schedules_a_frame() {
        let button = Button::new().with_label("Open").state(ButtonState::Pressed);
        let controller = ButtonController::new(button);
        let mut host = FixtureSurfaceHost::new(LogicalSize::new(320.0, 48.0), 1.25);
        let mut renderer = RecordingRenderer::default();

        present_button(&mut host, &mut renderer, &controller);

        assert_eq!(host.requested_frames, 1);
        assert_eq!(renderer.frames.len(), 1);
        assert_eq!(renderer.frames[0].label, "Open");
        assert_eq!(
            renderer.frames[0].background,
            TokenMode::light().color(Color::Accent)
        );
        assert_eq!(renderer.frames[0].corner_radius, Radius::Sm.px());
    }

    #[test]
    fn fractional_scale_is_converted_at_the_surface_boundary() {
        let size = LogicalSize::new(320.0, 48.0);
        assert_eq!(size.physical_pixels(1.25), (400, 60));
    }

    #[test]
    fn gesture_can_take_over_an_animation_without_losing_velocity() {
        let mut controller = ButtonController::new(Button::new());
        controller.set_velocity(420.0);
        controller.take_over_with_progress(0.37);

        assert_eq!(controller.frame().progress, 0.37);
        assert_eq!(controller.velocity(), 420.0);
    }

    #[test]
    fn accessibility_mode_changes_theme_text_scale_and_motion_at_token_boundary() {
        let controller = ButtonController::new(Button::new());
        let mode = TokenMode::dark()
            .high_contrast()
            .reduced_motion()
            .with_text_scale(sol_design::accessibility::TextScale::Large);

        let frame = controller.frame_for(mode);

        assert_eq!(frame.background, mode.color(Color::Elevated));
        assert!(frame.font_size > TokenMode::light().typography(FontStyle::Label).pixels);
        assert_eq!(controller.motion_spec(mode).duration_ms, 0);
    }
}
