//! sol-logind — SOL's login screen service.
//!
//! The authentication surface for a SOL machine: it decides who gets a session,
//! and it is the only client the compositor will let cover the whole screen with
//! exclusive input.
//!
//! # Architecture
//!
//! sol-logind is an SCP client, not a display server. The compositor starts
//! first; the greeter connects to it, engages the session lock
//! ([`Capability::SessionLock`], reserved for this service by name), and
//! presents the login UI on a lock surface:
//!
//! ```text
//! sol-compositor (SCP socket)
//!         ↓
//! sol-logind connects → LockSession → login UI on the lock surface
//!         ↓
//! user authenticates (PAM)
//!         ↓
//! sol-session starts behind the lock → shell commits → animated UnlockSession
//!         ↓
//! session ends → LockSession again
//! ```
//!
//! The lock is what makes this safe to build on. It sits above every layer, so
//! the shell cannot cover or forge it; it takes keyboard focus exclusively, so
//! keystrokes cannot reach a window behind it; and if this process dies the
//! session stays locked rather than falling open (see
//! `sol_compositor::scp::session_lock`).
//!
//! # Layering
//!
//! - [`users`] enumerates accounts, [`auth`] authenticates them through PAM
//! - [`ui`] is the state machine: it owns the password and every login decision,
//!   and resolves to a renderer-neutral [`ui::LoginFrame`]
//! - [`render`] rasterizes that frame with Slint's software renderer
//! - [`scp`] carries the frame to the compositor and input events back
//! - [`session`] launches the authenticated user's desktop
//!
//! Keyboard input goes to [`ui`], never through Slint: the password is this
//! crate's to hold. Pointer input goes to [`render`], because Slint is what
//! knows where the avatars and buttons ended up.
//!
//! [`Capability::SessionLock`]: sol_compositor::scp::capability::Capability::SessionLock

pub mod auth;
pub mod handoff;
pub mod render;
pub mod scp;
pub mod session;
pub mod ui;
pub mod users;

pub use auth::{AuthMode, AuthOutcome, AuthResult, AuthService, AuthToken, PamSession};
pub use handoff::{HandoffVisual, SessionHandoff};
pub use render::{LoginAction, LoginRenderer};
pub use scp::{
    FrameBuffer, KeyInput, LockDriver, LockError, LockEvent, LockPhase, Modifiers, ScpClient,
};
pub use session::{PendingUserSession, start_user_session};
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
    pub fn authenticate(&mut self) -> Result<AuthOutcome> {
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
            Ok(outcome) => {
                self.ui.authentication_complete();
                tracing::info!("Authentication successful for user: {}", username);
                Ok(outcome)
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

        let outcome = result.unwrap();
        assert_eq!(
            outcome.token.username,
            service.ui.selected_user().unwrap().username
        );
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
