use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::warn;

use crate::routing::AudioDeviceType;

/// Configuration loaded from ~/.config/sol/audiod.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub routing: RoutingConfig,

    #[serde(default)]
    pub bluetooth: BluetoothConfig,

    #[serde(default)]
    pub devices: HashMap<String, DeviceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Enable auto-switching for headphones
    #[serde(default = "default_true")]
    pub auto_switch_headphones: bool,

    /// Enable auto-switching for speakers
    #[serde(default)]
    pub auto_switch_speakers: bool,

    /// Enable auto-switching for wired devices
    #[serde(default = "default_true")]
    pub auto_switch_wired: bool,

    /// Crossfade duration in milliseconds
    #[serde(default = "default_crossfade_ms")]
    pub crossfade_duration_ms: u64,

    /// Detect shared usage scenarios
    #[serde(default = "default_true")]
    pub detect_shared_usage: bool,

    /// Battery-aware routing
    #[serde(default = "default_true")]
    pub battery_aware: bool,

    /// Per-device priority boosts
    #[serde(default)]
    pub priority_boosts: HashMap<String, i16>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            auto_switch_headphones: true,
            auto_switch_speakers: false,
            auto_switch_wired: true,
            crossfade_duration_ms: 300,
            detect_shared_usage: true,
            battery_aware: true,
            priority_boosts: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothConfig {
    /// Prefer specific codec (ldac > aptx > aac > sbc)
    #[serde(default = "default_codec")]
    pub prefer_codec: String,

    /// Auto-reconnect to last used device
    #[serde(default = "default_true")]
    pub auto_reconnect_last_device: bool,

    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_sec: u64,
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            prefer_codec: "ldac".to_string(),
            auto_reconnect_last_device: true,
            connection_timeout_sec: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Device name
    pub name: String,

    /// Device type (manual classification)
    #[serde(rename = "type")]
    pub device_type: Option<String>,

    /// Enable auto-switch for this device
    #[serde(default = "default_true")]
    pub auto_switch: bool,

    /// Mark device as trusted
    #[serde(default)]
    pub trusted: bool,

    /// Classification source
    #[serde(default)]
    pub classification_source: Option<String>,

    /// Last used timestamp (ISO 8601)
    pub last_used: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_crossfade_ms() -> u64 {
    300
}

fn default_codec() -> String {
    "ldac".to_string()
}

fn default_connection_timeout() -> u64 {
    5
}

impl Config {
    /// Load configuration from file, falling back to defaults
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if !config_path.exists() {
            warn!("Config file not found at {:?}, using defaults", config_path);
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();

        // Ensure directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(&config_path, content)?;

        Ok(())
    }

    /// Get config file path
    fn config_path() -> PathBuf {
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").expect("HOME not set");
                PathBuf::from(home).join(".config")
            });

        config_dir.join("sol").join("audiod.toml")
    }

    /// Parse device type string to enum
    pub fn parse_device_type(s: &str) -> Option<AudioDeviceType> {
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.routing.auto_switch_headphones);
        assert!(!config.routing.auto_switch_speakers);
        assert_eq!(config.routing.crossfade_duration_ms, 300);
    }

    #[test]
    fn test_parse_device_type() {
        assert_eq!(
            Config::parse_device_type("headphones"),
            Some(AudioDeviceType::Headphones)
        );
        assert_eq!(
            Config::parse_device_type("wired-headphones"),
            Some(AudioDeviceType::WiredHeadphones)
        );
        assert_eq!(
            Config::parse_device_type("speaker"),
            Some(AudioDeviceType::Speaker)
        );
        assert_eq!(Config::parse_device_type("invalid"), None);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml).unwrap();

        assert_eq!(
            config.routing.auto_switch_headphones,
            parsed.routing.auto_switch_headphones
        );
    }
}
