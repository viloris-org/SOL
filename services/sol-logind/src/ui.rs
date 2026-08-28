//! Login UI state machine (renderer-neutral).

use crate::users::UserAccount;
use sol_design::{
    accessibility::TokenMode,
    color::{Color, Rgba},
    radius::Radius,
    spacing::Spacing,
    typography::FontStyle,
};
use sol_ui::Button;

/// Login screen state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginState {
    /// Selecting user from avatar grid.
    SelectingUser,
    /// Entering password for selected user.
    EnteringPassword,
    /// Authenticating (loading state).
    Authenticating,
    /// Authentication succeeded, transitioning to session.
    Authenticated,
}

/// Password field visibility state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordVisibility {
    /// Password is hidden (dots).
    Hidden,
    /// Password is visible (plain text).
    Visible,
}

/// Login UI state machine.
pub struct LoginUi {
    /// Available user accounts.
    pub users: Vec<UserAccount>,
    /// Currently selected user index.
    pub selected_user_index: usize,
    /// Password input text.
    pub password: String,
    /// Password visibility toggle.
    pub password_visible: PasswordVisibility,
    /// Current login state.
    pub state: LoginState,
    /// Login button controller.
    pub login_button: sol_ui::ButtonController,
}

impl LoginUi {
    /// Create a new login UI with the given users.
    pub fn new(users: Vec<UserAccount>) -> Self {
        Self {
            users,
            selected_user_index: 0,
            password: String::new(),
            password_visible: PasswordVisibility::Hidden,
            state: LoginState::SelectingUser,
            login_button: sol_ui::ButtonController::new(
                Button::new().with_label("Log In").primary(),
            ),
        }
    }

    /// Get the currently selected user.
    pub fn selected_user(&self) -> Option<&UserAccount> {
        self.users.get(self.selected_user_index)
    }

    /// Select the next user in the avatar grid.
    pub fn select_next_user(&mut self) {
        if !self.users.is_empty() {
            self.selected_user_index = (self.selected_user_index + 1) % self.users.len();
            self.password.clear();
            self.state = LoginState::EnteringPassword;
        }
    }

    /// Select the previous user in the avatar grid.
    pub fn select_previous_user(&mut self) {
        if !self.users.is_empty() {
            if self.selected_user_index == 0 {
                self.selected_user_index = self.users.len() - 1;
            } else {
                self.selected_user_index -= 1;
            }
            self.password.clear();
            self.state = LoginState::EnteringPassword;
        }
    }

    /// Select a specific user by index.
    pub fn select_user(&mut self, index: usize) {
        if index < self.users.len() {
            self.selected_user_index = index;
            self.password.clear();
            self.state = LoginState::EnteringPassword;
        }
    }

    /// Toggle password visibility.
    pub fn toggle_password_visibility(&mut self) {
        self.password_visible = match self.password_visible {
            PasswordVisibility::Hidden => PasswordVisibility::Visible,
            PasswordVisibility::Visible => PasswordVisibility::Hidden,
        };
    }

    /// Update password input.
    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    /// Check if login button should be enabled.
    pub fn can_login(&self) -> bool {
        !self.password.is_empty() && matches!(self.state, LoginState::EnteringPassword)
    }

    /// Begin authentication process.
    pub fn begin_authentication(&mut self) {
        if self.can_login() {
            self.state = LoginState::Authenticating;
        }
    }

    /// Mark authentication as complete.
    pub fn authentication_complete(&mut self) {
        self.state = LoginState::Authenticated;
    }

    /// Reset to initial state (after failed auth or cancel).
    pub fn reset(&mut self) {
        self.password.clear();
        self.password_visible = PasswordVisibility::Hidden;
        self.state = LoginState::EnteringPassword;
    }

