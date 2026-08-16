//! Restricted, typed system APIs for SOL applications.
//!
//! The settings surface deliberately describes user intent rather than a
//! storage format or transport.  First-party applications can depend on this
//! crate while `sol-settingsd` remains free to use an in-memory store in tests,
//! a file today, and an IPC-backed implementation later.

use std::collections::HashSet;
use std::error::Error;
use std::fmt;

pub use sol_app::AppId;

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

/// Named user text-size preference shared with SolUI accessibility mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextScale {
    /// SOL baseline text size.
    #[default]
    Default,
    /// Larger standard reading size.
    Large,
    /// Largest standard reading size.
    ExtraLarge,
}

impl TextScale {
    /// Stable storage spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Large => "large",
            Self::ExtraLarge => "extra-large",
        }
    }
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
    /// Request stronger contrast for native SOL surfaces.
    pub high_contrast: bool,
    /// Request reduced non-essential motion.
    pub reduced_motion: bool,
    /// Named text size preference.
    pub text_scale: TextScale,
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
    /// Enable or disable high-contrast rendering.
    SetHighContrast(bool),
    /// Enable or disable reduced motion.
    SetReducedMotion(bool),
    /// Update the named text-size preference.
    SetTextScale(TextScale),
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

/// Result returned by the notification API.
pub type NotificationResult<T> = Result<T, NotificationError>;

/// An error returned while publishing or querying notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationError {
    /// The request cannot be represented by the stable notification contract.
    InvalidRequest(&'static str),
    /// The requested notification does not exist.
    NotFound(NotificationId),
    /// Only the application that created a notification may replace it.
    ReplacementOwnerMismatch,
    /// An action cannot be invoked after a notification has left the active state.
    NotActive(NotificationId),
    /// The requested action is not part of the notification.
    UnknownAction(NotificationActionId),
    /// The backing service or transport could not complete the operation.
    Backend(String),
}

impl NotificationError {
    /// Construct an error reported by the backing service or transport.
    #[must_use]
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend(message.into())
    }
}

impl fmt::Display for NotificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => {
                write!(formatter, "invalid notification request: {message}")
            }
            Self::NotFound(id) => write!(formatter, "notification {id} was not found"),
            Self::ReplacementOwnerMismatch => {
                formatter.write_str("an application may only replace its own notification")
            }
            Self::NotActive(id) => write!(formatter, "notification {id} is not active"),
            Self::UnknownAction(id) => write!(formatter, "notification action {id} was not found"),
            Self::Backend(message) => write!(formatter, "notification backend error: {message}"),
        }
    }
}

impl Error for NotificationError {}

/// A daemon-assigned notification identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NotificationId(u64);

impl NotificationId {
    /// Return the stable numeric value used by transports and storage adapters.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Construct an ID returned by a trusted storage or transport adapter.
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for NotificationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Priority requested by the emitting application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum NotificationUrgency {
    /// Informational, non-interrupting notification.
    Low,
    /// The normal user-visible notification priority.
    #[default]
    Normal,
    /// Time-sensitive notification requiring prominent presentation policy.
    Critical,
}

/// A typed, application-local action identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NotificationActionId(String);

impl NotificationActionId {
    /// Create an action ID.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError::InvalidRequest`] for an empty identifier.
    pub fn new(value: impl Into<String>) -> NotificationResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(NotificationError::InvalidRequest(
                "action IDs must not be empty",
            ));
        }
        Ok(Self(value))
    }

    /// Return the action's stable application-local identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NotificationActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A user-visible notification action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAction {
    /// Stable action key returned to the emitting application.
    pub id: NotificationActionId,
    /// User-visible action label.
    pub label: String,
}

impl NotificationAction {
    /// Create a notification action with a non-empty label.
    pub fn new(id: NotificationActionId, label: impl Into<String>) -> NotificationResult<Self> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(NotificationError::InvalidRequest(
                "action labels must not be empty",
            ));
        }
        Ok(Self { id, label })
    }
}

/// Typed request sent by an application to a notification service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRequest {
    /// Durable identity of the application that owns this notification.
    pub app_id: AppId,
    /// Concise, user-visible notification title.
    pub summary: String,
    /// Optional longer user-visible content.
    pub body: Option<String>,
    /// Presentation priority requested by the application.
    pub urgency: NotificationUrgency,
    /// User actions exposed by the notification center.
    pub actions: Vec<NotificationAction>,
    /// Existing notification to replace in place, if owned by `app_id`.
    pub replaces: Option<NotificationId>,
}

