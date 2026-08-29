//! Slint rendering for the login UI, targeting an SCP lock surface.
//!
//! There is no windowing backend here. The compositor owns the display, so the
//! login screen installs a custom Slint [`Platform`] whose only window is a
//! [`MinimalSoftwareWindow`], and rasterizes into the shared buffer that goes
//! over SCP with `AttachBuffer`.
//!
//! Two consequences worth knowing:
//!
//! - There is no Slint event loop. Frames are drawn when the caller asks
//!   ([`LoginRenderer::draw_into`]), and `slint::quit_event_loop` must never be
//!   called — without an event-loop proxy it panics. The login button records an
//!   action for the caller to notice instead.
//! - The password field does not edit text. `LoginUi` owns the password and
//!   hands over a string that is already masked or revealed; keystrokes never
//!   enter Slint's text-input stack.

use std::{cell::Cell, rc::Rc};

use slint::{
    ComponentHandle, LogicalPosition, Model, PhysicalSize,
    platform::{
        Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};

use crate::{scp::FrameBuffer, ui::LoginFrame};

slint::slint! {
    export struct UserRow {
        username: string,
        display_name: string,
        selected: bool,
    }

    component UserAvatar inherits Rectangle {
        in property <string> display-name;
        in property <bool> selected;
        in property <color> accent;
        in property <color> text-primary;
        in property <color> border;
        in property <length> avatar-size;
        in property <length> label-size;
        callback clicked();

        width: root.avatar-size + 16px;

        VerticalLayout {
            spacing: 8px;
            alignment: center;

            Rectangle {
                width: root.avatar-size;
                height: root.avatar-size;
                border-radius: root.avatar-size / 2;
                background: root.selected ? root.accent : #4a5568;
                border-width: root.selected ? 3px : 1px;
                border-color: root.selected ? root.accent : root.border;

                Text {
                    text: root.display-name.character-count > 0 ? root.display-name : " ";
                    color: white;
                    font-size: root.avatar-size / 2;
                    font-weight: 600;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }

                TouchArea {
                    clicked => { root.clicked(); }
                }
            }

            Text {
                text: root.display-name;
                color: root.text-primary;
                font-size: root.label-size;
                horizontal-alignment: center;
            }
        }
    }

    // Displays the password; it does not edit it. The login service owns the
    // text and decides whether it arrives masked, so that a typed password
    // never passes through a UI toolkit's input handling.
    component PasswordField inherits Rectangle {
        in property <string> password;
        in property <string> placeholder;
        in property <bool> password-visible;
        in property <color> bg;
        in property <color> text-color;
        in property <color> placeholder-color;
        in property <color> brd-color;
        in property <length> corner-radius;
        in property <length> font-size;
        callback toggle-visibility();

        height: 48px;
        border-radius: root.corner-radius;
        background: root.bg;
        border-width: 2px;
        border-color: root.brd-color;

        HorizontalLayout {
            padding-left: 16px;
            padding-right: 16px;
            spacing: 8px;

            Text {
                text: root.password.character-count > 0 ? root.password : root.placeholder;
                color: root.password.character-count > 0 ? root.text-color : root.placeholder-color;
                font-size: root.font-size;
                horizontal-alignment: left;
                vertical-alignment: center;
                overflow: elide;
            }

            Rectangle {
                width: 32px;
                height: 32px;
                border-radius: 16px;

                Text {
                    text: root.password-visible ? "👁" : "🔒";
                    font-size: 18px;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }

                TouchArea {
                    clicked => { root.toggle-visibility(); }
                }
            }
        }
    }

    component ActionButton inherits Rectangle {
        in property <string> label;
        in property <bool> enabled;
        in property <color> bg;
        in property <color> text-color;
        in property <length> corner-radius;
        in property <length> font-size;
        callback clicked();

        height: 48px;
        border-radius: root.corner-radius;
        background: root.enabled ? root.bg : #6b7280;
        opacity: root.enabled ? 1.0 : 0.5;

        Text {
            text: root.label;
            color: root.text-color;
            font-size: root.font-size;
            font-weight: 600;
            horizontal-alignment: center;
            vertical-alignment: center;
        }

        TouchArea {
            enabled: root.enabled;
            clicked => { root.clicked(); }
        }
    }

    export component LoginScreen inherits Window {
        in property <[UserRow]> users;
        in property <string> selected-user-name;
        in property <string> password;
        in property <bool> password-visible;
        in property <bool> can-login;
        in property <string> status;
        in property <color> page-background;
        in property <color> panel-background;
        in property <color> text-primary;
        in property <color> text-secondary;
        in property <color> accent;
        in property <color> border;
        in property <color> elevated;
        in property <length> display-size;
        in property <length> title-size;
        in property <length> body-size;
        in property <length> label-size;
        in property <length> panel-radius;
        in property <length> control-radius;
        in property <length> avatar-radius;
        in property <length> spacing-small;
        in property <length> spacing-medium;
        in property <length> spacing-large;
        in property <length> spacing-xlarge;
        in property <float> content-opacity: 1.0;
        in property <float> material-opacity: 1.0;

        callback user-selected(int);
        callback toggle-password-visibility();
        callback login-clicked();

        title: "SOL Login";
        background: root.page-background;

        VerticalLayout {
            alignment: center;
            padding: root.spacing-xlarge;

            Rectangle {
                width: 480px;
                border-radius: root.panel-radius;

                Rectangle {
                    border-radius: root.panel-radius;
                    background: root.panel-background;
                    drop-shadow-blur: 32px;
                    drop-shadow-color: #00000040;
                    opacity: root.material-opacity;
                }

                VerticalLayout {
                    padding: root.spacing-xlarge;
                    spacing: root.spacing-large;
                    alignment: center;
                    opacity: root.content-opacity;

                    // Avatar grid
                    HorizontalLayout {
                        spacing: root.spacing-medium;
                        alignment: center;

                        for user[index] in root.users : UserAvatar {
                            display-name: user.display-name;
                            selected: user.selected;
                            accent: root.accent;
                            text-primary: root.text-primary;
                            border: root.border;
                            avatar-size: 80px;
                            label-size: root.label-size;
                            clicked => { root.user-selected(index); }
                        }
                    }

                    // Selected user display name
                    Text {
                        text: root.selected-user-name;
                        color: root.text-primary;
                        font-size: root.title-size;
                        font-weight: 600;
                        horizontal-alignment: center;
                    }

                    // Password field
                    PasswordField {
                        password: root.password;
                        placeholder: "Password";
                        password-visible: root.password-visible;
                        bg: root.elevated;
                        text-color: root.text-primary;
                        placeholder-color: root.text-secondary;
                        brd-color: root.border;
                        corner-radius: root.control-radius;
                        font-size: root.body-size;
                        toggle-visibility => { root.toggle-password-visibility(); }
                    }

                    // Login button
                    ActionButton {
                        label: "Log In";
                        enabled: root.can-login;
                        bg: root.accent;
                        text-color: white;
                        corner-radius: root.control-radius;
                        font-size: root.label-size;
                        clicked => { root.login-clicked(); }
                    }

                    // Authentication feedback, empty when there is nothing to say.
                    Text {
                        text: root.status;
                        color: root.text-secondary;
                        font-size: root.label-size;
                        horizontal-alignment: center;
                    }
                }
            }
        }
    }
}

/// A request from the on-screen controls.
///
/// Only the login button produces one today; the system actions the design calls
/// for (sleep, shut down) will join it here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginAction {
    /// Submit the entered credentials.
    Authenticate,
}

