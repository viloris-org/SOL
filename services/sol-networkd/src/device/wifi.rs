use anyhow::{Result, Context};
use tracing::{info, warn, debug};
use zbus::{Connection, proxy};

use crate::device::Device;

/// WiFi device implementation using iwd (Intel Wireless Daemon)
pub struct WiFiDevice {
    device: Device,
    connection: Connection,
}

#[proxy(
    interface = "net.connman.iwd.Station",
    default_service = "net.connman.iwd",
    assume_defaults = true
)]
trait IwdStation {
    async fn scan(&self) -> zbus::Result<()>;
    async fn get_ordered_networks(&self) -> zbus::Result<Vec<(zbus::zvariant::OwnedObjectPath, i16)>>;
    async fn connect(&self, ssid: &str) -> zbus::Result<()>;
    async fn disconnect(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn connected_network(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn scanning(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "net.connman.iwd.Network",
    default_service = "net.connman.iwd",
    assume_defaults = true
)]
trait IwdNetwork {
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn type_(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
}

#[proxy(
    interface = "net.connman.iwd.Device",
    default_service = "net.connman.iwd",
    assume_defaults = true
)]
trait IwdDevice {
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn set_powered(&self, value: bool) -> zbus::Result<()>;

    #[zbus(property)]
    fn mode(&self) -> zbus::Result<String>;
}

impl WiFiDevice {
    pub async fn new(device: Device) -> Result<Self> {
        let connection = Connection::system().await
            .context("Failed to connect to system D-Bus")?;

        Ok(Self { device, connection })
    }

    fn station_path(&self) -> String {
        format!("/net/connman/iwd/0/{}", self.device.interface)
    }

    pub async fn scan(&self) -> Result<Vec<WiFiNetwork>> {
        info!("Scanning WiFi networks on {}", self.device.interface);

        let station_path = self.station_path();
        let station = IwdStationProxy::builder(&self.connection)
            .path(station_path.as_str())?
            .build()
            .await?;

        // Trigger scan
        station.scan().await.context("Failed to trigger WiFi scan")?;

        // Wait for scan to complete
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Get ordered networks (sorted by signal strength)
        let networks = station.get_ordered_networks().await
            .context("Failed to get network list")?;

        let mut results = Vec::new();
        for (network_path, signal_dbm) in networks {
            match self.parse_network(&network_path, signal_dbm).await {
                Ok(network) => results.push(network),
                Err(e) => warn!("Failed to parse network {}: {}", network_path, e),
            }
        }

        info!("Found {} WiFi networks", results.len());
        Ok(results)
    }

    async fn parse_network(&self, path: &zbus::zvariant::OwnedObjectPath, signal_dbm: i16) -> Result<WiFiNetwork> {
        let network = IwdNetworkProxy::builder(&self.connection)
            .path(path)?
            .build()
            .await?;

        let ssid = network.name().await?;
        let security_type = network.type_().await?;

        // Extract BSSID from path (format: /net/connman/iwd/0/wlan0/xx_xx_xx_xx_xx_xx_psk)
        let bssid = Self::extract_bssid_from_path(path.as_str());

        // Convert dBm to percentage (rough approximation)
        let signal_strength = Self::dbm_to_percentage(signal_dbm);

        let security = match security_type.as_str() {
            "open" => WiFiSecurity::Open,
            "wep" => WiFiSecurity::Wep,
            "psk" => WiFiSecurity::Wpa2,  // WPA2-PSK
            "8021x" => WiFiSecurity::Wpa2,  // WPA2-Enterprise
            "sae" => WiFiSecurity::Wpa3,  // WPA3
            _ => WiFiSecurity::Wpa2,
        };

        Ok(WiFiNetwork {
            ssid,
            bssid,
            signal_strength,
            frequency: 2437,  // TODO: Get actual frequency from iwd
            security,
        })
    }