    /// Resolve the UI into a renderer-neutral frame.
    pub fn frame_for(&self, mode: TokenMode) -> LoginFrame {
        LoginFrame {
            users: self.users.clone(),
            selected_user_index: self.selected_user_index,
            selected_user: self.selected_user().cloned(),
            password: match self.password_visible {
                PasswordVisibility::Hidden => "•".repeat(self.password.len()),
                PasswordVisibility::Visible => self.password.clone(),
            },
            password_visible: matches!(self.password_visible, PasswordVisibility::Visible),
            state: self.state,
            can_login: self.can_login(),
            // Visual tokens
            page_background: mode.color(Color::Surface),
            panel_background: mode.color(Color::Elevated),
            text_primary: mode.color(Color::TextPrimary),
            text_secondary: mode.color(Color::TextSecondary),
            accent: mode.color(Color::Accent),
            border: mode.color(Color::Border),
            elevated: mode.color(Color::Elevated),
            display_size: mode.typography(FontStyle::Display).pixels,
            title_size: mode.typography(FontStyle::Title).pixels,
            body_size: mode.typography(FontStyle::Body).pixels,
            label_size: mode.typography(FontStyle::Label).pixels,
            panel_radius: Radius::Md.px(),
            control_radius: Radius::Sm.px(),
            avatar_radius: Radius::Full.px(),
            spacing_small: Spacing::Sm.px(),
            spacing_medium: Spacing::Md.px(),
            spacing_large: Spacing::Lg.px(),
            spacing_xlarge: Spacing::Xl.px(),
            login_button: self.login_button.frame_for(mode),
        }
    }
}

/// Fully resolved login UI frame for rendering.
#[derive(Debug, Clone)]
pub struct LoginFrame {
    // State
    pub users: Vec<UserAccount>,
    pub selected_user_index: usize,
    pub selected_user: Option<UserAccount>,
    pub password: String,
    pub password_visible: bool,
    pub state: LoginState,
    pub can_login: bool,

    // Visual tokens
    pub page_background: Rgba,
    pub panel_background: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub accent: Rgba,
    pub border: Rgba,
    pub elevated: Rgba,

    // Typography
    pub display_size: f32,
    pub title_size: f32,
    pub body_size: f32,
    pub label_size: f32,

    // Metrics
    pub panel_radius: f32,
    pub control_radius: f32,
    pub avatar_radius: f32,
    pub spacing_small: f32,
    pub spacing_medium: f32,
    pub spacing_large: f32,
    pub spacing_xlarge: f32,

    // Controls
    pub login_button: sol_ui::ButtonFrame,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::UserAccount;

    fn mock_users() -> Vec<UserAccount> {
        vec![
            UserAccount::new("john".into(), "John Doe".into(), 1000),
            UserAccount::new("jane".into(), "Jane Smith".into(), 1001),
        ]
    }

    #[test]
    fn login_ui_starts_with_first_user_selected() {
        let ui = LoginUi::new(mock_users());
        assert_eq!(ui.selected_user_index, 0);
        assert_eq!(ui.selected_user().unwrap().username, "john");
    }

    #[test]
    fn login_ui_can_cycle_through_users() {
        let mut ui = LoginUi::new(mock_users());
        ui.select_next_user();
        assert_eq!(ui.selected_user().unwrap().username, "jane");
        ui.select_next_user();
        assert_eq!(ui.selected_user().unwrap().username, "john");
    }

    #[test]
    fn login_ui_can_cycle_backwards() {
        let mut ui = LoginUi::new(mock_users());
        ui.select_previous_user();
        assert_eq!(ui.selected_user().unwrap().username, "jane");
    }

    #[test]
    fn login_requires_non_empty_password() {
        let mut ui = LoginUi::new(mock_users());
        ui.state = LoginState::EnteringPassword;
        assert!(!ui.can_login());
        ui.set_password("password".into());
        assert!(ui.can_login());
    }

    #[test]
    fn password_visibility_toggle_works() {
        let mut ui = LoginUi::new(mock_users());
        assert_eq!(ui.password_visible, PasswordVisibility::Hidden);
        ui.toggle_password_visibility();
        assert_eq!(ui.password_visible, PasswordVisibility::Visible);
        ui.toggle_password_visibility();
        assert_eq!(ui.password_visible, PasswordVisibility::Hidden);
    }

    #[test]
    fn selecting_user_clears_password() {
        let mut ui = LoginUi::new(mock_users());
        ui.set_password("secret".into());
        ui.select_user(1);
        assert!(ui.password.is_empty());
    }

    #[test]
    fn authentication_state_machine_progresses() {
        let mut ui = LoginUi::new(mock_users());
        ui.state = LoginState::EnteringPassword;
        ui.set_password("pass".into());
        ui.begin_authentication();
        assert_eq!(ui.state, LoginState::Authenticating);
        ui.authentication_complete();
        assert_eq!(ui.state, LoginState::Authenticated);
    }
}
