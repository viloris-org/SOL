use anyhow::Result;
use tracing::info;
use tokio::net::UdpSocket;
use std::time::{Duration, Instant};
use dhcproto::{v4, Encodable, Decodable};
use std::net::Ipv4Addr;

/// DHCP client for automatic IP configuration
pub struct DhcpClient {
    interface: String,
    mac_address: [u8; 6],
}

impl DhcpClient {
    pub fn new(interface: String, mac_address: [u8; 6]) -> Self {
        Self { interface, mac_address }
    }

    pub async fn acquire_lease(&self) -> Result<DhcpLease> {
        info!("Acquiring DHCP lease on {}", self.interface);

        // Create UDP socket bound to DHCP client port
        let socket = UdpSocket::bind("0.0.0.0:68").await?;
        socket.set_broadcast(true)?;

        // Generate transaction ID
        let xid = rand::random::<u32>();

        // Send DHCPDISCOVER
        let discover = self.build_discover(xid)?;
        self.send_dhcp_packet(&socket, &discover).await?;

        // Wait for DHCPOFFER
        let offer = self.receive_dhcp_response(&socket, xid, Duration::from_secs(10)).await?;

        // Send DHCPREQUEST
        let request = self.build_request(xid, &offer)?;
        self.send_dhcp_packet(&socket, &request).await?;

        // Wait for DHCPACK
        let ack = self.receive_dhcp_response(&socket, xid, Duration::from_secs(10)).await?;

        self.parse_dhcp_lease(&ack)
    }

    pub async fn renew_lease(&self, lease: &DhcpLease) -> Result<DhcpLease> {
        info!("Renewing DHCP lease on {}", self.interface);

        let socket = UdpSocket::bind("0.0.0.0:68").await?;
        let xid = rand::random::<u32>();

        // Send DHCPREQUEST directly to server
        let request = self.build_renewal_request(xid, lease)?;
        self.send_dhcp_packet(&socket, &request).await?;

        let ack = self.receive_dhcp_response(&socket, xid, Duration::from_secs(5)).await?;
        self.parse_dhcp_lease(&ack)
    }

    pub async fn release_lease(&self, lease: &DhcpLease) -> Result<()> {
        info!("Releasing DHCP lease on {}", self.interface);

        let socket = UdpSocket::bind("0.0.0.0:68").await?;
        let xid = rand::random::<u32>();

        let release = self.build_release(xid, lease)?;
        self.send_dhcp_packet(&socket, &release).await?;

        Ok(())
    }

    fn build_discover(&self, xid: u32) -> Result<v4::Message> {
        let mut msg = v4::Message::default();
        msg.set_flags(v4::Flags::default().set_broadcast());
        msg.set_xid(xid);
        msg.set_chaddr(&self.mac_address);

        msg.opts_mut().insert(v4::DhcpOption::MessageType(v4::MessageType::Discover));
        msg.opts_mut().insert(v4::DhcpOption::ParameterRequestList(vec![
            v4::OptionCode::SubnetMask,
            v4::OptionCode::Router,
            v4::OptionCode::DomainNameServer,
            v4::OptionCode::DomainName,
        ]));

        Ok(msg)
    }

    fn build_request(&self, xid: u32, offer: &v4::Message) -> Result<v4::Message> {
        let mut msg = v4::Message::default();
        msg.set_flags(v4::Flags::default().set_broadcast());
        msg.set_xid(xid);
        msg.set_chaddr(&self.mac_address);

        msg.opts_mut().insert(v4::DhcpOption::MessageType(v4::MessageType::Request));
        msg.opts_mut().insert(v4::DhcpOption::RequestedIpAddress(offer.yiaddr()));

        if let Some(v4::DhcpOption::ServerIdentifier(server_id)) = offer.opts().get(v4::OptionCode::ServerIdentifier) {
            msg.opts_mut().insert(v4::DhcpOption::ServerIdentifier(*server_id));
        }

        Ok(msg)
    }

