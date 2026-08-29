//! Security coordinator interface — communicates with sol-securityd.

use crate::scp::capability::{Capability, CapabilityToken, Decision};
use crate::scp::random;
use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Authenticated application identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppId(pub String);

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Security coordinator trait — abstracts sol-securityd IPC.
///
/// In production this communicates with sol-securityd over D-Bus or a
/// dedicated Unix socket. For testing/Phase 1, a stub implementation grants
/// default capabilities without external coordination.
pub trait SecurityCoordinator: Send + Sync {
    /// Verify application identity from process credentials.
    ///
    /// Checks that the PID belongs to a legitimate app bundle with a verified
    /// publisher signature. Returns `None` if the PID is invalid or the app
    /// identity cannot be authenticated.
    fn verify_app_identity(&self, pid: u32) -> Option<AppId>;

    /// Evaluate a capability request.
    ///
    /// Returns an immediate decision (granted/denied) or indicates that user
    /// consent is required. The compositor should call this before honoring
    /// any capability-gated operation.
    fn evaluate_capability(&self, app_id: &AppId, cap: &Capability) -> Decision;

    /// Issue a signed capability token.
    ///
    /// Used when a capability is granted; the token is returned to the client
    /// and later verified by `verify_token`.
    fn issue_token(&self, app_id: &AppId, cap: &Capability) -> CapabilityToken;

    /// Verify that a token is valid and matches the claimed capability.
    ///
    /// Returns `Some((app_id, capability))` if valid, `None` if forged/expired.
    fn verify_token(&self, token: &CapabilityToken) -> Option<(AppId, Capability)>;

    /// Audit a capability use.
    ///
    /// Logs to sol-securityd's audit trail for security review. Called after
    /// every sensitive operation (screen capture, clipboard read, etc).
    fn audit_capability_use(&self, app_id: &AppId, cap: &Capability, outcome: AuditOutcome);

    /// Release tokens that will never be presented again.
    ///
    /// Called when a session ends. A grant does not outlive the connection that
    /// holds it, so a coordinator that keeps issued tokens can forget these —
    /// without this, a client reconnecting in a loop grows the coordinator's
    /// bookkeeping for the life of the compositor.
    ///
    /// Defaults to doing nothing, for coordinators that verify tokens by
    /// signature and keep no state to release.
    fn release_tokens(&self, _tokens: &[CapabilityToken]) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    Granted,
    Denied,
    Used,
}

/// Verified identity permitted to engage the session lock.
///
/// Real policy belongs in sol-securityd; this constant is what the stub
/// coordinator enforces until that IPC exists.
pub const LOCK_SERVICE_APP_ID: &str = "sol-logind";

/// Verified identity permitted to use layer shell.
pub const SHELL_APP_ID: &str = "sol-shell";

/// Identities that carry privilege and must never be taken on a peer's word.
///
/// Ordinary applications are identified by `/proc/<pid>/comm`, which is a label
/// the process writes for itself — fine for telemetry, worthless as proof. These
/// two names gate the layer shell and the session lock, so for them the peer's
/// *executable* has to check out as well: a process can rename itself freely, but
/// it cannot rewrite which binary it was launched from.
pub const RESERVED_APP_IDS: &[&str] = &[SHELL_APP_ID, LOCK_SERVICE_APP_ID];

/// Where a reserved identity's executable must live.
///
/// Only the system image writes here, which is what makes membership meaningful.
const DEFAULT_TRUSTED_BIN_DIR: &str = "/usr/lib/sol";

/// Environment override for [`DEFAULT_TRUSTED_BIN_DIR`], for development and
/// tests. It is read from the *compositor's* environment, which is set by
/// whatever launches the session, never by a connecting client.
const TRUSTED_BIN_DIR_ENV: &str = "SOL_SCP_TRUSTED_BIN_DIR";

fn trusted_bin_dir() -> PathBuf {
    std::env::var_os(TRUSTED_BIN_DIR_ENV)
        .map_or_else(|| PathBuf::from(DEFAULT_TRUSTED_BIN_DIR), PathBuf::from)
}

