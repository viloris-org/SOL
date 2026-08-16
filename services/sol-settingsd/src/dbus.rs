//! D-Bus transport for the stable, typed settings boundary.
//!
//! The service deliberately transfers one complete snapshot and accepts only
//! named setting mutations. Files, arbitrary property names, and backend
//! details stay private to `sol-settingsd`.

use std::sync::Arc;

use sol_system::{
    ColorScheme, OutputVolume, SettingsApi, SettingsChange, SettingsError, SettingsResult,
    SettingsSnapshot, TextScale,
};
use zbus::blocking::{Connection, Proxy, connection::Builder};

use crate::{SettingsDaemon, SettingsStore};

pub const SERVICE_NAME: &str = "org.sol.Settings1";
pub const OBJECT_PATH: &str = "/org/sol/Settings1";
pub const INTERFACE_NAME: &str = "org.sol.Settings1";

type WireSnapshot = (u64, String, bool, bool, String, u8, bool);

/// A session-bus server backed by a typed settings daemon.
pub struct SettingsDbusService<S> {
    daemon: Arc<SettingsDaemon<S>>,
}

impl<S: SettingsStore> SettingsDbusService<S> {
    #[must_use]
    pub fn new(daemon: SettingsDaemon<S>) -> Self {
        Self {
            daemon: Arc::new(daemon),
        }
    }

    fn snapshot_wire(&self) -> zbus::fdo::Result<WireSnapshot> {
        self.daemon
            .snapshot()
            .map(snapshot_to_wire)
            .map_err(fdo_error)
    }

    fn apply(&self, change: SettingsChange) -> zbus::fdo::Result<WireSnapshot> {
        self.daemon
            .apply(change)
            .map(snapshot_to_wire)
            .map_err(fdo_error)
    }
}

#[zbus::interface(name = "org.sol.Settings1")]
impl<S: SettingsStore + 'static> SettingsDbusService<S> {
    /// Return all settings in one coherent revisioned response.
    fn snapshot(&self) -> zbus::fdo::Result<WireSnapshot> {
        self.snapshot_wire()
    }

    fn set_color_scheme(&self, color_scheme: String) -> zbus::fdo::Result<WireSnapshot> {
        self.apply(SettingsChange::SetColorScheme(parse_color_scheme(
            &color_scheme,
        )?))
    }

    fn set_high_contrast(&self, high_contrast: bool) -> zbus::fdo::Result<WireSnapshot> {
        self.apply(SettingsChange::SetHighContrast(high_contrast))
    }

    fn set_reduced_motion(&self, reduced_motion: bool) -> zbus::fdo::Result<WireSnapshot> {
        self.apply(SettingsChange::SetReducedMotion(reduced_motion))
    }

    fn set_text_scale(&self, text_scale: String) -> zbus::fdo::Result<WireSnapshot> {
        self.apply(SettingsChange::SetTextScale(parse_text_scale(&text_scale)?))
    }

    fn set_output_volume(&self, percent: u8) -> zbus::fdo::Result<WireSnapshot> {
        let volume = OutputVolume::new(percent).map_err(fdo_error)?;
        self.apply(SettingsChange::SetOutputVolume(volume))
    }

    fn set_output_muted(&self, output_muted: bool) -> zbus::fdo::Result<WireSnapshot> {
        self.apply(SettingsChange::SetOutputMuted(output_muted))
    }
}

/// Own the stable settings name on the caller's session bus.
pub fn serve_session<S: SettingsStore + 'static>(
    daemon: SettingsDaemon<S>,
) -> SettingsResult<Connection> {
    Builder::session()
        .map_err(bus_error)?
        .name(SERVICE_NAME)
        .map_err(bus_error)?
        .serve_at(OBJECT_PATH, SettingsDbusService::new(daemon))
        .map_err(bus_error)?
        .build()
        .map_err(bus_error)
}

/// A blocking `SettingsApi` client for another process in the same session.
pub struct SettingsDbusProxy {
    proxy: Proxy<'static>,
}

impl std::fmt::Debug for SettingsDbusProxy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SettingsDbusProxy")
            .finish_non_exhaustive()
    }
}

impl SettingsDbusProxy {
    pub fn connect() -> SettingsResult<Self> {
        let connection = Connection::session().map_err(bus_error)?;
        Self::from_connection(connection)
    }

    pub fn from_connection(connection: Connection) -> SettingsResult<Self> {
        Proxy::new_owned(connection, SERVICE_NAME, OBJECT_PATH, INTERFACE_NAME)
            .map(|proxy| Self { proxy })
            .map_err(bus_error)
    }

