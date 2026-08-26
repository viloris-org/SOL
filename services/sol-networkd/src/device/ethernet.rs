use anyhow::{Result, Context};
use tracing::info;
use std::net::Ipv4Addr;

use crate::device::Device;
use crate::dhcp::{DhcpClient, DhcpLease};
use crate::profile::ethernet_profile::IpConfig;

/// Ethernet device implementation
pub struct EthernetDevice {
    device: Device,
}

impl EthernetDevice {
    pub fn new(device: Device) -> Self {
        Self { device }
    }

    pub async fn connect(&self, ip_config: &IpConfig) -> Result<()> {
        info!("Bringing up Ethernet interface: {}", self.device.interface);

        // Bring interface up
        self.set_link_up().await?;

        match ip_config {
            IpConfig::Dhcp => {
                info!("Starting DHCP on {}", self.device.interface);
                let lease = self.start_dhcp().await?;
                self.apply_dhcp_lease(&lease).await?;
            }
            IpConfig::Static { address, netmask, gateway, dns } => {
                info!("Configuring static IP on {}", self.device.interface);
                self.set_static_address(*address, *netmask).await?;
                if let Some(gw) = gateway {
                    self.add_default_route(*gw).await?;
                }
                if !dns.is_empty() {
                    self.set_dns_servers(dns).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        info!("Bringing down Ethernet interface: {}", self.device.interface);
        self.set_link_down().await?;
        Ok(())
    }

    async fn set_link_up(&self) -> Result<()> {
        tokio::process::Command::new("ip")
            .args(["link", "set", &self.device.interface, "up"])
            .output()
            .await
            .context("Failed to bring interface up")?;
        Ok(())
    }

    async fn set_link_down(&self) -> Result<()> {
        tokio::process::Command::new("ip")
            .args(["link", "set", &self.device.interface, "down"])
            .output()
            .await
            .context("Failed to bring interface down")?;
        Ok(())
    }

    async fn start_dhcp(&self) -> Result<DhcpLease> {
        let mac = self.get_mac_address().await?;
        let dhcp_client = DhcpClient::new(self.device.interface.clone(), mac);
        dhcp_client.acquire_lease().await
    }

    async fn apply_dhcp_lease(&self, lease: &DhcpLease) -> Result<()> {
        // Apply IP address
        self.set_static_address(lease.ip_address, lease.subnet_mask).await?;

        // Apply default route
        if let Some(router) = lease.router {
            self.add_default_route(router).await?;
        }

        // Apply DNS servers
        if !lease.dns_servers.is_empty() {
            self.set_dns_servers(&lease.dns_servers).await?;
        }

        Ok(())
    }

    async fn set_static_address(&self, address: Ipv4Addr, netmask: Ipv4Addr) -> Result<()> {
        let prefix_len = netmask_to_prefix(netmask);
        let cidr = format!("{}/{}", address, prefix_len);

        // Remove existing addresses
        let _ = tokio::process::Command::new("ip")
            .args(["addr", "flush", "dev", &self.device.interface])
            .output()
            .await;

        // Add new address
        tokio::process::Command::new("ip")
            .args(["addr", "add", &cidr, "dev", &self.device.interface])
            .output()
            .await
            .context("Failed to set IP address")?;

        Ok(())
    }

    async fn add_default_route(&self, gateway: Ipv4Addr) -> Result<()> {
        // Remove existing default route for this interface
        let _ = tokio::process::Command::new("ip")
            .args(["route", "del", "default", "dev", &self.device.interface])
            .output()
            .await;

        // Add new default route
        tokio::process::Command::new("ip")
            .args(["route", "add", "default", "via", &gateway.to_string(), "dev", &self.device.interface])
            .output()
            .await
            .context("Failed to add default route")?;

        Ok(())
    }

    async fn set_dns_servers(&self, servers: &[Ipv4Addr]) -> Result<()> {
        // Write to /etc/resolv.conf (temporary, should use systemd-resolved)
        let mut content = String::new();
        for server in servers {
            content.push_str(&format!("nameserver {}\n", server));
        }

        tokio::fs::write("/etc/resolv.conf", content)
            .await
            .context("Failed to write DNS configuration")?;

        Ok(())
    }

    async fn get_mac_address(&self) -> Result<[u8; 6]> {
        let path = format!("/sys/class/net/{}/address", self.device.interface);
        let mac_str = tokio::fs::read_to_string(path)
            .await
            .context("Failed to read MAC address")?;

        let mac_str = mac_str.trim();
        let parts: Vec<&str> = mac_str.split(':').collect();

        if parts.len() != 6 {
            anyhow::bail!("Invalid MAC address format");
        }

        let mut mac = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            mac[i] = u8::from_str_radix(part, 16)
                .context("Failed to parse MAC address")?;
        }

        Ok(mac)
    }

    pub fn is_carrier_detected(&self) -> bool {
        // Read carrier status from sysfs
        let path = format!("/sys/class/net/{}/carrier", self.device.interface);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .map(|v| v == 1)
            .unwrap_or(false)
    }

    pub fn link_speed(&self) -> Option<u32> {
        // Read link speed from sysfs (in Mbps)
        let path = format!("/sys/class/net/{}/speed", self.device.interface);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }
}

fn netmask_to_prefix(netmask: Ipv4Addr) -> u8 {
    let octets = netmask.octets();
    let mut prefix = 0u8;

    for octet in octets {
        prefix += octet.count_ones() as u8;
    }

    prefix
}