impl NotificationRequest {
    /// Create a notification request with normal urgency and no actions.
    pub fn new(app_id: AppId, summary: impl Into<String>) -> NotificationResult<Self> {
        let summary = summary.into();
        if summary.trim().is_empty() {
            return Err(NotificationError::InvalidRequest(
                "notification summaries must not be empty",
            ));
        }
        Ok(Self {
            app_id,
            summary,
            body: None,
            urgency: NotificationUrgency::Normal,
            actions: Vec::new(),
            replaces: None,
        })
    }

    /// Attach optional longer content.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Set the requested presentation urgency.
    #[must_use]
    pub fn with_urgency(mut self, urgency: NotificationUrgency) -> Self {
        self.urgency = urgency;
        self
    }

    /// Attach actions, rejecting duplicate action IDs.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError::InvalidRequest`] when action IDs repeat.
    pub fn with_actions(mut self, actions: Vec<NotificationAction>) -> NotificationResult<Self> {
        let mut identifiers = HashSet::new();
        if actions
            .iter()
            .any(|action| !identifiers.insert(action.id.clone()))
        {
            return Err(NotificationError::InvalidRequest(
                "notification action IDs must be unique",
            ));
        }
        self.actions = actions;
        Ok(self)
    }

    /// Replace an existing notification emitted by the same application.
    #[must_use]
    pub fn replacing(mut self, notification: NotificationId) -> Self {
        self.replaces = Some(notification);
        self
    }
}

/// The lifecycle state of a notification record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLifecycle {
    /// Available to transient notification presentation and the notification center.
    Active,
    /// Removed from active presentation but retained for history queries.
    Dismissed(NotificationDismissReason),
}

/// Why a notification left its active state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationDismissReason {
    /// The user dismissed it from a notification surface.
    User,
    /// Its originating application withdrew it.
    Application,
    /// Service policy expired the notification.
    Expired,
}

/// One notification stored by the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRecord {
    /// Daemon-assigned ID.
    pub id: NotificationId,
    /// Typed request and application attribution.
    pub notification: NotificationRequest,
    /// Current lifecycle state.
    pub lifecycle: NotificationLifecycle,
    /// Monotonic service sequence used for deterministic ordering.
    pub sequence: u64,
}

/// Query accepted by [`NotificationApi::query`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationQuery {
    /// Active notifications from every application, newest first.
    Active,
    /// All retained notification history, newest first.
    History,
    /// Notifications emitted by one application, optionally including dismissed history.
    ForApp {
        /// Application whose notifications are requested.
        app_id: AppId,
        /// Whether dismissed records are included.
        include_dismissed: bool,
    },
}

/// Event returned when a notification center invokes an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationActionInvocation {
    /// Notification owning the action.
    pub notification_id: NotificationId,
    /// Application that receives the action callback.
    pub app_id: AppId,
    /// The invoked application-local action ID.
    pub action_id: NotificationActionId,
}

/// Small stable boundary between applications, notificationd, and the Shell.
///
/// A future D-Bus proxy implements this trait.  Shell notification-center UI
/// can query records and invoke actions without observing the daemon's store.
pub trait NotificationApi: Send + Sync {
    /// Publish a new notification or replace an existing owned notification.
    fn publish(&self, request: NotificationRequest) -> NotificationResult<NotificationRecord>;

    /// Dismiss an active notification while retaining it for history.
    fn dismiss(
        &self,
        id: NotificationId,
        reason: NotificationDismissReason,
    ) -> NotificationResult<NotificationRecord>;

    /// Return notification records matching a typed query.
    fn query(&self, query: NotificationQuery) -> NotificationResult<Vec<NotificationRecord>>;

    /// Validate and return a notification action invocation for delivery.
    fn invoke_action(
        &self,
        id: NotificationId,
        action_id: NotificationActionId,
    ) -> NotificationResult<NotificationActionInvocation>;
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
                SettingsChange::SetHighContrast(high_contrast) => {
                    snapshot.appearance.high_contrast = high_contrast;
                }
                SettingsChange::SetReducedMotion(reduced_motion) => {
                    snapshot.appearance.reduced_motion = reduced_motion;
                }
                SettingsChange::SetTextScale(text_scale) => {
                    snapshot.appearance.text_scale = text_scale;
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
