use anyhow::Result;
use tracing::{info, warn};
use zbus::Connection;
use std::net::IpAddr;

/// DNS integration with systemd-resolved
pub struct DnsManager {
    connection: Option<Connection>,
}

impl DnsManager {
    pub fn new() -> Self {
        Self { connection: None }
    }

    pub async fn init(&mut self) -> Result<()> {
        self.connection = Some(Connection::system().await?);
        Ok(())
    }

    pub async fn set_dns_servers(&self, interface: &str, servers: Vec<IpAddr>) -> Result<()> {
        info!("Setting DNS servers for {}: {:?}", interface, servers);

        let Some(conn) = &self.connection else {
            warn!("DNS manager not initialized");
            return Ok(());
        };

        // Get interface index
        let ifindex = self.get_interface_index(interface)?;

        // Call systemd-resolved D-Bus method
        // org.freedesktop.resolve1.Manager.SetLinkDNS
        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.resolve1",
            "/org/freedesktop/resolve1",
            "org.freedesktop.resolve1.Manager",
        ).await?;

        // Convert IpAddr to D-Bus format (family, address bytes)
        let dns_entries: Vec<(i32, Vec<u8>)> = servers.into_iter().map(|addr| {
            match addr {
                IpAddr::V4(v4) => (2, v4.octets().to_vec()), // AF_INET = 2
                IpAddr::V6(v6) => (10, v6.octets().to_vec()), // AF_INET6 = 10
            }
        }).collect();

        let _: () = proxy.call("SetLinkDNS", &(ifindex, dns_entries)).await?;

        Ok(())
    }

    pub async fn set_search_domains(&self, interface: &str, domains: Vec<String>) -> Result<()> {
        info!("Setting search domains for {}: {:?}", interface, domains);

        let Some(conn) = &self.connection else {
            warn!("DNS manager not initialized");
            return Ok(());
        };

        let ifindex = self.get_interface_index(interface)?;

        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.resolve1",
            "/org/freedesktop/resolve1",
            "org.freedesktop.resolve1.Manager",
        ).await?;

        // Convert domains to D-Bus format (domain, routing_only)
        let domain_entries: Vec<(String, bool)> = domains.into_iter()
            .map(|d| (d, false))
            .collect();

        let _: () = proxy.call("SetLinkDomains", &(ifindex, domain_entries)).await?;

        Ok(())
    }

    pub async fn flush_caches(&self) -> Result<()> {
        info!("Flushing DNS caches");

        let Some(conn) = &self.connection else {
            warn!("DNS manager not initialized");
            return Ok(());
        };

        let proxy = zbus::Proxy::new(
            conn,
            "org.freedesktop.resolve1",
            "/org/freedesktop/resolve1",
            "org.freedesktop.resolve1.Manager",
        ).await?;

        proxy.call_method("FlushCaches", &()).await?;

        Ok(())
    }

    fn get_interface_index(&self, interface: &str) -> Result<i32> {
        // Parse interface index from /sys/class/net/<interface>/ifindex
        let path = format!("/sys/class/net/{}/ifindex", interface);
        let content = std::fs::read_to_string(&path)?;
        let ifindex = content.trim().parse::<i32>()?;
        Ok(ifindex)
    }
}
