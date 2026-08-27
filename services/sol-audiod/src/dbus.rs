//! Stable D-Bus control plane for SOL audio routing.

use std::sync::Arc;

use zbus::Connection;

use crate::{
    routing::AudioDevice,
    service::{AudioControl, AudioControlError, RefreshResult},
};

pub const SERVICE_NAME: &str = "org.sol.Audio1";
pub const OBJECT_PATH: &str = "/org/sol/Audio1";
pub const INTERFACE_NAME: &str = "org.sol.Audio1";

/// id, display name, type, connected, battery (-1 when unavailable), trusted,
/// effective priority.
pub type WireDevice = (String, String, String, bool, i16, bool, u8);

pub struct AudioDbusService {
    control: Arc<AudioControl>,
    signals: AudioSignals,
}

impl AudioDbusService {
    #[must_use]
    pub fn new(control: Arc<AudioControl>, signals: AudioSignals) -> Self {
        Self { control, signals }
    }
}

#[zbus::interface(name = "org.sol.Audio1")]
impl AudioDbusService {
    fn list_devices(&self) -> zbus::fdo::Result<Vec<WireDevice>> {
        self.control
            .list_devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .map(|(device, priority)| device_to_wire(device, priority))
                    .collect()
            })
            .map_err(fdo_error)
    }

    fn get_active_device(&self) -> zbus::fdo::Result<WireDevice> {
        self.control
            .active_device()
            .map_err(fdo_error)?
            .map(|(device, priority)| device_to_wire(device, priority))
            .ok_or_else(|| zbus::fdo::Error::Failed("no active audio output".to_owned()))
    }

    async fn set_output_device(&self, device_id: String) -> zbus::fdo::Result<WireDevice> {
        let (old, device, priority) = self
            .control
            .set_output_device(&device_id)
            .map_err(fdo_error)?;
        if old.as_deref() != Some(device.id.as_str()) {
            self.signals
                .device_changed(old.as_deref(), Some(device.id.as_str()))
                .await?;
        }
        Ok(device_to_wire(device, priority))
    }

    fn set_device_preference(&self, device_id: String, auto_switch: bool) -> zbus::fdo::Result<()> {
        self.control
            .set_device_auto_switch(&device_id, auto_switch)
            .map_err(fdo_error)
    }

    fn set_device_trusted(&self, device_id: String, trusted: bool) -> zbus::fdo::Result<()> {
        self.control
            .set_device_trusted(&device_id, trusted)
            .map_err(fdo_error)
    }

    async fn refresh_devices(&self) -> zbus::fdo::Result<Vec<WireDevice>> {
        let result = self.control.refresh().map_err(fdo_error)?;
        self.signals.emit_refresh(&result).await?;
        self.list_devices()
    }
}

/// Signal handle also used by the daemon's hotplug loop.
#[derive(Clone)]
pub struct AudioSignals {
    connection: Connection,
}

impl AudioSignals {
    async fn emit(
        &self,
        member: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> zbus::fdo::Result<()> {
        self.connection
            .emit_signal(None::<&str>, OBJECT_PATH, INTERFACE_NAME, member, body)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }

    pub async fn device_connected(&self, device_id: &str) -> zbus::fdo::Result<()> {
        self.emit("DeviceConnected", &device_id).await
    }

    pub async fn device_disconnected(&self, device_id: &str) -> zbus::fdo::Result<()> {
        self.emit("DeviceDisconnected", &device_id).await
    }

    pub async fn device_changed(
        &self,
        old: Option<&str>,
        new: Option<&str>,
    ) -> zbus::fdo::Result<()> {
        self.emit(
            "DeviceChanged",
            &(old.unwrap_or_default(), new.unwrap_or_default()),
        )
        .await
    }

    pub async fn emit_refresh(&self, result: &RefreshResult) -> zbus::fdo::Result<()> {
        for device_id in &result.connected {
            self.device_connected(device_id).await?;
        }
        for device_id in &result.disconnected {
            self.device_disconnected(device_id).await?;
        }
        if let Some((old, new)) = &result.active_change {
            self.device_changed(old.as_deref(), new.as_deref()).await?;
        }
        Ok(())
    }
}

/// Own the SOL audio name on the caller's session bus.
pub async fn serve_session(
    control: Arc<AudioControl>,
) -> Result<(Connection, AudioSignals), AudioControlError> {
    let connection = Connection::session().await.map_err(bus_error)?;
    let signals = AudioSignals {
        connection: connection.clone(),
    };
    connection
        .object_server()
        .at(OBJECT_PATH, AudioDbusService::new(control, signals.clone()))
        .await
        .map_err(bus_error)?;
    connection
        .request_name(SERVICE_NAME)
        .await
        .map_err(bus_error)?;
    Ok((connection, signals))
}

fn device_to_wire(device: AudioDevice, priority: u8) -> WireDevice {
    (
        device.id,
        device.name,
        device.device_type.as_str().to_owned(),
        device.is_connected,
        device.battery_level.map_or(-1, i16::from),
        device.trusted,
        priority,
    )
}

fn fdo_error(error: AudioControlError) -> zbus::fdo::Error {
    match error {
        AudioControlError::DeviceNotFound(message)
        | AudioControlError::DeviceDisconnected(message) => zbus::fdo::Error::InvalidArgs(message),
        other => zbus::fdo::Error::Failed(other.to_string()),
    }
}

fn bus_error(error: impl std::fmt::Display) -> AudioControlError {
    AudioControlError::Routing(format!("audio D-Bus: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::AudioDeviceType;

    #[test]
    fn wire_device_has_stable_type_and_battery_sentinel() {
        let wire = device_to_wire(
            AudioDevice {
                id: "speaker".to_owned(),
                name: "Speakers".to_owned(),
                device_type: AudioDeviceType::BuiltinSpeaker,
                is_connected: true,
                battery_level: None,
                is_charging: false,
                last_used: None,
                trusted: true,
            },
            15,
        );
        assert_eq!(wire.2, "builtin-speaker");
        assert_eq!(wire.4, -1);
        assert!(wire.5);
    }
}
