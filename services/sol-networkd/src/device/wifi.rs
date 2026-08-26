use anyhow::Result;
use tracing::info;

use crate::device::Device;

/// WiFi device implementation
pub struct WiFiDevice {
    device: Device,
}

impl WiFiDevice {
    pub fn new(device: Device) -> Self {
        Self { device }
    }

    pub async fn scan(&self) -> Result<Vec<WiFiNetwork>> {
        info!("Scanning WiFi networks on {}", self.device.interface);
        // TODO: Implement WiFi scanning via iwd D-Bus or nl80211
        Ok(vec![])
    }

    pub async fn connect(&self, ssid: &str, _passphrase: &str) -> Result<()> {
        info!("Connecting to WiFi network: {}", ssid);
        // TODO: Implement WiFi connection
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting WiFi on {}", self.device.interface);
        // TODO: Implement WiFi disconnection
        Ok(())
    }

    pub fn signal_strength(&self) -> Option<u8> {
        // TODO: Get current signal strength (0-100)
        None
    }
}

#[derive(Debug, Clone)]
pub struct WiFiNetwork {
    pub ssid: String,
    pub bssid: String,
    pub signal_strength: u8,  // 0-100
    pub frequency: u32,        // MHz
    pub security: WiFiSecurity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WiFiSecurity {
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
}
