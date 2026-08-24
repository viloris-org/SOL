//! Slint adapter used by the ADR-0004 native spike.
//!
//! This module is intentionally private. Its public owner (`NativeRenderer`)
//! exchanges only SolUI frames, keeping Slint out of application APIs.

use std::{cell::Cell, rc::Rc};

use crate::{ButtonFrame, GuidedPageFrame, GuidedStepState, Renderer};

slint::slint! {
    export component NativeButton inherits Window {
        in property <string> label;
        in property <color> fill;
        in property <color> text-fill;
        in property <length> corner-radius;
        in property <length> font-size;
        in property <float> progress;

        Rectangle {
            background: root.fill;
            border-radius: root.corner-radius;
            opacity: root.progress;
            Text {
                text: root.label;
                color: root.text-fill;
                font-size: root.font-size;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }
    }

    export struct GuidedStepRow {
        marker: string,
        title: string,
        description: string,
        current: bool,
        complete: bool,
    }

    component GuidedAction inherits Rectangle {
        in property <string> label;
        in property <color> fill;
        in property <color> text-fill;
        in property <color> outline-fill;
        in property <color> focus-fill;
        in property <length> resting-outline-width;
        in property <length> corner-radius;
        in property <length> label-size;
        out property <bool> has-focus: focus-scope.has-focus;
        callback clicked();

        height: 44px;
        horizontal-stretch: 1;
        border-radius: root.corner-radius;
        background: root.fill;
        border-width: root.has-focus ? 3px : root.resting-outline-width;
        border-color: root.has-focus ? root.focus-fill : root.outline-fill;
        forward-focus: focus-scope;
        accessible-role: button;
        accessible-enabled: true;
        accessible-label: root.label;
        accessible-action-default => { root.clicked(); }

        Text {
            text: root.label;
            color: root.text-fill;
            font-size: root.label-size;
            font-weight: 600;
            horizontal-alignment: center;
            vertical-alignment: center;
            accessible-role: none;
        }
        TouchArea { clicked => { root.clicked(); } }
        focus-scope := FocusScope {
            x: 0;
            width: 0px;
            key-pressed(event) => {
                if (event.text == " " || event.text == "\n") {
                    root.clicked();
                    return accept;
                }
                return reject;
            }
        }
    }

    export component NativeGuidedPage inherits Window {
        in property <string> eyebrow;
        in property <string> page-title;
        in property <string> description;
        in property <[string]> highlights;
        in property <[GuidedStepRow]> overview-steps;
        in property <string> primary-label;
        in property <string> secondary-label;
        in property <color> page-fill;
        in property <color> panel-fill;
        in property <color> primary-fill;
        in property <color> primary-text-fill;
        in property <color> secondary-fill;
        in property <color> secondary-text-fill;
        in property <color> text-primary;
        in property <color> text-secondary;
        in property <color> accent;
        in property <color> border-color;
        in property <length> display-size;
        in property <length> title-size;
        in property <length> body-size;
        in property <length> label-size;
        in property <length> control-radius;
        in property <length> panel-radius;
        in property <length> space-small;
        in property <length> space-medium;
        in property <length> space-large;
        in property <length> space-xlarge;
        callback primary-action();
        callback secondary-action();

        title: "Install SOL";
        width: 1100px;
        height: 680px;
        background: root.page-fill;

        VerticalLayout {
            padding: root.space-xlarge;
            spacing: root.space-xlarge;

            HorizontalLayout {
                spacing: root.space-medium;
                alignment: center;
                Text {
                    text: "SOL";
                    color: root.text-primary;
                    font-size: root.title-size;
                    font-weight: 700;
                }
                Rectangle {
                    width: 108px;
                    height: 28px;
                    border-radius: root.control-radius;
                    background: root.panel-fill;
                    border-width: 1px;
                    border-color: root.border-color;
                    Text {
                        text: root.eyebrow;
                        color: root.text-secondary;
                        font-size: root.label-size;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
                Rectangle { horizontal-stretch: 1; }
                Text {
                    text: "Nothing has been changed yet";
                    color: root.text-secondary;
                    font-size: root.label-size;
                    vertical-alignment: center;
                }
            }

            HorizontalLayout {
                spacing: root.space-xlarge;
                vertical-stretch: 1;

                VerticalLayout {
                    width: 610px;
                    spacing: root.space-large;
                    alignment: center;

                    Text {
                        text: root.page-title;
                        color: root.text-primary;
                        font-size: root.display-size;
                        font-weight: 700;
                        wrap: word-wrap;
                    }
                    Text {
                        text: root.description;
                        color: root.text-secondary;
                        font-size: root.body-size;
                        wrap: word-wrap;
                    }

                    VerticalLayout {
                        spacing: root.space-medium;
                        for highlight in root.highlights : Text {
                            text: "✓  " + highlight;
                            color: root.text-primary;
                            font-size: root.body-size;
                        }
                    }

                    Rectangle { vertical-stretch: 1; }

                    HorizontalLayout {
                        spacing: root.space-medium;
                        GuidedAction {
                            label: root.primary-label;
                            fill: root.primary-fill;
                            text-fill: root.primary-text-fill;
                            outline-fill: transparent;
                            focus-fill: root.primary-text-fill;
                            resting-outline-width: 0px;
                            corner-radius: root.control-radius;
                            label-size: root.label-size;
                            clicked => { root.primary-action(); }
                        }
                        GuidedAction {
                            label: root.secondary-label;
                            fill: root.secondary-fill;
                            text-fill: root.secondary-text-fill;
                            outline-fill: root.border-color;
                            focus-fill: root.accent;
                            resting-outline-width: 1px;
                            corner-radius: root.control-radius;
                            label-size: root.label-size;
                            clicked => { root.secondary-action(); }
                        }
                    }
                }

                Rectangle {
                    horizontal-stretch: 1;
                    background: root.panel-fill;
                    border-radius: root.panel-radius;
                    border-width: 1px;
                    border-color: root.border-color;

                    VerticalLayout {
                        padding: root.space-xlarge;
                        spacing: root.space-large;
                        Text {
                            text: "Installation overview";
                            color: root.text-primary;
                            font-size: root.title-size;
                            font-weight: 600;
                        }

                        for step in root.overview-steps : HorizontalLayout {
                            spacing: root.space-medium;
                            Rectangle {
                                width: 28px; height: 28px; border-radius: 14px;
                                background: step.current || step.complete ? root.accent : root.page-fill;
                                border-width: step.current || step.complete ? 0px : 1px;
                                border-color: root.border-color;
                                Text {
                                    text: step.complete ? "✓" : step.marker;
                                    color: step.current || step.complete ? root.primary-text-fill : root.text-secondary;
                                    horizontal-alignment: center;
                                    vertical-alignment: center;
                                }
                            }
                            VerticalLayout {
                                spacing: root.space-small;
                                Text { text: step.title; color: root.text-primary; font-size: root.body-size; font-weight: 600; }
                                Text { text: step.description; color: root.text-secondary; font-size: root.label-size; wrap: word-wrap; }
                            }
                        }
                        Rectangle { vertical-stretch: 1; }
                        Text {
                            text: "You will review every change before installation starts.";
                            color: root.text-secondary;
                            font-size: root.label-size;
                            wrap: word-wrap;
                        }
                    }
                }
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

    /// Return the token-resolved label size for test inspection.
    pub fn font_size(&self) -> f32 {
        self.button.get_font_size()
    }

    /// Enter the native Slint event loop for this semantic component.
    ///
    /// Applications call this through SolUI's renderer-neutral API; no Slint
    /// type is exposed at the call site.
    pub fn run(&self) -> Result<(), String> {
        self.button.run().map_err(|error| error.to_string())
    }
}

impl Renderer for NativeRenderer {
    fn render_button(&mut self, frame: &ButtonFrame) {
        let rgba = frame.background;
        self.button.set_label(frame.label.clone().into());
        self.button.set_fill(to_slint_color(rgba));
        self.button.set_text_fill(to_slint_color(frame.foreground));
        self.button.set_corner_radius(frame.corner_radius);
        self.button.set_font_size(frame.font_size);
        self.button.set_progress(frame.progress);
    }
}

/// The explicit exit selected from a guided welcome page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidedPageAction {
    /// Continue into the guided flow.
    Primary,
    /// Defer the flow and return to the surrounding environment.
    Secondary,
    /// Close the window without choosing either action.
    Dismissed,
}

/// Slint-backed full-window projection for [`crate::GuidedPage`].
pub struct NativeGuidedPageRenderer {
    page: NativeGuidedPage,
}

impl NativeGuidedPageRenderer {
    /// Build the retained native page.
    pub fn new() -> Result<Self, String> {
        NativeGuidedPage::new()
            .map(|page| Self { page })
            .map_err(|error| error.to_string())
    }

    /// Apply a resolved SolUI page frame without exposing native toolkit types.
    pub fn render(&self, frame: &GuidedPageFrame) {
        let highlights = frame
            .highlights
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Vec<_>>();
        let overview_steps = frame
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| GuidedStepRow {
                marker: (index + 1).to_string().into(),
                title: step.title.clone().into(),
                description: step.description.clone().into(),
                current: step.state == GuidedStepState::Current,
                complete: step.state == GuidedStepState::Complete,
            })
            .collect::<Vec<_>>();

        self.page.set_eyebrow(frame.eyebrow.clone().into());
        self.page.set_page_title(frame.title.clone().into());
        self.page.set_description(frame.description.clone().into());
        self.page
            .set_highlights(slint::ModelRc::new(slint::VecModel::from(highlights)));
        self.page
            .set_overview_steps(slint::ModelRc::new(slint::VecModel::from(overview_steps)));
        self.page
            .set_primary_label(frame.primary.label.clone().into());
        self.page
            .set_secondary_label(frame.secondary.label.clone().into());
        self.page
            .set_page_fill(to_slint_color(frame.page_background));
        self.page
            .set_panel_fill(to_slint_color(frame.panel_background));
        self.page
            .set_primary_fill(to_slint_color(frame.primary.background));
        self.page
            .set_primary_text_fill(to_slint_color(frame.primary.foreground));
        self.page
            .set_secondary_fill(to_slint_color(frame.secondary.background));
        self.page
            .set_secondary_text_fill(to_slint_color(frame.secondary.foreground));
        self.page
            .set_text_primary(to_slint_color(frame.text_primary));
        self.page
            .set_text_secondary(to_slint_color(frame.text_secondary));
        self.page.set_accent(to_slint_color(frame.accent));
        self.page.set_border_color(to_slint_color(frame.border));
        self.page.set_display_size(frame.display_size);
        self.page.set_title_size(frame.title_size);
        self.page.set_body_size(frame.body_size);
        self.page.set_label_size(frame.label_size);
        self.page.set_control_radius(frame.control_radius);
        self.page.set_panel_radius(frame.panel_radius);
        self.page.set_space_small(frame.spacing_small);
        self.page.set_space_medium(frame.spacing_medium);
        self.page.set_space_large(frame.spacing_large);
        self.page.set_space_xlarge(frame.spacing_xlarge);
    }

    /// Run until the user chooses an explicit exit or closes the window.
    pub fn run_until_action(&self) -> Result<GuidedPageAction, String> {
        let result = Rc::new(Cell::new(GuidedPageAction::Dismissed));
        let primary_result = Rc::clone(&result);
        self.page.on_primary_action(move || {
            primary_result.set(GuidedPageAction::Primary);
            let _ = slint::quit_event_loop();
        });
        let secondary_result = Rc::clone(&result);
        self.page.on_secondary_action(move || {
            secondary_result.set(GuidedPageAction::Secondary);
            let _ = slint::quit_event_loop();
        });
        self.page.run().map_err(|error| error.to_string())?;
        Ok(result.get())
    }
}

fn to_slint_color(rgba: sol_design::color::Rgba) -> slint::Color {
    slint::Color::from_argb_u8(
        (rgba.3 * 255.0) as u8,
        (rgba.0 * 255.0) as u8,
        (rgba.1 * 255.0) as u8,
        (rgba.2 * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use std::{rc::Rc, sync::Once};

    use slint::{
        ComponentHandle, Model, PhysicalSize,
        platform::{
            Key, Platform, PlatformError, WindowAdapter, WindowEvent,
            software_renderer::MinimalSoftwareWindow,
        },
    };

    use super::*;

    thread_local! {
        static WINDOW: Rc<MinimalSoftwareWindow> = MinimalSoftwareWindow::new(
            slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
        );
        static PLATFORM: Once = const { Once::new() };
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
        PLATFORM.with(|platform| {
            platform.call_once(|| {
                slint::platform::set_platform(Box::new(HeadlessPlatform)).expect(
                    "the spike test installs the Slint platform before creating components",
                );
            });
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
        assert!(renderer.font_size() > 0.0);
        WINDOW.with(|window| window.set_size(PhysicalSize::new(320, 48)));
    }

    #[test]
    fn guided_page_projects_every_highlight_and_step_state() {
        install_headless_platform();
        let renderer = NativeGuidedPageRenderer::new().expect("headless guided page should build");
        let page = crate::GuidedPage::new("LIVE", "Welcome", "Description", "Go", "Later")
            .highlight("One")
            .highlight("Two")
            .highlight("Three")
            .highlight("Four")
            .step(crate::GuidedPageStep::new("Done", "Complete").complete())
            .step(crate::GuidedPageStep::new("Now", "Current").current())
            .step(crate::GuidedPageStep::new("Next", "Upcoming"))
            .step(crate::GuidedPageStep::new("Later", "Still upcoming"));

        renderer.render(&page.frame_for(sol_design::accessibility::TokenMode::dark()));

        let highlights = renderer.page.get_highlights();
        assert_eq!(highlights.row_count(), 4);
        assert_eq!(highlights.row_data(3).as_deref(), Some("Four"));
        let steps = renderer.page.get_overview_steps();
        assert_eq!(steps.row_count(), 4);
        assert!(steps.row_data(0).expect("first row").complete);
        assert!(steps.row_data(1).expect("second row").current);
        assert_eq!(steps.row_data(3).expect("fourth row").marker, "4");
    }

    #[test]
    fn guided_page_actions_accept_keyboard_focus_and_activation() {
        install_headless_platform();
        let renderer = NativeGuidedPageRenderer::new().expect("headless guided page should build");
        let activated = Rc::new(std::cell::Cell::new(false));
        let activated_from_callback = Rc::clone(&activated);
        renderer
            .page
            .on_primary_action(move || activated_from_callback.set(true));

        renderer
            .page
            .window()
            .dispatch_event(WindowEvent::KeyPressed {
                text: Key::Tab.into(),
            });
        renderer
            .page
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: "\n".into() });

        assert!(activated.get());
    }
}
