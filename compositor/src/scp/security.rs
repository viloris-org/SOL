//! Security coordinator interface — communicates with sol-securityd.

use crate::scp::capability::{Capability, CapabilityToken, Decision};
use crate::scp::random;
use std::{collections::HashMap, fmt, sync::Mutex};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    Granted,
    Denied,
    Used,
}

/// Stub security coordinator for Phase 1 development.
///
/// Grants default capabilities without external coordination. Used until
/// sol-securityd IPC is implemented.
#[derive(Default)]
pub struct StubSecurityCoordinator {
    tokens: Mutex<TokenRegistry>,
}

type TokenRegistry = HashMap<Vec<u8>, (AppId, Capability, Option<std::time::Instant>)>;

impl SecurityCoordinator for StubSecurityCoordinator {
    fn verify_app_identity(&self, pid: u32) -> Option<AppId> {
        // Stub: accept all PIDs, derive AppId from /proc/<pid>/comm
        let comm_path = format!("/proc/{}/comm", pid);
        std::fs::read_to_string(&comm_path)
            .ok()
            .map(|name| AppId(name.trim().to_string()))
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
        if capability::shell_only_capabilities().contains(cap) && app_id.0 != "sol-shell" {
            return Decision::Denied {
                reason: "Reserved for sol-shell".to_string(),
            };
        }

        // Stub: grant everything else for development
        Decision::Granted {
            token: self.issue_token(app_id, cap),
            expires_at: None,
        }
    }

    fn issue_token(&self, _app_id: &AppId, _cap: &Capability) -> CapabilityToken {
        let mut data = vec![0_u8; 32];
        random::fill_bytes(&mut data).expect("generate random token");
        let token = CapabilityToken {
            data,
            expires_at: None,
            one_time: false,
        };
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(
                token.data.clone(),
                (_app_id.clone(), _cap.clone(), token.expires_at),
            );
        }
        token
    }

    fn verify_token(&self, token: &CapabilityToken) -> Option<(AppId, Capability)> {
        if token.is_expired() {
            return None;
        }
        let tokens = self.tokens.lock().ok()?;
        let (app_id, capability, expires_at) = tokens.get(&token.data)?;
        if expires_at.is_some_and(|expiry| std::time::Instant::now() >= expiry) {
            return None;
        }
        Some((app_id.clone(), capability.clone()))
    }

    fn audit_capability_use(&self, app_id: &AppId, cap: &Capability, outcome: AuditOutcome) {
        tracing::debug!(?app_id, ?cap, ?outcome, "capability audit (stub)");
    }
}
