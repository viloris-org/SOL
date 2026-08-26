use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpnProfile {
    pub vpn_type: VpnType,
    pub server: String,
    pub port: u16,
    pub credentials_encrypted: Vec<u8>,  // Encrypted credentials
    pub auto_connect: bool,
    pub kill_switch: bool,  // Prevent traffic leaks if VPN disconnects
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VpnType {
    WireGuard { public_key: String },
    OpenVpn { config_path: String },
    IPSec { auth_method: IPSecAuthMethod },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IPSecAuthMethod {
    Psk,
    Certificate,
}

impl VpnProfile {
    pub fn new_wireguard(server: String, public_key: String) -> Self {
        Self {
            vpn_type: VpnType::WireGuard { public_key },
            server,
            port: 51820,
            credentials_encrypted: vec![],
            auto_connect: false,
            kill_switch: true,
        }
    }
}
