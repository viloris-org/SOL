//! UPower adapter for the renderer-neutral top-bar power contract.

use crate::topbar::{PowerProvider, PowerStatus, ProviderState};
use std::error::Error;
use std::fmt;
use zbus::blocking::{Connection, Proxy};

const UPOWER_SERVICE: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";

const STATE_UNKNOWN: u32 = 0;
const STATE_CHARGING: u32 = 1;
const STATE_DISCHARGING: u32 = 2;
const STATE_EMPTY: u32 = 3;
const STATE_FULLY_CHARGED: u32 = 4;
const STATE_PENDING_CHARGE: u32 = 5;
const STATE_PENDING_DISCHARGE: u32 = 6;

/// Failure to connect to or read the typed UPower device contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpowerError(String);

impl fmt::Display for UpowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for UpowerError {}

/// Live system-bus provider backed by UPower's aggregate display device.
pub struct UpowerPowerProvider {
    connection: Connection,
}

impl UpowerPowerProvider {
    /// Connect to the host system bus. No power value is fabricated when the
    /// service or its aggregate display device is unavailable.
    pub fn connect_system() -> Result<Self, UpowerError> {
        let connection = Connection::system()
            .map_err(|error| UpowerError(format!("connect to system bus: {error}")))?;
        Ok(Self { connection })
    }

    fn snapshot(&self) -> Result<ProviderState<PowerStatus>, UpowerError> {
        let proxy = Proxy::new(
            &self.connection,
            UPOWER_SERVICE,
            DISPLAY_DEVICE_PATH,
            DEVICE_INTERFACE,
        )
        .map_err(|error| UpowerError(format!("create UPower display-device proxy: {error}")))?;
        let present: bool = proxy
            .get_property("IsPresent")
            .map_err(|error| UpowerError(format!("read UPower IsPresent: {error}")))?;
        let percentage: f64 = proxy
            .get_property("Percentage")
            .map_err(|error| UpowerError(format!("read UPower Percentage: {error}")))?;
        let state: u32 = proxy
            .get_property("State")
            .map_err(|error| UpowerError(format!("read UPower State: {error}")))?;
        map_display_device(present, percentage, state)
    }
}

impl PowerProvider for UpowerPowerProvider {
    fn power(&self) -> ProviderState<PowerStatus> {
        self.snapshot()
            .unwrap_or_else(|error| ProviderState::Error(error.to_string()))
    }
}

fn map_display_device(
    present: bool,
    percentage: f64,
    state: u32,
) -> Result<ProviderState<PowerStatus>, UpowerError> {
    if !present {
        return Ok(ProviderState::Unavailable);
    }
    if !percentage.is_finite() || !(0.0..=100.0).contains(&percentage) {
        return Err(UpowerError(format!(
            "UPower percentage is outside 0..=100: {percentage}"
        )));
    }
    let charging = match state {
        STATE_CHARGING | STATE_PENDING_CHARGE => true,
        STATE_DISCHARGING | STATE_EMPTY | STATE_FULLY_CHARGED | STATE_PENDING_DISCHARGE => false,
        STATE_UNKNOWN => return Err(UpowerError("UPower battery state is unknown".to_owned())),
        value => {
            return Err(UpowerError(format!(
                "unsupported UPower battery state {value}"
            )));
        }
    };
    Ok(ProviderState::Available {
        value: PowerStatus {
            percent: percentage.round() as u8,
            charging,
        },
        stale: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_battery_is_unavailable_instead_of_zero_percent() {
        assert_eq!(
            map_display_device(false, 0.0, STATE_UNKNOWN).expect("absence should be valid"),
            ProviderState::Unavailable
        );
    }

    #[test]
    fn battery_percentage_and_charge_state_are_typed() {
        assert_eq!(
            map_display_device(true, 72.6, STATE_CHARGING).expect("charging battery should map"),
            ProviderState::Available {
                value: PowerStatus {
                    percent: 73,
                    charging: true,
                },
                stale: false,
            }
        );
        assert_eq!(
            map_display_device(true, 41.2, STATE_DISCHARGING)
                .expect("discharging battery should map"),
            ProviderState::Available {
                value: PowerStatus {
                    percent: 41,
                    charging: false,
                },
                stale: false,
            }
        );
    }

    #[test]
    fn invalid_or_unknown_device_data_is_rejected() {
        assert!(map_display_device(true, f64::NAN, STATE_CHARGING).is_err());
        assert!(map_display_device(true, 101.0, STATE_CHARGING).is_err());
        assert!(map_display_device(true, 50.0, STATE_UNKNOWN).is_err());
        assert!(map_display_device(true, 50.0, 99).is_err());
    }
}