thread_local! {
    /// The one window this platform ever creates.
    ///
    /// Full repaints every frame: the buffer is reallocated on resize and the
    /// compositor may still be reading the previous one, so assuming the target
    /// still holds the last frame — what `ReusedBuffer` requires — would be
    /// wrong.
    static WINDOW: Rc<MinimalSoftwareWindow> =
        MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
}

/// A Slint platform with no event loop and no display of its own.
struct ScpPlatform;

impl Platform for ScpPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        WINDOW.with(|window| Ok(window.clone() as Rc<dyn WindowAdapter>))
    }
}

/// Install [`ScpPlatform`] for this thread, once.
///
/// Slint's platform is per-thread and can only be set before the first
/// component is created, so this must run before [`LoginScreen::new`]. A second
/// call is a no-op rather than an error, which is what lets several tests each
/// build a renderer.
fn install_platform() -> Result<(), String> {
    thread_local! {
        static INSTALLED: Cell<bool> = const { Cell::new(false) };
    }

    INSTALLED.with(|installed| {
        if installed.get() {
            return Ok(());
        }
        slint::platform::set_platform(Box::new(ScpPlatform))
            .map_err(|error| format!("could not install the SCP Slint platform: {error}"))?;
        installed.set(true);
        Ok(())
    })
}