    fn extract_bssid_from_path(path: &str) -> String {
        // Path format: /net/connman/iwd/0/wlan0/aa_bb_cc_dd_ee_ff_psk
        path.split('/')
            .last()
            .and_then(|s| s.split('_').take(6).collect::<Vec<_>>().get(..6).map(|parts| parts.join(":")))
            .unwrap_or_else(|| "00:00:00:00:00:00".to_string())
    }

    fn dbm_to_percentage(dbm: i16) -> u8 {
        // Convert dBm to percentage (rough WiFi quality scale)
        // -30 dBm = 100%, -90 dBm = 0%
        let clamped = dbm.clamp(-90, -30);
        let percentage = ((clamped + 90) as f32 / 60.0 * 100.0) as u8;
        percentage.min(100)
    }

    pub async fn connect(&self, ssid: &str, passphrase: Option<&str>) -> Result<()> {
        info!("Connecting to WiFi network: {}", ssid);

        // Store passphrase in iwd's known networks if provided
        if let Some(pass) = passphrase {
            self.store_passphrase(ssid, pass).await?;
        }

        let station_path = self.station_path();
        let station = IwdStationProxy::builder(&self.connection)
            .path(station_path.as_str())?
            .build()
            .await?;

        // iwd will handle authentication using stored credentials
        station.connect(ssid).await
            .context("Failed to connect to WiFi network")?;

        info!("Successfully connected to {}", ssid);
        Ok(())
    }

    async fn store_passphrase(&self, ssid: &str, passphrase: &str) -> Result<()> {
        // Write passphrase to iwd's storage
        // Format: /var/lib/iwd/<ssid>.psk
        let config = format!(
            "[Security]\nPassphrase={}\n",
            passphrase
        );

        let path = format!("/var/lib/iwd/{}.psk", ssid.replace('/', "_"));

        tokio::fs::write(&path, config).await
            .context("Failed to write WiFi credentials")?;

        debug!("Stored credentials for {}", ssid);
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting WiFi on {}", self.device.interface);

        let station_path = self.station_path();
        let station = IwdStationProxy::builder(&self.connection)
            .path(station_path.as_str())?
            .build()
            .await?;

        station.disconnect().await
            .context("Failed to disconnect WiFi")?;

        Ok(())
    }

    pub async fn signal_strength(&self) -> Result<Option<u8>> {
        let station_path = self.station_path();
        let station = IwdStationProxy::builder(&self.connection)
            .path(station_path.as_str())?
            .build()
            .await?;

        // Check if connected
        if let Ok(network_path) = station.connected_network().await {
            if !network_path.is_empty() {
                if let Ok(networks) = station.get_ordered_networks().await {
                    for (path, signal_dbm) in networks {
                        if path.as_str() == network_path {
                            return Ok(Some(Self::dbm_to_percentage(signal_dbm)));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    pub async fn get_current_network(&self) -> Result<Option<String>> {
        let station_path = self.station_path();
        let station = IwdStationProxy::builder(&self.connection)
            .path(station_path.as_str())?
            .build()
            .await?;

        if let Ok(network_path) = station.connected_network().await {
            if !network_path.is_empty() {
                let network = IwdNetworkProxy::builder(&self.connection)
                    .path(network_path.as_str())?
                    .build()
                    .await?;

                return Ok(Some(network.name().await?));
            }
        }

        Ok(None)
    }

    pub async fn set_powered(&self, powered: bool) -> Result<()> {
        let station_path = self.station_path();
        let device = IwdDeviceProxy::builder(&self.connection)
            .path(station_path.as_str())?
            .build()
            .await?;

        device.set_powered(powered).await
            .context("Failed to set WiFi power state")?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WiFiNetwork {
    pub ssid: String,
    pub bssid: String,
    pub signal_strength: u8,  // 0-100
    pub frequency: u32,        // MHz
    pub security: WiFiSecurity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WiFiSecurity {
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
}
