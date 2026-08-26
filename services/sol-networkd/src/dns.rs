use anyhow::Result;
use tracing::info;

/// DNS integration with systemd-resolved
pub struct DnsManager {
}

impl DnsManager {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn set_dns_servers(&self, interface: &str, servers: Vec<std::net::IpAddr>) -> Result<()> {
        info!("Setting DNS servers for {}: {:?}", interface, servers);
        // TODO: Integrate with systemd-resolved via D-Bus
        // org.freedesktop.resolve1.Manager.SetLinkDNS

        Ok(())
    }

    pub async fn set_search_domains(&self, interface: &str, domains: Vec<String>) -> Result<()> {
        info!("Setting search domains for {}: {:?}", interface, domains);
        // TODO: org.freedesktop.resolve1.Manager.SetLinkDomains

        Ok(())
    }

    pub async fn flush_caches(&self) -> Result<()> {
        info!("Flushing DNS caches");
        // TODO: org.freedesktop.resolve1.Manager.FlushCaches

        Ok(())
    }
}
