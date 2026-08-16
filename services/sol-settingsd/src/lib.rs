//! Settings service core and its storage boundary.
//!
//! Transport is intentionally outside this crate's core: a future D-Bus
//! adapter can implement or delegate to [`SettingsApi`] without changing the
//! typed API used by first-party applications.

pub mod dbus;

use sol_system::{
    ColorScheme, OutputVolume, SettingsApi, SettingsChange, SettingsError, SettingsResult,
    SettingsSnapshot, TextScale,
};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const STORAGE_VERSION: u32 = 1;

/// Durable backing store used by [`SettingsDaemon`].
///
/// Stores own serialization and I/O.  The daemon owns validation, revisions,
/// and the typed settings API, so neither concern leaks into client code.
pub trait SettingsStore: Send + Sync {
    /// Load the most recently persisted snapshot, if a profile exists.
    fn load(&self) -> SettingsResult<Option<SettingsSnapshot>>;

    /// Persist a fully validated snapshot.
    fn save(&self, snapshot: &SettingsSnapshot) -> SettingsResult<()>;
}

/// In-memory store useful for unit tests and embedded development.
#[derive(Debug, Default)]
pub struct MemorySettingsStore {
    snapshot: Mutex<Option<SettingsSnapshot>>,
}

impl MemorySettingsStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SettingsStore for MemorySettingsStore {
    fn load(&self) -> SettingsResult<Option<SettingsSnapshot>> {
        self.snapshot
            .lock()
            .map_err(|error| {
                SettingsError::backend(format!("settings store lock poisoned: {error}"))
            })
            .map(|snapshot| snapshot.clone())
    }

    fn save(&self, snapshot: &SettingsSnapshot) -> SettingsResult<()> {
        let mut stored = self.snapshot.lock().map_err(|error| {
            SettingsError::backend(format!("settings store lock poisoned: {error}"))
        })?;
        *stored = Some(snapshot.clone());
        Ok(())
    }
}

/// Line-oriented, atomically replaced settings file.
///
/// The format is a daemon implementation detail.  It is intentionally simple
/// while Phase 2 establishes the API boundary, and can be migrated behind
/// [`SettingsStore`] without changing Settings UI clients.
#[derive(Debug, Clone)]
pub struct FileSettingsStore {
    path: PathBuf,
}

impl FileSettingsStore {
    /// Create a store backed by `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Return the file path used by this store.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SettingsStore for FileSettingsStore {
    fn load(&self) -> SettingsResult<Option<SettingsSnapshot>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("read settings", error)),
        };

        parse_snapshot(&contents).map(Some)
    }

    fn save(&self, snapshot: &SettingsSnapshot) -> SettingsResult<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| io_error("create settings directory", error))?;

        let temporary_path = temporary_path(&self.path)?;
        let write_result = write_snapshot(&temporary_path, snapshot);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }

        fs::rename(&temporary_path, &self.path).map_err(|error| io_error("replace settings", error))
    }
}

/// Settings service implementation with write-through persistence.
#[derive(Debug)]
pub struct SettingsDaemon<S> {
    store: S,
    snapshot: RwLock<SettingsSnapshot>,
}

impl<S: SettingsStore> SettingsDaemon<S> {
    /// Initialize the daemon from its store, using API defaults for a new user.
    ///
    /// # Errors
    ///
    /// Returns an error when the existing stored snapshot cannot be read.
    pub fn new(store: S) -> SettingsResult<Self> {
        let snapshot = store.load()?.unwrap_or_default();
        Ok(Self {
            store,
            snapshot: RwLock::new(snapshot),
        })
    }

    /// Return the backing store for service setup and diagnostics.
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }
}

impl<S: SettingsStore> SettingsApi for SettingsDaemon<S> {
    fn snapshot(&self) -> SettingsResult<SettingsSnapshot> {
        self.snapshot
            .read()
            .map_err(|error| {
                SettingsError::backend(format!("settings state lock poisoned: {error}"))
            })
            .map(|snapshot| snapshot.clone())
    }

    fn apply(&self, change: SettingsChange) -> SettingsResult<SettingsSnapshot> {
        let mut current = self.snapshot.write().map_err(|error| {
            SettingsError::backend(format!("settings state lock poisoned: {error}"))
        })?;
        let mut next = current.clone();

        match change {
            SettingsChange::SetColorScheme(color_scheme) => {
                next.appearance.color_scheme = color_scheme;
            }
            SettingsChange::SetHighContrast(high_contrast) => {
                next.appearance.high_contrast = high_contrast
            }
            SettingsChange::SetReducedMotion(reduced_motion) => {
                next.appearance.reduced_motion = reduced_motion
            }
            SettingsChange::SetTextScale(text_scale) => next.appearance.text_scale = text_scale,
            SettingsChange::SetOutputVolume(output_volume) => {
                next.audio.output_volume = output_volume;
            }
            SettingsChange::SetOutputMuted(output_muted) => {
                next.audio.output_muted = output_muted;
            }
        }
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| SettingsError::backend("settings revision overflow"))?;

        self.store.save(&next)?;
        *current = next.clone();
        Ok(next)
    }
}

fn write_snapshot(path: &Path, snapshot: &SettingsSnapshot) -> SettingsResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_error("create temporary settings", error))?;
    file.write_all(serialize_snapshot(snapshot).as_bytes())
        .map_err(|error| io_error("write temporary settings", error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary settings", error))
}

fn temporary_path(path: &Path) -> SettingsResult<PathBuf> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| SettingsError::backend("settings path must have a UTF-8 file name"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            SettingsError::backend(format!("system clock is before Unix epoch: {error}"))
        })?
        .as_nanos();
    Ok(path.with_file_name(format!(".{filename}.tmp-{}-{nonce}", std::process::id())))
}

