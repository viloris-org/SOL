pub mod device;
pub mod dynamic;
pub mod manager;
pub mod profile;
pub mod vpn;
pub mod wifi;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use zbus::Connection;

use crate::manager::NetworkManager;
use dynamic::ObjectRegistry;

/// D-Bus service wrapper
pub struct DbusService {
    connection: Connection,
    registry: Arc<RwLock<ObjectRegistry>>,
}

impl DbusService {
    pub async fn new(manager: NetworkManager) -> Result<Self> {
        info!("Starting D-Bus service");

        // Connect to system bus
        let connection = Connection::system().await?;

        // Register manager interface
        connection
            .object_server()
            .at(
                "/org/sol/Network1",
                manager::ManagerInterface::new(manager.clone()),
            )
            .await?;

        // Register WiFi interface
        // TODO: Register per-device WiFi interfaces dynamically
        // For now, register a global WiFi interface
        connection
            .object_server()
            .at(
                "/org/sol/Network1/WiFi",
                wifi::WiFiInterface::new(manager.clone(), String::new()),
            )
            .await?;

        // Register VPN interface
        connection
            .object_server()
            .at(
                "/org/sol/Network1/VPN",
                vpn::VpnInterface::new(manager.clone()),
            )
            .await?;

        // Request well-known name
        connection.request_name("org.sol.Network1").await?;

        info!("D-Bus service registered at org.sol.Network1");

        // Create object registry for dynamic device/profile objects
        let registry = ObjectRegistry::new(connection.clone(), manager.clone());

        // Initial sync of devices and profiles
        if let Err(e) = registry.sync_devices().await {
            info!("Failed to sync devices on startup: {}", e);
        }
        if let Err(e) = registry.sync_profiles().await {
            info!("Failed to sync profiles on startup: {}", e);
        }

        let registry = Arc::new(RwLock::new(registry));

        // Spawn background task to periodically sync objects
        let registry_clone = registry.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let reg = registry_clone.read().await;
                if let Err(e) = reg.sync_devices().await {
                    info!("Failed to sync devices: {}", e);
                }
                if let Err(e) = reg.sync_profiles().await {
                    info!("Failed to sync profiles: {}", e);
                }
            }
        });

        Ok(Self {
            connection,
            registry,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}