/// Whether the process behind `proc_dir` runs the installed binary for `name`.
///
/// Fails closed: an unresolvable executable, a missing trusted directory, or a
/// binary sitting anywhere else all mean "not that service". The failure mode is
/// a privileged component refusing to start, which is noisy and recoverable —
/// the opposite mistake hands the lock screen to anything that asks.
fn runs_trusted_executable(proc_dir: &Path, name: &str) -> bool {
    executable_matches(proc_dir, name, &trusted_bin_dir())
}

/// Whether the process behind `proc_dir` runs `trusted/name`.
///
/// Split out from [`runs_trusted_executable`] so the check can be exercised
/// against a fixture directory instead of the system's install path.
fn executable_matches(proc_dir: &Path, name: &str, trusted: &Path) -> bool {
    let Ok(trusted) = trusted.canonicalize() else {
        return false;
    };
    // /proc/<pid>/exe resolves to the backing file even if the path was since
    // replaced, so this reads the binary actually running, not a name.
    let Ok(executable) = std::fs::read_link(proc_dir.join("exe")) else {
        return false;
    };
    let Ok(executable) = executable.canonicalize() else {
        return false;
    };
    executable.parent() == Some(trusted.as_path()) && executable.file_name() == Some(name.as_ref())
}

/// Width of an issued capability token.
const TOKEN_BYTES: usize = 32;

/// Stub security coordinator for Phase 1 development.
///
/// Grants default capabilities without external coordination. Used until
/// sol-securityd IPC is implemented.
#[derive(Default)]
pub struct StubSecurityCoordinator {
    tokens: Mutex<TokenRegistry>,
}

/// What a live token stands for.
#[derive(Debug, Clone)]
struct TokenRecord {
    app_id: AppId,
    capability: Capability,
    expires_at: Option<std::time::Instant>,
}

/// Live tokens, keyed by their opaque bytes.
///
/// Two sessions of the same application hold distinct tokens for the same
/// capability, so entries cannot be collapsed per `(app, capability)`. They are
/// dropped when the session that holds them goes away — see
/// [`SecurityCoordinator::release_tokens`] — which is what keeps a client
/// reconnecting in a loop from growing this without bound.
type TokenRegistry = HashMap<Vec<u8>, TokenRecord>;

impl SecurityCoordinator for StubSecurityCoordinator {
    /// Derive an identity from the peer's process, refusing to invent privilege.
    ///
    /// A [reserved](RESERVED_APP_IDS) name is only honored when the peer's
    ///    executable is the installed one, because `comm` is self-assigned.
    ///
    /// UID admission is enforced by the transport against the compositor's
    /// active session UID before this identity verifier runs. Keeping the two
    /// checks separate lets one system compositor safely outlive many users.
    ///
    /// Two gaps remain, both of which belong to real identity verification in
    /// sol-securityd rather than to this stub:
    ///
    /// - The PID comes from `SO_PEERCRED` and could in principle be recycled
    ///   between the connect and these reads. Closing that needs a pidfd held
    ///   across the check.
    /// - A same-UID process may `ptrace` the installed binary and drive it.
    ///   Nothing readable from `/proc` distinguishes that from the real service;
    ///   it needs a kernel-side attestation, or `yama.ptrace_scope` raised.
    fn verify_app_identity(&self, pid: u32) -> Option<AppId> {
        let proc_dir = PathBuf::from(format!("/proc/{pid}"));

        let name = std::fs::read_to_string(proc_dir.join("comm"))
            .ok()?
            .trim()
            .to_string();
        if name.is_empty() {
            return None;
        }

        if RESERVED_APP_IDS.contains(&name.as_str()) && !runs_trusted_executable(&proc_dir, &name) {
            tracing::warn!(
                pid,
                %name,
                trusted_dir = %trusted_bin_dir().display(),
                "refused a reserved identity claimed by an untrusted executable; a development \
                 build must set {TRUSTED_BIN_DIR_ENV} to the directory it runs from",
            );
            return None;
        }

        Some(AppId(name))
    }

