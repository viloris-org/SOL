//! Dynamic D-Bus object registration for devices and profiles
//!
//! This module handles runtime registration/unregistration of D-Bus objects
//! as devices are added/removed from the system.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use zbus::Connection;

use crate::device::{Device, DeviceId};
use crate::manager::NetworkManager;
use crate::profile::ProfileId;

use super::device::DeviceInterface;
use super::profile::ProfileInterface;

/// Registry of active D-Bus objects for devices and profiles
pub struct ObjectRegistry {
    connection: Connection,
    manager: NetworkManager,
    devices: Arc<RwLock<HashMap<DeviceId, ()>>>,
    profiles: Arc<RwLock<HashMap<ProfileId, ()>>>,
}

impl ObjectRegistry {
    pub fn new(connection: Connection, manager: NetworkManager) -> Self {
        Self {
            connection,
            manager,
            devices: Arc::new(RwLock::new(HashMap::new())),
            profiles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a device D-Bus object
    pub async fn register_device(&self, device: Device) -> Result<()> {
        let device_id = device.id.clone();
        let path = device_object_path(&device_id);

        info!("Registering device D-Bus object: {}", path);

        let interface = DeviceInterface::new(device);

        let _success = self
            .connection
            .object_server()
            .at(path.as_str(), interface)
            .await?;

        // Store the device_id to track registered devices
        self.devices.write().await.insert(device_id, ());

        Ok(())
    }

    /// Unregister a device D-Bus object
    pub async fn unregister_device(&self, device_id: &DeviceId) -> Result<()> {
        let path = device_object_path(device_id);

        info!("Unregistering device D-Bus object: {}", path);

        if self.devices.write().await.remove(device_id).is_some() {
            self.connection
                .object_server()
                .remove::<DeviceInterface, _>(path.as_str())
                .await?;
        }

        Ok(())
    }

    /// Register a profile D-Bus object
    pub async fn register_profile(&self, profile_id: ProfileId) -> Result<()> {
        let path = profile_object_path(&profile_id);

        info!("Registering profile D-Bus object: {}", path);

        let interface = ProfileInterface::new(profile_id.clone(), self.manager.clone());

        let _success = self
            .connection
            .object_server()
            .at(path.as_str(), interface)
            .await?;

        // Store the profile_id to track registered profiles
        self.profiles.write().await.insert(profile_id, ());

        Ok(())
    }

    /// Unregister a profile D-Bus object
    pub async fn unregister_profile(&self, profile_id: &ProfileId) -> Result<()> {
        let path = profile_object_path(profile_id);

        info!("Unregistering profile D-Bus object: {}", path);

        if self.profiles.write().await.remove(profile_id).is_some() {
            self.connection
                .object_server()
                .remove::<ProfileInterface, _>(path.as_str())
                .await?;
        }

        Ok(())
    }

    /// Sync all current devices from NetworkManager
    pub async fn sync_devices(&self) -> Result<()> {
        let device_ids = self.manager.list_devices().await;
        let current_devices = self.manager.get_all_devices().await;

        // Register devices that aren't already registered
        for device in current_devices {
            if !self.devices.read().await.contains_key(&device.id) {
                if let Err(e) = self.register_device(device).await {
                    warn!("Failed to register device: {}", e);
                }
            }
        }

        // Unregister devices that no longer exist
        let registered_ids: Vec<DeviceId> = self.devices.read().await.keys().cloned().collect();
        for device_id in registered_ids {
            if !device_ids.contains(&device_id) {
                if let Err(e) = self.unregister_device(&device_id).await {
                    warn!("Failed to unregister device: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Sync all current profiles from NetworkManager
    pub async fn sync_profiles(&self) -> Result<()> {
        let profile_ids = self.manager.list_profiles().await;

        // Register profiles that aren't already registered
        for profile_id in &profile_ids {
            if !self.profiles.read().await.contains_key(profile_id) {
                if let Err(e) = self.register_profile(profile_id.clone()).await {
                    warn!("Failed to register profile: {}", e);
                }
            }
        }

        // Unregister profiles that no longer exist
        let registered_ids: Vec<ProfileId> = self.profiles.read().await.keys().cloned().collect();
        for profile_id in registered_ids {
            if !profile_ids.contains(&profile_id) {
                if let Err(e) = self.unregister_profile(&profile_id).await {
                    warn!("Failed to unregister profile: {}", e);
                }
            }
        }

        Ok(())
    }
}

/// Convert device ID to D-Bus object path
fn device_object_path(device_id: &DeviceId) -> String {
    format!(
        "/org/sol/Network1/Device/{}",
        sanitize_object_path_component(&device_id.0)
    )
}

/// Convert profile ID to D-Bus object path
fn profile_object_path(profile_id: &ProfileId) -> String {
    format!(
        "/org/sol/Network1/Profile/{}",
        sanitize_object_path_component(&profile_id.0)
    )
}

/// Sanitize a string for use in D-Bus object path
fn sanitize_object_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_device_paths() {
        let id = DeviceId("wifi:0".to_string());
        assert_eq!(device_object_path(&id), "/org/sol/Network1/Device/wifi_0");
    }

    #[test]
    fn sanitizes_profile_paths() {
        let id = ProfileId("550e8400-e29b-41d4".to_string());
        assert_eq!(
            profile_object_path(&id),
            "/org/sol/Network1/Profile/550e8400_e29b_41d4"
        );
    }
}
