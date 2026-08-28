//! User account enumeration and avatar loading.

use std::path::PathBuf;
use uzers::os::unix::UserExt;

/// User loading mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMode {
    /// Load real users from /etc/passwd (production).
    System,
    /// Use mock users (development/testing).
    Mock,
}

/// A user account available for login.
#[derive(Debug, Clone)]
pub struct UserAccount {
    /// System username (login name).
    pub username: String,
    /// Display name (full name).
    pub full_name: String,
    /// Path to user avatar image, if available.
    pub avatar_path: Option<PathBuf>,
    /// User ID.
    pub uid: u32,
}

impl UserAccount {
    /// Create a new user account.
    pub fn new(username: String, full_name: String, uid: u32) -> Self {
        Self {
            username,
            full_name,
            avatar_path: None,
            uid,
        }
    }

    /// Set the avatar path.
    pub fn with_avatar(mut self, path: impl Into<PathBuf>) -> Self {
        self.avatar_path = Some(path.into());
        self
    }

    /// Get the display name, falling back to username if no full name.
    pub fn display_name(&self) -> &str {
        if self.full_name.is_empty() {
            &self.username
        } else {
            &self.full_name
        }
    }
}

/// Service for enumerating user accounts.
pub struct UserService {
    users: Vec<UserAccount>,
    mode: UserMode,
}

impl UserService {
    /// Create a new user service with system mode (reads /etc/passwd).
    pub fn new() -> Self {
        Self {
            users: Vec::new(),
            mode: UserMode::System,
        }
    }

    /// Create a user service with mock users (for development/testing).
    pub fn new_mock() -> Self {
        Self {
            users: Vec::new(),
            mode: UserMode::Mock,
        }
    }

    /// Load user accounts from the system.
    pub fn load_users(&mut self) -> anyhow::Result<()> {
        self.users = match self.mode {
            UserMode::System => self.load_system_users()?,
            UserMode::Mock => self.load_mock_users(),
        };

        tracing::info!(
            "Loaded {} user accounts (mode: {:?})",
            self.users.len(),
            self.mode
        );
        Ok(())
    }

    /// Load users from /etc/passwd.
    fn load_system_users(&self) -> anyhow::Result<Vec<UserAccount>> {
        let mut users = Vec::new();

        // Walk the passwd database in a single pass (getpwent) instead of
        // probing every UID in 1000..65534 individually — on systems backed
        // by LDAP/SSSD/AD, each individual UID lookup can be a name-service
        // round trip, and 64k of them at login-screen startup is very slow.
        //
        // Safety: this is the only place in the process that iterates the
        // passwd database, and it runs to completion (single-threaded,
        // during startup) before any other `all_users()` call could occur.
        for user in unsafe { uzers::all_users() } {
            let uid = user.uid();
            // Normal users typically have UID >= 1000 and < 65534
            if !(1000..65534).contains(&uid) {
                continue;
            }

            // Skip users with /usr/sbin/nologin or /bin/false as shell
            let shell = user.shell().to_string_lossy();
            if shell.contains("nologin") || shell.contains("false") {
                continue;
            }

            let username = user.name().to_string_lossy().to_string();

            // Parse GECOS field for full name (first field before comma)
            let gecos = user.gecos();
            let full_name = gecos
                .to_string_lossy()
                .split(',')
                .next()
                .unwrap_or("")
                .to_string();

            // Try to find user avatar
            let avatar_path = find_user_avatar(&username, user.home_dir());

            let account = UserAccount {
                username,
                full_name,
                avatar_path,
                uid,
            };

            users.push(account);
        }

        // Sort by UID (typically the first created user comes first)
        users.sort_by_key(|u| u.uid);

        if users.is_empty() {
            tracing::warn!("No normal users found on system, falling back to mock users");
            Ok(self.load_mock_users())
        } else {
            Ok(users)
        }
    }

    /// Load mock users for development.
    fn load_mock_users(&self) -> Vec<UserAccount> {
        vec![
            UserAccount::new("john".into(), "John Appleseed".into(), 1000),
            UserAccount::new("jane".into(), "Jane Smith".into(), 1001),
            UserAccount::new("admin".into(), "Administrator".into(), 1002),
        ]
    }

    /// Get all available user accounts.
    pub fn users(&self) -> &[UserAccount] {
        &self.users
    }

    /// Get a user by username.
    pub fn find_user(&self, username: &str) -> Option<&UserAccount> {
        self.users.iter().find(|u| u.username == username)
    }

    /// Get the current user loading mode.
    pub fn mode(&self) -> UserMode {
        self.mode
    }
}

impl Default for UserService {
    fn default() -> Self {
        Self::new()
    }
}

/// Try to find a user's avatar image.
///
/// Checks common locations:
/// - {home}/.face
/// - {home}/.face.icon
/// - /var/lib/AccountsService/icons/{username}
fn find_user_avatar(username: &str, home: &std::path::Path) -> Option<PathBuf> {
    let candidates = vec![
        home.join(".face"),
        home.join(".face.icon"),
        PathBuf::from(format!("/var/lib/AccountsService/icons/{}", username)),
    ];

    candidates.into_iter().find(|p| p.exists() && p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_account_display_name_prefers_full_name() {
        let user = UserAccount::new("jdoe".into(), "John Doe".into(), 1000);
        assert_eq!(user.display_name(), "John Doe");
    }

    #[test]
    fn user_account_display_name_falls_back_to_username() {
        let user = UserAccount::new("jdoe".into(), "".into(), 1000);
        assert_eq!(user.display_name(), "jdoe");
    }

    #[test]
    fn user_service_loads_mock_users() {
        let mut service = UserService::new_mock();
        service.load_users().unwrap();
        assert!(!service.users().is_empty());
        assert_eq!(service.users().len(), 3);
    }

    #[test]
    fn user_service_can_find_user_by_username() {
        let mut service = UserService::new_mock();
        service.load_users().unwrap();
        let user = service.find_user("john");
        assert!(user.is_some());
        assert_eq!(user.unwrap().display_name(), "John Appleseed");
    }

    #[test]
    fn user_mode_can_be_queried() {
        let system_service = UserService::new();
        assert_eq!(system_service.mode(), UserMode::System);

        let mock_service = UserService::new_mock();
        assert_eq!(mock_service.mode(), UserMode::Mock);
    }
}
