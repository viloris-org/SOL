use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use tracing::{info, warn};

use crate::device::{Device, DeviceId};
use crate::profile::{Profile, ProfileId, ProfileStore};
use crate::netlink::NetlinkMonitor;

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

        let inner = NetworkManagerInner {
            devices: HashMap::new(),
            profiles,
            state: NetworkState::Disconnected,
            auto_connect_policy: AutoConnectPolicy::default(),
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

        Ok(manager)
    }

    async fn run_netlink_monitor(&self) -> Result<()> {
        let mut monitor = NetlinkMonitor::new().await?;

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

    async fn handle_netlink_event(&self, event: crate::netlink::NetlinkEvent) {
        info!("Handling netlink event: {:?}", event);
        // TODO: implement event handling
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
        let profile = inner.profiles.get(profile_id).await?
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
}
