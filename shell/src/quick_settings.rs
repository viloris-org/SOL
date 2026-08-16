//! Renderer-neutral Quick Settings model.
//!
//! Audio and mute operations pass through [`sol_system::SystemActionApi`]
//! before reaching [`sol_system::SettingsApi`]. Appearance and accessibility
//! preferences already belong to the typed settings boundary. Network and
//! Bluetooth remain explicitly unavailable until their own service APIs exist.

use sol_design::{
    color::Color, metrics::ControlMetric, motion::Motion, radius::Radius, spacing::Spacing,
    typography::FontStyle,
};
use sol_system::{
    ActionSource, AppId, ColorScheme, OutputVolume, SettingsApi, SettingsChange, SettingsError,
    SettingsSnapshot, SystemAction, SystemActionApi, SystemActionRequest, SystemActionResult,
    TextScale,
};
use sol_ui::{
    AccessibilityNode, AccessibilityState, SemanticId, SemanticRole, VisualTokenContract,
};
use std::error::Error;
use std::fmt;

const SHELL_APP_ID: &str = "org.sol.shell";
const VOLUME_STEP: u8 = 5;

/// Capability represented by a Quick Settings control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickSettingsCapability {
    /// Typed audio output settings.
    Audio,
    /// Typed appearance and accessibility settings.
    Appearance,
    /// Network status and controls.
    Network,
    /// Bluetooth status and controls.
    Bluetooth,
}

/// Availability surfaced honestly by the Quick Settings model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityState {
    /// A typed service API is available to this model.
    Available,
    /// No typed service API has been introduced for this system capability.
    Unavailable(&'static str),
}

/// A capability row with stable ID, title, and availability state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityStatus {
    /// Capability modelled by this row.
    pub capability: QuickSettingsCapability,
    /// User-visible title.
    pub title: &'static str,
    /// Whether the row can dispatch a real typed operation.
    pub state: CapabilityState,
}

/// Quick Settings control receiving keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickSettingsControl {
    /// Output volume percentage.
    Volume,
    /// Output mute toggle.
    Mute,
    /// Requested color scheme.
    ColorScheme,
    /// High contrast preference.
    HighContrast,
    /// Reduced motion preference.
    ReducedMotion,
    /// Named text scale preference.
    TextScale,
    /// Network capability state.
    Network,
    /// Bluetooth capability state.
    Bluetooth,
}

impl QuickSettingsControl {
    const ALL: [Self; 8] = [
        Self::Volume,
        Self::Mute,
        Self::ColorScheme,
        Self::HighContrast,
        Self::ReducedMotion,
        Self::TextScale,
        Self::Network,
        Self::Bluetooth,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Mute => "Mute output",
            Self::ColorScheme => "Color scheme",
            Self::HighContrast => "High contrast",
            Self::ReducedMotion => "Reduced motion",
            Self::TextScale => "Text size",
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
        }
    }
}

/// Normalized keyboard input for Quick Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickSettingsKey {
    /// Focus the prior Quick Settings control.
    ArrowUp,
    /// Focus the next Quick Settings control.
    ArrowDown,
    /// Decrease volume at the volume control.
    ArrowLeft,
    /// Increase volume at the volume control.
    ArrowRight,
    /// Toggle/cycle the focused capability.
    Enter,
    /// Clear Quick Settings keyboard focus.
    Escape,
}

/// Authorization and application result for a quick setting change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickSettingsOutcome {
    /// The typed setting was persisted and a coherent snapshot returned.
    Applied(SettingsSnapshot),
    /// Authorization requires a trusted user-consent surface before applying.
    AwaitingConsent,
    /// Authorization denied this action; settings were not mutated.
    Denied,
    /// The selected capability does not yet have a typed service backend.
    Unavailable(CapabilityStatus),
}

/// Result of keyboard handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickSettingsKeyOutcome {
    /// Keyboard focus moved to a stable control.
    FocusChanged(QuickSettingsControl),
    /// A typed action or preference update was evaluated.
    Action(QuickSettingsOutcome),
    /// Focus cleared.
    FocusCleared,
    /// Input cannot apply in the current focused control.
    Ignored,
}

/// Failure returned by the renderer-neutral Quick Settings model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickSettingsError {
    /// Settings service rejected or could not persist a typed change.
    Settings(SettingsError),
    /// System-action authorization service could not evaluate a request.
    Authorization(String),
    /// The shell application's stable identity was unavailable.
    AppIdentity(String),
}

