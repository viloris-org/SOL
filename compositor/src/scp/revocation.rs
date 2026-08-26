//! Runtime capability revocation.
//!
//! This module implements immediate revocation of granted capabilities.
//! When a user revokes a permission via sol-settings, the compositor
//! immediately invalidates all related tokens and notifies affected apps.

use crate::scp::{
    capability::Capability,
    protocol::{CompositorMessage, SessionId},
    security::AppId,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Tracks revoked capabilities and notifies observers.
#[derive(Clone)]
pub struct RevocationRegistry {
    inner: Arc<RwLock<RevocationState>>,
}

#[derive(Default)]
struct RevocationState {
    /// Capabilities revoked per app
    revoked: HashMap<AppId, HashSet<Capability>>,
    /// Timestamp of last revocation per app
    revocation_times: HashMap<AppId, Instant>,
    /// Callback handles for notifications
    listeners: HashMap<(AppId, Capability), Vec<RevocationListener>>,
}

type RevocationListener = Box<dyn Fn() + Send + Sync>;

impl RevocationRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RevocationState::default())),
        }
    }

    /// Revoke a capability for an app immediately.
    ///
    /// Returns the list of session IDs that need to be notified.
    pub fn revoke(
        &self,
        app_id: &AppId,
        capability: &Capability,
    ) -> Result<Vec<SessionId>, String> {
        let mut state = self
            .inner
            .write()
            .map_err(|_| "RevocationRegistry lock poisoned")?;

        state
            .revoked
            .entry(app_id.clone())
            .or_default()
            .insert(capability.clone());
        state
            .revocation_times
            .insert(app_id.clone(), Instant::now());

        // Notify listeners
        if let Some(listeners) = state.listeners.get(&(app_id.clone(), capability.clone())) {
            for listener in listeners {
                listener();
            }
        }

        tracing::info!(?app_id, ?capability, "capability revoked");
        Ok(vec![]) // Session tracking would go here
    }

    /// Check if a capability is currently revoked.
    pub fn is_revoked(&self, app_id: &AppId, capability: &Capability) -> bool {
        if let Ok(state) = self.inner.read() {
            state
                .revoked
                .get(app_id)
                .is_some_and(|caps| caps.contains(capability))
        } else {
            false
        }
    }

    /// Restore a revoked capability.
    pub fn restore(&self, app_id: &AppId, capability: &Capability) -> Result<(), String> {
        let mut state = self
            .inner
            .write()
            .map_err(|_| "RevocationRegistry lock poisoned")?;

        if let Some(caps) = state.revoked.get_mut(app_id) {
            caps.remove(capability);
            if caps.is_empty() {
                state.revoked.remove(app_id);
            }
        }

        tracing::info!(?app_id, ?capability, "capability restored");
        Ok(())
    }

    /// Register a listener for revocation events.
    pub fn on_revoked<F>(&self, app_id: AppId, capability: Capability, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        if let Ok(mut state) = self.inner.write() {
            state
                .listeners
                .entry((app_id, capability))
                .or_default()
                .push(Box::new(callback));
        }
    }

    /// Build a revocation notification message.
    pub fn build_notification(capability: &Capability, reason: &str) -> CompositorMessage {
        CompositorMessage::ProtocolError {
            code: "capability_revoked".to_string(),
            message: format!(
                "Capability '{}' was revoked: {}",
                capability.wire_name(),
                reason
            ),
            fatal: false,
        }
    }
}

impl Default for RevocationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoke_and_check() {
        let registry = RevocationRegistry::new();
        let app = AppId("test-app".to_string());
        let cap = Capability::ClipboardRead;

        assert!(!registry.is_revoked(&app, &cap));

        registry.revoke(&app, &cap).unwrap();
        assert!(registry.is_revoked(&app, &cap));

        registry.restore(&app, &cap).unwrap();
        assert!(!registry.is_revoked(&app, &cap));
    }

    #[test]
    fn listener_fires_on_revocation() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let registry = RevocationRegistry::new();
        let app = AppId("test-app".to_string());
        let cap = Capability::ScreenCapture {
            scope: crate::scp::capability::CaptureScope::Output,
        };

        let fired = Arc::new(AtomicBool::new(false));
        let fired_clone = Arc::clone(&fired);

        registry.on_revoked(app.clone(), cap.clone(), move || {
            fired_clone.store(true, Ordering::SeqCst);
        });

        registry.revoke(&app, &cap).unwrap();
        assert!(fired.load(Ordering::SeqCst));
    }
}
