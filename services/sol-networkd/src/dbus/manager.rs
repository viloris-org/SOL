use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};
use std::collections::HashMap;

use crate::manager::NetworkManager;
use crate::profile::ProfileId;

/// D-Bus Manager interface implementation
pub struct ManagerInterface {
    manager: NetworkManager,
}

impl ManagerInterface {
    pub fn new(manager: NetworkManager) -> Self {
        Self { manager }
    }
}

#[interface(name = "org.sol.Network1.Manager")]
impl ManagerInterface {
    /// Get current network state
    async fn state(&self) -> String {
        format!("{:?}", self.manager.get_state().await)
    }

    /// Get connectivity state (0=none, 1=portal, 2=limited, 3=full)
    async fn connectivity(&self) -> u32 {
        // TODO: Implement actual connectivity checking
        3 // Full connectivity placeholder
    }

    /// List all network devices
    async fn list_devices(&self) -> Vec<OwnedObjectPath> {
        let device_ids = self.manager.list_devices().await;
        device_ids
            .into_iter()
            .map(|id| format!("/org/sol/Network1/Device/{}", id.0))
            .filter_map(|s| ObjectPath::try_from(s).ok().map(|p| p.into()))
            .collect()
    }

    /// Connect to a saved profile
    async fn connect_to_profile(&self, profile_id: String) -> zbus::fdo::Result<OwnedObjectPath> {
        let id = ProfileId(profile_id.clone());
        self.manager
            .connect_to_profile(&id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        let path = format!("/org/sol/Network1/Connection/{}", profile_id);
        ObjectPath::try_from(path)
            .map(|p| p.into())
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Disconnect a profile
    async fn disconnect_profile(&self, profile_id: String) -> zbus::fdo::Result<()> {
        let id = ProfileId(profile_id);
        self.manager
            .disconnect_profile(&id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Create a new network profile
    async fn create_profile(
        &self,
        _settings: HashMap<String, zbus::zvariant::Value<'_>>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        // TODO: Parse settings HashMap into Profile struct
        Err(zbus::fdo::Error::NotSupported(
            "Profile creation not yet implemented".into(),
        ))
    }

    /// Delete a profile
    async fn delete_profile(&self, profile_id: String) -> zbus::fdo::Result<()> {
        let id = ProfileId(profile_id);
        self.manager
            .delete_profile(&id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Signal: Network state changed
    #[zbus(signal)]
    async fn state_changed(
        signal_ctxt: &zbus::SignalContext<'_>,
        new_state: HashMap<String, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// Signal: Connectivity changed
    #[zbus(signal)]
    async fn connectivity_changed(
        signal_ctxt: &zbus::SignalContext<'_>,
        connectivity: u32,
    ) -> zbus::Result<()>;
}
