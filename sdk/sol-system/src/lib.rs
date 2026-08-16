//! Restricted, typed system APIs for SOL applications.
//!
//! The settings surface deliberately describes user intent rather than a
//! storage format or transport.  First-party applications can depend on this
//! crate while `sol-settingsd` remains free to use an in-memory store in tests,
//! a file today, and an IPC-backed implementation later.

use std::error::Error;
use std::fmt;

/// Result returned by the settings API.
pub type SettingsResult<T> = Result<T, SettingsError>;

/// An error returned while reading or changing settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    /// The requested value is outside the range supported by the API.
    InvalidValue(&'static str),
    /// The settings implementation could not complete the operation.
    Backend(String),
}

impl SettingsError {
    /// Construct an error reported by the backing service or transport.
    #[must_use]
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend(message.into())
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue(message) => write!(formatter, "invalid settings value: {message}"),
            Self::Backend(message) => write!(formatter, "settings backend error: {message}"),
        }
    }
}

impl Error for SettingsError {}

/// The preferred colour-scheme policy for SOL surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    /// Follow the platform's current appearance policy.
    #[default]
    System,
    /// Prefer the light appearance.
    Light,
    /// Prefer the dark appearance.
    Dark,
}

impl ColorScheme {
    /// Return the stable storage spelling for this policy.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Appearance settings currently shared by Settings and Quick Settings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppearanceSettings {
    /// The preferred colour-scheme policy.
    pub color_scheme: ColorScheme,
}

/// An output-volume percentage validated by the typed API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputVolume(u8);

impl OutputVolume {
    /// The quietest output level.
    pub const MIN: Self = Self(0);
    /// The default output level for a new profile.
    pub const DEFAULT: Self = Self(50);
    /// The loudest output level supported by this API.
    pub const MAX: Self = Self(100);

    /// Create an output-volume percentage.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::InvalidValue`] when `percent` exceeds 100.
    pub const fn new(percent: u8) -> SettingsResult<Self> {
        if percent <= Self::MAX.0 {
            Ok(Self(percent))
        } else {
            Err(SettingsError::InvalidValue(
                "output volume must be between 0 and 100",
            ))
        }
    }

    /// Return this volume as a percentage.
    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }
}

impl Default for OutputVolume {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Audio-output settings intended for the first Settings UI increment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSettings {
    /// The requested output volume.
    pub output_volume: OutputVolume,
    /// Whether output should be muted without losing its requested volume.
    pub output_muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            output_volume: OutputVolume::DEFAULT,
            output_muted: false,
        }
    }
}

/// A coherent, versioned view of settings returned by [`SettingsApi`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SettingsSnapshot {
    /// Monotonically increasing revision assigned by the settings service.
    pub revision: u64,
    /// Appearance preferences.
    pub appearance: AppearanceSettings,
    /// Audio-output preferences.
    pub audio: AudioSettings,
}

/// One typed setting change accepted by [`SettingsApi::apply`].
///
/// New settings are added as explicit variants.  This keeps the public API
/// discoverable and prevents a string-key/value protocol from leaking into the
/// Settings UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsChange {
    /// Update the requested colour-scheme policy.
    SetColorScheme(ColorScheme),
    /// Update the requested output volume.
    SetOutputVolume(OutputVolume),
    /// Mute or unmute the current output.
    SetOutputMuted(bool),
}

/// Minimal stable boundary between a Settings UI and its implementation.
///
/// Implementations may live in-process for tests or proxy to `sol-settingsd`
/// over IPC.  Callers never receive a storage path or storage-format detail.
pub trait SettingsApi: Send + Sync {
    /// Return a coherent snapshot of the current settings.
    fn snapshot(&self) -> SettingsResult<SettingsSnapshot>;

    /// Persist one typed user-intent change and return the resulting snapshot.
    fn apply(&self, change: SettingsChange) -> SettingsResult<SettingsSnapshot>;
}

#[cfg(test)]
mod tests {
    use super::{
        ColorScheme, OutputVolume, SettingsApi, SettingsChange, SettingsResult, SettingsSnapshot,
    };
    use std::sync::Mutex;

    struct MockSettingsApi {
        snapshot: Mutex<SettingsSnapshot>,
    }

    impl MockSettingsApi {
        fn new() -> Self {
            Self {
                snapshot: Mutex::new(SettingsSnapshot::default()),
            }
        }
    }

    impl SettingsApi for MockSettingsApi {
        fn snapshot(&self) -> SettingsResult<SettingsSnapshot> {
            Ok(self
                .snapshot
                .lock()
                .map_err(|error| {
                    super::SettingsError::backend(format!("mock settings lock poisoned: {error}"))
                })?
                .clone())
        }

        fn apply(&self, change: SettingsChange) -> SettingsResult<SettingsSnapshot> {
            let mut snapshot = self.snapshot.lock().map_err(|error| {
                super::SettingsError::backend(format!("mock settings lock poisoned: {error}"))
            })?;
            match change {
                SettingsChange::SetColorScheme(color_scheme) => {
                    snapshot.appearance.color_scheme = color_scheme;
                }
                SettingsChange::SetOutputVolume(output_volume) => {
                    snapshot.audio.output_volume = output_volume;
                }
                SettingsChange::SetOutputMuted(output_muted) => {
                    snapshot.audio.output_muted = output_muted;
                }
            }
            snapshot.revision += 1;
            Ok(snapshot.clone())
        }
    }

    #[test]
    fn mock_settings_api_supports_a_typed_round_trip() {
        let settings = MockSettingsApi::new();

        settings
            .apply(SettingsChange::SetColorScheme(ColorScheme::Dark))
            .expect("mock settings change should succeed");
        let volume = OutputVolume::new(72).expect("valid volume should construct");
        let changed = settings
            .apply(SettingsChange::SetOutputVolume(volume))
            .expect("mock settings change should succeed");

        assert_eq!(changed.revision, 2);
        assert_eq!(changed.appearance.color_scheme, ColorScheme::Dark);
        assert_eq!(changed.audio.output_volume.percent(), 72);
        assert_eq!(
            settings.snapshot().expect("snapshot should succeed"),
            changed
        );
    }

    #[test]
    fn output_volume_rejects_values_above_100_percent() {
        assert!(OutputVolume::new(101).is_err());
    }
}
