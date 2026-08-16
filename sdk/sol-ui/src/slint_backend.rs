//! Slint adapter used by the ADR-0004 native spike.
//!
//! This module is intentionally private. Its public owner (`NativeRenderer`)
//! exchanges only SolUI frames, keeping Slint out of application APIs.

use crate::{ButtonFrame, Renderer};

slint::slint! {
    export component NativeButton inherits Window {
        in property <string> label;
        in property <color> fill;
        in property <length> corner-radius;
        in property <float> progress;

        Rectangle {
            background: root.fill;
            border-radius: root.corner-radius;
            opacity: root.progress;
            Text {
                text: root.label;
                color: #ffffff;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }
    }
}

/// The Slint-backed renderer selected by ADR-0004.
///
/// This spike implements the semantic button path end-to-end. Its fields and
/// methods intentionally do not expose Slint types.
pub struct NativeRenderer {
    button: NativeButton,
}

impl NativeRenderer {
    /// Build the retained Slint component tree.
    pub fn new() -> Result<Self, String> {
        NativeButton::new()
            .map(|button| Self { button })
            .map_err(|error| error.to_string())
    }

    /// Return the progress last supplied by SolAnimation for test inspection.
    pub fn progress(&self) -> f32 {
        self.button.get_progress()
    }

    /// Return the label last supplied through SolUI for test inspection.
    pub fn label(&self) -> String {
        self.button.get_label().to_string()
    }
}

impl Renderer for NativeRenderer {
    fn render_button(&mut self, frame: &ButtonFrame) {
        let rgba = frame.background;
        self.button.set_label(frame.label.clone().into());
        self.button.set_fill(slint::Color::from_argb_u8(
            (rgba.3 * 255.0) as u8,
            (rgba.0 * 255.0) as u8,
            (rgba.1 * 255.0) as u8,
            (rgba.2 * 255.0) as u8,
        ));
        self.button.set_corner_radius(frame.corner_radius);
        self.button.set_progress(frame.progress);
    }
}

#[cfg(test)]
mod tests {
    use std::{rc::Rc, sync::Once};

    use slint::{
        PhysicalSize,
        platform::{
            Platform, PlatformError, WindowAdapter, software_renderer::MinimalSoftwareWindow,
        },
    };

    use super::*;

    thread_local! {
        static WINDOW: Rc<MinimalSoftwareWindow> = MinimalSoftwareWindow::new(
            slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
        );
    }

    struct HeadlessPlatform;

    impl Platform for HeadlessPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            WINDOW.with(|window| {
                let adapter: Rc<dyn WindowAdapter> = window.clone();
                Ok(adapter)
            })
        }
    }

    fn install_headless_platform() {
        static PLATFORM: Once = Once::new();
        PLATFORM.call_once(|| {
            slint::platform::set_platform(Box::new(HeadlessPlatform))
                .expect("the spike test installs the Slint platform before creating components");
        });
    }

    #[test]
    fn slint_adapter_receives_tokens_and_external_gesture_progress() {
        install_headless_platform();
        let mut renderer = NativeRenderer::new().expect("headless Slint component should build");
        let mut controller =
            crate::ButtonController::new(crate::Button::new().with_label("Launch"));
        controller.take_over_with_progress(0.64);

        renderer.render_button(&controller.frame());

        assert_eq!(renderer.label(), "Launch");
        assert_eq!(renderer.progress(), 0.64);
        WINDOW.with(|window| window.set_size(PhysicalSize::new(320, 48)));
    }
}
