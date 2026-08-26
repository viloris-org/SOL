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
use crate::nts::{NtsClient, DEFAULT_NTS_SERVERS};

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
    nts_client: NtsClient,
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
            nts_client: NtsClient::new(DEFAULT_NTS_SERVERS[0].to_string()),
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
        let profile = inner.profiles.get(profile_id).await?
            .ok_or_else(|| anyhow::anyhow!("Profile not found"))?;

        inner.state = NetworkState::Connecting;
        drop(inner); // Release lock during connection

        // Dispatch based on profile type
        match &profile.profile_type {
            crate::profile::ProfileType::WiFi(wifi_profile) => {
                self.connect_wifi(profile_id, wifi_profile).await?;
            }
            crate::profile::ProfileType::Ethernet(eth_profile) => {
                self.connect_ethernet(profile_id, eth_profile).await?;
            }
            crate::profile::ProfileType::Vpn(vpn_profile) => {
                self.connect_vpn(profile_id, vpn_profile).await?;
            }
        }

        let mut inner = self.inner.write().await;
        inner.state = NetworkState::Connected;

        // Sync time after successful connection
        let manager_clone = self.clone();
        tokio::spawn(async move {
            if let Err(e) = manager_clone.sync_time_after_connect().await {
                warn!("Failed to sync time: {}", e);
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
                info!("Time synchronized: {} from {}", time_info.current_time, time_info.server);
                Ok(())
            }
            Err(e) => {
                warn!("Time sync failed: {}", e);
                Err(e)
            }
        }
    }

    async fn connect_wifi(&self, _profile_id: &ProfileId, wifi_profile: &crate::profile::wifi_profile::WiFiProfile) -> Result<()> {
        // Find a WiFi device
        let inner = self.inner.read().await;
        let wifi_device = inner.devices.values()
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

        wifi.connect(&wifi_profile.ssid, passphrase.as_deref()).await?;

        // Wait for connection to establish
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        Ok(())
    }

    async fn connect_ethernet(&self, _profile_id: &ProfileId, eth_profile: &crate::profile::ethernet_profile::EthernetProfile) -> Result<()> {
        // Find an Ethernet device (or specific one if specified)
        let inner = self.inner.read().await;
        let eth_device = if let Some(iface) = &eth_profile.interface {
            inner.devices.values()
                .find(|d| d.device_type == crate::device::DeviceType::Ethernet && &d.interface == iface)
                .cloned()
        } else {
            inner.devices.values()
                .find(|d| d.device_type == crate::device::DeviceType::Ethernet)
                .cloned()
        }
        .ok_or_else(|| anyhow::anyhow!("No Ethernet device found"))?;
        drop(inner);

        // Create Ethernet device and connect
        let ethernet = crate::device::ethernet::EthernetDevice::new(eth_device);
        ethernet.connect(&eth_profile.ip_config).await?;

        Ok(())
    }

    async fn connect_vpn(&self, _profile_id: &ProfileId, _vpn_profile: &crate::profile::vpn_profile::VpnProfile) -> Result<()> {
        // VPN connection logic
        info!("VPN connection not yet fully implemented");
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

    pub async fn scan_wifi(&self) -> Result<Vec<crate::device::wifi::WiFiNetwork>> {
        // Find a WiFi device
        let inner = self.inner.read().await;
        let wifi_device = inner.devices.values()
            .find(|d| d.device_type == crate::device::DeviceType::WiFi)
            .ok_or_else(|| anyhow::anyhow!("No WiFi device found"))?
            .clone();
        drop(inner);

        let wifi = crate::device::wifi::WiFiDevice::new(wifi_device).await?;
        wifi.scan().await
    }

    pub async fn connect_wifi_quick(&self, ssid: String, passphrase: Option<String>) -> Result<ProfileId> {
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
        self.connect_to_profile(&profile_id).await?;

        Ok(profile_id)
    }

    pub async fn get_wifi_signal_strength(&self) -> Result<Option<u8>> {
        let inner = self.inner.read().await;
        let wifi_device = inner.devices.values()
            .find(|d| d.device_type == crate::device::DeviceType::WiFi)
            .ok_or_else(|| anyhow::anyhow!("No WiFi device found"))?
            .clone();
        drop(inner);

        let wifi = crate::device::wifi::WiFiDevice::new(wifi_device).await?;
        wifi.signal_strength().await
    }

    pub async fn get_active_connection_info(&self) -> Result<Option<ConnectionInfo>> {
        let inner = self.inner.read().await;

        if inner.state != NetworkState::Connected {
            return Ok(None);
        }

        // Try WiFi first
        if let Some(wifi_device) = inner.devices.values()
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
        if let Some(eth_device) = inner.devices.values()
            .find(|d| d.device_type == crate::device::DeviceType::Ethernet)
        {
            let eth = crate::device::ethernet::EthernetDevice::new(eth_device.clone());
            if eth.is_carrier_detected() {
                let speed = eth.link_speed();
                return Ok(Some(ConnectionInfo {
                    connection_type: "Ethernet".to_string(),
                    interface: eth_device.interface.clone(),
                    details: speed.map(|s| format!("{}Mbps", s)).unwrap_or_else(|| "Unknown speed".to_string()),
                    signal_strength: None,
                }));
            }
        }

        Ok(None)
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
