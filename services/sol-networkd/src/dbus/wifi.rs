use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use zbus::{fdo, interface};

use crate::device::wifi::WiFiNetwork;
use crate::manager::NetworkManager;

/// D-Bus WiFi interface implementation
pub struct WiFiInterface {
    manager: NetworkManager,
    _device_id: String,
    networks: Arc<RwLock<Vec<WiFiNetwork>>>,
}

impl WiFiInterface {
    pub fn new(manager: NetworkManager, device_id: String) -> Self {
        Self {
            manager,
            _device_id: device_id,
            networks: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[interface(name = "org.sol.Network1.WiFi")]
impl WiFiInterface {
    /// Scan for available WiFi networks
    async fn scan(&self) -> fdo::Result<()> {
        let networks = self
            .manager
            .scan_wifi()
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        *self.networks.write().await = networks;
        Ok(())
    }

    /// Get list of available networks from last scan
    async fn get_networks(&self) -> Vec<HashMap<String, zbus::zvariant::Value<'static>>> {
        self.networks
            .read()
            .await
            .iter()
            .map(network_to_dict)
            .collect()
    }

    /// Connect to a WiFi network
    ///
    /// # Arguments
    /// * `ssid` - Network SSID
    /// * `passphrase` - Network passphrase (empty for open networks)
    async fn connect(&self, ssid: String, passphrase: String) -> fdo::Result<()> {
        let passphrase = (!passphrase.is_empty()).then_some(passphrase);
        self.manager
            .connect_wifi_quick(ssid, passphrase)
            .await
            .map(|_| ())
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    /// Disconnect from current WiFi network
    async fn disconnect(&self) -> fdo::Result<()> {
        self.manager
            .disconnect_active_wifi()
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    /// Get current signal strength (0-100)
    #[zbus(property)]
    async fn signal_strength(&self) -> u8 {
        self.manager
            .get_wifi_signal_strength()
            .await
            .ok()
            .flatten()
            .unwrap_or(0)
    }

    /// Get currently connected network SSID
    #[zbus(property)]
    async fn current_network(&self) -> String {
        self.manager
            .get_wifi_current_network()
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// WiFi enabled state
    #[zbus(property)]
    async fn enabled(&self) -> bool {
        self.manager.get_wifi_powered().await.unwrap_or(false)
    }

    #[zbus(property)]
    async fn set_enabled(&self, enabled: bool) -> zbus::Result<()> {
        self.manager
            .set_wifi_powered(enabled)
            .await
            .map_err(|error| zbus::Error::Failure(error.to_string()))
    }
}

/// Convert WiFiNetwork to D-Bus dict
pub fn network_to_dict(network: &WiFiNetwork) -> HashMap<String, zbus::zvariant::Value<'static>> {
    let mut map = HashMap::new();

    map.insert("ssid".to_string(), network.ssid.clone().into());
    map.insert("bssid".to_string(), network.bssid.clone().into());
    map.insert(
        "signal_strength".to_string(),
        network.signal_strength.into(),
    );
    map.insert("frequency".to_string(), network.frequency.into());
    map.insert(
        "security".to_string(),
        format!("{:?}", network.security).into(),
    );

    map
}
