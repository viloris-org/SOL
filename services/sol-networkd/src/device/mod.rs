pub mod ethernet;
pub mod vpn;
pub mod wifi;

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub device_type: DeviceType,
    pub interface: String, // e.g., wlan0, eth0
    pub state: DeviceState,
    pub ifindex: u32,
    pub hw_address: Option<String>,
    pub mtu: Option<u32>,
    pub carrier: bool,
    pub ip_addresses: Vec<std::net::IpAddr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeviceType {
    WiFi,
    Ethernet,
    Vpn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeviceState {
    /// Device exists but is not usable (no carrier, disabled, etc.)
    Unavailable,
    /// Device is ready but not connected to any network
    Disconnected,
    /// Connection attempt is starting (device preparation)
    Preparing,
    /// Device is being configured (DHCP, static IP setup)
    Configuring,
    /// Authentication is required (WiFi passphrase, VPN credentials)
    NeedAuth,
    /// IP configuration is being applied
    IpConfig,
    /// Connectivity check is running
    IpCheck,
    /// Device is fully configured and connected
    Active,
    /// Device is disconnecting
    Deactivating,
    /// Configuration or connection failed
    Failed,
}

impl Device {
    pub fn new(id: DeviceId, device_type: DeviceType, interface: String, ifindex: u32) -> Self {
        Self {
            id,
            device_type,
            interface,
            state: DeviceState::Unavailable,
            ifindex,
            hw_address: None,
            mtu: None,
            carrier: false,
            ip_addresses: Vec::new(),
        }
    }

    /// Update device state and return true if state changed
    pub fn set_state(&mut self, new_state: DeviceState) -> bool {
        if self.state != new_state {
            self.state = new_state;
            true
        } else {
            false
        }
    }

    /// Check if device can accept connections
    pub fn is_available(&self) -> bool {
        matches!(self.state, DeviceState::Disconnected | DeviceState::Active)
    }

    /// Check if device is currently connected
    pub fn is_connected(&self) -> bool {
        matches!(self.state, DeviceState::Active)
    }
}
