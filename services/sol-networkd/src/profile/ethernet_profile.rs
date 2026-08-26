use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EthernetProfile {
    pub interface: Option<String>,  // Specific interface, or None for any
    pub ip_config: IpConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IpConfig {
    Dhcp,
    Static {
        address: Ipv4Addr,
        netmask: Ipv4Addr,
        gateway: Option<Ipv4Addr>,
        dns: Vec<Ipv4Addr>,
    },
}

impl EthernetProfile {
    pub fn new_dhcp() -> Self {
        Self {
            interface: None,
            ip_config: IpConfig::Dhcp,
        }
    }

    pub fn new_static(address: Ipv4Addr, netmask: Ipv4Addr, gateway: Ipv4Addr) -> Self {
        Self {
            interface: None,
            ip_config: IpConfig::Static {
                address,
                netmask,
                gateway: Some(gateway),
                dns: vec![],
            },
        }
    }
}
