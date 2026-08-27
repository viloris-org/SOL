//! Core device types and identity model for sol-deviced.
//!
//! Implements the three-level device graph (PhysicalDevice -> Function -> Endpoint)
//! with stable identity, attachment generations, and typed state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Opaque physical device identity. Durable only when identity confidence is sufficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub Uuid);

impl DeviceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Identifies one connection instance. Every reconnect creates a new value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachmentId(pub Uuid);

impl AttachmentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Identifies a function under a physical device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FunctionId(pub Uuid);

impl FunctionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Identifies a connection-local subsystem endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EndpointId(pub Uuid);

impl EndpointId {
    #[allow(dead_code)] // Phase 1: constructed when endpoints are reconciled
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Identifies an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId(pub Uuid);

impl OperationId {
    #[allow(dead_code)] // Phase 1: constructed when operations begin
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Identity confidence level from strongest to weakest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IdentityConfidence {
    /// Cryptographic identity (secure element, certificate)
    Cryptographic,
    /// Vendor/product/serial triplet
    VendorProductSerial,
    /// Stable protocol identity (MAC address, Bluetooth address with caution)
    StableProtocol,
    /// Topology-derived (port path, hub position)
    TopologyDerived,
    /// Ephemeral attachment only
    Ephemeral,
}

/// Device attachment lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentState {
    /// Being probed and discovered
    Discovering,
    /// Physically present and claimed
    Present,
    /// Preparing for safe removal
    Quiescing,
    /// Physically removed
    Removed,
}

/// Device usability state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsabilityState {
    /// Awaiting authorization
    PendingAuthorization,
    /// Ready for use
    Ready,
    /// Degraded but functional
    Degraded,
    /// Blocked by policy
    Blocked,
    /// Hardware unsupported
    Unsupported,
}

/// Device activity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityState {
    /// Not in use
    Idle,
    /// Currently in use
    InUse,
}

/// Device trust policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustPolicy {
    /// Unknown device, default policy applies
    Unknown,
    /// Explicitly allowed
    Allowed,
    /// Restricted capabilities
    Restricted,
    /// Explicitly blocked
    Blocked,
    /// Managed by admin policy
    Managed,
}

/// Function capability type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionType {
    AudioOutput,
    AudioInput,
    Display,
    Network,
    Input,
    Storage,
    Camera,
    PowerDelivery,
    Hub,
    Other,
}

/// Authoritative owner of a function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionOwner {
    Compositor,
    Audio,
    Network,
    Storage,
    Bluetooth,
    Power,
    Unknown,
}

/// Physical device in the device graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalDevice {
    pub id: DeviceId,
    pub attachment_id: AttachmentId,
    pub connection_generation: u64,

    /// Identity and discovery
    pub identity_confidence: IdentityConfidence,
    pub vendor_name: Option<String>,
    pub product_name: Option<String>,
    pub user_label: Option<String>,

    /// State
    pub attachment_state: AttachmentState,
    pub usability_state: UsabilityState,
    pub activity_state: ActivityState,
    pub trust_policy: TrustPolicy,

    /// Graph structure
    pub parent: Option<DeviceId>,
    pub children: Vec<DeviceId>,
    pub functions: Vec<FunctionId>,
}

/// Function capability provided by a physical device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub id: FunctionId,
    pub device_id: DeviceId,
    pub function_type: FunctionType,
    pub owner: FunctionOwner,
    pub endpoints: Vec<EndpointId>,
    pub usability_state: UsabilityState,
    pub metadata: HashMap<String, String>,
}

/// Connection-local subsystem endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub id: EndpointId,
    pub function_id: FunctionId,
    pub subsystem: String,
    pub attachment_generation: u64,
}

/// Operation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationState {
    Pending,
    Running,
    WaitingForUser,
    Completed,
    Failed,
    Cancelled,
}

/// Typed operation failure reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureReason {
    DeviceNotFound,
    DeviceRemoved,
    PermissionDenied,
    ResourceBusy { blocker: String },
    Unsupported,
    Timeout,
    BackendUnavailable,
    Other { message: String },
}

/// Asynchronous operation tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub id: OperationId,
    pub state: OperationState,
    pub device_id: Option<DeviceId>,
    pub function_id: Option<FunctionId>,
    pub attachment_generation: u64,
    pub progress: Option<f32>,
    pub failure_reason: Option<FailureReason>,
    pub blockers: Vec<String>,
}

/// Complete device snapshot with monotonic revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    pub revision: u64,
    pub devices: HashMap<DeviceId, PhysicalDevice>,
    pub functions: HashMap<FunctionId, Function>,
    pub endpoints: HashMap<EndpointId, Endpoint>,
    pub operations: HashMap<OperationId, Operation>,
}

impl DeviceSnapshot {
    pub fn new() -> Self {
        Self {
            revision: 0,
            devices: HashMap::new(),
            functions: HashMap::new(),
            endpoints: HashMap::new(),
            operations: HashMap::new(),
        }
    }

    pub fn next_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}
