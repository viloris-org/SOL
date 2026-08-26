use zbus::{interface, fdo};
use std::collections::HashMap;

use crate::device::wifi::WiFiNetwork;
use crate::manager::NetworkManager;

/// D-Bus WiFi interface implementation
pub struct WiFiInterface {
    manager: NetworkManager,
    device_id: String,
}

impl WiFiInterface {
    pub fn new(manager: NetworkManager, device_id: String) -> Self {
        Self { manager, device_id }
    }
}

#[interface(name = "org.sol.Network1.WiFi")]
impl WiFiInterface {
    /// Scan for available WiFi networks
    async fn scan(&self) {
        // Trigger scan through manager
        // TODO: Implement scan triggering
    }

    /// Get list of available networks from last scan
    async fn get_networks(&self) -> Vec<HashMap<String, zbus::zvariant::Value<'static>>> {
        // TODO: Return cached scan results
        vec![]
    }

    /// Connect to a WiFi network
    ///
    /// # Arguments
    /// * `ssid` - Network SSID
    /// * `passphrase` - Network passphrase (empty for open networks)
    async fn connect(&self, _ssid: String, _passphrase: String) -> fdo::Result<()> {
        // TODO: Implement connection through manager
        Ok(())
    }

    /// Disconnect from current WiFi network
    async fn disconnect(&self) -> fdo::Result<()> {
        // TODO: Implement disconnection
        Ok(())
    }

    /// Get current signal strength (0-100)
    #[zbus(property)]
    async fn signal_strength(&self) -> u8 {
        // TODO: Get actual signal strength
        0
    }

    /// Get currently connected network SSID
    #[zbus(property)]
    async fn current_network(&self) -> String {
        // TODO: Get current network
        String::new()
    }

    /// WiFi enabled state
    #[zbus(property)]
    async fn enabled(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn set_enabled(&self, _enabled: bool) {
        // TODO: Enable/disable WiFi
    }
}

/// Convert WiFiNetwork to D-Bus dict
pub fn network_to_dict(network: &WiFiNetwork) -> HashMap<String, zbus::zvariant::Value<'static>> {
    let mut map = HashMap::new();

    map.insert("ssid".to_string(), network.ssid.clone().into());
    map.insert("bssid".to_string(), network.bssid.clone().into());
    map.insert("signal_strength".to_string(), network.signal_strength.into());
    map.insert("frequency".to_string(), network.frequency.into());
    map.insert("security".to_string(), format!("{:?}", network.security).into());

    map
}
