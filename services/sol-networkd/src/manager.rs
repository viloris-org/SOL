use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use tracing::{info, warn};

use crate::device::{Device, DeviceId};
use crate::profile::{Profile, ProfileId, ProfileStore};
use crate::netlink::NetlinkMonitor;
use crate::captive_portal::{CaptivePortalDetector, ConnectivityState as PortalState};
use crate::dns::DnsManager;

/// Network manager core - handles connection policy and coordination
#[derive(Clone)]
pub struct NetworkManager {
    inner: Arc<RwLock<NetworkManagerInner>>,
}

struct NetworkManagerInner {
    devices: HashMap<DeviceId, Device>,
    profiles: ProfileStore,
    state: NetworkState,
    auto_connect_policy: AutoConnectPolicy,
    dns_manager: DnsManager,
    captive_portal: CaptivePortalDetector,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkState {
    Disconnected,
    Connecting,
    Connected,
    Limited,  // Connected but no internet
}

#[derive(Debug, Clone)]
pub struct AutoConnectPolicy {
    pub prefer_ethernet: bool,
    pub avoid_metered: bool,
    pub location_based: bool,
    pub time_based: bool,
}

impl Default for AutoConnectPolicy {
    fn default() -> Self {
        Self {
            prefer_ethernet: true,
            avoid_metered: false,
            location_based: false,
            time_based: false,
        }
    }
}

impl NetworkManager {
    pub async fn new() -> Result<Self> {
        info!("Initializing network manager");

        let profiles = ProfileStore::new().await?;
        let mut dns_manager = DnsManager::new();
        let _ = dns_manager.init().await; // Don't fail if systemd-resolved is unavailable

        let inner = NetworkManagerInner {
            devices: HashMap::new(),
            profiles,
            state: NetworkState::Disconnected,
            auto_connect_policy: AutoConnectPolicy::default(),
            dns_manager,
            captive_portal: CaptivePortalDetector::new(),
        };

        let manager = Self {
            inner: Arc::new(RwLock::new(inner)),
        };

        // Start netlink monitor
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            if let Err(e) = manager_clone.run_netlink_monitor().await {
                warn!("Netlink monitor failed: {}", e);
            }
        });

        // Start connectivity monitor
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            if let Err(e) = manager_clone.run_connectivity_monitor().await {
                warn!("Connectivity monitor failed: {}", e);
            }
        });

        Ok(manager)
    }

    async fn run_netlink_monitor(&self) -> Result<()> {
        let mut monitor = NetlinkMonitor::new().await?;
        monitor.start_monitoring().await?;

        // Initial device discovery
        let interfaces = monitor.list_interfaces().await?;
        {
            let mut inner = self.inner.write().await;
            for (index, name) in interfaces {
                let device_type = if name.starts_with("wlan") || name.starts_with("wlp") {
                    crate::device::DeviceType::WiFi
                } else if name.starts_with("eth") || name.starts_with("enp") {
                    crate::device::DeviceType::Ethernet
                } else {
                    continue; // Skip unknown types
                };

                let device = Device::new(
                    DeviceId(format!("{}:{}", device_type_str(&device_type), index)),
                    device_type,
                    name,
                );
                inner.devices.insert(device.id.clone(), device);
            }
        }

        loop {
            match monitor.next_event().await {
                Ok(event) => {
                    self.handle_netlink_event(event).await;
                }
                Err(e) => {
                    warn!("Netlink event error: {}", e);
                }
            }
        }
    }

    async fn run_connectivity_monitor(&self) -> Result<()> {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let inner = self.inner.read().await;
            if inner.state == NetworkState::Connected {
                match inner.captive_portal.check_connectivity().await {
                    Ok(PortalState::Full) => {
                        info!("Connectivity check: Full internet");
                    }
                    Ok(PortalState::Portal) => {
                        warn!("Captive portal detected");
                        // TODO: Emit signal for UI notification
                    }
                    Ok(PortalState::Limited) => {
                        warn!("Limited connectivity");
                    }
                    Ok(PortalState::None) => {
                        warn!("No connectivity despite being connected");
                    }
                    Err(e) => {
                        warn!("Connectivity check failed: {}", e);
                    }
                }
            }
        }
    }

    async fn handle_netlink_event(&self, event: crate::netlink::NetlinkEvent) {
        info!("Handling netlink event: {:?}", event);

        match event {
            crate::netlink::NetlinkEvent::LinkUp { interface, index } => {
                info!("Link up: {} ({})", interface, index);
                // TODO: Trigger auto-connect if appropriate
            }
            crate::netlink::NetlinkEvent::LinkDown { interface, index } => {
                info!("Link down: {} ({})", interface, index);
                let mut inner = self.inner.write().await;
                if inner.state == NetworkState::Connected {
                    inner.state = NetworkState::Disconnected;
                }
            }
            crate::netlink::NetlinkEvent::NewAddress { interface, address } => {
                info!("New address on {}: {}", interface, address);

                // Update DNS if we have DNS servers from DHCP
                let _inner = self.inner.read().await;
                // TODO: Get DNS servers from active connection
            }
            _ => {}
        }
    }

    pub async fn list_devices(&self) -> Vec<DeviceId> {
        let inner = self.inner.read().await;
        inner.devices.keys().cloned().collect()
    }

    pub async fn get_device(&self, id: &DeviceId) -> Option<Device> {
        let inner = self.inner.read().await;
        inner.devices.get(id).cloned()
    }

    pub async fn connect_to_profile(&self, profile_id: &ProfileId) -> Result<()> {
        info!("Connecting to profile: {}", profile_id);

        let mut inner = self.inner.write().await;
        let _profile = inner.profiles.get(profile_id).await?
            .ok_or_else(|| anyhow::anyhow!("Profile not found"))?;

        // TODO: Find appropriate device and initiate connection
        inner.state = NetworkState::Connecting;

        Ok(())
    }

    pub async fn disconnect_profile(&self, profile_id: &ProfileId) -> Result<()> {
        info!("Disconnecting profile: {}", profile_id);

        let mut inner = self.inner.write().await;

        // TODO: implement disconnection logic
        inner.state = NetworkState::Disconnected;

        Ok(())
    }

    pub async fn get_state(&self) -> NetworkState {
        let inner = self.inner.read().await;
        inner.state.clone()
    }

    pub async fn create_profile(&self, profile: Profile) -> Result<ProfileId> {
        let mut inner = self.inner.write().await;
        inner.profiles.create(profile).await
    }

    pub async fn delete_profile(&self, profile_id: &ProfileId) -> Result<()> {
        let mut inner = self.inner.write().await;
        inner.profiles.delete(profile_id).await
    }

    pub async fn list_profiles(&self) -> Vec<ProfileId> {
        let inner = self.inner.read().await;
        inner.profiles.list().await
    }

    pub async fn check_connectivity(&self) -> Result<PortalState> {
        let inner = self.inner.read().await;
        inner.captive_portal.comprehensive_check().await
    }
}

fn device_type_str(dt: &crate::device::DeviceType) -> &'static str {
    match dt {
        crate::device::DeviceType::WiFi => "wifi",
        crate::device::DeviceType::Ethernet => "eth",
        crate::device::DeviceType::Vpn => "vpn",
    }
}
