use anyhow::{Context, Result};
use futures::{channel::mpsc::UnboundedReceiver, stream::StreamExt, stream::TryStreamExt};
use netlink_packet_core::{NetlinkMessage, NetlinkPayload};
use netlink_packet_route::{
    address::AddressAttribute,
    link::{LinkAttribute, LinkFlag},
    RouteNetlinkMessage,
};
use netlink_sys::{AsyncSocket, SocketAddr};
use rtnetlink::{
    constants::{
        RTMGRP_IPV4_IFADDR, RTMGRP_IPV4_ROUTE, RTMGRP_IPV6_IFADDR, RTMGRP_IPV6_ROUTE, RTMGRP_LINK,
    },
    new_connection, Handle,
};
use std::net::IpAddr;
use tracing::info;

/// Netlink monitor for kernel network events
pub struct NetlinkMonitor {
    handle: Handle,
    messages: UnboundedReceiver<(NetlinkMessage<RouteNetlinkMessage>, SocketAddr)>,
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
        let (mut connection, handle, messages) = new_connection()?;

        let groups = RTMGRP_LINK
            | RTMGRP_IPV4_IFADDR
            | RTMGRP_IPV6_IFADDR
            | RTMGRP_IPV4_ROUTE
            | RTMGRP_IPV6_ROUTE;
        connection
            .socket_mut()
            .socket_mut()
            .bind(&SocketAddr::new(0, groups))
            .context("failed to subscribe to netlink multicast groups")?;

        // Spawn the connection to run in the background
        tokio::spawn(connection);

        info!("Netlink monitor initialized");

        Ok(Self { handle, messages })
    }

    /// Start monitoring link and address changes
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!("Monitoring link, address, and route changes");
        Ok(())
    }

    pub async fn next_event(&mut self) -> Result<NetlinkEvent> {
        loop {
            let (message, _) = self
                .messages
                .next()
                .await
                .ok_or_else(|| anyhow::anyhow!("netlink event stream closed"))?;

            let NetlinkPayload::InnerMessage(message) = message.payload else {
                continue;
            };

            match message {
                RouteNetlinkMessage::NewLink(link) => {
                    let interface = link_interface_name(&link.attributes)
                        .or_else(|| self.get_interface_name(link.header.index).ok())
                        .unwrap_or_else(|| format!("ifindex-{}", link.header.index));
                    let is_up = link.header.flags.contains(&LinkFlag::Up);

                    return Ok(if is_up {
                        NetlinkEvent::LinkUp {
                            interface,
                            index: link.header.index,
                        }
                    } else {
                        NetlinkEvent::LinkDown {
                            interface,
                            index: link.header.index,
                        }
                    });
                }
                RouteNetlinkMessage::DelLink(link) => {
                    let interface = link_interface_name(&link.attributes)
                        .unwrap_or_else(|| format!("ifindex-{}", link.header.index));
                    return Ok(NetlinkEvent::LinkDown {
                        interface,
                        index: link.header.index,
                    });
                }
                RouteNetlinkMessage::NewAddress(address) => {
                    if let Some(ip) = address_ip(&address.attributes) {
                        let interface = self
                            .get_interface_name(address.header.index)
                            .unwrap_or_else(|_| format!("ifindex-{}", address.header.index));
                        return Ok(NetlinkEvent::NewAddress {
                            interface,
                            address: ip,
                        });
                    }
                }
                RouteNetlinkMessage::DelAddress(address) => {
                    if let Some(ip) = address_ip(&address.attributes) {
                        let interface = self
                            .get_interface_name(address.header.index)
                            .unwrap_or_else(|_| format!("ifindex-{}", address.header.index));
                        return Ok(NetlinkEvent::DelAddress {
                            interface,
                            address: ip,
                        });
                    }
                }
                RouteNetlinkMessage::NewRoute(_) | RouteNetlinkMessage::DelRoute(_) => {
                    return Ok(NetlinkEvent::RouteChanged);
                }
                _ => {}
            }
        }
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

fn link_interface_name(attributes: &[LinkAttribute]) -> Option<String> {
    attributes.iter().find_map(|attribute| match attribute {
        LinkAttribute::IfName(name) => Some(name.clone()),
        _ => None,
    })
}

fn address_ip(attributes: &[AddressAttribute]) -> Option<IpAddr> {
    attributes.iter().find_map(|attribute| match attribute {
        AddressAttribute::Local(address) | AddressAttribute::Address(address) => Some(*address),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4_and_ipv6_address_attributes() {
        let ipv4 = vec![AddressAttribute::Local("192.0.2.10".parse().unwrap())];
        assert_eq!(address_ip(&ipv4), Some("192.0.2.10".parse().unwrap()));

        let ipv6 = vec![AddressAttribute::Address("2001:db8::1".parse().unwrap())];
        assert_eq!(address_ip(&ipv6), Some("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn extracts_link_interface_name() {
        assert_eq!(
            link_interface_name(&[LinkAttribute::IfName("wlan0".into())]),
            Some("wlan0".into())
        );
    }
}