impl fmt::Display for QuickSettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Settings(error) => error.fmt(formatter),
            Self::Authorization(message) => {
                write!(formatter, "quick settings authorization failed: {message}")
            }
            Self::AppIdentity(message) => write!(formatter, "invalid shell identity: {message}"),
        }
    }
}

impl Error for QuickSettingsError {}

impl From<SettingsError> for QuickSettingsError {
    fn from(error: SettingsError) -> Self {
        Self::Settings(error)
    }
}

/// Result returned by Quick Settings operations.
pub type QuickSettingsResult<T> = Result<T, QuickSettingsError>;

/// Stable, renderer-neutral Quick Settings state.
pub struct QuickSettings<S: SettingsApi, A: SystemActionApi> {
    settings: S,
    actions: A,
    snapshot: SettingsSnapshot,
    focused: Option<QuickSettingsControl>,
    shell_id: AppId,
}

impl<S: SettingsApi, A: SystemActionApi> QuickSettings<S, A> {
    /// Create the model from a coherent settings snapshot.
    pub fn new(settings: S, actions: A) -> QuickSettingsResult<Self> {
        let snapshot = settings.snapshot()?;
        let shell_id = AppId::parse(SHELL_APP_ID)
            .map_err(|error| QuickSettingsError::AppIdentity(error.to_string()))?;
        Ok(Self {
            settings,
            actions,
            snapshot,
            focused: None,
            shell_id,
        })
    }

    /// Return the last coherent settings snapshot.
    #[must_use]
    pub fn snapshot(&self) -> &SettingsSnapshot {
        &self.snapshot
    }

    /// Return the currently focused Quick Settings control.
    #[must_use]
    pub const fn focused(&self) -> Option<QuickSettingsControl> {
        self.focused
    }

    /// Return typed capability state without assuming PipeWire, BlueZ, or a network daemon.
    #[must_use]
    pub const fn capabilities(&self) -> [CapabilityStatus; 4] {
        [
            CapabilityStatus {
                capability: QuickSettingsCapability::Audio,
                title: "Audio",
                state: CapabilityState::Available,
            },
            CapabilityStatus {
                capability: QuickSettingsCapability::Appearance,
                title: "Appearance & accessibility",
                state: CapabilityState::Available,
            },
            CapabilityStatus {
                capability: QuickSettingsCapability::Network,
                title: "Network",
                state: CapabilityState::Unavailable(
                    "Network controls are unavailable until a typed network service API is provided.",
                ),
            },
            CapabilityStatus {
                capability: QuickSettingsCapability::Bluetooth,
                title: "Bluetooth",
                state: CapabilityState::Unavailable(
                    "Bluetooth controls are unavailable until a typed Bluetooth service API is provided.",
                ),
            },
        ]
    }

    /// Refresh the current typed settings snapshot from the service.
    pub fn refresh(&mut self) -> QuickSettingsResult<()> {
        self.snapshot = self.settings.snapshot()?;
        Ok(())
    }

    /// Request a permission-aware output-volume change.
    pub fn set_volume(
        &mut self,
        volume: OutputVolume,
    ) -> QuickSettingsResult<QuickSettingsOutcome> {
        self.apply_authorized(
            SystemAction::SetOutputVolume { volume },
            SettingsChange::SetOutputVolume(volume),
        )
    }

    /// Request a permission-aware mute toggle.
    pub fn set_muted(&mut self, muted: bool) -> QuickSettingsResult<QuickSettingsOutcome> {
        self.apply_authorized(
            SystemAction::SetOutputMuted { muted },
            SettingsChange::SetOutputMuted(muted),
        )
    }

    /// Apply an appearance preference through the typed settings boundary.
    pub fn set_color_scheme(
        &mut self,
        scheme: ColorScheme,
    ) -> QuickSettingsResult<QuickSettingsOutcome> {
        self.apply_preference(SettingsChange::SetColorScheme(scheme))
    }

    /// Apply high-contrast preference through the typed settings boundary.
    pub fn set_high_contrast(
        &mut self,
        enabled: bool,
    ) -> QuickSettingsResult<QuickSettingsOutcome> {
        self.apply_preference(SettingsChange::SetHighContrast(enabled))
    }

    /// Apply reduced-motion preference through the typed settings boundary.
    pub fn set_reduced_motion(
        &mut self,
        enabled: bool,
    ) -> QuickSettingsResult<QuickSettingsOutcome> {
        self.apply_preference(SettingsChange::SetReducedMotion(enabled))
    }

