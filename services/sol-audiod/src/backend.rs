//! Audio-server boundary used by routing policy and the D-Bus control plane.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use thiserror::Error;
use tracing::warn;

use crate::{bluetooth::classify_from_name, routing::AudioDeviceType};

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("cannot run {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{program} exited with {status}: {stderr}")]
    Command {
        program: String,
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error("{program} returned non-UTF-8 output")]
    NonUtf8 { program: String },
    #[error("invalid PipeWire response: {0}")]
    InvalidResponse(String),
    #[error("invalid output device id")]
    InvalidDeviceId,
}

pub type BackendResult<T> = Result<T, BackendError>;

/// An output endpoint projected from the audio server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendOutput {
    pub id: String,
    pub name: String,
    pub device_type: AudioDeviceType,
    pub is_default: bool,
}

/// Narrow backend contract. Routing policy remains independent from PipeWire.
pub trait AudioBackend: Send + Sync {
    fn list_outputs(&self) -> BackendResult<Vec<BackendOutput>>;
    fn set_default_output(&self, device_id: &str) -> BackendResult<()>;
}

/// PipeWire adapter using the structured Pulse compatibility API.
///
/// `pactl` is deliberately treated as a replaceable adapter rather than leaked
/// into the daemon's public API. Existing sink inputs are moved as part of a
/// route change, so the operation affects already-running applications too.
#[derive(Debug, Clone)]
pub struct PipeWireBackend {
    pactl: PathBuf,
}

impl Default for PipeWireBackend {
    fn default() -> Self {
        Self::new("pactl")
    }
}

impl PipeWireBackend {
    #[must_use]
    pub fn new(pactl: impl Into<PathBuf>) -> Self {
        Self {
            pactl: pactl.into(),
        }
    }

    fn run(&self, arguments: &[&str]) -> BackendResult<String> {
        run_command(&self.pactl, arguments)
    }
}

impl AudioBackend for PipeWireBackend {
    fn list_outputs(&self) -> BackendResult<Vec<BackendOutput>> {
        let info = self.run(&["--format=json", "info"])?;
        let sinks = self.run(&["--format=json", "list", "sinks"])?;
        map_outputs(&info, &sinks)
    }

    fn set_default_output(&self, device_id: &str) -> BackendResult<()> {
        validate_device_id(device_id)?;

        // Capture inputs first. If inventory fails, the default route is left
        // untouched rather than returning after a partial operation.
        let inputs_json = self.run(&["--format=json", "list", "sink-inputs"])?;
        let inputs: Vec<PactlSinkInput> = serde_json::from_str(&inputs_json)
            .map_err(|error| BackendError::InvalidResponse(error.to_string()))?;

        self.run(&["set-default-sink", device_id])?;
        for input in inputs {
            if let Err(error) = self.run(&["move-sink-input", &input.index.to_string(), device_id])
            {
                // The default route already changed. A stream can disappear
                // between inventory and migration, so keep control-plane state
                // aligned with PipeWire and report this secondary failure.
                warn!(index = input.index, %error, "failed to migrate a sink input");
            }
        }
        Ok(())
    }
}

