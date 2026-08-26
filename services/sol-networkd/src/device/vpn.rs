use anyhow::{Result, Context, anyhow};
use tracing::{info, warn, debug};
use std::net::IpAddr;
use wireguard_control::{Backend, DeviceUpdate, InterfaceName, Key, PeerConfigBuilder};
use std::str::FromStr;
use ipnet::IpNet;

use crate::device::Device;

/// VPN device implementation
pub struct VpnDevice {
    device: Device,
    backend: Backend,
}

impl VpnDevice {
    pub fn new(device: Device) -> Self {
        // Use kernel backend for WireGuard
        Self {
            device,
            backend: Backend::Kernel,
        }
    }

    pub async fn connect(&self, config: &VpnConfig) -> Result<()> {
        info!("Connecting VPN: {} ({})", config.name, config.vpn_type_str());

        match &config.vpn_type {
            VpnType::WireGuard(wg_config) => {
                self.connect_wireguard(wg_config).await
            }
            VpnType::OpenVpn => {
                warn!("OpenVPN support not yet implemented");
                Err(anyhow!("OpenVPN not supported"))
            }
            VpnType::IPSec => {
                warn!("IPSec support not yet implemented");
                Err(anyhow!("IPSec not supported"))
            }
        }
    }

    async fn connect_wireguard(&self, config: &WireGuardConfig) -> Result<()> {
        info!("Setting up WireGuard interface: {}", self.device.interface);

        let interface = InterfaceName::from_str(&self.device.interface)
            .context("Invalid interface name")?;

        // Parse private key
        let private_key = Key::from_base64(&config.private_key)
            .context("Invalid private key")?;

        // Create device configuration
        let mut device_update = DeviceUpdate::new()
            .set_private_key(private_key);

        if let Some(port) = config.listen_port {
            device_update = device_update.set_listen_port(port);
        }

        // Add peers
        for peer in &config.peers {
            let public_key = Key::from_base64(&peer.public_key)
                .context("Invalid peer public key")?;

            let mut peer_builder = PeerConfigBuilder::new(&public_key);

            if let Some(ref endpoint) = peer.endpoint {
                peer_builder = peer_builder.set_endpoint(endpoint.parse()
                    .context("Invalid peer endpoint")?);
            }

            if let Some(ref psk) = peer.preshared_key {
                let psk_key = Key::from_base64(psk)
                    .context("Invalid preshared key")?;
                peer_builder = peer_builder.set_preshared_key(psk_key);
            }

            if let Some(keepalive) = peer.persistent_keepalive {
                peer_builder = peer_builder.set_persistent_keepalive_interval(keepalive);
            }

            // Add allowed IPs
            for allowed_ip in &peer.allowed_ips {
                let ip_net: IpNet = allowed_ip.parse()
                    .context("Invalid allowed IP")?;
                peer_builder = peer_builder.add_allowed_ip(ip_net.addr(), ip_net.prefix_len());
            }

            device_update = device_update.add_peer(peer_builder);
        }

        // Apply configuration
        device_update.apply(&interface, self.backend)
            .context("Failed to apply WireGuard configuration")?;

        // Configure interface IP address
        if let Some(ref address) = config.address {
            self.configure_interface_ip(address).await?;
        }

        info!("WireGuard VPN connected successfully");
        Ok(())
    }

