use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::captive_portal::{CaptivePortalDetector, ConnectivityState as PortalState};
use crate::device::{Device, DeviceId, DeviceState, DeviceType};
use crate::dns::DnsManager;
use crate::netlink::NetlinkMonitor;
use crate::nts::{NtsClient, DEFAULT_NTS_SERVERS};
use crate::profile::{Profile, ProfileId, ProfileStore};
use crate::queue::RequestQueue;
use crate::state_file::{OperationalState, StateFile};

/// Network manager core - handles connection policy and coordination
#[derive(Clone)]
pub struct NetworkManager {
    inner: Arc<RwLock<NetworkManagerInner>>,
}

struct NetworkManagerInner {
    devices: HashMap<DeviceId, Device>,
    devices_by_ifindex: HashMap<u32, DeviceId>,
    profiles: ProfileStore,
    state: NetworkState,
    connectivity: PortalState,
    active_profile: Option<ProfileId>,
    auto_connect_policy: AutoConnectPolicy,
    dns_manager: DnsManager,
    captive_portal: CaptivePortalDetector,
    nts_client: NtsClient,
    request_queue: RequestQueue,
    state_file: StateFile,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkState {
    Disconnected,
    Connecting,
    Connected,
    Limited, // Connected but no internet
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

        // Load state file
        let state_file = StateFile::load().await.unwrap_or_default();
        info!(
            "Loaded network state: operational={:?}, online={:?}",
            state_file.operational_state, state_file.online_state
        );

        let inner = NetworkManagerInner {
            devices: HashMap::new(),
            devices_by_ifindex: HashMap::new(),
            profiles,
            state: NetworkState::Disconnected,
            connectivity: PortalState::None,
            active_profile: None,
            auto_connect_policy: AutoConnectPolicy::default(),
            dns_manager,
            captive_portal: CaptivePortalDetector::new(),
            nts_client: NtsClient::new(DEFAULT_NTS_SERVERS[0].to_string()),
            request_queue: RequestQueue::new(),
            state_file,
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
                let Some(device_type) = classify_interface(&name) else {
                    continue; // Skip unknown types
                };

                let device = Device::new(
                    DeviceId(format!("{}:{}", device_type_str(&device_type), index)),
                    device_type,
                    name.clone(),
                    index,
                );
                inner.devices.insert(device.id.clone(), device);
            }
        }

