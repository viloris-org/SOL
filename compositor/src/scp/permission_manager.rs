//! Unified permission manager integrating manifest, audit, and revocation.
//!
//! This module ties together the capability system with manifest parsing,
//! audit logging, and runtime revocation to provide a complete permission
//! management solution.

use crate::scp::{
    audit::AuditLog,
    capability::{Capability, CapabilityToken, Decision},
    manifest::{AppManifest, ManifestError},
    revocation::RevocationRegistry,
    security::{AppId, AuditOutcome, SecurityCoordinator},
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Comprehensive permission manager.
pub struct PermissionManager {
    /// Loaded app manifests
    manifests: Arc<RwLock<HashMap<AppId, AppManifest>>>,
    /// Revocation registry
    revocations: RevocationRegistry,
    /// Audit log
    audit: AuditLog,
    /// Security coordinator (communicates with sol-securityd)
    security: Arc<dyn SecurityCoordinator>,
}

impl PermissionManager {
    /// Create a new permission manager.
    pub fn new(security: Arc<dyn SecurityCoordinator>) -> Self {
        let audit = AuditLog::new(1000);
        Self {
            manifests: Arc::new(RwLock::new(HashMap::new())),
            revocations: RevocationRegistry::new(),
            audit,
            security,
        }
    }

    /// Create with persistent audit logging.
    pub fn with_audit_log<P: AsRef<Path>>(
        security: Arc<dyn SecurityCoordinator>,
        log_path: P,
    ) -> Result<Self, std::io::Error> {
        let audit = AuditLog::with_file(1000, log_path)?;
        Ok(Self {
            manifests: Arc::new(RwLock::new(HashMap::new())),
            revocations: RevocationRegistry::new(),
            audit,
            security,
        })
    }

    /// Load an app's manifest.
    pub fn load_manifest<P: AsRef<Path>>(
        &self,
        app_id: AppId,
        path: P,
    ) -> Result<(), ManifestError> {
        let manifest = AppManifest::from_file(path)?;
        manifest.validate()?;

        if manifest.app.id != app_id.0 {
            return Err(ManifestError::InvalidAppId(format!(
                "Manifest app ID '{}' does not match expected '{}'",
                manifest.app.id, app_id.0
            )));
        }

        if let Ok(mut manifests) = self.manifests.write() {
            manifests.insert(app_id, manifest);
        }

        Ok(())
    }

    /// Evaluate a capability request against manifest and policy.
    pub fn evaluate_capability(
        &self,
        app_id: &AppId,
        capability: &Capability,
        justification: &str,
    ) -> Decision {
        // Log the request
        self.audit
            .log_capability_requested(app_id, capability, justification);

        // Check if revoked
        if self.revocations.is_revoked(app_id, capability) {
            self.audit
                .log_capability_denied(app_id, capability, "Previously revoked by user");
            return Decision::Denied {
                reason: "This capability was revoked by the user".to_string(),
            };
        }

        // Check manifest
        if let Ok(manifests) = self.manifests.read() {
            if let Some(manifest) = manifests.get(app_id) {
                // Check if explicitly forbidden
                if manifest.is_forbidden(capability) {
                    self.audit
                        .log_capability_denied(app_id, capability, "Forbidden in manifest");
                    return Decision::Denied {
                        reason: "Capability is forbidden in the app manifest".to_string(),
                    };
                }

                // Check if statically declared
                if manifest.static_capabilities().contains(capability) {
                    let token = self.security.issue_token(app_id, capability);
                    self.audit
                        .log_capability_granted(app_id, capability, "static");
                    return Decision::Granted {
                        token,
                        expires_at: None,
                    };
                }

                // Check if declared in dynamic section
                let is_dynamic = manifest
                    .dynamic_capabilities()
                    .iter()
                    .any(|(cap, _)| cap == capability);

                if !is_dynamic {
                    self.audit
                        .log_capability_denied(app_id, capability, "Not in manifest");
                    return Decision::Denied {
                        reason: "Capability not declared in app manifest".to_string(),
                    };
                }
            }
        }

        // Delegate to security coordinator
        let decision = self.security.evaluate_capability(app_id, capability);

        match &decision {
            Decision::Granted { .. } => {
                self.audit
                    .log_capability_granted(app_id, capability, "runtime");
            }
            Decision::Denied { reason } => {
                self.audit.log_capability_denied(app_id, capability, reason);
            }
            Decision::NeedsUserConsent { .. } => {
                // User consent dialog will be shown
            }
        }

        decision
    }

    /// Record capability use.
    pub fn record_use(&self, app_id: &AppId, capability: &Capability, details: Option<String>) {
        self.audit.log_capability_used(app_id, capability, details);
        self.security
            .audit_capability_use(app_id, capability, AuditOutcome::Used);
    }

    /// Revoke a capability immediately.
    pub fn revoke_capability(&self, app_id: &AppId, capability: &Capability) -> Result<(), String> {
        self.revocations.revoke(app_id, capability)?;
        self.audit.log_capability_revoked(app_id, capability);
        Ok(())
    }

    /// Restore a revoked capability.
    pub fn restore_capability(
        &self,
        app_id: &AppId,
        capability: &Capability,
    ) -> Result<(), String> {
        self.revocations.restore(app_id, capability)?;
        self.audit
            .log_capability_granted(app_id, capability, "restored");
        Ok(())
    }

    /// Check if a capability is currently valid for an app.
    pub fn is_capability_valid(&self, app_id: &AppId, capability: &Capability) -> bool {
        !self.revocations.is_revoked(app_id, capability)
    }

    /// Get audit events for an app.
    pub fn query_audit_log(
        &self,
        app_id: &AppId,
        limit: usize,
    ) -> Vec<crate::scp::audit::AuditEvent> {
        self.audit.query_app_events(app_id, limit)
    }

    /// Get event counts for an app.
    pub fn event_counts(&self, app_id: &AppId) -> crate::scp::audit::EventCounts {
        self.audit.count_events(app_id)
    }

    /// Get the manifest for an app.
    pub fn get_manifest(&self, app_id: &AppId) -> Option<AppManifest> {
        self.manifests.read().ok()?.get(app_id).cloned()
    }

    /// Get the audit log instance.
    pub fn audit_log(&self) -> &AuditLog {
        &self.audit
    }

    /// Get the revocation registry.
    pub fn revocation_registry(&self) -> &RevocationRegistry {
        &self.revocations
    }

    /// Verify a capability token.
    pub fn verify_token(
        &self,
        app_id: &AppId,
        capability: &Capability,
        token: &CapabilityToken,
    ) -> Result<(), String> {
        // Check not revoked
        if self.revocations.is_revoked(app_id, capability) {
            return Err("Capability was revoked".to_string());
        }

        // Check token validity
        let (verified_app, verified_cap) = self
            .security
            .verify_token(token)
            .ok_or("Invalid or expired token")?;

        if &verified_app != app_id {
            return Err("Token does not belong to this app".to_string());
        }

        if &verified_cap != capability {
            return Err("Token does not match this capability".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::security::StubSecurityCoordinator;

    fn create_test_manifest() -> String {
        r#"
[app]
id = "org.sol.test"
name = "Test App"
version = "1.0.0"

[capabilities.static_caps]
window-toplevel = true

[capabilities.dynamic.clipboard-read]
justification = "Read copied text"
optional = false

[capabilities.forbidden]
layer-shell = true
"#
        .to_string()
    }

    #[test]
    fn grants_manifest_static_capabilities() {
        let manager = PermissionManager::new(Arc::new(StubSecurityCoordinator::default()));
        let app = AppId("org.sol.test".to_string());

        // Load manifest
        let manifest_path = std::env::temp_dir().join("test_manifest.toml");
        std::fs::write(&manifest_path, create_test_manifest()).unwrap();
        manager.load_manifest(app.clone(), &manifest_path).unwrap();
        std::fs::remove_file(&manifest_path).unwrap();

        // Request static capability
        let decision =
            manager.evaluate_capability(&app, &Capability::WindowToplevel, "Create window");

        assert!(matches!(decision, Decision::Granted { .. }));
    }

    #[test]
    fn denies_forbidden_capabilities() {
        let manager = PermissionManager::new(Arc::new(StubSecurityCoordinator::default()));
        let app = AppId("org.sol.test".to_string());

        let manifest_path = std::env::temp_dir().join("test_manifest2.toml");
        std::fs::write(&manifest_path, create_test_manifest()).unwrap();
        manager.load_manifest(app.clone(), &manifest_path).unwrap();
        std::fs::remove_file(&manifest_path).unwrap();

        let decision =
            manager.evaluate_capability(&app, &Capability::LayerShell, "Use layer shell");

        match decision {
            Decision::Denied { reason } => {
                assert!(reason.contains("forbidden"));
            }
            _ => panic!("Expected denial for forbidden capability"),
        }
    }

    #[test]
    fn revocation_invalidates_capability() {
        let manager = PermissionManager::new(Arc::new(StubSecurityCoordinator::default()));
        let app = AppId("org.sol.test".to_string());
        let cap = Capability::ClipboardRead;

        // Grant initially succeeds
        let decision = manager.evaluate_capability(&app, &cap, "Test");
        assert!(matches!(decision, Decision::Granted { .. }));

        // Revoke
        manager.revoke_capability(&app, &cap).unwrap();
        assert!(!manager.is_capability_valid(&app, &cap));

        // Request again - should be denied
        let decision = manager.evaluate_capability(&app, &cap, "Test again");
        match decision {
            Decision::Denied { reason } => {
                assert!(reason.contains("revoked"));
            }
            _ => panic!("Expected denial after revocation"),
        }
    }

    #[test]
    fn audit_log_tracks_operations() {
        let manager = PermissionManager::new(Arc::new(StubSecurityCoordinator::default()));
        let app = AppId("org.sol.test".to_string());
        let cap = Capability::WindowToplevel;

        manager.evaluate_capability(&app, &cap, "Test request");
        manager.record_use(&app, &cap, Some("Created window".to_string()));

        let events = manager.query_audit_log(&app, 10);
        assert!(events.len() >= 2);

        let counts = manager.event_counts(&app);
        assert!(counts.requested > 0);
    }
}