    fn call<A>(&self, method: &str, arguments: &A) -> SettingsResult<SettingsSnapshot>
    where
        A: serde::ser::Serialize + zbus::zvariant::Type,
    {
        let snapshot: WireSnapshot = self.proxy.call(method, arguments).map_err(bus_error)?;
        snapshot_from_wire(snapshot)
    }
}

impl SettingsApi for SettingsDbusProxy {
    fn snapshot(&self) -> SettingsResult<SettingsSnapshot> {
        self.call("Snapshot", &())
    }

    fn apply(&self, change: SettingsChange) -> SettingsResult<SettingsSnapshot> {
        match change {
            SettingsChange::SetColorScheme(value) => self.call("SetColorScheme", &value.as_str()),
            SettingsChange::SetHighContrast(value) => self.call("SetHighContrast", &value),
            SettingsChange::SetReducedMotion(value) => self.call("SetReducedMotion", &value),
            SettingsChange::SetTextScale(value) => self.call("SetTextScale", &value.as_str()),
            SettingsChange::SetOutputVolume(value) => {
                self.call("SetOutputVolume", &value.percent())
            }
            SettingsChange::SetOutputMuted(value) => self.call("SetOutputMuted", &value),
        }
    }
}

fn snapshot_to_wire(snapshot: SettingsSnapshot) -> WireSnapshot {
    (
        snapshot.revision,
        snapshot.appearance.color_scheme.as_str().to_owned(),
        snapshot.appearance.high_contrast,
        snapshot.appearance.reduced_motion,
        snapshot.appearance.text_scale.as_str().to_owned(),
        snapshot.audio.output_volume.percent(),
        snapshot.audio.output_muted,
    )
}

fn snapshot_from_wire(snapshot: WireSnapshot) -> SettingsResult<SettingsSnapshot> {
    let (
        revision,
        color_scheme,
        high_contrast,
        reduced_motion,
        text_scale,
        output_volume,
        output_muted,
    ) = snapshot;
    Ok(SettingsSnapshot {
        revision,
        appearance: sol_system::AppearanceSettings {
            color_scheme: parse_color_scheme(&color_scheme).map_err(from_fdo_error)?,
            high_contrast,
            reduced_motion,
            text_scale: parse_text_scale(&text_scale).map_err(from_fdo_error)?,
        },
        audio: sol_system::AudioSettings {
            output_volume: OutputVolume::new(output_volume)?,
            output_muted,
        },
    })
}

fn parse_color_scheme(value: &str) -> zbus::fdo::Result<ColorScheme> {
    match value {
        "system" => Ok(ColorScheme::System),
        "light" => Ok(ColorScheme::Light),
        "dark" => Ok(ColorScheme::Dark),
        _ => Err(zbus::fdo::Error::InvalidArgs(
            "color scheme must be system, light, or dark".to_owned(),
        )),
    }
}

fn parse_text_scale(value: &str) -> zbus::fdo::Result<TextScale> {
    match value {
        "default" => Ok(TextScale::Default),
        "large" => Ok(TextScale::Large),
        "extra-large" => Ok(TextScale::ExtraLarge),
        _ => Err(zbus::fdo::Error::InvalidArgs(
            "text scale must be default, large, or extra-large".to_owned(),
        )),
    }
}

fn fdo_error(error: SettingsError) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(error.to_string())
}

fn from_fdo_error(error: zbus::fdo::Error) -> SettingsError {
    SettingsError::backend(error.to_string())
}

fn bus_error(error: impl std::fmt::Display) -> SettingsError {
    SettingsError::backend(format!("settings D-Bus: {error}"))
}

#[cfg(test)]
mod tests {
    use sol_system::{ColorScheme, OutputVolume, TextScale};

    use super::{snapshot_from_wire, snapshot_to_wire};

    #[test]
    fn snapshot_wire_round_trip_preserves_typed_values() {
        let snapshot = sol_system::SettingsSnapshot {
            revision: 7,
            appearance: sol_system::AppearanceSettings {
                color_scheme: ColorScheme::Dark,
                high_contrast: true,
                reduced_motion: true,
                text_scale: TextScale::Large,
            },
            audio: sol_system::AudioSettings {
                output_volume: OutputVolume::new(73).expect("volume is valid"),
                output_muted: true,
            },
        };
        assert_eq!(
            snapshot_from_wire(snapshot_to_wire(snapshot.clone())),
            Ok(snapshot)
        );
    }
}
