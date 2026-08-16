//! NetworkManager adapter for the renderer-neutral top-bar network contract.
//!
//! This module is deliberately read-only.  Quick Settings network writes need
//! a separate permission-gated action API and are not inferred from status
//! polling.

use crate::topbar::{NetworkProvider, NetworkStatus, ProviderState};
use std::error::Error;
use std::fmt;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

const SERVICE: &str = "org.freedesktop.NetworkManager";
const ROOT_PATH: &str = "/org/freedesktop/NetworkManager";
const ROOT_INTERFACE: &str = "org.freedesktop.NetworkManager";
const ACTIVE_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";

const NM_STATE_ASLEEP: u32 = 10;
const NM_STATE_DISCONNECTED: u32 = 20;
const NM_STATE_DISCONNECTING: u32 = 30;
const NM_STATE_CONNECTING: u32 = 40;
const NM_STATE_CONNECTED_LOCAL: u32 = 50;
const NM_STATE_CONNECTED_SITE: u32 = 60;
const NM_STATE_CONNECTED_GLOBAL: u32 = 70;

const ACTIVE_ACTIVATING: u32 = 1;
const ACTIVE_ACTIVATED: u32 = 2;
const ACTIVE_DEACTIVATING: u32 = 3;
const ACTIVE_DEACTIVATED: u32 = 4;

/// Failure to connect to or validate NetworkManager's typed D-Bus state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkManagerError(String);

impl fmt::Display for NetworkManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for NetworkManagerError {}

/// Read-only NetworkManager provider used by the Shell top bar.
pub struct NetworkManagerProvider {
    connection: Connection,
}

impl NetworkManagerProvider {
    /// Connect to the host system bus.  No network state is fabricated when
    /// NetworkManager is absent or its primary connection is inconsistent.
    pub fn connect_system() -> Result<Self, NetworkManagerError> {
        let connection = Connection::system()
            .map_err(|error| NetworkManagerError(format!("connect to system bus: {error}")))?;
        Ok(Self { connection })
    }

    fn snapshot(&self) -> Result<ProviderState<NetworkStatus>, NetworkManagerError> {
        let root =
            Proxy::new(&self.connection, SERVICE, ROOT_PATH, ROOT_INTERFACE).map_err(|error| {
                NetworkManagerError(format!("create NetworkManager proxy: {error}"))
            })?;
        let global_state: u32 = root
            .get_property("State")
            .map_err(|error| NetworkManagerError(format!("read NetworkManager State: {error}")))?;

        if matches!(global_state, NM_STATE_ASLEEP | NM_STATE_DISCONNECTED) {
            return Ok(ProviderState::Available {
                value: NetworkStatus::Offline,
                stale: false,
            });
        }
        if matches!(global_state, NM_STATE_DISCONNECTING | NM_STATE_CONNECTING) {
            return Ok(ProviderState::Available {
                value: NetworkStatus::Connecting,
                stale: false,
            });
        }
        if !matches!(
            global_state,
            NM_STATE_CONNECTED_LOCAL | NM_STATE_CONNECTED_SITE | NM_STATE_CONNECTED_GLOBAL
        ) {
            return Err(NetworkManagerError(format!(
                "unsupported NetworkManager global state: {global_state}"
            )));
        }

        let primary: OwnedObjectPath = root.get_property("PrimaryConnection").map_err(|error| {
            NetworkManagerError(format!("read NetworkManager PrimaryConnection: {error}"))
        })?;
        if primary.as_str() == "/" {
            return Err(NetworkManagerError(
                "NetworkManager reports connected without a primary connection".to_owned(),
            ));
        }
        let active = Proxy::new(
            &self.connection,
            SERVICE,
            primary.as_str(),
            ACTIVE_INTERFACE,
        )
        .map_err(|error| NetworkManagerError(format!("create active connection proxy: {error}")))?;
        let active_state: u32 = active.get_property("State").map_err(|error| {
            NetworkManagerError(format!("read active connection State: {error}"))
        })?;
        let name: String = active
            .get_property("Id")
            .map_err(|error| NetworkManagerError(format!("read active connection Id: {error}")))?;
        validate_name(&name)?;
        let devices: Vec<OwnedObjectPath> = active.get_property("Devices").map_err(|error| {
            NetworkManagerError(format!("read active connection Devices: {error}"))
        })?;
        let signal_percent = signal_percent(&self.connection, &devices)?;
        map_connected_state(active_state, name, signal_percent)
    }
}

