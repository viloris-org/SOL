//! Renderer-neutral SOL top bar and status-area model.
//!
//! Platform providers supply snapshots; they never leak NetworkManager,
//! PipeWire, UPower, compositor, or portal types into the shell model.

use std::{error::Error, fmt};

use sol_app::AppId;
use sol_design::color::Color;
use sol_graphics::Surface;
use sol_system::{
    ActionSource, SystemAction, SystemActionApi, SystemActionRequest, SystemActionResult,
};
use sol_ui::{AccessibilityNode, Button, InteractionTree, Key, KeyboardOutcome, SemanticControl};

const SHELL_APP_ID: &str = "org.sol.shell";
const SETTINGS_APP_ID: &str = "org.sol.settings";

/// A provider result exposed faithfully to rendering and accessibility layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderState<T> {
    /// A current or last-known snapshot. `stale` means it should be visually
    /// qualified rather than silently presented as current.
    Available { value: T, stale: bool },
    /// No platform adapter has been installed for this capability.
    Unavailable,
    /// An installed adapter failed; message is diagnostic, not a fabricated value.
    Error(String),
}

impl<T> ProviderState<T> {
    /// Return the data only when it is available (including safely retained stale data).
    #[must_use]
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Available { value, .. } => Some(value),
            _ => None,
        }
    }
    /// Whether the state must be marked stale to the user.
    #[must_use]
    pub fn stale(&self) -> bool {
        matches!(self, Self::Available { stale: true, .. })
    }
}

/// Clock/date data supplied by a clock adapter. Formatting is deliberately a
/// model concern, so deterministic fixtures do not depend on host time zones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockStatus {
    pub time: String,
    pub date: String,
}
/// Compositor-provided workspace position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceStatus {
    pub current: u8,
    pub total: u8,
}
/// Network summary, not a NetworkManager object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkStatus {
    Offline,
    Connecting,
    Connected { name: String, signal_percent: u8 },
}
/// Audio summary, not a PipeWire object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioStatus {
    pub volume_percent: u8,
    pub muted: bool,
}
/// Power summary, not a UPower object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerStatus {
    pub percent: u8,
    pub charging: bool,
}
/// Privacy/activity indicator that must be explicitly observed by a trusted adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityIndicator {
    ScreenCapture,
    Microphone,
    Camera,
    RemoteControl,
}

/// Typed provider contracts. Adapters may poll, subscribe, or cache externally;
/// this boundary stays synchronous and deterministic for the retained shell model.
pub trait ClockProvider {
    fn clock(&self) -> ProviderState<ClockStatus>;
}
pub trait WorkspaceProvider {
    fn workspace(&self) -> ProviderState<WorkspaceStatus>;
}
pub trait NetworkProvider {
    fn network(&self) -> ProviderState<NetworkStatus>;
}
pub trait AudioProvider {
    fn audio(&self) -> ProviderState<AudioStatus>;
}
pub trait PowerProvider {
    fn power(&self) -> ProviderState<PowerStatus>;
}
pub trait ActivityProvider {
    fn activity(&self) -> ProviderState<Vec<ActivityIndicator>>;
}

/// One complete top-bar snapshot pulled from platform provider boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopBarSnapshot {
    pub clock: ProviderState<ClockStatus>,
    pub workspace: ProviderState<WorkspaceStatus>,
    pub network: ProviderState<NetworkStatus>,
    pub audio: ProviderState<AudioStatus>,
    pub power: ProviderState<PowerStatus>,
    pub activity: ProviderState<Vec<ActivityIndicator>>,
}

/// Bundle of providers accepted by [`TopBarModel::refresh`].
pub struct TopBarProviders<C, W, N, A, P, I> {
    pub clock: C,
    pub workspace: W,
    pub network: N,
    pub audio: A,
    pub power: P,
    pub activity: I,
}
impl<
    C: ClockProvider,
    W: WorkspaceProvider,
    N: NetworkProvider,
    A: AudioProvider,
    P: PowerProvider,
    I: ActivityProvider,
