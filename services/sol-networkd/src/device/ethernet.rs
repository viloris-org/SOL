use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr};
use tracing::info;

use crate::device::Device;
use crate::dhcp::{DhcpClient, DhcpLease};
use crate::dns::DnsManager;
use crate::profile::ethernet_profile::IpConfig;

/// Ethernet device implementation
pub struct EthernetDevice {
    device: Device,
    dns_manager: Option<DnsManager>,
}

impl EthernetDevice {
    pub fn new(device: Device) -> Self {
        Self {
            device,
            dns_manager: None,
        }
    }

    pub fn with_dns_manager(device: Device, dns_manager: DnsManager) -> Self {
        Self {
            device,
            dns_manager: Some(dns_manager),
        }
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
            IpConfig::Static {
                address,
                netmask,
                gateway,
                dns,
            } => {
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
        info!(
            "Bringing down Ethernet interface: {}",
            self.device.interface
        );
        self.set_link_down().await?;
        Ok(())
    }

    async fn set_link_up(&self) -> Result<()> {
        self.run_ip(&["link", "set", &self.device.interface, "up"])
            .await
    }

    async fn set_link_down(&self) -> Result<()> {
        self.run_ip(&["link", "set", &self.device.interface, "down"])
            .await
    }

    async fn start_dhcp(&self) -> Result<DhcpLease> {
        let mac = self.get_mac_address().await?;
        let dhcp_client = DhcpClient::new(self.device.interface.clone(), mac);
        dhcp_client.acquire_lease().await
    }

    async fn apply_dhcp_lease(&self, lease: &DhcpLease) -> Result<()> {
        // Apply IP address
        self.set_static_address(lease.ip_address, lease.subnet_mask)
            .await?;

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
        let prefix_len = netmask_to_prefix(netmask)?;
        let cidr = format!("{}/{}", address, prefix_len);

        self.run_ip(&["address", "replace", &cidr, "dev", &self.device.interface])
            .await
    }

    async fn add_default_route(&self, gateway: Ipv4Addr) -> Result<()> {
        self.run_ip(&[
            "route",
            "replace",
            "default",
            "via",
            &gateway.to_string(),
            "dev",
            &self.device.interface,
        ])
        .await
    }

    async fn set_dns_servers(&self, servers: &[Ipv4Addr]) -> Result<()> {
        let dns_manager = self
            .dns_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DNS manager is unavailable"))?;
        dns_manager
            .set_dns_servers(
                &self.device.interface,
                servers.iter().copied().map(IpAddr::V4).collect(),
            )
            .await
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
            mac[i] = u8::from_str_radix(part, 16).context("Failed to parse MAC address")?;
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

    async fn run_ip(&self, args: &[&str]) -> Result<()> {
        let output = tokio::process::Command::new("ip")
            .args(args)
            .output()
            .await
            .with_context(|| format!("failed to run `ip {}`", args.join(" ")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            anyhow::bail!("`ip {}` failed: {}", args.join(" "), stderr);
        }

        Ok(())
    }
}

fn netmask_to_prefix(netmask: Ipv4Addr) -> Result<u8> {
    let mask = u32::from(netmask);
    let prefix = mask.leading_ones() as u8;
    let expected = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    if mask != expected {
        anyhow::bail!("non-contiguous IPv4 netmask: {netmask}");
    }

    Ok(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_contiguous_netmasks_to_prefixes() {
        assert_eq!(netmask_to_prefix(Ipv4Addr::new(0, 0, 0, 0)).unwrap(), 0);
        assert_eq!(
            netmask_to_prefix(Ipv4Addr::new(255, 255, 255, 0)).unwrap(),
            24
        );
        assert_eq!(
            netmask_to_prefix(Ipv4Addr::new(255, 255, 255, 255)).unwrap(),
            32
        );
    }

    #[test]
    fn rejects_non_contiguous_netmasks() {
        assert!(netmask_to_prefix(Ipv4Addr::new(255, 0, 255, 0)).is_err());
    }
}