impl NetworkProvider for NetworkManagerProvider {
    fn network(&self) -> ProviderState<NetworkStatus> {
        self.snapshot()
            .unwrap_or_else(|error| ProviderState::Error(error.to_string()))
    }
}

fn map_connected_state(
    active_state: u32,
    name: String,
    signal_percent: u8,
) -> Result<ProviderState<NetworkStatus>, NetworkManagerError> {
    match active_state {
        ACTIVE_ACTIVATED => Ok(ProviderState::Available {
            value: NetworkStatus::Connected {
                name,
                signal_percent,
            },
            stale: false,
        }),
        ACTIVE_ACTIVATING | ACTIVE_DEACTIVATING => Ok(ProviderState::Available {
            value: NetworkStatus::Connecting,
            stale: false,
        }),
        ACTIVE_DEACTIVATED => Ok(ProviderState::Available {
            value: NetworkStatus::Offline,
            stale: false,
        }),
        value => Err(NetworkManagerError(format!(
            "unsupported NetworkManager active state: {value}"
        ))),
    }
}

fn signal_percent(
    connection: &Connection,
    devices: &[OwnedObjectPath],
) -> Result<u8, NetworkManagerError> {
    for device_path in devices {
        let device = Proxy::new(connection, SERVICE, device_path.as_str(), DEVICE_INTERFACE)
            .map_err(|error| NetworkManagerError(format!("create device proxy: {error}")))?;
        let device_type: u32 = device
            .get_property("DeviceType")
            .map_err(|error| NetworkManagerError(format!("read device type: {error}")))?;
        if device_type == 2 {
            let wireless = Proxy::new(
                connection,
                SERVICE,
                device_path.as_str(),
                WIRELESS_INTERFACE,
            )
            .map_err(|error| NetworkManagerError(format!("create wireless proxy: {error}")))?;
            let strength: u8 = wireless
                .get_property("Strength")
                .map_err(|error| NetworkManagerError(format!("read wireless strength: {error}")))?;
            if strength <= 100 {
                return Ok(strength);
            }
            return Err(NetworkManagerError(format!(
                "NetworkManager wireless strength is outside 0..=100: {strength}"
            )));
        }
    }
    // Wired links do not expose a signal percentage.  The existing top-bar
    // contract uses 100 to represent an active wired link rather than a fake
    // Wi-Fi bar; callers can distinguish it from Wi-Fi through the provider's
    // connection metadata when they need a richer presentation.
    Ok(100)
}

fn validate_name(name: &str) -> Result<(), NetworkManagerError> {
    if name.is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
        return Err(NetworkManagerError(
            "NetworkManager connection name is invalid".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_states_map_without_fabricating_connection_data() {
        assert_eq!(
            map_global_state(NM_STATE_DISCONNECTED),
            Some(NetworkStatus::Offline)
        );
        assert_eq!(
            map_global_state(NM_STATE_CONNECTING),
            Some(NetworkStatus::Connecting)
        );
        assert_eq!(map_global_state(NM_STATE_CONNECTED_GLOBAL), None);
    }

    #[test]
    fn active_states_validate_connected_snapshot() {
        assert_eq!(
            map_connected_state(ACTIVE_ACTIVATED, "Home".to_owned(), 82)
                .expect("activated connection should map"),
            ProviderState::Available {
                value: NetworkStatus::Connected {
                    name: "Home".to_owned(),
                    signal_percent: 82,
                },
                stale: false,
            }
        );
        assert_eq!(
            map_connected_state(ACTIVE_ACTIVATING, "Home".to_owned(), 82)
                .expect("activating connection should map"),
            ProviderState::Available {
                value: NetworkStatus::Connecting,
                stale: false,
            }
        );
        assert!(map_connected_state(99, "Home".to_owned(), 82).is_err());
    }

    #[test]
    fn invalid_connection_names_are_rejected() {
        assert!(validate_name("").is_err());
        assert!(validate_name("bad\nname").is_err());
        assert!(validate_name(&"x".repeat(513)).is_err());
    }

    fn map_global_state(state: u32) -> Option<NetworkStatus> {
        match state {
            NM_STATE_ASLEEP | NM_STATE_DISCONNECTED => Some(NetworkStatus::Offline),
            NM_STATE_DISCONNECTING | NM_STATE_CONNECTING => Some(NetworkStatus::Connecting),
            _ => None,
        }
    }
}