    /// Apply named text scale through the typed settings boundary.
    pub fn set_text_scale(
        &mut self,
        scale: TextScale,
    ) -> QuickSettingsResult<QuickSettingsOutcome> {
        self.apply_preference(SettingsChange::SetTextScale(scale))
    }

    /// Handle normalized keyboard input without exposing renderer or backend details.
    pub fn handle_key(
        &mut self,
        key: QuickSettingsKey,
    ) -> QuickSettingsResult<QuickSettingsKeyOutcome> {
        match key {
            QuickSettingsKey::ArrowUp => Ok(self.move_focus(true)),
            QuickSettingsKey::ArrowDown => Ok(self.move_focus(false)),
            QuickSettingsKey::Escape => {
                self.focused = None;
                Ok(QuickSettingsKeyOutcome::FocusCleared)
            }
            QuickSettingsKey::ArrowLeft => self.adjust_volume(false),
            QuickSettingsKey::ArrowRight => self.adjust_volume(true),
            QuickSettingsKey::Enter => self.activate_focused(),
        }
    }

    /// Build a renderer-independent accessibility representation.
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        AccessibilityNode {
            id: SemanticId::new("quick-settings"),
            role: SemanticRole::Group,
            label: "Quick Settings".to_owned(),
            value: None,
            state: AccessibilityState::default(),
            children: QuickSettingsControl::ALL
                .into_iter()
                .map(|control| AccessibilityNode {
                    id: SemanticId::new(format!("quick-settings-{control:?}")),
                    role: SemanticRole::Button,
                    label: control.label().to_owned(),
                    value: self.control_value(control),
                    state: AccessibilityState {
                        focused: self.focused == Some(control),
                        disabled: matches!(
                            control,
                            QuickSettingsControl::Network | QuickSettingsControl::Bluetooth
                        ),
                        selected: self.control_selected(control),
                        ..AccessibilityState::default()
                    },
                    children: Vec::new(),
                })
                .collect(),
        }
    }

    /// Return the token-only visual contract consumed by a future native panel.
    #[must_use]
    pub const fn visual_tokens(&self) -> VisualTokenContract {
        VisualTokenContract {
            background: Color::Elevated,
            foreground: Color::TextPrimary,
            padding: Spacing::Lg,
            radius: Radius::Md,
            metric: ControlMetric::Toolbar,
            motion: Motion::Panel,
            typography: FontStyle::Body,
        }
    }

    fn apply_authorized(
        &mut self,
        action: SystemAction,
        change: SettingsChange,
    ) -> QuickSettingsResult<QuickSettingsOutcome> {
        let result = self
            .actions
            .request(SystemActionRequest {
                caller: self.shell_id.clone(),
                source: ActionSource::QuickSettings,
                action,
            })
            .map_err(|error| QuickSettingsError::Authorization(error.to_string()))?;
        match result {
            SystemActionResult::Authorized(_) => self.apply_preference(change),
            SystemActionResult::AwaitingUserConsent(_) => Ok(QuickSettingsOutcome::AwaitingConsent),
            SystemActionResult::Denied { .. } => Ok(QuickSettingsOutcome::Denied),
        }
    }

    fn apply_preference(
        &mut self,
        change: SettingsChange,
    ) -> QuickSettingsResult<QuickSettingsOutcome> {
        self.snapshot = self.settings.apply(change)?;
        Ok(QuickSettingsOutcome::Applied(self.snapshot.clone()))
    }

    fn move_focus(&mut self, previous: bool) -> QuickSettingsKeyOutcome {
        let current = self.focused.and_then(|control| {
            QuickSettingsControl::ALL
                .iter()
                .position(|item| *item == control)
        });
        let index = match current {
            Some(index) if previous => {
                (index + QuickSettingsControl::ALL.len() - 1) % QuickSettingsControl::ALL.len()
            }
            Some(index) => (index + 1) % QuickSettingsControl::ALL.len(),
            None if previous => QuickSettingsControl::ALL.len() - 1,
            None => 0,
        };
        let control = QuickSettingsControl::ALL[index];
        self.focused = Some(control);
        QuickSettingsKeyOutcome::FocusChanged(control)
    }

    fn adjust_volume(&mut self, increase: bool) -> QuickSettingsResult<QuickSettingsKeyOutcome> {
        if self.focused != Some(QuickSettingsControl::Volume) {
            return Ok(QuickSettingsKeyOutcome::Ignored);
        }
        let current = self.snapshot.audio.output_volume.percent();
        let next = if increase {
            current
                .saturating_add(VOLUME_STEP)
                .min(OutputVolume::MAX.percent())
        } else {
            current.saturating_sub(VOLUME_STEP)
        };
        Ok(QuickSettingsKeyOutcome::Action(
            self.set_volume(OutputVolume::new(next)?)?,
        ))
    }

    fn activate_focused(&mut self) -> QuickSettingsResult<QuickSettingsKeyOutcome> {
        let Some(control) = self.focused else {
            return Ok(QuickSettingsKeyOutcome::Ignored);
        };
        let outcome = match control {
            QuickSettingsControl::Volume => return self.adjust_volume(true),
            QuickSettingsControl::Mute => self.set_muted(!self.snapshot.audio.output_muted)?,
            QuickSettingsControl::ColorScheme => {
                self.set_color_scheme(next_scheme(self.snapshot.appearance.color_scheme))?
            }
            QuickSettingsControl::HighContrast => {
                self.set_high_contrast(!self.snapshot.appearance.high_contrast)?
            }
            QuickSettingsControl::ReducedMotion => {
                self.set_reduced_motion(!self.snapshot.appearance.reduced_motion)?
            }
            QuickSettingsControl::TextScale => {
                self.set_text_scale(next_text_scale(self.snapshot.appearance.text_scale))?
            }
            QuickSettingsControl::Network => {
                QuickSettingsOutcome::Unavailable(self.capability(QuickSettingsCapability::Network))
            }
            QuickSettingsControl::Bluetooth => QuickSettingsOutcome::Unavailable(
                self.capability(QuickSettingsCapability::Bluetooth),
            ),
        };
        Ok(QuickSettingsKeyOutcome::Action(outcome))
    }

    fn capability(&self, capability: QuickSettingsCapability) -> CapabilityStatus {
        self.capabilities()
            .into_iter()
            .find(|status| status.capability == capability)
            .expect("all quick-setting capabilities have a status")
    }

    fn control_value(&self, control: QuickSettingsControl) -> Option<String> {
        match control {
            QuickSettingsControl::Volume => {
                Some(format!("{}%", self.snapshot.audio.output_volume.percent()))
            }
            QuickSettingsControl::Mute => Some(self.snapshot.audio.output_muted.to_string()),
            QuickSettingsControl::ColorScheme => {
                Some(self.snapshot.appearance.color_scheme.as_str().to_owned())
            }
            QuickSettingsControl::HighContrast => {
                Some(self.snapshot.appearance.high_contrast.to_string())
            }
            QuickSettingsControl::ReducedMotion => {
                Some(self.snapshot.appearance.reduced_motion.to_string())
            }
            QuickSettingsControl::TextScale => {
                Some(self.snapshot.appearance.text_scale.as_str().to_owned())
            }
            QuickSettingsControl::Network | QuickSettingsControl::Bluetooth => {
                Some("unavailable".to_owned())
            }
        }
    }

    fn control_selected(&self, control: QuickSettingsControl) -> bool {
        match control {
            QuickSettingsControl::Mute => self.snapshot.audio.output_muted,
            QuickSettingsControl::HighContrast => self.snapshot.appearance.high_contrast,
            QuickSettingsControl::ReducedMotion => self.snapshot.appearance.reduced_motion,
            _ => false,
        }
    }
}

