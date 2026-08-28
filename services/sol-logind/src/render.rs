//! Slint rendering for the login UI.

use std::{cell::Cell, rc::Rc};

use crate::ui::LoginFrame;

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

    component PasswordField inherits Rectangle {
        in property <string> password;
        in property <bool> password-visible;
        in property <color> bg;
        in property <color> text-color;
        in property <color> brd-color;
        in property <length> corner-radius;
        in property <length> font-size;
        callback text-changed(string);
        callback toggle-visibility();
        callback submit();

        height: 48px;
        border-radius: root.corner-radius;
        background: root.bg;
        border-width: 2px;
        border-color: root.brd-color;

        HorizontalLayout {
            padding-left: 16px;
            padding-right: 16px;
            spacing: 8px;

            input := TextInput {
                text: root.password;
                color: root.text-color;
                font-size: root.font-size;
                input-type: root.password-visible ? InputType.text : InputType.password;
                horizontal-alignment: left;
                vertical-alignment: center;
                edited => { root.text-changed(self.text); }
                accepted => { root.submit(); }
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

        callback user-selected(int);
        callback password-changed(string);
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
                background: root.panel-background;
                drop-shadow-blur: 32px;
                drop-shadow-color: #00000040;

                VerticalLayout {
                    padding: root.spacing-xlarge;
                    spacing: root.spacing-large;
                    alignment: center;

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
                        password-visible: root.password-visible;
                        bg: root.elevated;
                        text-color: root.text-primary;
                        brd-color: root.border;
                        corner-radius: root.control-radius;
                        font-size: root.body-size;
                        text-changed(text) => { root.password-changed(text); }
                        toggle-visibility => { root.toggle-password-visibility(); }
                        submit => { if root.can-login { root.login-clicked(); } }
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
                }
            }
        }
    }
}

/// Result of running the login screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginAction {
    /// User successfully authenticated.
    Authenticated,
    /// Window was dismissed without logging in.
    Dismissed,
}

/// Slint-backed login screen renderer.
pub struct LoginRenderer {
    screen: LoginScreen,
}

impl LoginRenderer {
    /// Create a new login renderer.
    pub fn new() -> Result<Self, String> {
        LoginScreen::new()
            .map(|screen| Self { screen })
            .map_err(|error| error.to_string())
    }

    /// Apply a resolved login UI frame.
    pub fn render(&self, frame: &LoginFrame) {
        apply_frame(&self.screen, frame);
    }

    /// Run the login screen until authentication or dismissal.
    ///
    /// `refresh` is called after every user-driven state change (selection,
    /// password edit, visibility toggle) to recompute the frame and re-push
    /// it into the live Slint properties — otherwise things like `can-login`
    /// would stay frozen at whatever they were when `render()` was last
    /// called explicitly.
    pub fn run_until_action(
        &self,
        on_user_selected: impl Fn(usize) + 'static,
        on_password_changed: impl Fn(String) + 'static,
        on_toggle_visibility: impl Fn() + 'static,
        on_login: impl Fn() + 'static,
        refresh: impl Fn() -> LoginFrame + 'static,
    ) -> Result<LoginAction, String> {
        let result = Rc::new(Cell::new(LoginAction::Dismissed));
        let refresh = Rc::new(refresh);

        // Connect user selection
        {
            let weak = self.screen.as_weak();
            let refresh = Rc::clone(&refresh);
            self.screen.on_user_selected(move |index| {
                on_user_selected(index as usize);
                if let Some(screen) = weak.upgrade() {
                    apply_frame(&screen, &refresh());
                }
            });
        }

        // Connect password changes
        {
            let weak = self.screen.as_weak();
            let refresh = Rc::clone(&refresh);
            self.screen.on_password_changed(move |text| {
                on_password_changed(text.to_string());
                if let Some(screen) = weak.upgrade() {
                    apply_frame(&screen, &refresh());
                }
            });
        }

        // Connect visibility toggle
        {
            let weak = self.screen.as_weak();
            let refresh = Rc::clone(&refresh);
            self.screen.on_toggle_password_visibility(move || {
                on_toggle_visibility();
                if let Some(screen) = weak.upgrade() {
                    apply_frame(&screen, &refresh());
                }
            });
        }

        // Connect login button
        let login_result = Rc::clone(&result);
        self.screen.on_login_clicked(move || {
            on_login();
            login_result.set(LoginAction::Authenticated);
            let _ = slint::quit_event_loop();
        });

        self.screen.run().map_err(|error| error.to_string())?;
        Ok(result.get())
    }
}

/// Push a resolved login UI frame into the live Slint screen properties.
fn apply_frame(screen: &LoginScreen, frame: &LoginFrame) {
    // Convert all users to UserRow model
    let user_rows: Vec<UserRow> = frame
        .users
        .iter()
        .enumerate()
        .map(|(index, user)| UserRow {
            username: user.username.clone().into(),
            display_name: user.display_name().into(),
            selected: index == frame.selected_user_index,
        })
        .collect();

    screen.set_users(slint::ModelRc::new(slint::VecModel::from(user_rows)));

    // Set selected user name
    if let Some(user) = &frame.selected_user {
        screen.set_selected_user_name(user.display_name().into());
    }

    // Set password display
    screen.set_password(frame.password.clone().into());
    screen.set_password_visible(frame.password_visible);
    screen.set_can_login(frame.can_login);

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
}

fn to_slint_color(rgba: sol_design::color::Rgba) -> slint::Color {
    slint::Color::from_argb_u8(
        (rgba.3 * 255.0) as u8,
        (rgba.0 * 255.0) as u8,
        (rgba.1 * 255.0) as u8,
        (rgba.2 * 255.0) as u8,
    )
}