        loop {
            let event = monitor
                .next_event()
                .await
                .context("Netlink event stream failed")?;
            self.handle_netlink_event(event).await;
        }
    }

    async fn run_connectivity_monitor(&self) -> Result<()> {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let detector = {
                let inner = self.inner.read().await;
                if !matches!(inner.state, NetworkState::Connected | NetworkState::Limited) {
                    continue;
                }
                inner.captive_portal.clone()
            };

            match detector.check_connectivity().await {
                Ok(PortalState::Full) => {
                    info!("Connectivity check: Full internet");
                    self.update_connectivity(PortalState::Full).await;
                }
                Ok(PortalState::Portal) => {
                    warn!("Captive portal detected");
                    self.update_connectivity(PortalState::Portal).await;
                }
                Ok(PortalState::Limited) => {
                    warn!("Limited connectivity");
                    self.update_connectivity(PortalState::Limited).await;
                }
                Ok(PortalState::None) => {
                    warn!("No connectivity despite being connected");
                    self.update_connectivity(PortalState::None).await;
                }
                Err(e) => {
                    warn!("Connectivity check failed: {}", e);
                }
            }
        }
    }

    async fn update_connectivity(&self, connectivity: PortalState) {
        let mut inner = self.inner.write().await;
        inner.connectivity = connectivity;
        inner.state = match connectivity {
            PortalState::Full => NetworkState::Connected,
            PortalState::Portal | PortalState::Limited | PortalState::None => NetworkState::Limited,
        };
    }

    async fn handle_netlink_event(&self, event: crate::netlink::NetlinkEvent) {
        info!("Handling netlink event: {:?}", event);

        match event {
            crate::netlink::NetlinkEvent::LinkUp { interface, index } => {
                info!("Link up: {} ({})", interface, index);
                let Some(device_type) = classify_interface(&interface) else {
                    return;
                };
                {
                    let mut inner = self.inner.write().await;
                    let id = DeviceId(format!("{}:{}", device_type_str(&device_type), index));

                    let device = inner.devices.entry(id.clone()).or_insert_with(|| {
                        Device::new(id.clone(), device_type.clone(), interface.clone(), index)
                    });

                    device.carrier = true;
                    device.state = DeviceState::Disconnected;
                    inner.devices_by_ifindex.insert(index, id);
                }

                if let Some(profile_id) = self
                    .select_auto_connect_profile(&device_type, &interface)
                    .await
                {
                    let manager = self.clone();
                    tokio::spawn(async move {
                        if let Err(error) = manager.connect_to_profile(&profile_id).await {
                            warn!("Auto-connect failed for profile {profile_id}: {error}");
                        }
                    });
                }
            }
            crate::netlink::NetlinkEvent::LinkDown { interface, index } => {
                info!("Link down: {} ({})", interface, index);
                let mut inner = self.inner.write().await;
                let active_profile = if let Some(profile_id) = &inner.active_profile {
                    inner.profiles.get(profile_id).await.ok().flatten()
                } else {
                    None
                };
                let affected_type = inner
                    .devices
                    .values_mut()
                    .find(|device| device.interface == interface)
                    .map(|device| {
                        device.carrier = false;
                        device.state = DeviceState::Unavailable;
                        device.device_type.clone()
                    });
                let active_was_affected = active_profile
                    .as_ref()
                    .zip(affected_type.as_ref())
                    .is_some_and(|(profile, device_type)| {
                        profile_matches_device(profile, device_type, &interface)
                    });
                if active_was_affected {
                    inner.state = NetworkState::Disconnected;
                    inner.connectivity = PortalState::None;
                    inner.active_profile = None;
                }
            }
            crate::netlink::NetlinkEvent::NewAddress {
                interface,
                address,
                prefix_len,
            } => {
                info!("New address on {}: {}/{}", interface, address, prefix_len);
                if let Some(device) = self
                    .inner
                    .write()
                    .await
                    .devices
                    .values_mut()
                    .find(|device| device.interface == interface)
                {
                    if !device.ip_addresses.contains(&address) {
                        device.ip_addresses.push(address);
                    }
                    if device.state == DeviceState::IpConfig {
                        device.state = DeviceState::Active;
                    }
                }
            }
            crate::netlink::NetlinkEvent::DelAddress { interface, address } => {
                info!("Address removed from {}: {}", interface, address);
                if let Some(device) = self
                    .inner
                    .write()
                    .await
                    .devices
                    .values_mut()
                    .find(|device| device.interface == interface)
                {
                    device.ip_addresses.retain(|a| a != &address);
                }
            }
            crate::netlink::NetlinkEvent::NewRoute {
                interface,
                destination,
                gateway,
            } => {
                info!(
                    "New route: interface={:?}, dest={:?}, gateway={:?}",
                    interface, destination, gateway
                );
            }
            crate::netlink::NetlinkEvent::DelRoute {
                interface,
                destination,
            } => {
                info!(
                    "Route removed: interface={:?}, dest={:?}",
                    interface, destination
                );
            }
            crate::netlink::NetlinkEvent::NewNeighbor { interface, address } => {
                info!("New neighbor on {}: {}", interface, address);
            }
            crate::netlink::NetlinkEvent::DelNeighbor { interface, address } => {
                info!("Neighbor removed from {}: {}", interface, address);
            }
            crate::netlink::NetlinkEvent::NewRule { priority } => {
                info!("New routing rule with priority {}", priority);
            }
            crate::netlink::NetlinkEvent::DelRule { priority } => {
                info!("Routing rule removed with priority {}", priority);
            }
            crate::netlink::NetlinkEvent::LinkChanged {
                interface,
                index,
                flags,
            } => {
                info!("Link changed: {} ({}) flags={}", interface, index, flags);
            }
        }
    }

    async fn select_auto_connect_profile(
        &self,
        device_type: &DeviceType,
        interface: &str,
    ) -> Option<ProfileId> {
        let inner = self.inner.read().await;
        if inner.state != NetworkState::Disconnected {
            return None;
        }

        let mut candidates = Vec::new();
        for id in inner.profiles.list().await {
            let Some(profile) = inner.profiles.get(&id).await.ok().flatten() else {
                continue;
            };
            if !profile.auto_connect
                || (inner.auto_connect_policy.avoid_metered && profile.metered)
                || !profile_matches_device(&profile, device_type, interface)
            {
                continue;
            }

            let score = match &profile.profile_type {
                crate::profile::ProfileType::Ethernet(_) => {
                    u64::from(inner.auto_connect_policy.prefer_ethernet) * 10_000
                }
                crate::profile::ProfileType::WiFi(wifi) => u64::from(wifi.priority),
                crate::profile::ProfileType::Vpn(_) => 0,
            };
            candidates.push((score, id));
        }

        candidates.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1 .0.cmp(&right.1 .0))
        });
        candidates.into_iter().next().map(|(_, id)| id)
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
        if inner.state == NetworkState::Connecting {
            anyhow::bail!("Another connection attempt is already in progress");
        }
        if let Some(active_profile) = &inner.active_profile {
            if active_profile == profile_id {
                return Ok(());
            }
            anyhow::bail!("Profile {active_profile} is already active");
        }
        let profile = inner
            .profiles
            .get(profile_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Profile not found"))?;

        inner.state = NetworkState::Connecting;
        inner.connectivity = PortalState::None;
        drop(inner); // Release lock during connection

        // Dispatch based on profile type
        let connect_result = match &profile.profile_type {
            crate::profile::ProfileType::WiFi(wifi_profile) => {
                self.connect_wifi(profile_id, wifi_profile).await
            }
            crate::profile::ProfileType::Ethernet(eth_profile) => {
                self.connect_ethernet(profile_id, eth_profile).await
            }
            crate::profile::ProfileType::Vpn(vpn_profile) => {
                self.connect_vpn(profile_id, vpn_profile).await
            }
        };

        if let Err(error) = connect_result {
            let mut inner = self.inner.write().await;
            inner.state = NetworkState::Disconnected;
            inner.connectivity = PortalState::None;
            inner.active_profile = None;
            return Err(error).with_context(|| format!("failed to connect profile {profile_id}"));
        }

        let mut inner = self.inner.write().await;
        inner.state = NetworkState::Connected;
        inner.active_profile = Some(profile_id.clone());

        // Sync time after successful connection
        let manager_clone = self.clone();
        tokio::spawn(async move {
            if let Err(e) = manager_clone.sync_time_after_connect().await {
                warn!("Failed to sync time: {}", e);
            }
        });

        let manager_clone = self.clone();
        tokio::spawn(async move {
            if let Err(error) = manager_clone.check_connectivity().await {
                warn!("Initial connectivity check failed: {error}");
            }
        });

        Ok(())
    }

    async fn sync_time_after_connect(&self) -> Result<()> {
        // Wait a bit for network to stabilize
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let inner = self.inner.read().await;
        match inner.nts_client.sync_time().await {
            Ok(time_info) => {
                info!(
                    "Time synchronized: {} from {}",
                    time_info.current_time, time_info.server
                );
                Ok(())
            }
            Err(e) => {
                warn!("Time sync failed: {}", e);
                Err(e)
            }
        }
    }

    async fn connect_wifi(
        &self,
        _profile_id: &ProfileId,
        wifi_profile: &crate::profile::wifi_profile::WiFiProfile,
    ) -> Result<()> {
        // Find a WiFi device
        let inner = self.inner.read().await;
        let wifi_device = inner
            .devices
            .values()
            .find(|d| d.device_type == crate::device::DeviceType::WiFi)
            .ok_or_else(|| anyhow::anyhow!("No WiFi device found"))?
            .clone();
        drop(inner);

        // Create WiFi device and connect
        let wifi = crate::device::wifi::WiFiDevice::new(wifi_device).await?;

        // Decrypt passphrase if present
        let passphrase = if let Some(encrypted) = &wifi_profile.passphrase_encrypted {
            let inner = self.inner.read().await;
            let decrypted = inner.profiles.decrypt_passphrase(encrypted).await?;
            Some(String::from_utf8(decrypted)?)
        } else {
            None
        };

        wifi.connect(&wifi_profile.ssid, passphrase.as_deref())
            .await?;

        // Wait for connection to establish
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        Ok(())
    }

    async fn connect_ethernet(
        &self,
        _profile_id: &ProfileId,
        eth_profile: &crate::profile::ethernet_profile::EthernetProfile,
    ) -> Result<()> {
        // Find an Ethernet device (or specific one if specified)
        let inner = self.inner.read().await;
        let eth_device = if let Some(iface) = &eth_profile.interface {
            inner
                .devices
                .values()
                .find(|d| {
                    d.device_type == crate::device::DeviceType::Ethernet && &d.interface == iface
                })
                .cloned()
        } else {
            inner
                .devices
                .values()
                .find(|d| d.device_type == crate::device::DeviceType::Ethernet)
                .cloned()
        }
        .ok_or_else(|| anyhow::anyhow!("No Ethernet device found"))?;
        let dns_manager = inner.dns_manager.clone();
        drop(inner);

        let ethernet =
            crate::device::ethernet::EthernetDevice::with_dns_manager(eth_device, dns_manager);
        ethernet.connect(&eth_profile.ip_config).await?;

        Ok(())
    }

    async fn connect_vpn(
        &self,
        _profile_id: &ProfileId,
        _vpn_profile: &crate::profile::vpn_profile::VpnProfile,
    ) -> Result<()> {
        anyhow::bail!("VPN profile activation is not implemented")
    }

    pub async fn disconnect_profile(&self, profile_id: &ProfileId) -> Result<()> {
        info!("Disconnecting profile: {}", profile_id);

        let (profile, devices, is_active) = {
            let inner = self.inner.read().await;
            let profile = inner
                .profiles
                .get(profile_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Profile not found"))?;
            (
                profile,
                inner.devices.values().cloned().collect::<Vec<_>>(),
                inner.active_profile.as_ref() == Some(profile_id),
            )
        };

        if !is_active {
            anyhow::bail!("Profile {profile_id} is not active");
        }

        match profile.profile_type {
            crate::profile::ProfileType::WiFi(_) => {
                let device = devices
                    .into_iter()
                    .find(|device| device.device_type == crate::device::DeviceType::WiFi)
                    .ok_or_else(|| anyhow::anyhow!("No WiFi device found"))?;
                crate::device::wifi::WiFiDevice::new(device)
                    .await?
                    .disconnect()
                    .await?;
            }
            crate::profile::ProfileType::Ethernet(profile) => {
                let device = devices
                    .into_iter()
                    .find(|device| {
                        device.device_type == crate::device::DeviceType::Ethernet
                            && profile
                                .interface
                                .as_ref()
                                .is_none_or(|name| name == &device.interface)
                    })
                    .ok_or_else(|| anyhow::anyhow!("No Ethernet device found"))?;
                crate::device::ethernet::EthernetDevice::new(device)
                    .disconnect()
                    .await?;
            }
            crate::profile::ProfileType::Vpn(_) => {
                anyhow::bail!("VPN profile deactivation is not implemented");
            }
        }

        let mut inner = self.inner.write().await;
        inner.state = NetworkState::Disconnected;
        inner.connectivity = PortalState::None;
        inner.active_profile = None;

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
        if inner.active_profile.as_ref() == Some(profile_id) {
            anyhow::bail!("Cannot delete active profile {profile_id}");
        }
        inner.profiles.delete(profile_id).await
    }

    pub async fn list_profiles(&self) -> Vec<ProfileId> {
        let inner = self.inner.read().await;
        inner.profiles.list().await
    }

    pub async fn check_connectivity(&self) -> Result<PortalState> {
        let detector = self.inner.read().await.captive_portal.clone();
        let connectivity = detector.comprehensive_check().await?;
        self.update_connectivity(connectivity).await;
        Ok(connectivity)
    }

    pub async fn get_connectivity(&self) -> PortalState {
        self.inner.read().await.connectivity
    }

    pub async fn scan_wifi(&self) -> Result<Vec<crate::device::wifi::WiFiNetwork>> {
        // Find a WiFi device
        let inner = self.inner.read().await;
        let wifi_device = inner
            .devices
            .values()
            .find(|d| d.device_type == crate::device::DeviceType::WiFi)
            .ok_or_else(|| anyhow::anyhow!("No WiFi device found"))?
            .clone();
        drop(inner);

        let wifi = crate::device::wifi::WiFiDevice::new(wifi_device).await?;
        wifi.scan().await
    }

    pub async fn connect_wifi_quick(
        &self,
        ssid: String,
        passphrase: Option<String>,
    ) -> Result<ProfileId> {
        // Determine security type
        let security = if passphrase.is_some() {
            crate::profile::wifi_profile::WiFiSecurity::Wpa2
        } else {
            crate::profile::wifi_profile::WiFiSecurity::Open
        };

        // Encrypt passphrase if present
        let passphrase_encrypted = if let Some(pass) = passphrase {
            let inner = self.inner.read().await;
            Some(inner.profiles.encrypt_passphrase(&pass).await?)
        } else {
            None
        };

        // Create WiFi profile
        let wifi_profile = crate::profile::wifi_profile::WiFiProfile {
            ssid: ssid.clone(),
            bssid: None,
            security,
            passphrase_encrypted,
            hidden: false,
            priority: 0,
        };

        let profile = crate::profile::Profile {
            id: ProfileId(uuid::Uuid::new_v4().to_string()),
            name: ssid,
            profile_type: crate::profile::ProfileType::WiFi(wifi_profile),
            auto_connect: true,
            metered: false,
        };

        // Save and connect
        let profile_id = self.create_profile(profile).await?;
        if let Err(error) = self.connect_to_profile(&profile_id).await {
            if let Err(cleanup_error) = self.delete_profile(&profile_id).await {
                warn!("Failed to remove unusable WiFi profile {profile_id}: {cleanup_error}");
            }
            return Err(error);
        }

        Ok(profile_id)
    }

    pub async fn get_wifi_signal_strength(&self) -> Result<Option<u8>> {
        let inner = self.inner.read().await;
        let wifi_device = inner
            .devices
            .values()
            .find(|d| d.device_type == crate::device::DeviceType::WiFi)
            .ok_or_else(|| anyhow::anyhow!("No WiFi device found"))?
            .clone();
        drop(inner);

        let wifi = crate::device::wifi::WiFiDevice::new(wifi_device).await?;
        wifi.signal_strength().await
    }

    pub async fn get_active_connection_info(&self) -> Result<Option<ConnectionInfo>> {
        let devices = {
            let inner = self.inner.read().await;
            if !matches!(inner.state, NetworkState::Connected | NetworkState::Limited) {
                return Ok(None);
            }
            inner.devices.values().cloned().collect::<Vec<_>>()
        };

        // Try WiFi first
        if let Some(wifi_device) = devices
            .iter()
            .find(|d| d.device_type == crate::device::DeviceType::WiFi)
        {
            let wifi = crate::device::wifi::WiFiDevice::new(wifi_device.clone()).await?;
            if let Ok(Some(ssid)) = wifi.get_current_network().await {
                let signal = wifi.signal_strength().await.ok().flatten();
                return Ok(Some(ConnectionInfo {
                    connection_type: "WiFi".to_string(),
                    interface: wifi_device.interface.clone(),
                    details: format!("SSID: {}", ssid),
                    signal_strength: signal,
                }));
            }
        }

        // Try Ethernet
        if let Some(eth_device) = devices
            .iter()
            .find(|d| d.device_type == crate::device::DeviceType::Ethernet)
        {
            let eth = crate::device::ethernet::EthernetDevice::new(eth_device.clone());
            if eth.is_carrier_detected() {
                let speed = eth.link_speed();
                return Ok(Some(ConnectionInfo {
                    connection_type: "Ethernet".to_string(),
                    interface: eth_device.interface.clone(),
                    details: speed
                        .map(|s| format!("{}Mbps", s))
                        .unwrap_or_else(|| "Unknown speed".to_string()),
                    signal_strength: None,
                }));
            }
        }

        Ok(None)
    }

    pub async fn set_auto_connect(&self, profile_id: &ProfileId, enabled: bool) -> Result<()> {
        let mut inner = self.inner.write().await;
        let mut profile = inner
            .profiles
            .get(profile_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Profile not found"))?;
        profile.auto_connect = enabled;
        inner.profiles.update(profile).await
    }

    pub async fn disconnect_active_wifi(&self) -> Result<()> {
        let active_profile = {
            let inner = self.inner.read().await;
            let id = inner
                .active_profile
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No active profile"))?;
            let profile = inner
                .profiles
                .get(&id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Active profile not found"))?;
            if !matches!(profile.profile_type, crate::profile::ProfileType::WiFi(_)) {
                anyhow::bail!("The active profile is not a WiFi profile");
            }
            id
        };
        self.disconnect_profile(&active_profile).await
    }

    pub async fn get_wifi_current_network(&self) -> Result<Option<String>> {
        let device = self.wifi_device().await?;
        crate::device::wifi::WiFiDevice::new(device)
            .await?
            .get_current_network()
            .await
    }

    pub async fn get_wifi_powered(&self) -> Result<bool> {
        let device = self.wifi_device().await?;
        crate::device::wifi::WiFiDevice::new(device)
            .await?
            .powered()
            .await
    }

    pub async fn set_wifi_powered(&self, powered: bool) -> Result<()> {
        let device = self.wifi_device().await?;
        crate::device::wifi::WiFiDevice::new(device)
            .await?
            .set_powered(powered)
            .await
    }

    async fn wifi_device(&self) -> Result<Device> {
        self.inner
            .read()
            .await
            .devices
            .values()
            .find(|device| device.device_type == crate::device::DeviceType::WiFi)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No WiFi device found"))
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_type: String,
    pub interface: String,
    pub details: String,
    pub signal_strength: Option<u8>,
}

fn device_type_str(dt: &crate::device::DeviceType) -> &'static str {
    match dt {
        crate::device::DeviceType::WiFi => "wifi",
        crate::device::DeviceType::Ethernet => "eth",
        crate::device::DeviceType::Vpn => "vpn",
    }
}

fn classify_interface(interface: &str) -> Option<DeviceType> {
    if interface.starts_with("wlan") || interface.starts_with("wlp") {
        Some(DeviceType::WiFi)
    } else if interface.starts_with("eth") || interface.starts_with("en") {
        Some(DeviceType::Ethernet)
    } else {
        None
    }
}

fn device_state_to_operational(state: &DeviceState) -> OperationalState {
    match state {
        DeviceState::Unavailable => OperationalState::Off,
        DeviceState::Disconnected => OperationalState::NoCarrier,
        DeviceState::Preparing | DeviceState::Configuring | DeviceState::NeedAuth => {
            OperationalState::Dormant
        }
        DeviceState::IpConfig => OperationalState::DegradedCarrier,
        DeviceState::IpCheck => OperationalState::Carrier,
        DeviceState::Active => OperationalState::Routable,
        DeviceState::Deactivating => OperationalState::Degraded,
        DeviceState::Failed => OperationalState::Off,
    }
}

fn profile_matches_device(profile: &Profile, device_type: &DeviceType, interface: &str) -> bool {
    match (&profile.profile_type, device_type) {
        (crate::profile::ProfileType::WiFi(_), DeviceType::WiFi) => true,
        (crate::profile::ProfileType::Ethernet(profile), DeviceType::Ethernet) => profile
            .interface
            .as_ref()
            .is_none_or(|configured| configured == interface),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ethernet_profile::EthernetProfile;

    #[test]
    fn classifies_predictable_linux_interface_names() {
        assert_eq!(classify_interface("wlp2s0"), Some(DeviceType::WiFi));
        assert_eq!(classify_interface("enp3s0"), Some(DeviceType::Ethernet));
        assert_eq!(classify_interface("lo"), None);
    }

    #[test]
    fn ethernet_profile_honors_its_interface_constraint() {
        let mut ethernet = EthernetProfile::new_dhcp();
        ethernet.interface = Some("enp3s0".into());
        let profile = Profile {
            id: ProfileId("wired".into()),
            name: "Wired".into(),
            profile_type: crate::profile::ProfileType::Ethernet(ethernet),
            auto_connect: true,
            metered: false,
        };

        assert!(profile_matches_device(
            &profile,
            &DeviceType::Ethernet,
            "enp3s0"
        ));
        assert!(!profile_matches_device(
            &profile,
            &DeviceType::Ethernet,
            "enp4s0"
        ));
    }
}
