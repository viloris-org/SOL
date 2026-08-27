pub mod device;
pub mod manager;
pub mod profile;
pub mod vpn;
pub mod wifi;

use anyhow::Result;
use tracing::info;
use zbus::Connection;

use crate::manager::NetworkManager;

/// D-Bus service wrapper
pub struct DbusService {
    connection: Connection,
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

        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}