    fn evaluate_capability(&self, app_id: &AppId, cap: &Capability) -> Decision {
        use crate::scp::capability;

        // Grant default capabilities immediately
        if capability::default_app_capabilities().contains(cap) {
            return Decision::Granted {
                token: self.issue_token(app_id, cap),
                expires_at: None,
            };
        }

        // Deny shell-only capabilities to non-shell apps
        if capability::shell_only_capabilities().contains(cap) && app_id.0 != SHELL_APP_ID {
            return Decision::Denied {
                reason: format!("Reserved for {SHELL_APP_ID}"),
            };
        }

        // The session lock is the authentication surface: nothing but the login
        // service may draw it, not even the shell. The stub grants everything
        // else for development, so this has to be denied explicitly rather than
        // left to fall through.
        if capability::lock_only_capabilities().contains(cap) && app_id.0 != LOCK_SERVICE_APP_ID {
            return Decision::Denied {
                reason: format!("Reserved for {LOCK_SERVICE_APP_ID}"),
            };
        }

        // Stub: grant everything else for development
        Decision::Granted {
            token: self.issue_token(app_id, cap),
            expires_at: None,
        }
    }

    fn issue_token(&self, app_id: &AppId, cap: &Capability) -> CapabilityToken {
        let mut data = vec![0_u8; TOKEN_BYTES];
        if let Err(error) = random::fill_bytes(&mut data) {
            // getrandom cannot fail for a small blocking read on a running
            // system, so this is unreachable rather than merely unlikely. Should
            // it ever happen, an unusable token is the only safe answer: it is
            // registered nowhere, so every verification of it fails.
            tracing::error!(?error, "failed to generate a capability token");
            return CapabilityToken {
                data: Vec::new(),
                expires_at: None,
                one_time: false,
            };
        }

        let token = CapabilityToken {
            data,
            expires_at: None,
            one_time: false,
        };
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(
                token.data.clone(),
                TokenRecord {
                    app_id: app_id.clone(),
                    capability: cap.clone(),
                    expires_at: token.expires_at,
                },
            );
        }
        token
    }

    fn verify_token(&self, token: &CapabilityToken) -> Option<(AppId, Capability)> {
        // An empty token is what issuing hands back when it could not produce
        // random bytes. Nothing registers one, but refuse it explicitly rather
        // than relying on a lookup miss.
        if token.data.is_empty() || token.is_expired() {
            return None;
        }
        let mut tokens = self.tokens.lock().ok()?;
        let record = tokens.get(&token.data)?.clone();
        if record
            .expires_at
            .is_some_and(|expiry| std::time::Instant::now() >= expiry)
        {
            tokens.remove(&token.data);
            return None;
        }
        if token.one_time {
            tokens.remove(&token.data);
        }
        Some((record.app_id, record.capability))
    }

    fn audit_capability_use(&self, app_id: &AppId, cap: &Capability, outcome: AuditOutcome) {
        tracing::debug!(?app_id, ?cap, ?outcome, "capability audit (stub)");
    }

    fn release_tokens(&self, tokens: &[CapabilityToken]) {
        if let Ok(mut registry) = self.tokens.lock() {
            for token in tokens {
                registry.remove(&token.data);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own_proc_dir() -> PathBuf {
        PathBuf::from(format!("/proc/{}", std::process::id()))
    }

    /// The directory the running test binary lives in, and its file name.
    fn own_executable() -> (PathBuf, String) {
        let executable = std::fs::read_link(own_proc_dir().join("exe"))
            .and_then(|path| path.canonicalize())
            .expect("a process can always resolve its own executable");
        let directory = executable
            .parent()
            .expect("an executable has a parent directory")
            .to_path_buf();
        let name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the test binary has a UTF-8 name")
            .to_string();
        (directory, name)
    }

    #[test]
    fn a_binary_outside_the_trusted_directory_is_not_the_service() {
        // The test binary is not installed as sol-logind, so nothing about it
        // should satisfy the check — this is the shape of the attack that used
        // to work: right name, wrong binary.
        assert!(!executable_matches(
            &own_proc_dir(),
            LOCK_SERVICE_APP_ID,
            Path::new("/usr/lib/sol"),
        ));
    }

    #[test]
    fn the_installed_binary_is_recognized() {
        // Point the check at the directory the test binary really runs from and
        // ask about its real name: that is the one case that must pass, and it
        // proves the check is matching on the executable rather than rejecting
        // everything unconditionally.
        let (directory, name) = own_executable();
        assert!(executable_matches(&own_proc_dir(), &name, &directory));
    }

    #[test]
    fn the_right_name_in_the_wrong_directory_is_refused() {
        let (_, name) = own_executable();
        let elsewhere = std::env::temp_dir();
        assert!(!executable_matches(&own_proc_dir(), &name, &elsewhere));
    }

    #[test]
    fn a_missing_trusted_directory_refuses_rather_than_admits() {
        let (_, name) = own_executable();
        let absent = std::env::temp_dir().join("sol-scp-no-such-trusted-dir");
        assert!(
            !executable_matches(&own_proc_dir(), &name, &absent),
            "an uninstalled system must not be a system where anything is trusted"
        );
    }

    #[test]
    fn reserved_names_survive_the_kernel_comm_limit() {
        // `/proc/<pid>/comm` holds 15 characters plus a NUL. A reserved name
        // longer than that could never be read back intact, so it would never
        // match RESERVED_APP_IDS — and would silently stop being reserved.
        for reserved in RESERVED_APP_IDS {
            assert!(
                reserved.len() <= 15,
                "'{reserved}' is too long to round-trip through comm, so the \
                 reserved-name check would never fire for it"
            );
        }
    }

    #[test]
    fn comm_alone_never_yields_a_reserved_identity() {
        // verify_app_identity refuses a reserved name whose executable does not
        // check out. The test binary is never installed as one of these, so
        // whatever it calls itself, it cannot become one.
        for reserved in RESERVED_APP_IDS {
            assert!(
                !runs_trusted_executable(&own_proc_dir(), reserved),
                "{reserved} must not be claimable by an arbitrary binary"
            );
        }
    }

    #[test]
    fn an_ordinary_identity_is_accepted() {
        let coordinator = StubSecurityCoordinator::default();
        let identity = coordinator.verify_app_identity(std::process::id());
        assert!(
            identity.is_some(),
            "a same-user process with an unreserved name is a normal app"
        );
    }

    #[test]
    fn two_sessions_of_one_app_hold_independent_tokens() {
        let coordinator = StubSecurityCoordinator::default();
        let app = AppId("org.sol.test".to_string());

        // The same application opening a second window is a second connection,
        // not a replacement for the first: retiring the earlier token here would
        // disconnect a running window.
        let first = coordinator.issue_token(&app, &Capability::WindowToplevel);
        let second = coordinator.issue_token(&app, &Capability::WindowToplevel);

        assert_ne!(first.data, second.data, "tokens must be unique");
        assert!(coordinator.verify_token(&first).is_some());
        assert!(coordinator.verify_token(&second).is_some());
    }

    #[test]
    fn releasing_a_sessions_tokens_clears_the_registry() {
        let coordinator = StubSecurityCoordinator::default();
        let app = AppId("org.sol.test".to_string());

        let held = coordinator.issue_token(&app, &Capability::WindowToplevel);
        let released = coordinator.issue_token(&app, &Capability::WindowPopup);
        coordinator.release_tokens(std::slice::from_ref(&released));

        assert!(
            coordinator.verify_token(&released).is_none(),
            "a departed session's token must stop verifying"
        );
        assert!(
            coordinator.verify_token(&held).is_some(),
            "another session's token is untouched"
        );
        assert_eq!(coordinator.tokens.lock().expect("token lock").len(), 1);
    }

    #[test]
    fn tokens_for_different_capabilities_coexist() {
        let coordinator = StubSecurityCoordinator::default();
        let app = AppId("org.sol.test".to_string());

        let window = coordinator.issue_token(&app, &Capability::WindowToplevel);
        let popup = coordinator.issue_token(&app, &Capability::WindowPopup);

        assert!(coordinator.verify_token(&window).is_some());
        assert!(coordinator.verify_token(&popup).is_some());
    }

    #[test]
    fn a_one_time_token_verifies_exactly_once() {
        let coordinator = StubSecurityCoordinator::default();
        let app = AppId("org.sol.test".to_string());
        let mut token = coordinator.issue_token(&app, &Capability::ClipboardRead);
        token.one_time = true;

        assert!(coordinator.verify_token(&token).is_some());
        assert!(
            coordinator.verify_token(&token).is_none(),
            "a single-use token must be consumed on first use"
        );
    }
}