    fn build_renewal_request(&self, xid: u32, lease: &DhcpLease) -> Result<v4::Message> {
        let mut msg = v4::Message::default();
        msg.set_xid(xid);
        msg.set_chaddr(&self.mac_address);
        msg.set_ciaddr(lease.ip_address);

        msg.opts_mut().insert(v4::DhcpOption::MessageType(v4::MessageType::Request));

        Ok(msg)
    }

    fn build_release(&self, xid: u32, lease: &DhcpLease) -> Result<v4::Message> {
        let mut msg = v4::Message::default();
        msg.set_xid(xid);
        msg.set_chaddr(&self.mac_address);
        msg.set_ciaddr(lease.ip_address);

        msg.opts_mut().insert(v4::DhcpOption::MessageType(v4::MessageType::Release));

        Ok(msg)
    }

    async fn send_dhcp_packet(&self, socket: &UdpSocket, msg: &v4::Message) -> Result<()> {
        let mut buf = Vec::new();
        let mut encoder = dhcproto::Encoder::new(&mut buf);
        msg.encode(&mut encoder)?;

        socket.send_to(&buf, "255.255.255.255:67").await?;
        Ok(())
    }

    async fn receive_dhcp_response(&self, socket: &UdpSocket, expected_xid: u32, timeout: Duration) -> Result<v4::Message> {
        let start = Instant::now();
        let mut buf = vec![0u8; 1500];

        loop {
            if start.elapsed() > timeout {
                return Err(anyhow::anyhow!("DHCP response timeout"));
            }

            tokio::time::timeout(Duration::from_secs(1), socket.recv_from(&mut buf))
                .await
                .ok()
                .and_then(|r| r.ok())
                .and_then(|(len, _)| {
                    let mut decoder = dhcproto::Decoder::new(&buf[..len]);
                    v4::Message::decode(&mut decoder).ok()
                })
                .and_then(|msg| {
                    if msg.xid() == expected_xid {
                        Some(msg)
                    } else {
                        None
                    }
                })
                .map(Ok::<v4::Message, anyhow::Error>);
        }
    }

    fn parse_dhcp_lease(&self, ack: &v4::Message) -> Result<DhcpLease> {
        let ip_address = ack.yiaddr();

        let subnet_mask = ack.opts()
            .get(v4::OptionCode::SubnetMask)
            .and_then(|opt| {
                if let v4::DhcpOption::SubnetMask(mask) = opt {
                    Some(*mask)
                } else {
                    None
                }
            })
            .unwrap_or(Ipv4Addr::new(255, 255, 255, 0));

        let router = ack.opts()
            .get(v4::OptionCode::Router)
            .and_then(|opt| {
                if let v4::DhcpOption::Router(routers) = opt {
                    routers.first().copied()
                } else {
                    None
                }
            });

        let dns_servers = ack.opts()
            .get(v4::OptionCode::DomainNameServer)
            .and_then(|opt| {
                if let v4::DhcpOption::DomainNameServer(servers) = opt {
                    Some(servers.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let lease_time = ack.opts()
            .get(v4::OptionCode::AddressLeaseTime)
            .and_then(|opt| {
                if let v4::DhcpOption::AddressLeaseTime(time) = opt {
                    Some(*time)
                } else {
                    None
                }
            })
            .unwrap_or(86400); // Default 24 hours

        let renewal_time = ack.opts()
            .get(v4::OptionCode::Renewal)
            .and_then(|opt| {
                if let v4::DhcpOption::Renewal(time) = opt {
                    Some(*time)
                } else {
                    None
                }
            })
            .unwrap_or(lease_time / 2);

        Ok(DhcpLease {
            ip_address,
            subnet_mask,
            router,
            dns_servers,
            lease_time,
            renewal_time,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DhcpLease {
    pub ip_address: std::net::Ipv4Addr,
    pub subnet_mask: std::net::Ipv4Addr,
    pub router: Option<std::net::Ipv4Addr>,
    pub dns_servers: Vec<std::net::Ipv4Addr>,
    pub lease_time: u32,  // seconds
    pub renewal_time: u32,  // seconds
}