> TopBarProviders<C, W, N, A, P, I>
{
    #[must_use]
    pub fn snapshot(&self) -> TopBarSnapshot {
        TopBarSnapshot {
            clock: self.clock.clock(),
            workspace: self.workspace.workspace(),
            network: self.network.network(),
            audio: self.audio.audio(),
            power: self.power.power(),
            activity: self.activity.activity(),
        }
    }
}

/// Explicit interaction targets. They produce typed system actions; no click
/// handler contains a shell command or direct backend call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBarControl {
    Network,
    Audio,
    Power,
    Privacy,
}
impl TopBarControl {
    /// Return the typed intent associated with activating this control.
    #[must_use]
    pub fn action(self, snapshot: &TopBarSnapshot) -> SystemAction {
        match self {
            Self::Audio => SystemAction::SetOutputMuted {
                muted: !snapshot
                    .audio
                    .value()
                    .map(|audio| audio.muted)
                    .unwrap_or(false),
            },
            Self::Network | Self::Power | Self::Privacy => SystemAction::LaunchApplication {
                app_id: AppId::parse(SETTINGS_APP_ID).expect("fixed settings ID is valid"),
            },
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Audio => "audio",
            Self::Power => "power",
            Self::Privacy => "privacy",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Network => "Network status",
            Self::Audio => "Audio status",
            Self::Power => "Power status",
            Self::Privacy => "Privacy and activity status",
        }
    }
}

/// Permission result surfaced without falsely claiming a system change occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentOutcome {
    Authorized,
    AwaitingConsent,
    Denied,
}
/// Top-bar model failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopBarError(String);
impl fmt::Display for TopBarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for TopBarError {}

/// Token-only renderer projection for a top-bar surface.
#[derive(Debug, Clone, PartialEq)]
pub struct TopBarFrame {
    pub pixel_size: (u32, u32),
    pub snapshot: TopBarSnapshot,
    pub background: Color,
    pub foreground: Color,
}

