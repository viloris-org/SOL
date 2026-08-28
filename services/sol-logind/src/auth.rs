//! Authentication service.

use anyhow::{Context, Result};
use pam::Authenticator;
use std::sync::{Arc, Mutex};

/// Authentication token returned on successful login.
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// Authenticated username.
    pub username: String,
    /// Session identifier.
    pub session_id: String,
}

/// Authentication service result.
pub type AuthResult = Result<AuthToken>;

/// Authentication mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// Real PAM authentication (production).
    Pam,
    /// Stub authentication that always succeeds (development).
    Stub,
}

/// Authentication service.
///
/// Supports both real PAM authentication and stub mode for development.
pub struct AuthService {
    mode: AuthMode,
}

impl AuthService {
    /// Create a new authentication service with PAM mode.
    pub fn new() -> Self {
        Self {
            mode: AuthMode::Pam,
        }
    }

    /// Create an authentication service in stub mode (for development/testing).
    pub fn new_stub() -> Self {
        Self {
            mode: AuthMode::Stub,
        }
    }

    /// Authenticate a user with username and password.
    pub fn authenticate(&self, username: &str, password: &str) -> AuthResult {
        tracing::info!(
            "Authentication attempt: username={}, password_length={}, mode={:?}",
            username,
            password.len(),
            self.mode
        );

        match self.mode {
            AuthMode::Pam => self.authenticate_pam(username, password),
            AuthMode::Stub => self.authenticate_stub(username, password),
        }
    }

    /// Authenticate using PAM.
    fn authenticate_pam(&self, username: &str, password: &str) -> AuthResult {
        if username.is_empty() {
            anyhow::bail!("Username cannot be empty");
        }

        if password.is_empty() {
            anyhow::bail!("Password cannot be empty");
        }

        // Create a custom conversation handler
        let conv = PasswordConversation::new(username.to_string(), password.to_string());

        // Create PAM authenticator for the "login" service
        let mut auth = Authenticator::with_handler("login", conv)
            .context("Failed to initialize PAM authenticator")?;

        // Perform authentication
        match auth.authenticate() {
            Ok(_) => {
                tracing::info!("PAM authentication successful for user: {}", username);

                // Open PAM session - required for a fully valid login (resource
                // limits, systemd-logind registration, audit trail).
                auth.open_session().context("Failed to open PAM session")?;

                Ok(AuthToken {
                    username: username.to_string(),
                    session_id: format!("session-{}-{}", username, generate_session_id()),
                })
            }
            Err(e) => {
                tracing::warn!("PAM authentication failed for user {}: {:?}", username, e);
                anyhow::bail!("Authentication failed: Invalid username or password")
            }
        }
    }

    /// Authenticate using stub (always succeeds).
    fn authenticate_stub(&self, username: &str, password: &str) -> AuthResult {
        tracing::warn!("Using authentication stub - always succeeds (development mode)");

        if username.is_empty() {
            anyhow::bail!("Username cannot be empty");
        }

        if password.is_empty() {
            anyhow::bail!("Password cannot be empty");
        }

        Ok(AuthToken {
            username: username.to_string(),
            session_id: format!("session-{}-{}", username, generate_session_id()),
        })
    }

    /// Validate if a username exists (for future use).
    pub fn user_exists(&self, username: &str) -> bool {
        !username.is_empty()
    }

    /// Get the current authentication mode.
    pub fn mode(&self) -> AuthMode {
        self.mode
    }
}

impl Default for AuthService {
    fn default() -> Self {
        Self::new()
    }
}

/// PAM conversation handler that provides credentials.
struct PasswordConversation {
    username: String,
    password: Arc<Mutex<Option<String>>>,
}

impl PasswordConversation {
    fn new(username: String, password: String) -> Self {
        Self {
            username,
            password: Arc::new(Mutex::new(Some(password))),
        }
    }
}

impl pam::Converse for PasswordConversation {
    fn username(&self) -> &str {
        &self.username
    }

    fn prompt_echo(&mut self, _msg: &std::ffi::CStr) -> Result<std::ffi::CString, ()> {
        tracing::debug!("PAM prompt (echo): {:?}", _msg);
        Err(())
    }

    fn prompt_blind(&mut self, _msg: &std::ffi::CStr) -> Result<std::ffi::CString, ()> {
        tracing::debug!("PAM prompt (blind): {:?}", _msg);
        // Answer every blind prompt with the supplied password rather than
        // consuming it after the first — PAM stacks with more than one
        // module (e.g. an OTP challenge chained after pam_unix) can issue
        // more than one blind prompt per conversation.
        if let Ok(pass) = self.password.lock()
            && let Some(p) = pass.as_ref()
        {
            return std::ffi::CString::new(p.clone()).map_err(|_| ());
        }
        Err(())
    }

    fn info(&mut self, msg: &std::ffi::CStr) {
        tracing::debug!("PAM info: {:?}", msg);
    }

    fn error(&mut self, msg: &std::ffi::CStr) {
        tracing::warn!("PAM error: {:?}", msg);
    }
}

/// Generate a session ID from a timestamp and a cryptographically random
/// value, so concurrent logins within the same second can't collide and the
/// ID can't be guessed from the login time alone.
fn generate_session_id() -> String {
    use rand::RngCore;
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let random: u64 = rand::thread_rng().next_u64();
    format!("{:x}-{:x}", timestamp_nanos, random)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_stub_always_succeeds() {
        let auth = AuthService::new_stub();
        let result = auth.authenticate("testuser", "password");
        assert!(result.is_ok());
        let token = result.unwrap();
        assert_eq!(token.username, "testuser");
        assert!(!token.session_id.is_empty());
    }

    #[test]
    fn auth_stub_rejects_empty_username() {
        let auth = AuthService::new_stub();
        let result = auth.authenticate("", "password");
        assert!(result.is_err());
    }

    #[test]
    fn auth_stub_rejects_empty_password() {
        let auth = AuthService::new_stub();
        let result = auth.authenticate("testuser", "");
        assert!(result.is_err());
    }

    #[test]
    fn auth_service_accepts_any_non_empty_username() {
        let auth = AuthService::new_stub();
        assert!(auth.user_exists("john"));
        assert!(auth.user_exists("admin"));
        assert!(!auth.user_exists(""));
    }

    #[test]
    fn session_id_is_unique() {
        let auth = AuthService::new_stub();
        let token1 = auth.authenticate("user1", "pass").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let token2 = auth.authenticate("user2", "pass").unwrap();
        assert_ne!(token1.session_id, token2.session_id);
    }

    #[test]
    fn auth_mode_can_be_queried() {
        let pam_auth = AuthService::new();
        assert_eq!(pam_auth.mode(), AuthMode::Pam);

        let stub_auth = AuthService::new_stub();
        assert_eq!(stub_auth.mode(), AuthMode::Stub);
    }
}
