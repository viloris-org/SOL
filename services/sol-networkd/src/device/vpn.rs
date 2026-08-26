use anyhow::Result;
use tracing::info;

use crate::device::Device;

/// VPN device implementation
pub struct VpnDevice {
    device: Device,
}

impl VpnDevice {
    pub fn new(device: Device) -> Self {
        Self { device }
    }

    pub async fn connect(&self, config: &VpnConfig) -> Result<()> {
        info!("Connecting VPN: {}", config.name);
        // TODO: Implement VPN connection (WireGuard, OpenVPN, etc.)
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting VPN on {}", self.device.interface);
        // TODO: Implement VPN disconnection
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct VpnConfig {
    pub name: String,
    pub vpn_type: VpnType,
    pub server: String,
    pub credentials: VpnCredentials,
}

#[derive(Debug, Clone)]
pub enum VpnType {
    WireGuard,
    OpenVpn,
    IPSec,
}

#[derive(Debug, Clone)]
pub struct VpnCredentials {
    // TODO: Implement credential storage
    // Should be encrypted at rest
}
