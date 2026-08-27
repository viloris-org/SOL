//! D-Bus service interface for org.sol.Device1.

use crate::types::*;
use zbus::fdo;

/// Request identifier for idempotent operations.
pub type RequestId = String;

/// Authorization scope for device authorization.
#[derive(Debug, Clone)]
pub enum AuthorizationScope {
    Full,
    Restricted,
}

/// D-Bus interface for org.sol.Device1.
pub struct Device1Interface {
    snapshot: tokio::sync::RwLock<DeviceSnapshot>,
    operation_tx: tokio::sync::mpsc::UnboundedSender<OperationRequest>,
}

impl Device1Interface {
    pub fn new(operation_tx: tokio::sync::mpsc::UnboundedSender<OperationRequest>) -> Self {
        Self {
            snapshot: tokio::sync::RwLock::new(DeviceSnapshot::new()),
            operation_tx,
        }
    }

    pub async fn update_snapshot(&self, snapshot: DeviceSnapshot) {
        let mut current = self.snapshot.write().await;
        *current = snapshot;
    }

    pub async fn get_snapshot_copy(&self) -> DeviceSnapshot {
        self.snapshot.read().await.clone()
    }
}

/// Operation request from D-Bus to the service core.
#[derive(Debug, Clone)]
pub enum OperationRequest {
    Authorize {
        device_id: DeviceId,
        scope: AuthorizationScope,
        request_id: RequestId,
    },
    SetTrust {
        device_id: DeviceId,
        policy: TrustPolicy,
        request_id: RequestId,
    },
    Forget {
        device_id: DeviceId,
        request_id: RequestId,
    },
    Eject {
        function_id: FunctionId,
        request_id: RequestId,
    },
    PrepareRemoval {
        device_id: DeviceId,
        request_id: RequestId,
    },
    CancelOperation {
        operation_id: OperationId,
    },
}

#[zbus::interface(name = "org.sol.Device1")]
impl Device1Interface {
    /// Get complete device snapshot with monotonic revision.
    async fn get_snapshot(&self) -> fdo::Result<(u64, String)> {
        let snapshot = self.snapshot.read().await;
        let json = serde_json::to_string(&*snapshot)
            .map_err(|e| fdo::Error::Failed(format!("Serialization failed: {}", e)))?;
        Ok((snapshot.revision, json))
    }

    /// Get a single device by ID.
    async fn get_device(&self, device_id: String) -> fdo::Result<String> {
        let uuid = device_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid DeviceId".into()))?;
        let device_id = DeviceId(uuid);

        let snapshot = self.snapshot.read().await;
        let device = snapshot
            .devices
            .get(&device_id)
            .ok_or_else(|| fdo::Error::Failed("Device not found".into()))?;

        let json = serde_json::to_string(device)
            .map_err(|e| fdo::Error::Failed(format!("Serialization failed: {}", e)))?;
        Ok(json)
    }

    /// Authorize a device for use.
    async fn authorize(
        &self,
        device_id: String,
        scope: String,
        request_id: String,
    ) -> fdo::Result<String> {
        let uuid = device_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid DeviceId".into()))?;
        let device_id = DeviceId(uuid);

        let scope = match scope.as_str() {
            "full" => AuthorizationScope::Full,
            "restricted" => AuthorizationScope::Restricted,
            _ => return Err(fdo::Error::InvalidArgs("Invalid scope".into())),
        };

        self.operation_tx
            .send(OperationRequest::Authorize {
                device_id,
                scope,
                request_id: request_id.clone(),
            })
            .map_err(|_| fdo::Error::Failed("Service unavailable".into()))?;

        Ok(request_id)
    }

    /// Set device trust policy.
    async fn set_trust(
        &self,
        device_id: String,
        policy: String,
        request_id: String,
    ) -> fdo::Result<String> {
        let uuid = device_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid DeviceId".into()))?;
        let device_id = DeviceId(uuid);

        let policy = match policy.as_str() {
            "unknown" => TrustPolicy::Unknown,
            "allowed" => TrustPolicy::Allowed,
            "restricted" => TrustPolicy::Restricted,
            "blocked" => TrustPolicy::Blocked,
            "managed" => TrustPolicy::Managed,
            _ => return Err(fdo::Error::InvalidArgs("Invalid policy".into())),
        };

        self.operation_tx
            .send(OperationRequest::SetTrust {
                device_id,
                policy,
                request_id: request_id.clone(),
            })
            .map_err(|_| fdo::Error::Failed("Service unavailable".into()))?;

        Ok(request_id)
    }

    /// Forget a device (remove saved preferences and trust).
    async fn forget(&self, device_id: String, request_id: String) -> fdo::Result<String> {
        let uuid = device_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid DeviceId".into()))?;
        let device_id = DeviceId(uuid);

        self.operation_tx
            .send(OperationRequest::Forget {
                device_id,
                request_id: request_id.clone(),
            })
            .map_err(|_| fdo::Error::Failed("Service unavailable".into()))?;

        Ok(request_id)
    }

    /// Eject a storage function.
    async fn eject(&self, function_id: String, request_id: String) -> fdo::Result<String> {
        let uuid = function_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid FunctionId".into()))?;
        let function_id = FunctionId(uuid);

        self.operation_tx
            .send(OperationRequest::Eject {
                function_id,
                request_id: request_id.clone(),
            })
            .map_err(|_| fdo::Error::Failed("Service unavailable".into()))?;

        Ok(request_id)
    }

    /// Prepare a device for safe removal.
    async fn prepare_removal(&self, device_id: String, request_id: String) -> fdo::Result<String> {
        let uuid = device_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid DeviceId".into()))?;
        let device_id = DeviceId(uuid);

        self.operation_tx
            .send(OperationRequest::PrepareRemoval {
                device_id,
                request_id: request_id.clone(),
            })
            .map_err(|_| fdo::Error::Failed("Service unavailable".into()))?;

        Ok(request_id)
    }

    /// Cancel an ongoing operation.
    async fn cancel_operation(&self, operation_id: String) -> fdo::Result<()> {
        let uuid = operation_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid OperationId".into()))?;
        let operation_id = OperationId(uuid);

        self.operation_tx
            .send(OperationRequest::CancelOperation { operation_id })
            .map_err(|_| fdo::Error::Failed("Service unavailable".into()))?;

        Ok(())
    }

    /// Get an operation by ID.
    async fn get_operation(&self, operation_id: String) -> fdo::Result<String> {
        let uuid = operation_id
            .parse::<uuid::Uuid>()
            .map_err(|_| fdo::Error::InvalidArgs("Invalid OperationId".into()))?;
        let operation_id = OperationId(uuid);

        let snapshot = self.snapshot.read().await;
        let operation = snapshot
            .operations
            .get(&operation_id)
            .ok_or_else(|| fdo::Error::Failed("Operation not found".into()))?;

        let json = serde_json::to_string(operation)
            .map_err(|e| fdo::Error::Failed(format!("Serialization failed: {}", e)))?;
        Ok(json)
    }
}