/// Retained, renderer-independent top-bar controller.
pub struct TopBarModel<A: SystemActionApi> {
    actions: A,
    snapshot: TopBarSnapshot,
    tree: InteractionTree,
}
impl<A: SystemActionApi> TopBarModel<A> {
    /// Build an initial model from provider output.
    #[must_use]
    pub fn new(actions: A, snapshot: TopBarSnapshot) -> Self {
        let mut tree = InteractionTree::new("top-bar", "SOL Top Bar");
        for control in [
            TopBarControl::Network,
            TopBarControl::Audio,
            TopBarControl::Power,
            TopBarControl::Privacy,
        ] {
            tree.push(SemanticControl::button(
                control.id(),
                &Button::new().with_label(control.label()),
            ));
        }
        Self {
            actions,
            snapshot,
            tree,
        }
    }
    /// Replace the retained snapshot after polling or a provider subscription event.
    pub fn refresh(&mut self, snapshot: TopBarSnapshot) {
        self.snapshot = snapshot;
    }
    #[must_use]
    pub fn snapshot(&self) -> &TopBarSnapshot {
        &self.snapshot
    }
    /// Submit a typed, caller-attributed action for authorization only.
    pub fn activate(&self, control: TopBarControl) -> Result<IntentOutcome, TopBarError> {
        let caller = AppId::parse(SHELL_APP_ID).expect("fixed shell ID is valid");
        let result = self
            .actions
            .request(SystemActionRequest {
                caller,
                source: ActionSource::QuickSettings,
                action: control.action(&self.snapshot),
            })
            .map_err(|error| TopBarError(error.to_string()))?;
        Ok(match result {
            SystemActionResult::Authorized(_) => IntentOutcome::Authorized,
            SystemActionResult::AwaitingUserConsent(_) => IntentOutcome::AwaitingConsent,
            SystemActionResult::Denied { .. } => IntentOutcome::Denied,
        })
    }
    pub fn handle_key(&mut self, key: Key) -> KeyboardOutcome {
        self.tree.handle_key(key)
    }
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        self.tree.accessibility_tree()
    }
    #[must_use]
    pub fn frame_for(&self, surface: &Surface) -> TopBarFrame {
        TopBarFrame {
            pixel_size: (
                (surface.size.0 * surface.scale) as u32,
                (surface.size.1 * surface.scale) as u32,
            ),
            snapshot: self.snapshot.clone(),
            background: Color::Elevated,
            foreground: Color::TextPrimary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sol_system::{
        DefaultDenyPolicy, MemoryActionAuditStore, MemoryPermissionStore, SystemActionService,
    };
    #[derive(Clone)]
    struct Fake<T>(ProviderState<T>);
    impl ClockProvider for Fake<ClockStatus> {
        fn clock(&self) -> ProviderState<ClockStatus> {
            self.0.clone()
        }
    }
    impl WorkspaceProvider for Fake<WorkspaceStatus> {
        fn workspace(&self) -> ProviderState<WorkspaceStatus> {
            self.0.clone()
        }
    }
    impl NetworkProvider for Fake<NetworkStatus> {
        fn network(&self) -> ProviderState<NetworkStatus> {
            self.0.clone()
        }
    }
    impl AudioProvider for Fake<AudioStatus> {
        fn audio(&self) -> ProviderState<AudioStatus> {
            self.0.clone()
        }
    }
    impl PowerProvider for Fake<PowerStatus> {
        fn power(&self) -> ProviderState<PowerStatus> {
            self.0.clone()
        }
    }
    impl ActivityProvider for Fake<Vec<ActivityIndicator>> {
        fn activity(&self) -> ProviderState<Vec<ActivityIndicator>> {
            self.0.clone()
        }
    }
    fn snapshot() -> TopBarSnapshot {
        TopBarProviders {
            clock: Fake(ProviderState::Available {
                value: ClockStatus {
                    time: "09:41".into(),
                    date: "2026-08-16".into(),
                },
                stale: false,
            }),
            workspace: Fake(ProviderState::Available {
                value: WorkspaceStatus {
                    current: 2,
                    total: 4,
                },
                stale: false,
            }),
            network: Fake(ProviderState::Available {
                value: NetworkStatus::Connected {
                    name: "SOLNet".into(),
                    signal_percent: 80,
                },
                stale: true,
            }),
            audio: Fake(ProviderState::Available {
                value: AudioStatus {
                    volume_percent: 42,
                    muted: false,
                },
                stale: false,
            }),
            power: Fake(ProviderState::Unavailable),
            activity: Fake(ProviderState::Error("portal unavailable".into())),
        }
        .snapshot()
    }
    fn model() -> TopBarModel<
        SystemActionService<DefaultDenyPolicy, MemoryPermissionStore, MemoryActionAuditStore>,
    > {
        TopBarModel::new(
            SystemActionService::new(
                DefaultDenyPolicy,
                MemoryPermissionStore::default(),
                MemoryActionAuditStore::default(),
            ),
            snapshot(),
        )
    }
    #[test]
    fn provider_states_are_retained_without_fabricating_data() {
        let state = snapshot();
        assert_eq!(state.clock.value().unwrap().time, "09:41");
        assert!(state.network.stale());
        assert!(matches!(state.power, ProviderState::Unavailable));
        assert!(matches!(state.activity, ProviderState::Error(_)));
    }
    #[test]
    fn controls_emit_typed_intents_but_default_deny_executes_nothing() {
        let model = model();
        assert!(matches!(
            TopBarControl::Audio.action(model.snapshot()),
            SystemAction::SetOutputMuted { muted: true }
        ));
        assert_eq!(
            model.activate(TopBarControl::Audio).unwrap(),
            IntentOutcome::Denied
        );
        assert!(matches!(
            TopBarControl::Network.action(model.snapshot()),
            SystemAction::LaunchApplication { .. }
        ));
    }
    #[test]
    fn keyboard_accessibility_and_frame_are_deterministic() {
        let mut model = model();
        assert!(matches!(
            model.handle_key(Key::Tab),
            KeyboardOutcome::FocusMoved(_)
        ));
        assert!(model.accessibility_tree().children[0].state.focused);
        let frame = model.frame_for(&Surface::high_dpi(400.0, 40.0, 1.25));
        assert_eq!(frame.pixel_size, (500, 50));
    }
}