fn run_command(path: &Path, arguments: &[&str]) -> BackendResult<String> {
    let output = Command::new(path)
        .args(arguments)
        .output()
        .map_err(|source| BackendError::Spawn {
            program: path.display().to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(BackendError::Command {
            program: path.display().to_string(),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    String::from_utf8(output.stdout).map_err(|_| BackendError::NonUtf8 {
        program: path.display().to_string(),
    })
}

fn validate_device_id(device_id: &str) -> BackendResult<()> {
    if device_id.is_empty() || device_id.len() > 512 || device_id.chars().any(char::is_control) {
        return Err(BackendError::InvalidDeviceId);
    }
    Ok(())
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
    #[serde(default)]
    properties: HashMap<String, String>,
    active_port: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PactlSinkInput {
    index: u32,
}

fn map_outputs(info_json: &str, sinks_json: &str) -> BackendResult<Vec<BackendOutput>> {
    let info: PactlInfo = serde_json::from_str(info_json)
        .map_err(|error| BackendError::InvalidResponse(error.to_string()))?;
    if !info.server_name.contains("PipeWire") {
        return Err(BackendError::InvalidResponse(format!(
            "audio server is not PipeWire-backed: {}",
            info.server_name
        )));
    }
    let sinks: Vec<PactlSink> = serde_json::from_str(sinks_json)
        .map_err(|error| BackendError::InvalidResponse(error.to_string()))?;
    let mut seen = std::collections::HashSet::new();
    let mut outputs = Vec::with_capacity(sinks.len());
    for sink in sinks {
        validate_device_id(&sink.name)?;
        if sink.description.is_empty() || sink.description.chars().any(char::is_control) {
            return Err(BackendError::InvalidResponse(
                "output description is empty or contains control characters".to_owned(),
            ));
        }
        if sink.driver != "PipeWire" {
            return Err(BackendError::InvalidResponse(format!(
                "output {} is not owned by PipeWire",
                sink.name
            )));
        }
        if !seen.insert(sink.name.clone()) {
            return Err(BackendError::InvalidResponse(format!(
                "duplicate output id: {}",
                sink.name
            )));
        }
        let device_type = classify_sink(&sink);
        outputs.push(BackendOutput {
            is_default: info.default_sink_name.as_deref() == Some(sink.name.as_str()),
            id: sink.name,
            name: sink.description,
            device_type,
        });
    }
    if let Some(default) = info.default_sink_name
        && !outputs.iter().any(|output| output.id == default)
    {
        return Err(BackendError::InvalidResponse(format!(
            "default output is missing from inventory: {default}"
        )));
    }
    Ok(outputs)
}

fn classify_sink(sink: &PactlSink) -> AudioDeviceType {
    let bus = sink.properties.get("device.bus").map(String::as_str);
    let form_factor = sink
        .properties
        .get("device.form_factor")
        .map(|value| value.to_ascii_lowercase());
    let port = sink
        .active_port
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bluetooth = bus == Some("bluetooth") || sink.name.starts_with("bluez_");

    if port.contains("hdmi") || port.contains("displayport") {
        return AudioDeviceType::HDMI;
    }
    if let Some(kind) = classify_from_name(&sink.description) {
        return kind;
    }
    match form_factor.as_deref() {
        Some("headphone" | "headset" | "hands-free") => {
            if bluetooth {
                AudioDeviceType::Headphones
            } else {
                AudioDeviceType::WiredHeadphones
            }
        }
        Some("speaker") if bluetooth => AudioDeviceType::Speaker,
        Some("speaker") => AudioDeviceType::BuiltinSpeaker,
        Some("car") => AudioDeviceType::CarAudio,
        _ if port.contains("headphone") => AudioDeviceType::WiredHeadphones,
        _ if port.contains("speaker") => AudioDeviceType::BuiltinSpeaker,
        _ => AudioDeviceType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = r#"{
        "server_name": "PulseAudio (on PipeWire 1.6.8)",
        "default_sink_name": "alsa_output.speaker"
    }"#;
    const SINKS: &str = r#"[
      {
        "name": "alsa_output.speaker",
        "description": "Built-in Audio",
        "driver": "PipeWire",
        "properties": {"device.bus":"pci", "device.form_factor":"speaker"},
        "active_port": "analog-output-speaker"
      },
      {
        "name": "bluez_output.00_11_22_33_44_55.1",
        "description": "Sony WH-1000XM5",
        "driver": "PipeWire",
        "properties": {"device.bus":"bluetooth"},
        "active_port": null
      }
    ]"#;

    #[test]
    fn maps_and_classifies_pipewire_outputs() {
        let outputs = map_outputs(INFO, SINKS).expect("valid fixture");
        assert_eq!(outputs.len(), 2);
        assert!(outputs[0].is_default);
        assert_eq!(outputs[0].device_type, AudioDeviceType::BuiltinSpeaker);
        assert_eq!(outputs[1].device_type, AudioDeviceType::Headphones);
    }

    #[test]
    fn rejects_non_pipewire_and_dangling_default() {
        assert!(map_outputs(&INFO.replace("PipeWire", "PulseAudio"), SINKS).is_err());
        assert!(map_outputs(&INFO.replace("alsa_output.speaker", "missing"), SINKS).is_err());
    }

    #[test]
    fn rejects_unsafe_device_ids() {
        assert!(validate_device_id("").is_err());
        assert!(validate_device_id("output\nnext").is_err());
        assert!(validate_device_id("alsa_output.safe").is_ok());
    }
}
