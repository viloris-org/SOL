//! Device graph reconciliation engine.
//!
//! Reconciles backend observations into a coherent device graph using idempotent
//! add, update, claim, and removal transitions.

use crate::types::*;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};

/// Backend observation of a discovered device.
#[derive(Debug, Clone)]
pub struct DeviceObservation {
    pub identity_evidence: IdentityEvidence,
    pub vendor_name: Option<String>,
    pub product_name: Option<String>,
    pub parent_evidence: Option<IdentityEvidence>,
}

/// Identity evidence from a backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdentityEvidence {
    VendorProductSerial {
        vendor: String,
        product: String,
        serial: String,
    },
    StableProtocol {
        protocol: String,
        address: String,
    },
    Topology {
        path: String,
    },
    Ephemeral {
        hint: String,
    },
}

impl IdentityEvidence {
    pub fn confidence(&self) -> IdentityConfidence {
        match self {
            Self::VendorProductSerial { .. } => IdentityConfidence::VendorProductSerial,
            Self::StableProtocol { .. } => IdentityConfidence::StableProtocol,
            Self::Topology { .. } => IdentityConfidence::TopologyDerived,
            Self::Ephemeral { .. } => IdentityConfidence::Ephemeral,
        }
    }
}

/// Backend function observation.
#[derive(Debug, Clone)]
pub struct FunctionObservation {
    pub device_evidence: IdentityEvidence,
    pub function_type: FunctionType,
    pub owner: FunctionOwner,
    pub subsystem: String,
    pub metadata: HashMap<String, String>,
}

/// Device graph reconciler.
pub struct Reconciler {
    /// Known device identities mapped to DeviceId
    identity_map: HashMap<IdentityEvidence, DeviceId>,
    /// Current connection generation per device
    device_generations: HashMap<DeviceId, u64>,
}

impl Reconciler {
    pub fn new() -> Self {
        Self {
            identity_map: HashMap::new(),
            device_generations: HashMap::new(),
        }
    }

    /// Reconcile device observations into the snapshot.
    pub fn reconcile_devices(
        &mut self,
        observations: Vec<DeviceObservation>,
        snapshot: &mut DeviceSnapshot,
    ) {
        let mut seen_devices = HashSet::new();

        for obs in observations {
            let device_id = self.resolve_device_id(&obs.identity_evidence);
            seen_devices.insert(device_id);

            let generation = *self.device_generations.entry(device_id).or_insert(0);

            // Check if device exists and is not removed
            let device_exists = snapshot
                .devices
                .get(&device_id)
                .map(|d| d.attachment_state != AttachmentState::Removed)
                .unwrap_or(false);

            if device_exists {
                // Update existing device
                if let Some(device) = snapshot.devices.get_mut(&device_id) {
                    self.update_device(device, &obs);
                }
            } else {
                // Add new device (or re-add after removal)
                let new_generation = generation + 1;
                self.device_generations.insert(device_id, new_generation);
                let device = self.create_device(device_id, new_generation, &obs);
                snapshot.devices.insert(device_id, device);
                info!(?device_id, generation = new_generation, "Device attached");
            }
        }

        // Mark unseen devices as removed
        let all_device_ids: Vec<_> = snapshot.devices.keys().copied().collect();
        for device_id in all_device_ids {
            if !seen_devices.contains(&device_id) {
                if let Some(device) = snapshot.devices.get_mut(&device_id) {
                    if device.attachment_state != AttachmentState::Removed {
                        device.attachment_state = AttachmentState::Removed;
                        info!(?device_id, "Device removed");
                    }
                }
            }
        }

        snapshot.next_revision();
    }

    /// Reconcile function observations into the snapshot.
    pub fn reconcile_functions(
        &mut self,
        observations: Vec<FunctionObservation>,
        snapshot: &mut DeviceSnapshot,
    ) {
        for obs in observations {
            let device_id = self.resolve_device_id(&obs.device_evidence);

            // Skip if device doesn't exist or is removed
            if let Some(device) = snapshot.devices.get(&device_id) {
                if device.attachment_state == AttachmentState::Removed {
                    continue;
                }
            } else {
                warn!(?device_id, "Function observed for unknown device");
                continue;
            }

            // Find or create function
            let function_id = self.find_or_create_function(device_id, &obs, snapshot);

            debug!(?function_id, ?device_id, function_type = ?obs.function_type, "Function reconciled");
        }

        snapshot.next_revision();
    }

    /// Resolve identity evidence to a stable DeviceId.
    fn resolve_device_id(&mut self, evidence: &IdentityEvidence) -> DeviceId {
        if let Some(&existing_id) = self.identity_map.get(evidence) {
            existing_id
        } else {
            let new_id = DeviceId::new();
            self.identity_map.insert(evidence.clone(), new_id);
            new_id
        }
    }

    /// Create a new physical device from observation.
    fn create_device(
        &self,
        device_id: DeviceId,
        generation: u64,
        obs: &DeviceObservation,
    ) -> PhysicalDevice {
        PhysicalDevice {
            id: device_id,
            attachment_id: AttachmentId::new(),
            connection_generation: generation,
            identity_confidence: obs.identity_evidence.confidence(),
            vendor_name: obs.vendor_name.clone(),
            product_name: obs.product_name.clone(),
            user_label: None,
            attachment_state: AttachmentState::Present,
            usability_state: UsabilityState::Ready,
            activity_state: ActivityState::Idle,
            trust_policy: TrustPolicy::Unknown,
            parent: obs
                .parent_evidence
                .as_ref()
                .map(|e| self.identity_map.get(e).copied())
                .flatten(),
            children: Vec::new(),
            functions: Vec::new(),
        }
    }

