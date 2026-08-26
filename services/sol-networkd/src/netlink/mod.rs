use anyhow::Result;
use rtnetlink::{new_connection, Handle};
use futures::stream::TryStreamExt;
use tracing::info;
use std::net::IpAddr;

/// Netlink monitor for kernel network events
pub struct NetlinkMonitor {
    handle: Handle,
}

#[derive(Debug, Clone)]
pub enum NetlinkEvent {
    LinkUp { interface: String, index: u32 },
    LinkDown { interface: String, index: u32 },
    NewAddress { interface: String, address: IpAddr },
    DelAddress { interface: String, address: IpAddr },
    RouteChanged,
}

impl NetlinkMonitor {
    pub async fn new() -> Result<Self> {
        let (connection, handle, _) = new_connection()?;

        // Spawn the connection to run in the background
        tokio::spawn(connection);

        info!("Netlink monitor initialized");

        Ok(Self { handle })
    }

    /// Start monitoring link and address changes
    pub async fn start_monitoring(&mut self) -> Result<()> {
        // Real netlink monitoring would use rtnetlink's link/address streams
        // For now, this is a placeholder
        Ok(())
    }

    pub async fn next_event(&mut self) -> Result<NetlinkEvent> {
        // TODO: Implement proper netlink event monitoring
        // This requires upgrading to rtnetlink 0.23.0+ for the newer API
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        Ok(NetlinkEvent::RouteChanged)
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// List all network interfaces
    pub async fn list_interfaces(&self) -> Result<Vec<(u32, String)>> {
        let mut interfaces = Vec::new();
        let mut links = self.handle.link().get().execute();

        while let Some(link) = links.try_next().await? {
            let index = link.header.index;

            // Get name from /sys/class/net by index
            if let Ok(name) = self.get_interface_name(index) {
                interfaces.push((index, name));
            }
        }

        Ok(interfaces)
    }

    fn get_interface_name(&self, index: u32) -> Result<String> {
        // Read from /sys/class/net to get interface name by index
        let path = "/sys/class/net";
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let ifindex_path = entry.path().join("ifindex");
            if let Ok(content) = std::fs::read_to_string(&ifindex_path) {
                if let Ok(idx) = content.trim().parse::<u32>() {
                    if idx == index {
                        return Ok(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }
        anyhow::bail!("Interface with index {} not found", index)
    }
}
