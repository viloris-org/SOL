//! PipeWire-backed audio status adapter using its Pulse compatibility API.

use crate::topbar::{AudioProvider, AudioStatus, ProviderState};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Failure to query or validate the PipeWire-backed audio service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipewireAudioError(String);

impl fmt::Display for PipewireAudioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PipewireAudioError {}

/// Live provider that invokes `pactl` only for its structured JSON API.
pub struct PipewireAudioProvider {
    pactl: PathBuf,
}

/// Runtime state reported for a PipeWire output node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutputState {
    Running,
    Idle,
    Suspended,
}

/// Availability reported for a physical or logical output port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioPortAvailability {
    Available,
    Unavailable,
    Unknown,
}

/// One validated port exposed by a PipeWire output device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOutputPort {
    pub name: String,
    pub description: String,
    pub availability: AudioPortAvailability,
    pub active: bool,
}

/// Read-only output device projection for Quick Settings and status UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOutputDevice {
    pub name: String,
    pub description: String,
    pub state: AudioOutputState,
    pub is_default: bool,
    pub ports: Vec<AudioOutputPort>,
}

/// Typed read-only inventory boundary. Switching remains a separate,
/// permission-gated capability.
pub trait AudioDeviceInventoryProvider {
    fn output_devices(&self) -> ProviderState<Vec<AudioOutputDevice>>;
}

impl Default for PipewireAudioProvider {
    fn default() -> Self {
        Self::new("pactl")
    }
}

impl PipewireAudioProvider {
    /// Use a particular `pactl` executable. Tests can provide a controlled
    /// adapter while production uses the executable resolved by `PATH`.
    #[must_use]
    pub fn new(pactl: impl Into<PathBuf>) -> Self {
        Self {
            pactl: pactl.into(),
        }
    }

    fn snapshot(&self) -> Result<ProviderState<AudioStatus>, PipewireAudioError> {
        let info = run_pactl(&self.pactl, &["--format=json", "info"])?;
        let sinks = run_pactl(&self.pactl, &["--format=json", "list", "sinks"])?;
        map_pactl_snapshot(&info, &sinks)
    }

    fn device_snapshot(&self) -> Result<ProviderState<Vec<AudioOutputDevice>>, PipewireAudioError> {
        let info = run_pactl(&self.pactl, &["--format=json", "info"])?;
        let sinks = run_pactl(&self.pactl, &["--format=json", "list", "sinks"])?;
        map_pactl_devices(&info, &sinks)
    }
}

impl AudioProvider for PipewireAudioProvider {
    fn audio(&self) -> ProviderState<AudioStatus> {
        self.snapshot()
            .unwrap_or_else(|error| ProviderState::Error(error.to_string()))
    }
}

impl AudioDeviceInventoryProvider for PipewireAudioProvider {
    fn output_devices(&self) -> ProviderState<Vec<AudioOutputDevice>> {
        self.device_snapshot()
            .unwrap_or_else(|error| ProviderState::Error(error.to_string()))
    }
}