/// Draws the login UI into a shared buffer and turns pointer input into actions.
pub struct LoginRenderer {
    screen: LoginScreen,
    window: Rc<MinimalSoftwareWindow>,
    /// The avatar row model, held rather than rebuilt.
    ///
    /// `ModelRc` compares by pointer, so handing Slint a freshly allocated model
    /// each frame reads as a change every time — which redraws the whole surface
    /// on a login screen that is doing nothing at all.
    users: Rc<slint::VecModel<UserRow>>,
    action: Rc<Cell<Option<LoginAction>>>,
    /// Last pointer position, because SCP reports button presses without one.
    pointer: Rc<Cell<(f32, f32)>>,
}

impl LoginRenderer {
    /// Create the renderer, installing the software platform on this thread.
    pub fn new() -> Result<Self, String> {
        install_platform()?;
        let window = WINDOW.with(Rc::clone);
        let screen = LoginScreen::new().map_err(|error| error.to_string())?;
        let users = Rc::new(slint::VecModel::from(Vec::<UserRow>::new()));
        screen.set_users(users.clone().into());
        screen.show().map_err(|error| error.to_string())?;

        Ok(Self {
            screen,
            window,
            users,
            action: Rc::new(Cell::new(None)),
            pointer: Rc::new(Cell::new((0.0, 0.0))),
        })
    }

    /// Wire the callbacks the on-screen controls fire.
    ///
    /// Keyboard input is deliberately absent: it goes straight to `LoginUi`.
    /// Only pointer-driven controls need a callback, because Slint is the one
    /// that knows where the avatars and buttons ended up.
    pub fn connect(
        &self,
        on_user_selected: impl Fn(usize) + 'static,
        on_toggle_visibility: impl Fn() + 'static,
    ) {
        self.screen
            .on_user_selected(move |index| on_user_selected(index.max(0) as usize));
        self.screen
            .on_toggle_password_visibility(on_toggle_visibility);

        let action = Rc::clone(&self.action);
        self.screen.on_login_clicked(move || {
            action.set(Some(LoginAction::Authenticate));
        });
    }

    /// Match the surface geometry the compositor configured.
    pub fn resize(&self, width: i32, height: i32) {
        let width = u32::try_from(width).unwrap_or(0);
        let height = u32::try_from(height).unwrap_or(0);
        self.window.set_size(PhysicalSize::new(width, height));
        self.window.request_redraw();
    }

    /// Push a resolved frame into the live Slint properties.
    ///
    /// Cheap to call every iteration: Slint compares each property and only
    /// marks the scene dirty when a value really changed, so an idle login
    /// screen stops redrawing after its first frame.
    pub fn render(&self, frame: &LoginFrame) {
        self.apply_users(frame);
        apply_frame(&self.screen, frame);
    }

    /// Refresh the avatar row, but only when it differs from what is displayed.
    fn apply_users(&self, frame: &LoginFrame) {
        let rows: Vec<UserRow> = frame
            .users
            .iter()
            .enumerate()
            .map(|(index, user)| UserRow {
                username: user.username.clone().into(),
                display_name: user.display_name().into(),
                selected: index == frame.selected_user_index,
            })
            .collect();

        if self.users.iter().eq(rows.iter().cloned()) {
            return;
        }
        self.users.set_vec(rows);
    }

