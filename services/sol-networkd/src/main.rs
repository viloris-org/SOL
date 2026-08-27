//! sol-networkd: network control plane service.
//!
//! This service is under active Phase-1 development. Most subsystems (VPN,
//! WiFi, NTS time sync, DHCP lease management, state-file persistence, config
//! request queue, per-device/profile D-Bus interfaces) expose broad APIs that
//! are not yet fully wired into the live D-Bus surface. Allow dead code while
//! this scaffolding lands so CI stays green; remove `allow(dead_code)` once the
//! Phase-1 control path is complete.
#![allow(dead_code)]

use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod captive_portal;
mod dbus;
mod device;
mod dhcp;
mod dns;
mod manager;
mod netlink;
mod nts;
mod profile;
mod queue;
mod security;
mod state_file;

use dbus::DbusService;
use manager::NetworkManager;

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
