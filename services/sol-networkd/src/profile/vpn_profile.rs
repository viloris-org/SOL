use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpnProfile {
    pub name: String,
    pub vpn_type: VpnType,
    pub auto_connect: bool,
    pub kill_switch: bool, // Prevent traffic leaks if VPN disconnects
    pub on_demand: bool,   // Connect automatically when certain conditions are met
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VpnType {
    WireGuard(WireGuardProfile),
    OpenVpn(OpenVpnProfile),
    IPSec(IPSecProfile),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireGuardProfile {
    pub private_key_encrypted: Vec<u8>, // Encrypted with system key
    pub address: Option<String>,        // CIDR notation (e.g., "10.0.0.2/24")
    pub listen_port: Option<u16>,
    pub peers: Vec<WireGuardPeerProfile>,
    pub dns: Vec<IpAddr>,
    pub mtu: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireGuardPeerProfile {
    pub name: Option<String>, // Friendly name for this peer
    pub public_key: String,
    pub preshared_key_encrypted: Option<Vec<u8>>,
    pub endpoint: Option<String>,          // "host:port"
    pub allowed_ips: Vec<String>,          // CIDR notation
    pub persistent_keepalive: Option<u16>, // Seconds
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenVpnProfile {
    pub server: String,
    pub port: u16,
    pub protocol: OpenVpnProtocol,
    pub auth_type: OpenVpnAuthType,
    pub ca_cert: Vec<u8>,
    pub client_cert: Option<Vec<u8>>,
    pub client_key_encrypted: Option<Vec<u8>>,
    pub username: Option<String>,
    pub password_encrypted: Option<Vec<u8>>,
    pub compression: bool,
    pub cipher: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OpenVpnProtocol {
    Udp,
    Tcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OpenVpnAuthType {
    Certificate,
    UsernamePassword,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IPSecProfile {
    pub server: String,
    pub ipsec_type: IPSecType,
    pub username: Option<String>,
    pub password_encrypted: Option<Vec<u8>>,
    pub psk_encrypted: Option<Vec<u8>>,
    pub remote_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IPSecType {
    IKEv2,
    L2TP,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IPSecAuthMethod {
    Psk,
    Certificate,
}

impl VpnProfile {
    pub fn new_wireguard(name: String) -> Self {
        Self {
            name,
            vpn_type: VpnType::WireGuard(WireGuardProfile {
                private_key_encrypted: Vec::new(),
                address: None,
                listen_port: None,
                peers: Vec::new(),
                dns: Vec::new(),
                mtu: Some(1420),
            }),
            auto_connect: false,
            kill_switch: true,
            on_demand: false,
        }
    }

    pub fn new_openvpn(name: String, server: String, port: u16) -> Self {
        Self {
            name,
            vpn_type: VpnType::OpenVpn(OpenVpnProfile {
                server,
                port,
                protocol: OpenVpnProtocol::Udp,
                auth_type: OpenVpnAuthType::Certificate,
                ca_cert: Vec::new(),
                client_cert: None,
                client_key_encrypted: None,
                username: None,
                password_encrypted: None,
                compression: false,
                cipher: None,
            }),
            auto_connect: false,
            kill_switch: true,
            on_demand: false,
        }
    }

    pub fn new_ipsec(name: String, server: String, ipsec_type: IPSecType) -> Self {
        Self {
            name,
            vpn_type: VpnType::IPSec(IPSecProfile {
                server,
                ipsec_type,
                username: None,
                password_encrypted: None,
                psk_encrypted: None,
                remote_id: None,
            }),
            auto_connect: false,
            kill_switch: true,
            on_demand: false,
        }
    }
}

impl WireGuardPeerProfile {
    pub fn new(public_key: String) -> Self {
        Self {
            name: None,
            public_key,
            preshared_key_encrypted: None,
            endpoint: None,
            allowed_ips: vec!["0.0.0.0/0".to_string()], // Route all traffic by default
            persistent_keepalive: Some(25),
        }
    }

    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    pub fn with_allowed_ips(mut self, ips: Vec<String>) -> Self {
        self.allowed_ips = ips;
        self
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}