    async fn configure_interface_ip(&self, address: &str) -> Result<()> {
        // Use ip command to configure interface
        let output = tokio::process::Command::new("ip")
            .args(["address", "add", address, "dev", &self.device.interface])
            .output()
            .await
            .context("Failed to run ip command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to configure IP address: {}", stderr));
        }

        // Bring interface up
        let output = tokio::process::Command::new("ip")
            .args(["link", "set", &self.device.interface, "up"])
            .output()
            .await
            .context("Failed to bring interface up")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to bring interface up: {}", stderr));
        }

        debug!("Configured {} with address {}", self.device.interface, address);
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting VPN on {}", self.device.interface);

        let interface = InterfaceName::from_str(&self.device.interface)
            .context("Invalid interface name")?;

        // Get current device info
        if let Ok(device_info) = wireguard_control::Device::get(&interface, self.backend) {
            // Remove all peers
            let mut device_update = DeviceUpdate::new();
            for peer in device_info.peers {
                device_update = device_update.remove_peer_by_key(&peer.config.public_key);
            }

            device_update.apply(&interface, self.backend)
                .context("Failed to remove WireGuard peers")?;
        }

        // Bring interface down
        let output = tokio::process::Command::new("ip")
            .args(["link", "set", &self.device.interface, "down"])
            .output()
            .await
            .context("Failed to bring interface down")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to bring interface down: {}", stderr);
        }

        info!("VPN disconnected");
        Ok(())
    }

    pub async fn get_status(&self) -> Result<VpnStatus> {
        let interface = InterfaceName::from_str(&self.device.interface)
            .context("Invalid interface name")?;

        match wireguard_control::Device::get(&interface, self.backend) {
            Ok(device_info) => {
                let connected = !device_info.peers.is_empty();
                let mut peers = Vec::new();

                for peer in device_info.peers {
                    peers.push(PeerStatus {
                        public_key: peer.config.public_key.to_base64(),
                        endpoint: peer.config.endpoint.map(|e| e.to_string()),
                        rx_bytes: peer.stats.rx_bytes,
                        tx_bytes: peer.stats.tx_bytes,
                        last_handshake: peer.stats.last_handshake_time
                            .and_then(|t| t.elapsed().ok())
                            .map(|d| d.as_secs()),
                    });
                }

                Ok(VpnStatus {
                    connected,
                    peers,
                })
            }
            Err(_) => {
                Ok(VpnStatus {
                    connected: false,
                    peers: Vec::new(),
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct VpnConfig {
    pub name: String,
    pub vpn_type: VpnType,
}

impl VpnConfig {
    pub fn vpn_type_str(&self) -> &'static str {
        match self.vpn_type {
            VpnType::WireGuard(_) => "WireGuard",
            VpnType::OpenVpn => "OpenVPN",
            VpnType::IPSec => "IPSec",
        }
    }
}

#[derive(Debug, Clone)]
pub enum VpnType {
    WireGuard(WireGuardConfig),
    OpenVpn,
    IPSec,
}

#[derive(Debug, Clone)]
pub struct WireGuardConfig {
    pub private_key: String,  // Base64-encoded
    pub address: Option<String>,  // CIDR notation (e.g., "10.0.0.2/24")
    pub listen_port: Option<u16>,
    pub peers: Vec<WireGuardPeer>,
    pub dns: Vec<IpAddr>,
}

#[derive(Debug, Clone)]
pub struct WireGuardPeer {
    pub public_key: String,  // Base64-encoded
    pub preshared_key: Option<String>,  // Base64-encoded
    pub endpoint: Option<String>,  // "host:port"
    pub allowed_ips: Vec<String>,  // CIDR notation
    pub persistent_keepalive: Option<u16>,  // Seconds
}

impl WireGuardConfig {
    pub fn new(private_key: String) -> Self {
        Self {
            private_key,
            address: None,
            listen_port: None,
            peers: Vec::new(),
            dns: Vec::new(),
        }
    }

    pub fn add_peer(mut self, peer: WireGuardPeer) -> Self {
        self.peers.push(peer);
        self
    }

    pub fn set_address(mut self, address: String) -> Self {
        self.address = Some(address);
        self
    }

    pub fn set_listen_port(mut self, port: u16) -> Self {
        self.listen_port = Some(port);
        self
    }

    pub fn add_dns(mut self, dns: IpAddr) -> Self {
        self.dns.push(dns);
        self
    }
}

#[derive(Debug, Clone)]
pub struct VpnStatus {
    pub connected: bool,
    pub peers: Vec<PeerStatus>,
}

#[derive(Debug, Clone)]
pub struct PeerStatus {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub last_handshake: Option<u64>,  // Seconds ago
}

/// Generate a new WireGuard keypair
pub fn generate_keypair() -> Result<(String, String)> {
    use ring::rand::SecureRandom;

    let rng = ring::rand::SystemRandom::new();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key)
        .map_err(|_| anyhow!("Failed to generate random key"))?;

    let private_key_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, private_key);

    // Derive public key from private key
    let key = Key::from_base64(&private_key_base64)
        .context("Failed to parse generated private key")?;
    let public_key = key.get_public();
    let public_key_base64 = public_key.to_base64();

    Ok((private_key_base64, public_key_base64))
}
