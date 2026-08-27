use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Network manager state file (similar to systemd-networkd's /run/systemd/netif/state)
/// Stores runtime state about network configuration for persistence across restarts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateFile {
    pub operational_state: OperationalState,
    pub carrier_state: CarrierState,
    pub address_state: AddressState,
    pub ipv4_address_state: AddressState,
    pub ipv6_address_state: AddressState,
    pub online_state: OnlineState,
    pub links: Vec<LinkState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationalState {
    Off,
    NoCarrier,
    Dormant,
    DegradedCarrier,
    Carrier,
    Degraded,
    Routable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarrierState {
    Off,
    NoCarrier,
    Dormant,
    DegradedCarrier,
    Carrier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressState {
    Off,
    Degraded,
    Routable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnlineState {
    Offline,
    Partial,
    Online,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkState {
    pub ifindex: u32,
    pub ifname: String,
    pub operational_state: OperationalState,
    pub carrier_state: CarrierState,
    pub address_state: AddressState,
    pub ipv4_address_state: AddressState,
    pub ipv6_address_state: AddressState,
    pub online_state: OnlineState,
    pub addresses: Vec<IpAddr>,
    pub routes: Vec<RouteInfo>,
    pub dns_servers: Vec<IpAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInfo {
    pub destination: Option<IpAddr>,
    pub gateway: Option<IpAddr>,
    pub metric: u32,
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            operational_state: OperationalState::Off,
            carrier_state: CarrierState::Off,
            address_state: AddressState::Off,
            ipv4_address_state: AddressState::Off,
            ipv6_address_state: AddressState::Off,
            online_state: OnlineState::Offline,
            links: Vec::new(),
        }
    }
}

impl StateFile {
    const STATE_FILE_PATH: &'static str = "/run/sol-networkd/state";

    pub async fn load() -> Result<Self> {
        let path = Path::new(Self::STATE_FILE_PATH);

        if !path.exists() {
            debug!("State file does not exist, using default state");
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .await
            .context("failed to read state file")?;

        let state: Self = serde_json::from_str(&contents).context("failed to parse state file")?;

        debug!("Loaded state file with {} links", state.links.len());
        Ok(state)
    }

    pub async fn save(&self) -> Result<()> {
        let path = Path::new(Self::STATE_FILE_PATH);

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("failed to create state directory")?;
        }

        let contents = serde_json::to_string_pretty(self).context("failed to serialize state")?;

        // Atomic write using temporary file
        let tmp_path = PathBuf::from(format!("{}.tmp", Self::STATE_FILE_PATH));
        fs::write(&tmp_path, contents)
            .await
            .context("failed to write temporary state file")?;

        fs::rename(&tmp_path, path)
            .await
            .context("failed to rename temporary state file")?;

        debug!("Saved state file");
        Ok(())
    }

    pub fn update_link(&mut self, link: LinkState) {
        if let Some(existing) = self.links.iter_mut().find(|l| l.ifindex == link.ifindex) {
            *existing = link;
        } else {
            self.links.push(link);
        }
        self.update_global_state();
    }

    pub fn remove_link(&mut self, ifindex: u32) {
        self.links.retain(|l| l.ifindex != ifindex);
        self.update_global_state();
    }

    /// Update global operational state based on individual links
    fn update_global_state(&mut self) {
        if self.links.is_empty() {
            self.operational_state = OperationalState::Off;
            self.carrier_state = CarrierState::Off;
            self.address_state = AddressState::Off;
            self.online_state = OnlineState::Offline;
            return;
        }

        // Aggregate link states to determine global state
        let has_routable = self
            .links
            .iter()
            .any(|l| matches!(l.operational_state, OperationalState::Routable));

        let has_carrier = self.links.iter().any(|l| {
            matches!(
                l.carrier_state,
                CarrierState::Carrier | CarrierState::DegradedCarrier
            )
        });

        let has_addresses = self.links.iter().any(|l| !l.addresses.is_empty());

        self.operational_state = if has_routable {
            OperationalState::Routable
        } else if has_carrier {
            OperationalState::Carrier
        } else {
            OperationalState::NoCarrier
        };

        self.carrier_state = if has_carrier {
            CarrierState::Carrier
        } else {
            CarrierState::NoCarrier
        };

        self.address_state = if has_addresses {
            AddressState::Routable
        } else {
            AddressState::Off
        };

        // Determine online state based on connectivity
        let online_links = self
            .links
            .iter()
            .filter(|l| matches!(l.online_state, OnlineState::Online))
            .count();

        self.online_state = if online_links == self.links.len() {
            OnlineState::Online
        } else if online_links > 0 {
            OnlineState::Partial
        } else {
            OnlineState::Offline
        };
    }

    pub fn get_link(&self, ifindex: u32) -> Option<&LinkState> {
        self.links.iter().find(|l| l.ifindex == ifindex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_offline() {
        let state = StateFile::default();
        assert_eq!(state.operational_state, OperationalState::Off);
        assert_eq!(state.online_state, OnlineState::Offline);
    }

    #[test]
    fn updates_global_state_when_link_added() {
        let mut state = StateFile::default();

        let link = LinkState {
            ifindex: 2,
            ifname: "eth0".to_string(),
            operational_state: OperationalState::Routable,
            carrier_state: CarrierState::Carrier,
            address_state: AddressState::Routable,
            ipv4_address_state: AddressState::Routable,
            ipv6_address_state: AddressState::Off,
            online_state: OnlineState::Online,
            addresses: vec!["192.168.1.100".parse().unwrap()],
            routes: vec![],
            dns_servers: vec![],
        };

        state.update_link(link);

        assert_eq!(state.operational_state, OperationalState::Routable);
        assert_eq!(state.carrier_state, CarrierState::Carrier);
        assert_eq!(state.address_state, AddressState::Routable);
        assert_eq!(state.online_state, OnlineState::Online);
    }

    #[test]
    fn removes_link_and_updates_state() {
        let mut state = StateFile::default();

        let link = LinkState {
            ifindex: 2,
            ifname: "eth0".to_string(),
            operational_state: OperationalState::Routable,
            carrier_state: CarrierState::Carrier,
            address_state: AddressState::Routable,
            ipv4_address_state: AddressState::Routable,
            ipv6_address_state: AddressState::Off,
            online_state: OnlineState::Online,
            addresses: vec!["192.168.1.100".parse().unwrap()],
            routes: vec![],
            dns_servers: vec![],
        };

        state.update_link(link);
        assert_eq!(state.links.len(), 1);

        state.remove_link(2);
        assert_eq!(state.links.len(), 0);
        assert_eq!(state.operational_state, OperationalState::Off);
    }
}
