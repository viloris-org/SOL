use serde::{Deserialize, Serialize};

/// Audio device classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioDeviceType {
    /// Wired over-ear or on-ear headphones (3.5mm/USB)
    WiredHeadphones,
    /// Wired speakers (3.5mm/USB)
    WiredSpeaker,
    /// Bluetooth/wireless earbuds
    Earbuds,
    /// Bluetooth/wireless over-ear headphones
    Headphones,
    /// Car audio system
    CarAudio,
    /// Bluetooth/wireless speaker (portable or desktop)
    Speaker,
    /// Soundbar or home theater speaker
    Soundbar,
    /// HDMI audio output (monitor/TV speakers)
    HDMI,
    /// Built-in laptop/device speakers
    BuiltinSpeaker,
    /// Unknown or unclassified device
    Unknown,
}

impl AudioDeviceType {
    /// Stable machine-readable name used by configuration and D-Bus.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WiredHeadphones => "wired-headphones",
            Self::WiredSpeaker => "wired-speaker",
            Self::Earbuds => "earbuds",
            Self::Headphones => "headphones",
            Self::CarAudio => "car-audio",
            Self::Speaker => "speaker",
            Self::Soundbar => "soundbar",
            Self::HDMI => "hdmi",
            Self::BuiltinSpeaker => "builtin-speaker",
            Self::Unknown => "unknown",
        }
    }

    /// Base priority for automatic routing (higher = more preferred)
    pub fn base_priority(&self) -> u8 {
        match self {
            // Wired = explicit physical action
            Self::WiredHeadphones => 100,
            Self::WiredSpeaker => 95,

            // Bluetooth headphones = personal device, privacy intent
            Self::Earbuds => 80,
            Self::Headphones => 75,

            // Situational audio
            Self::CarAudio => 60,

            // Shared speakers = don't auto-switch
            Self::Speaker => 40,
            Self::Soundbar => 35,

            // HDMI = passive (connected to display)
            Self::HDMI => 20,

            // Builtin = fallback only
            Self::BuiltinSpeaker => 10,

            Self::Unknown => 0,
        }
    }

    /// Whether this device type is personal (headphones) vs shared (speakers)
    pub fn is_personal(&self) -> bool {
        matches!(
            self,
            Self::WiredHeadphones | Self::Earbuds | Self::Headphones
        )
    }

    /// Whether this device type is portable
    pub fn is_portable(&self) -> bool {
        matches!(self, Self::Earbuds | Self::Headphones | Self::Speaker)
    }

    /// Whether this device typically supports calls/microphone
    pub fn supports_calls(&self) -> bool {
        matches!(
            self,
            Self::WiredHeadphones | Self::Earbuds | Self::Headphones | Self::CarAudio
        )
    }
}

impl std::str::FromStr for AudioDeviceType {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "wired_headphones" | "wired-headphones" => Ok(Self::WiredHeadphones),
            "wired_speaker" | "wired-speaker" => Ok(Self::WiredSpeaker),
            "earbuds" => Ok(Self::Earbuds),
            "headphones" => Ok(Self::Headphones),
            "car_audio" | "car-audio" => Ok(Self::CarAudio),
            "speaker" => Ok(Self::Speaker),
            "soundbar" => Ok(Self::Soundbar),
            "hdmi" => Ok(Self::HDMI),
            "builtin_speaker" | "builtin-speaker" => Ok(Self::BuiltinSpeaker),
            "unknown" => Ok(Self::Unknown),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for AudioDeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WiredHeadphones => write!(f, "Wired Headphones"),
            Self::WiredSpeaker => write!(f, "Wired Speaker"),
            Self::Earbuds => write!(f, "Earbuds"),
            Self::Headphones => write!(f, "Headphones"),
            Self::CarAudio => write!(f, "Car Audio"),
            Self::Speaker => write!(f, "Speaker"),
            Self::Soundbar => write!(f, "Soundbar"),
            Self::HDMI => write!(f, "HDMI Audio"),
            Self::BuiltinSpeaker => write!(f, "Built-in Speaker"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}