    /// Note a new pointer position and let Slint update hover state.
    pub fn pointer_moved(&self, x: f64, y: f64) {
        self.pointer.set((x as f32, y as f32));
        self.window.dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(x as f32, y as f32),
        });
    }

    /// Deliver a pointer button at the last known position.
    pub fn pointer_button(&self, pressed: bool) {
        let (x, y) = self.pointer.get();
        let position = LogicalPosition::new(x, y);
        let button = PointerEventButton::Left;
        self.window.dispatch_event(if pressed {
            WindowEvent::PointerPressed { position, button }
        } else {
            WindowEvent::PointerReleased { position, button }
        });
    }

    /// Report that the login screen has, or has lost, keyboard focus.
    pub fn set_active(&self, active: bool) {
        self.window
            .dispatch_event(WindowEvent::WindowActiveChanged(active));
    }

    /// Advance animations and timers. Call once per loop iteration.
    pub fn tick(&self) {
        slint::platform::update_timers_and_animations();
    }

    /// Force the next [`Self::draw_into`] to redraw.
    pub fn invalidate(&self) {
        self.window.request_redraw();
    }

    /// Rasterize into `buffer`, returning whether anything was drawn.
    ///
    /// `false` means the scene is unchanged and the previous frame still stands,
    /// so there is nothing new to hand the compositor.
    pub fn draw_into(&self, buffer: &mut FrameBuffer) -> bool {
        let pixel_stride = buffer.pixel_stride();
        let pixels = buffer.pixels();
        self.window.draw_if_needed(|renderer| {
            // The returned dirty region is not useful here: SCP damage covers
            // the whole surface because the buffer is redrawn in full.
            let _ = renderer.render(pixels, pixel_stride);
        })
    }

    /// Take the pending action, if the user asked for one.
    pub fn take_action(&self) -> Option<LoginAction> {
        self.action.take()
    }
}