fn run_pactl(path: &Path, arguments: &[&str]) -> Result<String, PipewireAudioError> {
    let output = Command::new(path)
        .args(arguments)
        .output()
        .map_err(|error| PipewireAudioError(format!("run {}: {error}", path.display())))?;
    if !output.status.success() {
        return Err(PipewireAudioError(format!(
            "{} exited with {}: {}",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| PipewireAudioError(format!("pactl returned non-UTF-8 JSON: {error}")))
}

#[derive(Debug, Deserialize)]
struct PactlInfo {
    server_name: String,
    default_sink_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PactlSink {
    name: String,
    description: String,
    driver: String,
    state: String,
    mute: bool,
    volume: BTreeMap<String, PactlVolume>,
    #[serde(default)]
    ports: Vec<PactlPort>,
    active_port: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PactlVolume {
    value_percent: String,
}

#[derive(Debug, Deserialize)]
struct PactlPort {
    name: String,
    description: String,
    availability: String,
}

fn map_pactl_snapshot(
    info_json: &str,
    sinks_json: &str,
) -> Result<ProviderState<AudioStatus>, PipewireAudioError> {
    let info: PactlInfo = serde_json::from_str(info_json)
        .map_err(|error| PipewireAudioError(format!("parse pactl info JSON: {error}")))?;
    if !info.server_name.contains("PipeWire") {
        return Err(PipewireAudioError(format!(
            "audio server is not PipeWire-backed: {}",
            info.server_name
        )));
    }
    let Some(default_sink) = info.default_sink_name else {
        return Ok(ProviderState::Unavailable);
    };
    let sinks: Vec<PactlSink> = serde_json::from_str(sinks_json)
        .map_err(|error| PipewireAudioError(format!("parse pactl sinks JSON: {error}")))?;
    let sink = sinks
        .into_iter()
        .find(|sink| sink.name == default_sink)
        .ok_or_else(|| {
            PipewireAudioError(format!("default PipeWire sink is missing: {default_sink}"))
        })?;
    if sink.driver != "PipeWire" {
        return Err(PipewireAudioError(format!(
            "default sink is not owned by PipeWire: {}",
            sink.driver
        )));
    }
    if sink.volume.is_empty() {
        return Err(PipewireAudioError(
            "default PipeWire sink has no volume channels".to_owned(),
        ));
    }
    let mut percentages = sink
        .volume
        .values()
        .map(|volume| parse_percentage(&volume.value_percent));
    let first = percentages
        .next()
        .expect("non-empty volume map has a first channel")?;
    let mut total = u32::from(first);
    let mut channel_count = 1_u32;
    for percentage in percentages {
        total += u32::from(percentage?);
        channel_count += 1;
    }
    Ok(ProviderState::Available {
        value: AudioStatus {
            volume_percent: (total / channel_count) as u8,
            muted: sink.mute,
        },
        stale: false,
    })
}

fn map_pactl_devices(
    info_json: &str,
    sinks_json: &str,
) -> Result<ProviderState<Vec<AudioOutputDevice>>, PipewireAudioError> {
    let info: PactlInfo = serde_json::from_str(info_json)
        .map_err(|error| PipewireAudioError(format!("parse pactl info JSON: {error}")))?;
    validate_pipewire_server(&info)?;
    let sinks: Vec<PactlSink> = serde_json::from_str(sinks_json)
        .map_err(|error| PipewireAudioError(format!("parse pactl sinks JSON: {error}")))?;
    if sinks.is_empty() {
        return Ok(ProviderState::Unavailable);
    }

    let mut devices = Vec::with_capacity(sinks.len());
    let mut names = std::collections::BTreeSet::new();
    for sink in sinks {
        validate_text(&sink.name, "PipeWire output name")?;
        validate_text(&sink.description, "PipeWire output description")?;
        if !names.insert(sink.name.clone()) {
            return Err(PipewireAudioError(format!(
                "duplicate PipeWire output name: {}",
                sink.name
            )));
        }
        if sink.driver != "PipeWire" {
            return Err(PipewireAudioError(format!(
                "output is not owned by PipeWire: {} ({})",
                sink.name, sink.driver
            )));
        }
        let state = match sink.state.as_str() {
            "RUNNING" => AudioOutputState::Running,
            "IDLE" => AudioOutputState::Idle,
            "SUSPENDED" => AudioOutputState::Suspended,
            value => {
                return Err(PipewireAudioError(format!(
                    "unsupported PipeWire output state: {value}"
                )));
            }
        };
        let active_port = sink.active_port.as_deref().filter(|port| !port.is_empty());
        if let Some(active_port) = active_port
            && !sink.ports.iter().any(|port| port.name == active_port)
        {
            return Err(PipewireAudioError(format!(
                "active PipeWire port is missing from output {}: {active_port}",
                sink.name
            )));
        }
        let ports = sink
            .ports
            .into_iter()
            .map(|port| {
                validate_text(&port.name, "PipeWire port name")?;
                validate_text(&port.description, "PipeWire port description")?;
                let availability = match port.availability.as_str() {
                    "available" => AudioPortAvailability::Available,
                    "not available" => AudioPortAvailability::Unavailable,
                    "availability unknown" => AudioPortAvailability::Unknown,
                    value => {
                        return Err(PipewireAudioError(format!(
                            "unsupported PipeWire port availability: {value}"
                        )));
                    }
                };
                Ok(AudioOutputPort {
                    active: active_port == Some(port.name.as_str()),
                    name: port.name,
                    description: port.description,
                    availability,
                })
            })
            .collect::<Result<Vec<_>, PipewireAudioError>>()?;
        devices.push(AudioOutputDevice {
            is_default: info.default_sink_name.as_deref() == Some(sink.name.as_str()),
            name: sink.name,
            description: sink.description,
            state,
            ports,
        });
    }

    if let Some(default_sink) = &info.default_sink_name
        && !devices.iter().any(|device| device.name == *default_sink)
    {
        return Err(PipewireAudioError(format!(
            "default PipeWire sink is missing: {default_sink}"
        )));
    }
    Ok(ProviderState::Available {
        value: devices,
        stale: false,
    })
}

fn validate_pipewire_server(info: &PactlInfo) -> Result<(), PipewireAudioError> {
    if !info.server_name.contains("PipeWire") {
        return Err(PipewireAudioError(format!(
            "audio server is not PipeWire-backed: {}",
            info.server_name
        )));
    }
    Ok(())
}

fn validate_text(value: &str, label: &str) -> Result<(), PipewireAudioError> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(PipewireAudioError(format!("invalid {label}")));
    }
    Ok(())
}

fn parse_percentage(value: &str) -> Result<u8, PipewireAudioError> {
    let percentage = value
        .strip_suffix('%')
        .ok_or_else(|| PipewireAudioError(format!("invalid PipeWire volume: {value}")))?
        .parse::<u8>()
        .map_err(|_| PipewireAudioError(format!("invalid PipeWire volume: {value}")))?;
    if percentage > 100 {
        return Err(PipewireAudioError(format!(
            "PipeWire volume exceeds the SOL 0..=100 contract: {percentage}%"
        )));
    }
    Ok(percentage)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = r#"{
        "server_name": "PulseAudio (on PipeWire 1.6.8)",
        "default_sink_name": "sol_output"
    }"#;
    const SINKS: &str = r#"[{
        "name": "sol_output",
        "description": "SOL Speakers",
        "driver": "PipeWire",
        "state": "RUNNING",
        "mute": false,
        "volume": {
            "front-left": {"value_percent": "62%"},
            "front-right": {"value_percent": "64%"}
        },
        "ports": [{
            "name": "analog-output-speaker",
            "description": "Speakers",
            "availability": "available"
        }],
        "active_port": "analog-output-speaker"
    }]"#;

    #[test]
    fn default_pipewire_sink_maps_to_typed_audio_status() {
        assert_eq!(
            map_pactl_snapshot(INFO, SINKS).expect("valid PipeWire snapshot"),
            ProviderState::Available {
                value: AudioStatus {
                    volume_percent: 63,
                    muted: false,
                },
                stale: false,
            }
        );
    }

    #[test]
    fn missing_default_sink_is_explicitly_unavailable() {
        let info = r#"{
            "server_name": "PulseAudio (on PipeWire 1.6.8)",
            "default_sink_name": null
        }"#;
        assert_eq!(
            map_pactl_snapshot(info, "[]").expect("missing sink should be valid"),
            ProviderState::Unavailable
        );
    }

    #[test]
    fn malformed_or_non_pipewire_snapshots_are_rejected() {
        assert!(map_pactl_snapshot("{}", SINKS).is_err());
        assert!(
            map_pactl_snapshot(
                r#"{"server_name":"PulseAudio","default_sink_name":"sol_output"}"#,
                SINKS
            )
            .is_err()
        );
        assert!(map_pactl_snapshot(INFO, "[]").is_err());
        assert!(map_pactl_snapshot(INFO, &SINKS.replace("64%", "164%")).is_err());
    }

    #[test]
    fn output_inventory_maps_default_state_and_ports() {
        assert_eq!(
            map_pactl_devices(INFO, SINKS).expect("valid device inventory"),
            ProviderState::Available {
                value: vec![AudioOutputDevice {
                    name: "sol_output".to_owned(),
                    description: "SOL Speakers".to_owned(),
                    state: AudioOutputState::Running,
                    is_default: true,
                    ports: vec![AudioOutputPort {
                        name: "analog-output-speaker".to_owned(),
                        description: "Speakers".to_owned(),
                        availability: AudioPortAvailability::Available,
                        active: true,
                    }],
                }],
                stale: false,
            }
        );
    }

    #[test]
    fn device_inventory_rejects_ambiguous_or_unknown_backend_data() {
        let duplicate = format!(
            "[{},{}]",
            &SINKS[1..SINKS.len() - 1],
            &SINKS[1..SINKS.len() - 1]
        );
        assert!(map_pactl_devices(INFO, &duplicate).is_err());
        assert!(map_pactl_devices(INFO, &SINKS.replace("RUNNING", "BROKEN")).is_err());
        assert!(
            map_pactl_devices(INFO, &SINKS.replace("available", "sometimes available")).is_err()
        );
        assert!(
            map_pactl_devices(
                INFO,
                &SINKS.replace(
                    "\"active_port\": \"analog-output-speaker\"",
                    "\"active_port\": \"missing-port\""
                )
            )
            .is_err()
        );
        assert!(map_pactl_devices(INFO, &SINKS.replace("PipeWire", "LegacyAudio")).is_err());
    }
}
