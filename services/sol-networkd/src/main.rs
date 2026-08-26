use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod manager;
mod device;
mod profile;
mod netlink;
mod dhcp;
mod dns;
mod captive_portal;
mod dbus;
mod security;
mod nts;

use manager::NetworkManager;
use dbus::DbusService;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sol_networkd=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting sol-networkd");

    // Create network manager
    let manager = NetworkManager::new().await?;

    // Start D-Bus service
    let _dbus_service = DbusService::new(manager.clone()).await?;

    info!("sol-networkd started, listening on D-Bus");

    // Run until interrupted
    tokio::signal::ctrl_c().await?;

    info!("Shutting down sol-networkd");

    Ok(())
}
