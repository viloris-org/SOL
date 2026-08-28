//! sol-logind — SOL's login screen service.
//!
//! A macOS-inspired authentication interface that displays before the user
//! session starts. Built with SolKit components and sol-design tokens.
//!
//! # Architecture
//!
//! - Runs as a system service before compositor/shell
//! - Displays visual login UI using Material::Floating panel
//! - Handles user selection from avatar grid
//! - Authenticates via password (PAM integration in Phase 2)
//! - Spawns user session after successful login
//!
//! # Phase 1
//!
//! Visual-only implementation with authentication stub:
//! - macOS-like UI layout with generous spacing
//! - User avatar grid for account selection
//! - Password field with show/hide toggle
//! - Primary "Log In" button styled with accent color
//! - Authentication stub (always succeeds for development)

pub mod auth;
pub mod render;
pub mod ui;
pub mod users;

pub use auth::{AuthMode, AuthResult, AuthService, AuthToken};
pub use render::{LoginAction, LoginRenderer};
pub use ui::{LoginFrame, LoginState, LoginUi, PasswordVisibility};
pub use users::{UserAccount, UserMode, UserService};

use anyhow::Result;
use sol_app::{App, AppId};

/// Login service application.
pub struct LoginService {
    /// Application instance.
    pub app: App,
    /// User service for account enumeration.
    pub user_service: UserService,
    /// Authentication service.
    pub auth_service: AuthService,
    /// Login UI state.
    pub ui: LoginUi,
}

impl LoginService {
    /// Create a new login service.
    pub fn new() -> Result<Self> {
        let app_id = AppId::parse("org.sol.login")?;
        let mut user_service = UserService::new();
        user_service.load_users()?;

        let users = user_service.users().to_vec();
        let ui = LoginUi::new(users);

        Ok(Self {
            app: App::new(app_id),
            user_service,
            auth_service: AuthService::new(),
            ui,
        })
    }

    /// Create a new login service in development mode (mock users, stub auth).
    pub fn new_development() -> Result<Self> {
        let app_id = AppId::parse("org.sol.login")?;
        let mut user_service = UserService::new_mock();
        user_service.load_users()?;

        let users = user_service.users().to_vec();
        let ui = LoginUi::new(users);

        Ok(Self {
            app: App::new(app_id),
            user_service,
            auth_service: AuthService::new_stub(),
            ui,
        })
    }

    /// Attempt to authenticate the current user/password.
    pub fn authenticate(&mut self) -> Result<AuthToken> {
        if !self.ui.can_login() {
            anyhow::bail!("Cannot authenticate: no user selected or password not entered");
        }

        // Extract username and password before mutating
        let (username, password) = if let Some(user) = self.ui.selected_user() {
            (user.username.clone(), self.ui.password.clone())
        } else {
            anyhow::bail!("No user selected")
        };

        self.ui.begin_authentication();
        let result = self.auth_service.authenticate(&username, &password);

        match result {
            Ok(token) => {
                self.ui.authentication_complete();
                tracing::info!("Authentication successful for user: {}", username);
                Ok(token)
            }
            Err(e) => {
                tracing::warn!("Authentication failed: {}", e);
                self.ui.reset();
                Err(e)
            }
        }
    }

    /// Start the login service.
    pub fn start(&mut self) -> Result<()> {
        self.app.start()?;
        tracing::info!("Login service started");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_service_can_be_created() {
        let service = LoginService::new_development();
        assert!(service.is_ok());
    }

    #[test]
    fn login_service_loads_users_on_creation() {
        let service = LoginService::new_development().unwrap();
        assert!(!service.user_service.users().is_empty());
        assert!(!service.ui.users.is_empty());
    }

    #[test]
    fn login_service_can_authenticate() {
        let mut service = LoginService::new_development().unwrap();
        service.ui.state = LoginState::EnteringPassword;
        service.ui.set_password("testpass".into());

        let result = service.authenticate();
        assert!(result.is_ok());

        let token = result.unwrap();
        assert_eq!(token.username, service.ui.selected_user().unwrap().username);
    }

    #[test]
    fn login_service_authentication_updates_ui_state() {
        let mut service = LoginService::new_development().unwrap();
        service.ui.state = LoginState::EnteringPassword;
        service.ui.set_password("testpass".into());

        service.authenticate().unwrap();
        assert_eq!(service.ui.state, LoginState::Authenticated);
    }
}