fn serialize_snapshot(snapshot: &SettingsSnapshot) -> String {
    format!(
        "# SOL settings storage; format version {STORAGE_VERSION}\nversion={STORAGE_VERSION}\nrevision={}\ncolor_scheme={}\nhigh_contrast={}\nreduced_motion={}\ntext_scale={}\noutput_volume={}\noutput_muted={}\n",
        snapshot.revision,
        snapshot.appearance.color_scheme.as_str(),
        snapshot.appearance.high_contrast,
        snapshot.appearance.reduced_motion,
        snapshot.appearance.text_scale.as_str(),
        snapshot.audio.output_volume.percent(),
        snapshot.audio.output_muted,
    )
}

fn parse_snapshot(contents: &str) -> SettingsResult<SettingsSnapshot> {
    let mut version = None;
    let mut revision = None;
    let mut color_scheme = None;
    let mut high_contrast = None;
    let mut reduced_motion = None;
    let mut text_scale = None;
    let mut output_volume = None;
    let mut output_muted = None;

    for line in contents.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| SettingsError::backend("invalid settings storage line"))?;
        match key {
            "version" => version = Some(parse_value(value, "storage version")?),
            "revision" => revision = Some(parse_value(value, "revision")?),
            "color_scheme" => color_scheme = Some(parse_color_scheme(value)?),
            "high_contrast" => high_contrast = Some(parse_value(value, "high contrast")?),
            "reduced_motion" => reduced_motion = Some(parse_value(value, "reduced motion")?),
            "text_scale" => text_scale = Some(parse_text_scale(value)?),
            "output_volume" => {
                let percent = parse_value(value, "output volume")?;
                output_volume = Some(OutputVolume::new(percent)?);
            }
            "output_muted" => output_muted = Some(parse_value(value, "output muted")?),
            _ => {}
        }
    }

    match version {
        Some(STORAGE_VERSION) => {}
        Some(version) => {
            return Err(SettingsError::backend(format!(
                "unsupported settings storage version {version}"
            )));
        }
        None => return Err(SettingsError::backend("settings storage has no version")),
    }

    Ok(SettingsSnapshot {
        revision: revision
            .ok_or_else(|| SettingsError::backend("settings storage has no revision"))?,
        appearance: sol_system::AppearanceSettings {
            color_scheme: color_scheme
                .ok_or_else(|| SettingsError::backend("settings storage has no color scheme"))?,
            high_contrast: high_contrast.unwrap_or(false),
            reduced_motion: reduced_motion.unwrap_or(false),
            text_scale: text_scale.unwrap_or_default(),
        },
        audio: sol_system::AudioSettings {
            output_volume: output_volume
                .ok_or_else(|| SettingsError::backend("settings storage has no output volume"))?,
            output_muted: output_muted.ok_or_else(|| {
                SettingsError::backend("settings storage has no output mute state")
            })?,
        },
    })
}

fn parse_value<T>(value: &str, label: &str) -> SettingsResult<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| SettingsError::backend(format!("invalid {label} in settings storage")))
}

fn parse_color_scheme(value: &str) -> SettingsResult<ColorScheme> {
    match value {
        "system" => Ok(ColorScheme::System),
        "light" => Ok(ColorScheme::Light),
        "dark" => Ok(ColorScheme::Dark),
        _ => Err(SettingsError::backend(
            "invalid color scheme in settings storage",
        )),
    }
}

fn parse_text_scale(value: &str) -> SettingsResult<TextScale> {
    match value {
        "default" => Ok(TextScale::Default),
        "large" => Ok(TextScale::Large),
        "extra-large" => Ok(TextScale::ExtraLarge),
        _ => Err(SettingsError::backend(
            "invalid text scale in settings storage",
        )),
    }
}

fn io_error(action: &str, error: io::Error) -> SettingsError {
    SettingsError::backend(format!("could not {action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{FileSettingsStore, MemorySettingsStore, SettingsDaemon, SettingsStore};
    use sol_system::{ColorScheme, OutputVolume, SettingsApi, SettingsChange, TextScale};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn service_round_trip_writes_through_its_store() {
        let store = MemorySettingsStore::new();
        let daemon = SettingsDaemon::new(store).expect("empty memory store should initialize");

        daemon
            .apply(SettingsChange::SetColorScheme(ColorScheme::Dark))
            .expect("color scheme update should succeed");
        let expected = daemon
            .apply(SettingsChange::SetOutputVolume(
                OutputVolume::new(64).expect("valid volume should construct"),
            ))
            .expect("volume update should succeed");

        let persisted = daemon
            .store()
            .load()
            .expect("memory store should load")
            .expect("service should have persisted a snapshot");
        assert_eq!(persisted, expected);
        assert_eq!(persisted.revision, 2);
    }

    #[test]
    fn file_store_round_trips_a_service_snapshot() {
        let path = temporary_test_path();
        let store = FileSettingsStore::new(&path);
        let daemon = SettingsDaemon::new(store).expect("new file store should initialize");
        daemon
            .apply(SettingsChange::SetHighContrast(true))
            .expect("contrast update should succeed");
        let expected = daemon
            .apply(SettingsChange::SetTextScale(TextScale::Large))
            .expect("mute update should succeed");

        let reloaded = SettingsDaemon::new(FileSettingsStore::new(&path))
            .expect("persisted file should reload")
            .snapshot()
            .expect("reloaded daemon should return a snapshot");
        assert_eq!(reloaded, expected);
        assert!(reloaded.appearance.high_contrast);
        assert_eq!(reloaded.appearance.text_scale, TextScale::Large);

        fs::remove_file(path).expect("test settings file should be removable");
    }

    fn temporary_test_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sol-settingsd-test-{}-{nonce}.conf",
            std::process::id()
        ))
    }
}
