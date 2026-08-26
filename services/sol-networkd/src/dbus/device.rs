use zbus::interface;
use std::collections::HashMap;

use crate::device::{Device, DeviceType};

/// D-Bus Device interface implementation
pub struct DeviceInterface {
    device: Device,
}

impl DeviceInterface {
    pub fn new(device: Device) -> Self {
        Self { device }
    }
}

#[interface(name = "org.sol.Network1.Device")]
impl DeviceInterface {
    /// Device type (wifi, ethernet, vpn)
    async fn device_type(&self) -> String {
        match self.device.device_type {
            DeviceType::WiFi => "wifi".to_string(),
            DeviceType::Ethernet => "ethernet".to_string(),
            DeviceType::Vpn => "vpn".to_string(),
        }
    }

    /// Device state
    async fn state(&self) -> String {
        format!("{:?}", self.device.state)
    }

    /// Network interface name (e.g., wlan0, eth0)
    async fn interface(&self) -> String {
        self.device.interface.clone()
    }

    /// Scan for networks (WiFi only)
    async fn scan(&self) -> zbus::fdo::Result<()> {
        if self.device.device_type != DeviceType::WiFi {
            return Err(zbus::fdo::Error::NotSupported(
                "Scan only supported on WiFi devices".into(),
            ));
        }

        // TODO: Trigger WiFi scan
        Ok(())
    }

    /// Get available networks (WiFi only)
    async fn get_networks(&self) -> zbus::fdo::Result<Vec<HashMap<String, zbus::zvariant::Value<'static>>>> {
        if self.device.device_type != DeviceType::WiFi {
            return Err(zbus::fdo::Error::NotSupported(
                "GetNetworks only supported on WiFi devices".into(),
            ));
        }

        // TODO: Return actual scan results
        Ok(vec![])
    }
}
