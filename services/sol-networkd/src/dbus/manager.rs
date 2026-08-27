use std::collections::HashMap;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use crate::captive_portal::ConnectivityState;
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
        match self.manager.get_connectivity().await {
            ConnectivityState::None => 0,
            ConnectivityState::Portal => 1,
            ConnectivityState::Limited => 2,
            ConnectivityState::Full => 3,
        }
    }

    /// List all network devices
    async fn list_devices(&self) -> Vec<OwnedObjectPath> {
        let device_ids = self.manager.list_devices().await;
        device_ids
            .into_iter()
            .map(|id| format!("/org/sol/Network1/Device/{}", object_path_component(&id.0)))
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

        let path = format!(
            "/org/sol/Network1/Connection/{}",
            object_path_component(&profile_id)
        );
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

    /// Scan for WiFi networks
    async fn scan_wifi(
        &self,
    ) -> zbus::fdo::Result<Vec<HashMap<String, zbus::zvariant::Value<'static>>>> {
        let networks = self
            .manager
            .scan_wifi()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        let mut results = Vec::new();
        for network in networks {
            let mut map = HashMap::new();
            map.insert("ssid".to_string(), zbus::zvariant::Value::new(network.ssid));
            map.insert(
                "bssid".to_string(),
                zbus::zvariant::Value::new(network.bssid),
            );
            map.insert(
                "signal_strength".to_string(),
                zbus::zvariant::Value::new(network.signal_strength),
            );
            map.insert(
                "frequency".to_string(),
                zbus::zvariant::Value::new(network.frequency),
            );
            map.insert(
                "security".to_string(),
                zbus::zvariant::Value::new(format!("{:?}", network.security)),
            );
            results.push(map);
        }

        Ok(results)
    }

    /// Quick connect to WiFi network (creates profile automatically)
    async fn connect_wifi(&self, ssid: String, passphrase: String) -> zbus::fdo::Result<String> {
        let pass = if passphrase.is_empty() {
            None
        } else {
            Some(passphrase)
        };

        let profile_id = self
            .manager
            .connect_wifi_quick(ssid, pass)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        Ok(profile_id.0)
    }

    /// Get WiFi signal strength (0-100)
    async fn wifi_signal_strength(&self) -> zbus::fdo::Result<u8> {
        self.manager
            .get_wifi_signal_strength()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .ok_or_else(|| zbus::fdo::Error::Failed("Not connected to WiFi".to_string()))
    }

    /// Get active connection information
    async fn active_connection(
        &self,
    ) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::Value<'static>>> {
        let info = self
            .manager
            .get_active_connection_info()
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .ok_or_else(|| zbus::fdo::Error::Failed("No active connection".to_string()))?;

        let mut map = HashMap::new();
        map.insert(
            "type".to_string(),
            zbus::zvariant::Value::new(info.connection_type),
        );
        map.insert(
            "interface".to_string(),
            zbus::zvariant::Value::new(info.interface),
        );
        map.insert(
            "details".to_string(),
            zbus::zvariant::Value::new(info.details),
        );
        if let Some(strength) = info.signal_strength {
            map.insert(
                "signal_strength".to_string(),
                zbus::zvariant::Value::new(strength),
            );
        }

        Ok(map)
    }

    /// List all saved profiles
    async fn list_profiles(&self) -> zbus::fdo::Result<Vec<String>> {
        let profiles = self.manager.list_profiles().await;
        Ok(profiles.into_iter().map(|id| id.0).collect())
    }

    /// Enable/disable auto-connect for a profile
    async fn set_auto_connect(&self, profile_id: String, enabled: bool) -> zbus::fdo::Result<()> {
        self.manager
            .set_auto_connect(&ProfileId(profile_id), enabled)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
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

    /// Signal: WiFi scan completed
    #[zbus(signal)]
    async fn scan_completed(
        signal_ctxt: &zbus::SignalContext<'_>,
        networks_count: u32,
    ) -> zbus::Result<()>;
}

fn object_path_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
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
    fn encodes_ids_as_valid_dbus_path_components() {
        assert_eq!(object_path_component("wifi:3"), "wifi_3");
        assert_eq!(
            object_path_component("ca8dfd21-5d20-41a2"),
            "ca8dfd21_5d20_41a2"
        );
    }
}
