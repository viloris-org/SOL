pub mod manager;
pub mod device;
pub mod profile;

use anyhow::Result;
use zbus::Connection;
use tracing::info;

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

        // Request well-known name
        connection
            .request_name("org.sol.Network1")
            .await?;

        info!("D-Bus service registered at org.sol.Network1");

        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}
