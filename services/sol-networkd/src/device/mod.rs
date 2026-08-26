pub mod wifi;
pub mod ethernet;
pub mod vpn;

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: DeviceId,
    pub device_type: DeviceType,
    pub interface: String,  // e.g., wlan0, eth0
    pub state: DeviceState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceType {
    WiFi,
    Ethernet,
    Vpn,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceState {
    Unavailable,
    Disconnected,
    Preparing,
    Configuring,
    NeedAuth,
    IpConfig,
    IpCheck,
    Active,
    Deactivating,
    Failed,
}

impl Device {
    pub fn new(id: DeviceId, device_type: DeviceType, interface: String) -> Self {
        Self {
            id,
            device_type,
            interface,
            state: DeviceState::Unavailable,
        }
    }
}