const fn next_scheme(scheme: ColorScheme) -> ColorScheme {
    match scheme {
        ColorScheme::System => ColorScheme::Light,
        ColorScheme::Light => ColorScheme::Dark,
        ColorScheme::Dark => ColorScheme::System,
    }
}

const fn next_text_scale(scale: TextScale) -> TextScale {
    match scale {
        TextScale::Default => TextScale::Large,
        TextScale::Large => TextScale::ExtraLarge,
        TextScale::ExtraLarge => TextScale::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_system::{
        DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore, PermissionGrant,
        PermissionKey, PermissionStore, SettingsResult, SystemActionService, SystemCapability,
    };
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct FixtureSettingsAdapter {
        snapshot: Mutex<SettingsSnapshot>,
    }

    impl SettingsApi for FixtureSettingsAdapter {
        fn snapshot(&self) -> SettingsResult<SettingsSnapshot> {
            self.snapshot
                .lock()
                .map_err(|error| SettingsError::backend(error.to_string()))
                .map(|snapshot| snapshot.clone())
        }

        fn apply(&self, change: SettingsChange) -> SettingsResult<SettingsSnapshot> {
            let mut snapshot = self
                .snapshot
                .lock()
                .map_err(|error| SettingsError::backend(error.to_string()))?;
            match change {
                SettingsChange::SetColorScheme(value) => snapshot.appearance.color_scheme = value,
                SettingsChange::SetHighContrast(value) => snapshot.appearance.high_contrast = value,
                SettingsChange::SetReducedMotion(value) => {
                    snapshot.appearance.reduced_motion = value
                }
                SettingsChange::SetTextScale(value) => snapshot.appearance.text_scale = value,
                SettingsChange::SetOutputVolume(value) => snapshot.audio.output_volume = value,
                SettingsChange::SetOutputMuted(value) => snapshot.audio.output_muted = value,
            }
            snapshot.revision += 1;
            Ok(snapshot.clone())
        }
    }

    type FixtureActions =
        SystemActionService<DefaultDenyPolicy, MemoryPermissionStore, MemoryActionAuditStore>;

    fn actions(grant: Option<PermissionGrant>) -> FixtureActions {
        let permissions = MemoryPermissionStore::default();
        if let Some(grant) = grant {
            permissions
                .set(
                    PermissionKey::new(
                        AppId::parse(SHELL_APP_ID).unwrap(),
                        SystemCapability::ChangeQuickSettings,
                    ),
                    grant,
                )
                .unwrap();
        }
        FixtureActions::new(
            DefaultDenyPolicy,
            permissions,
            MemoryActionAuditStore::default(),
        )
    }

    #[test]
    fn quick_settings_round_trip_audio_appearance_and_accessibility() {
        let mut quick = QuickSettings::new(
            FixtureSettingsAdapter::default(),
            actions(Some(PermissionGrant::Allow)),
        )
        .unwrap();
        assert!(matches!(
            quick.set_volume(OutputVolume::new(73).unwrap()).unwrap(),
            QuickSettingsOutcome::Applied(_)
        ));
        assert!(matches!(
            quick.set_muted(true).unwrap(),
            QuickSettingsOutcome::Applied(_)
        ));
        quick.set_color_scheme(ColorScheme::Dark).unwrap();
        quick.set_high_contrast(true).unwrap();
        quick.set_reduced_motion(true).unwrap();
        quick.set_text_scale(TextScale::Large).unwrap();
        assert_eq!(quick.snapshot().audio.output_volume.percent(), 73);
        assert!(quick.snapshot().audio.output_muted);
        assert_eq!(quick.snapshot().appearance.color_scheme, ColorScheme::Dark);
        assert!(quick.snapshot().appearance.high_contrast);
        assert!(quick.snapshot().appearance.reduced_motion);
        assert_eq!(quick.snapshot().appearance.text_scale, TextScale::Large);
        assert_eq!(quick.accessibility_tree().children.len(), 8);
        assert_eq!(quick.visual_tokens().motion, Motion::Panel);
    }

    #[test]
    fn keyboard_controls_authorization_and_unavailable_capabilities_are_truthful() {
        let mut quick =
            QuickSettings::new(FixtureSettingsAdapter::default(), actions(None)).unwrap();
        assert_eq!(
            quick.handle_key(QuickSettingsKey::ArrowDown).unwrap(),
            QuickSettingsKeyOutcome::FocusChanged(QuickSettingsControl::Volume)
        );
        assert_eq!(
            quick.handle_key(QuickSettingsKey::ArrowRight).unwrap(),
            QuickSettingsKeyOutcome::Action(QuickSettingsOutcome::Denied)
        );
        assert_eq!(
            quick.snapshot().audio.output_volume.percent(),
            OutputVolume::DEFAULT.percent()
        );
        for _ in 0..7 {
            quick.handle_key(QuickSettingsKey::ArrowDown).unwrap();
        }
        assert_eq!(quick.focused(), Some(QuickSettingsControl::Bluetooth));
        assert!(matches!(
            quick.handle_key(QuickSettingsKey::Enter).unwrap(),
            QuickSettingsKeyOutcome::Action(QuickSettingsOutcome::Unavailable(CapabilityStatus {
                capability: QuickSettingsCapability::Bluetooth,
                ..
            }))
        ));
    }
}
