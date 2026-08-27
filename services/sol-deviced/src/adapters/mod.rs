//! Backend adapter trait and fake implementation for testing.

use crate::reconcile::{DeviceObservation, FunctionObservation};
use crate::types::*;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Backend adapter trait for device discovery and operations.
#[async_trait]
pub trait Adapter: Send + Sync {
    /// Backend name for diagnostics.
    fn name(&self) -> &str;

    /// Initialize and begin monitoring. Must establish monitoring before or atomically
    /// with initial enumeration to avoid subscribe/enumerate gaps.
    async fn start(&mut self) -> Result<(), AdapterError>;

    /// Stop monitoring and clean up.
    async fn stop(&mut self) -> Result<(), AdapterError>;

    /// Enumerate current devices and functions.
    async fn enumerate(&self) -> Result<AdapterSnapshot, AdapterError>;

    /// Prepare a device for safe removal.
    async fn prepare_removal(&self, device_id: DeviceId) -> Result<PrepareResult, AdapterError>;

    /// Complete safe removal (flush, unmount, power off).
    async fn commit_removal(&self, device_id: DeviceId) -> Result<(), AdapterError>;

    /// Abort a safe removal operation.
    async fn abort_removal(&self, device_id: DeviceId) -> Result<(), AdapterError>;
}

/// Adapter-specific error.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("Backend unavailable")]
    Unavailable,
    #[error("Device not found")]
    DeviceNotFound,
    #[error("Operation not supported")]
    Unsupported,
    #[error("Resource busy: {0}")]
    Busy(String),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Timeout")]
    Timeout,
    #[error("Other error: {0}")]
    Other(String),
}

/// Snapshot from a backend adapter.
#[derive(Debug, Clone, Default)]
pub struct AdapterSnapshot {
    pub devices: Vec<DeviceObservation>,
    pub functions: Vec<FunctionObservation>,
}

/// Result of prepare_removal operation.
#[derive(Debug, Clone)]
pub struct PrepareResult {
    pub can_proceed: bool,
    pub blockers: Vec<String>,
}

/// Fake adapter for testing without real hardware.
pub struct FakeAdapter {
    name: String,
    snapshot: Arc<Mutex<AdapterSnapshot>>,
    started: Arc<Mutex<bool>>,
}

impl FakeAdapter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            snapshot: Arc::new(Mutex::new(AdapterSnapshot::default())),
            started: Arc::new(Mutex::new(false)),
        }
    }

    /// Inject a device observation for testing.
    pub async fn inject_device(&self, obs: DeviceObservation) {
        let mut snapshot = self.snapshot.lock().await;
        snapshot.devices.push(obs);
    }

    /// Inject a function observation for testing.
    pub async fn inject_function(&self, obs: FunctionObservation) {
        let mut snapshot = self.snapshot.lock().await;
        snapshot.functions.push(obs);
    }

    /// Clear all observations.
    pub async fn clear(&self) {
        let mut snapshot = self.snapshot.lock().await;
        snapshot.devices.clear();
        snapshot.functions.clear();
    }
}

#[async_trait]
impl Adapter for FakeAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self) -> Result<(), AdapterError> {
        let mut started = self.started.lock().await;
        if *started {
            return Err(AdapterError::Other("Already started".into()));
        }
        *started = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        let mut started = self.started.lock().await;
        *started = false;
        Ok(())
    }

    async fn enumerate(&self) -> Result<AdapterSnapshot, AdapterError> {
        let started = self.started.lock().await;
        if !*started {
            return Err(AdapterError::Unavailable);
        }
        let snapshot = self.snapshot.lock().await;
        Ok(snapshot.clone())
    }

    async fn prepare_removal(&self, _device_id: DeviceId) -> Result<PrepareResult, AdapterError> {
        Ok(PrepareResult {
            can_proceed: true,
            blockers: Vec::new(),
        })
    }

    async fn commit_removal(&self, _device_id: DeviceId) -> Result<(), AdapterError> {
        Ok(())
    }

    async fn abort_removal(&self, _device_id: DeviceId) -> Result<(), AdapterError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reconcile::IdentityEvidence;

    #[tokio::test]
    async fn test_fake_adapter_lifecycle() {
        let mut adapter = FakeAdapter::new("test");

        // Cannot enumerate before start
        assert!(adapter.enumerate().await.is_err());

        // Start
        adapter.start().await.unwrap();

        // Can enumerate after start
        let snapshot = adapter.enumerate().await.unwrap();
        assert_eq!(snapshot.devices.len(), 0);

        // Inject device
        adapter
            .inject_device(DeviceObservation {
                identity_evidence: IdentityEvidence::Ephemeral {
                    hint: "test".into(),
                },
                vendor_name: Some("Test".into()),
                product_name: Some("Device".into()),
                parent_evidence: None,
            })
            .await;

        let snapshot = adapter.enumerate().await.unwrap();
        assert_eq!(snapshot.devices.len(), 1);

        // Stop
        adapter.stop().await.unwrap();
    }
}