    /// Update existing device from observation.
    fn update_device(&self, device: &mut PhysicalDevice, obs: &DeviceObservation) {
        // Update mutable fields
        if device.attachment_state == AttachmentState::Removed {
            device.attachment_state = AttachmentState::Present;
            device.attachment_id = AttachmentId::new();
        }

        device.vendor_name = obs.vendor_name.clone();
        device.product_name = obs.product_name.clone();
    }

    /// Find or create a function for a device.
    fn find_or_create_function(
        &self,
        device_id: DeviceId,
        obs: &FunctionObservation,
        snapshot: &mut DeviceSnapshot,
    ) -> FunctionId {
        // Try to find existing function by type and owner
        let existing = snapshot.functions.values().find(|f| {
            f.device_id == device_id && f.function_type == obs.function_type && f.owner == obs.owner
        });

        if let Some(func) = existing {
            return func.id;
        }

        // Create new function
        let function_id = FunctionId::new();
        let function = Function {
            id: function_id,
            device_id,
            function_type: obs.function_type,
            owner: obs.owner.clone(),
            endpoints: Vec::new(),
            usability_state: UsabilityState::Ready,
            metadata: obs.metadata.clone(),
        };

        snapshot.functions.insert(function_id, function);

        // Add to device's function list
        if let Some(device) = snapshot.devices.get_mut(&device_id) {
            if !device.functions.contains(&function_id) {
                device.functions.push(function_id);
            }
        }

        function_id
    }

    /// Mark a device for quiescing (safe removal preparation).
    pub fn begin_quiesce(
        &self,
        device_id: DeviceId,
        snapshot: &mut DeviceSnapshot,
    ) -> Result<(), String> {
        let device = snapshot
            .devices
            .get_mut(&device_id)
            .ok_or_else(|| "Device not found".to_string())?;

        if device.attachment_state == AttachmentState::Removed {
            return Err("Device already removed".to_string());
        }

        device.attachment_state = AttachmentState::Quiescing;
        snapshot.next_revision();
        Ok(())
    }

    /// Complete device removal.
    pub fn complete_removal(&mut self, device_id: DeviceId, snapshot: &mut DeviceSnapshot) {
        if let Some(device) = snapshot.devices.get_mut(&device_id) {
            device.attachment_state = AttachmentState::Removed;

            // Increment generation to fence stale operations
            let generation = self.device_generations.entry(device_id).or_insert(0);
            *generation += 1;

            snapshot.next_revision();
            info!(
                ?device_id,
                generation = *generation,
                "Device removal completed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coldplug_equals_hotplug() {
        let mut reconciler = Reconciler::new();
        let mut snapshot_cold = DeviceSnapshot::new();
        let mut snapshot_hot = DeviceSnapshot::new();

        let observations = vec![DeviceObservation {
            identity_evidence: IdentityEvidence::VendorProductSerial {
                vendor: "Acme".into(),
                product: "Keyboard".into(),
                serial: "12345".into(),
            },
            vendor_name: Some("Acme".into()),
            product_name: Some("Keyboard".into()),
            parent_evidence: None,
        }];

        // Coldplug: all at once
        reconciler.reconcile_devices(observations.clone(), &mut snapshot_cold);

        // Hotplug: one by one
        let mut reconciler2 = Reconciler::new();
        for obs in observations {
            reconciler2.reconcile_devices(vec![obs], &mut snapshot_hot);
        }

        // Both should have same device count
        assert_eq!(snapshot_cold.devices.len(), snapshot_hot.devices.len());
    }

    #[test]
    fn test_duplicate_events_converge() {
        let mut reconciler = Reconciler::new();
        let mut snapshot = DeviceSnapshot::new();

        let obs = DeviceObservation {
            identity_evidence: IdentityEvidence::VendorProductSerial {
                vendor: "Acme".into(),
                product: "Mouse".into(),
                serial: "67890".into(),
            },
            vendor_name: Some("Acme".into()),
            product_name: Some("Mouse".into()),
            parent_evidence: None,
        };

        // Process same observation multiple times
        reconciler.reconcile_devices(vec![obs.clone()], &mut snapshot);
        reconciler.reconcile_devices(vec![obs.clone()], &mut snapshot);
        reconciler.reconcile_devices(vec![obs], &mut snapshot);

        // Should still have exactly one device
        assert_eq!(snapshot.devices.len(), 1);
    }

    #[test]
    fn test_removal_fences_generation() {
        let mut reconciler = Reconciler::new();
        let mut snapshot = DeviceSnapshot::new();

        let obs = DeviceObservation {
            identity_evidence: IdentityEvidence::Topology {
                path: "/devices/pci0000:00/usb1/1-1".into(),
            },
            vendor_name: Some("Test".into()),
            product_name: Some("Device".into()),
            parent_evidence: None,
        };

        // Add device
        reconciler.reconcile_devices(vec![obs.clone()], &mut snapshot);
        let device_id = snapshot.devices.keys().next().copied().unwrap();
        let gen1 = snapshot.devices[&device_id].connection_generation;

        // Remove device
        reconciler.complete_removal(device_id, &mut snapshot);

        // Re-add same device (reconnect)
        reconciler.reconcile_devices(vec![obs], &mut snapshot);
        let gen2 = snapshot.devices[&device_id].connection_generation;

        // Generation must increment
        assert!(gen2 > gen1);
    }
}