/// Push a resolved login UI frame into the live Slint screen properties.
///
/// The avatar model is handled by [`LoginRenderer::apply_users`]; everything here
/// is a scalar, string, or color that Slint compares for itself.
fn apply_frame(screen: &LoginScreen, frame: &LoginFrame) {
    // Set selected user name
    if let Some(user) = &frame.selected_user {
        screen.set_selected_user_name(user.display_name().into());
    }

    // Set password display
    screen.set_password(frame.password.clone().into());
    screen.set_password_visible(frame.password_visible);
    screen.set_can_login(frame.can_login);
    screen.set_status(frame.status.clone().into());

    // Set colors
    screen.set_page_background(to_slint_color(frame.page_background));
    screen.set_panel_background(to_slint_color(frame.panel_background));
    screen.set_text_primary(to_slint_color(frame.text_primary));
    screen.set_text_secondary(to_slint_color(frame.text_secondary));
    screen.set_accent(to_slint_color(frame.accent));
    screen.set_border(to_slint_color(frame.border));
    screen.set_elevated(to_slint_color(frame.elevated));

    // Set typography
    screen.set_display_size(frame.display_size);
    screen.set_title_size(frame.title_size);
    screen.set_body_size(frame.body_size);
    screen.set_label_size(frame.label_size);

    // Set metrics
    screen.set_panel_radius(frame.panel_radius);
    screen.set_control_radius(frame.control_radius);
    screen.set_avatar_radius(frame.avatar_radius);
    screen.set_spacing_small(frame.spacing_small);
    screen.set_spacing_medium(frame.spacing_medium);
    screen.set_spacing_large(frame.spacing_large);
    screen.set_spacing_xlarge(frame.spacing_xlarge);
    screen.set_content_opacity(frame.content_opacity);
    screen.set_material_opacity(frame.material_opacity);
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
    use super::*;
    use crate::{ui::LoginUi, users::UserAccount};
    use sol_design::accessibility::TokenMode;

    fn frame() -> LoginFrame {
        let ui = LoginUi::new(vec![
            UserAccount::new("jdoe".into(), "John Doe".into(), 1000),
            UserAccount::new("asmith".into(), "Ann Smith".into(), 1001),
        ]);
        ui.frame_for(TokenMode::light())
    }

    #[test]
    fn rasterizes_the_login_screen_into_a_shared_buffer() {
        let renderer = LoginRenderer::new().expect("build the software renderer");
        let mut buffer = FrameBuffer::new(640, 480).expect("allocate frame buffer");

        renderer.resize(640, 480);
        renderer.render(&frame());
        renderer.tick();

        assert!(
            renderer.draw_into(&mut buffer),
            "the first frame must be drawn"
        );

        // A blank buffer would still be "drawn", so check that real pixels
        // landed: every pixel opaque, and more than one distinct color.
        let pixels = buffer.pixels();
        assert!(
            pixels.iter().all(|pixel| pixel.alpha == u8::MAX),
            "the login screen must cover its whole surface opaquely"
        );
        let distinct = pixels
            .iter()
            .map(|pixel| (pixel.red, pixel.green, pixel.blue))
            .collect::<std::collections::HashSet<_>>();
        assert!(
            distinct.len() > 2,
            "expected a rendered UI, got {} distinct colors",
            distinct.len()
        );
    }

    #[test]
    fn an_unchanged_scene_is_not_redrawn() {
        let renderer = LoginRenderer::new().expect("build the software renderer");
        let mut buffer = FrameBuffer::new(320, 240).expect("allocate frame buffer");
        renderer.resize(320, 240);
        renderer.render(&frame());
        assert!(renderer.draw_into(&mut buffer), "first frame draws");
        assert!(
            !renderer.draw_into(&mut buffer),
            "a second draw with no change must be skipped"
        );

        // The login loop re-applies the frame every iteration. Doing so with
        // identical content must not dirty the scene, or an idle greeter
        // re-rasterizes its whole surface several times a second forever.
        for _ in 0..3 {
            renderer.render(&frame());
            renderer.tick();
            assert!(
                !renderer.draw_into(&mut buffer),
                "re-applying an identical frame must not request a redraw"
            );
        }
    }

    #[test]
    fn a_property_change_requests_a_new_frame() {
        let renderer = LoginRenderer::new().expect("build the software renderer");
        let mut buffer = FrameBuffer::new(320, 240).expect("allocate frame buffer");
        renderer.resize(320, 240);

        let mut ui = LoginUi::new(vec![UserAccount::new(
            "jdoe".into(),
            "John Doe".into(),
            1000,
        )]);
        renderer.render(&ui.frame_for(TokenMode::light()));
        assert!(renderer.draw_into(&mut buffer), "first frame draws");

        ui.set_password("secret".into());
        renderer.render(&ui.frame_for(TokenMode::light()));
        assert!(
            renderer.draw_into(&mut buffer),
            "a changed password must produce a new frame"
        );
    }

    #[test]
    fn the_login_button_records_an_action() {
        let renderer = LoginRenderer::new().expect("build the software renderer");
        renderer.connect(|_| {}, || {});
        assert_eq!(renderer.take_action(), None);

        // Invoking the callback is what a click on the button ends up doing.
        renderer.screen.invoke_login_clicked();
        assert_eq!(renderer.take_action(), Some(LoginAction::Authenticate));
        assert_eq!(renderer.take_action(), None, "an action is taken only once");
    }

    #[test]
    fn pointer_callbacks_reach_the_login_state() {
        let renderer = LoginRenderer::new().expect("build the software renderer");
        let selected = Rc::new(Cell::new(usize::MAX));
        let toggled = Rc::new(Cell::new(0_u32));

        renderer.connect(
            {
                let selected = Rc::clone(&selected);
                move |index| selected.set(index)
            },
            {
                let toggled = Rc::clone(&toggled);
                move || toggled.set(toggled.get() + 1)
            },
        );

        renderer.screen.invoke_user_selected(1);
        renderer.screen.invoke_toggle_password_visibility();

        assert_eq!(selected.get(), 1);
        assert_eq!(toggled.get(), 1);
    }

    #[test]
    fn the_final_handoff_frame_keeps_only_the_stationary_background() {
        let renderer = LoginRenderer::new().expect("build the software renderer");
        let mut buffer = FrameBuffer::new(320, 240).expect("allocate frame buffer");
        renderer.resize(320, 240);

        let ui = LoginUi::new(vec![UserAccount::new(
            "jdoe".into(),
            "John Doe".into(),
            1000,
        )]);
        let frame = ui.frame_for_handoff(
            TokenMode::dark(),
            crate::handoff::HandoffVisual {
                content_opacity: 0.0,
                material_opacity: 0.0,
                finished: true,
            },
        );
        renderer.render(&frame);
        assert!(renderer.draw_into(&mut buffer));

        let distinct = buffer
            .pixels()
            .iter()
            .map(|pixel| (pixel.red, pixel.green, pixel.blue, pixel.alpha))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            distinct.len(),
            1,
            "handoff must not move or replace the page background"
        );
    }
}
