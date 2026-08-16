//! Renderer-neutral workspace overview and interactive gesture policy.
//!
//! The compositor remains authoritative for live surfaces and workspace
//! membership. This module consumes snapshots through [`WorkspaceBridge`] and
//! emits typed [`WorkspaceAction`] intent; it deliberately has no libinput,
//! Smithay, Wayland, or renderer dependency.

use sol_animation::InterruptibleAnimation;
use sol_design::{
    accessibility::TokenMode,
    color::Color,
    motion::{Motion, MotionSpec},
    spacing::Spacing,
};
use sol_ui::{
    AccessibilityNode, AccessibilityState, SemanticId, SemanticRole, VisualTokenContract,
};
use std::error::Error;
use std::fmt;

/// Stable workspace identifier shared by shell snapshots and compositor IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceId(u32);

impl WorkspaceId {
    /// Construct an ID assigned by the compositor workspace model.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the transport-friendly numeric ID.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable window identifier owned by the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OverviewWindowId(u64);

impl OverviewWindowId {
    /// Construct an ID returned by a trusted compositor snapshot adapter.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the transport-friendly numeric ID.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for OverviewWindowId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A compositor-owned toplevel projected into the overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverviewWindowSnapshot {
    /// Stable compositor identity for typed activation and movement intent.
    pub id: OverviewWindowId,
    /// User-visible window title.
    pub title: String,
    /// Application identity or title reported by the compositor adapter.
    pub application: String,
    /// Whether the compositor currently considers this window focused.
    pub focused: bool,
}

/// One workspace and its visible overview cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    /// Stable workspace ID.
    pub id: WorkspaceId,
    /// User-visible workspace name.
    pub label: String,
    /// Windows belonging to this workspace, in compositor z-order projection.
    pub windows: Vec<OverviewWindowSnapshot>,
}

/// Complete workspace state supplied by a compositor IPC adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshotSet {
    /// Current compositor-active workspace.
    pub active: WorkspaceId,
    /// All addressable workspaces in visual order.
    pub workspaces: Vec<WorkspaceSnapshot>,
}

/// Typed actions the shell may request from the compositor.
///
/// These are intent only. A compositor-side IPC adapter validates the IDs and
/// performs the actual workspace or surface mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceAction {
    /// Make a workspace current.
    SwitchWorkspace { workspace: WorkspaceId },
    /// Relocate one compositor-owned window to another workspace.
    MoveWindow {
        /// Window to move.
        window: OverviewWindowId,
        /// Destination workspace.
        workspace: WorkspaceId,
    },
    /// Ask the compositor to focus one visible window.
    ActivateWindow { window: OverviewWindowId },
}

/// The shell/compositor boundary required by overview and gesture models.
///
/// A future D-Bus proxy or in-process fixture implements this trait. Keeping
/// the boundary snapshot- and action-based avoids leaking Wayland surfaces
/// into the shell and makes all behavior testable without a GPU session.
pub trait WorkspaceBridge {
    /// Read the compositor's current workspace/window snapshot.
    fn snapshot(&self) -> Result<WorkspaceSnapshotSet, WorkspaceBridgeError>;

    /// Dispatch one validated shell intent to the compositor adapter.
    fn dispatch(&mut self, action: WorkspaceAction) -> Result<(), WorkspaceBridgeError>;
}

/// Failure reported by a workspace IPC adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBridgeError {
    message: String,
}

impl WorkspaceBridgeError {
    /// Create an adapter error suitable for diagnostics and UI feedback.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for WorkspaceBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for WorkspaceBridgeError {}

/// Visibility state of the overview surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverviewVisibility {
    /// The regular workspace is presented; no overview surface is requested.
    #[default]
    Hidden,
    /// Overview cards are presented and receive overview keyboard input.
    Visible,
}

/// Selected target inside the overview's keyboard and accessibility model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewFocus {
    /// A workspace card is selected.
    Workspace(WorkspaceId),
    /// A window card nested in a workspace is selected.
    Window {
        /// Owning workspace.
        workspace: WorkspaceId,
        /// Selected window.
        window: OverviewWindowId,
    },
}

/// Keyboard input interpreted by the renderer-neutral overview model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewKey {
    /// Select the previous workspace card.
    ArrowLeft,
    /// Select the next workspace card.
    ArrowRight,
    /// Select the previous window card across workspace cards.
    ArrowUp,
    /// Select the next window card across workspace cards.
    ArrowDown,
    /// Apply the selected workspace/window action.
    Enter,
    /// Close overview without mutating compositor state.
    Escape,
}

/// Result of overview keyboard handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverviewOutcome {
    /// Keyboard focus changed within the renderer-neutral model.
    FocusChanged(OverviewFocus),
    /// A typed compositor intent should be dispatched by a bridge.
    Action(WorkspaceAction),
    /// Overview was hidden without an action.
    Hidden,
    /// Input could not apply in the current model state.
    Ignored,
}

/// Error returned while constructing or manipulating overview state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverviewError {
    /// A snapshot contains no workspaces or does not contain its active ID.
    InvalidSnapshot(&'static str),
    /// A requested workspace does not occur in the current snapshot.
    UnknownWorkspace(WorkspaceId),
    /// A requested window does not occur in the named workspace.
    UnknownWindow {
        /// Workspace searched.
        workspace: WorkspaceId,
        /// Window searched.
        window: OverviewWindowId,
    },
    /// A gesture cannot start because there is no adjacent workspace.
    NoAdjacentWorkspace,
    /// A gesture update was received before it began.
    GestureInactive,
}

impl fmt::Display for OverviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSnapshot(message) => {
                write!(formatter, "invalid workspace snapshot: {message}")
            }
            Self::UnknownWorkspace(workspace) => write!(formatter, "unknown workspace {workspace}"),
            Self::UnknownWindow { workspace, window } => {
                write!(formatter, "window {window} is not in workspace {workspace}")
            }
            Self::NoAdjacentWorkspace => formatter.write_str("no adjacent workspace for gesture"),
            Self::GestureInactive => formatter.write_str("workspace gesture is not active"),
        }
    }
}

impl Error for OverviewError {}

/// Result returned by the overview and gesture models.
pub type OverviewResult<T> = Result<T, OverviewError>;

/// Renderer-neutral workspace overview state.
#[derive(Debug, Clone)]
pub struct OverviewModel {
    snapshots: WorkspaceSnapshotSet,
    visibility: OverviewVisibility,
    focus: OverviewFocus,
}

impl OverviewModel {
    /// Create an overview model from a compositor snapshot.
    pub fn new(snapshots: WorkspaceSnapshotSet) -> OverviewResult<Self> {
        validate_snapshot(&snapshots)?;
        let focus = OverviewFocus::Workspace(snapshots.active);
        Ok(Self {
            snapshots,
            visibility: OverviewVisibility::Hidden,
            focus,
        })
    }

    /// Return snapshots currently displayed by the overview.
    #[must_use]
    pub fn snapshots(&self) -> &WorkspaceSnapshotSet {
        &self.snapshots
    }

    /// Return whether the overview is currently visible.
    #[must_use]
    pub const fn visibility(&self) -> OverviewVisibility {
        self.visibility
    }

    /// Return the current keyboard/accessibility target.
    #[must_use]
    pub const fn focus(&self) -> OverviewFocus {
        self.focus
    }

    /// Replace snapshots after a compositor state update while preserving focus
    /// whenever the selected item is still present.
    pub fn refresh(&mut self, snapshots: WorkspaceSnapshotSet) -> OverviewResult<()> {
        validate_snapshot(&snapshots)?;
        let old_focus = self.focus;
        self.snapshots = snapshots;
        self.focus = match old_focus {
            OverviewFocus::Workspace(workspace) if self.workspace(workspace).is_ok() => {
                OverviewFocus::Workspace(workspace)
            }
            OverviewFocus::Window { workspace, window }
                if self.window(workspace, window).is_ok() =>
            {
                OverviewFocus::Window { workspace, window }
            }
            _ => OverviewFocus::Workspace(self.snapshots.active),
        };
        Ok(())
    }

    /// Populate the model from the shell/compositor bridge.
    pub fn refresh_from(
        &mut self,
        bridge: &impl WorkspaceBridge,
    ) -> Result<(), WorkspaceBridgeError> {
        let snapshots = bridge.snapshot()?;
        self.refresh(snapshots)
            .map_err(|error| WorkspaceBridgeError::new(error.to_string()))
    }

    /// Enter overview without directly mutating compositor workspace state.
    pub fn enter(&mut self) {
        self.visibility = OverviewVisibility::Visible;
        self.focus = OverviewFocus::Workspace(self.snapshots.active);
    }

    /// Exit overview without directly mutating compositor workspace state.
    pub fn exit(&mut self) {
        self.visibility = OverviewVisibility::Hidden;
    }

    /// Dispatch a typed overview action through an explicit compositor bridge.
    pub fn dispatch(
        &self,
        bridge: &mut impl WorkspaceBridge,
        action: WorkspaceAction,
    ) -> Result<(), WorkspaceBridgeError> {
        bridge.dispatch(action)
    }

    /// Move the selected window to `workspace`, returning typed compositor intent.
    pub fn move_focused_window(&self, workspace: WorkspaceId) -> OverviewResult<WorkspaceAction> {
        self.workspace(workspace)?;
        match self.focus {
            OverviewFocus::Window { window, .. } => {
                Ok(WorkspaceAction::MoveWindow { window, workspace })
            }
            OverviewFocus::Workspace(_) => Err(OverviewError::InvalidSnapshot(
                "select a window before moving it to another workspace",
            )),
        }
    }

    /// Apply overview keyboard navigation and return any typed intent.
    pub fn handle_key(&mut self, key: OverviewKey) -> OverviewOutcome {
        if self.visibility != OverviewVisibility::Visible {
            return OverviewOutcome::Ignored;
        }
        match key {
            OverviewKey::ArrowLeft => self.move_workspace_focus(true),
            OverviewKey::ArrowRight => self.move_workspace_focus(false),
            OverviewKey::ArrowUp => self.move_window_focus(true),
            OverviewKey::ArrowDown => self.move_window_focus(false),
            OverviewKey::Enter => match self.focus {
                OverviewFocus::Workspace(workspace) => {
                    OverviewOutcome::Action(WorkspaceAction::SwitchWorkspace { workspace })
                }
                OverviewFocus::Window { window, .. } => {
                    OverviewOutcome::Action(WorkspaceAction::ActivateWindow { window })
                }
            },
            OverviewKey::Escape => {
                self.exit();
                OverviewOutcome::Hidden
            }
        }
    }

    /// Produce a semantic tree for renderer and accessibility adapters.
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        AccessibilityNode {
            id: SemanticId::new("overview"),
            role: SemanticRole::Group,
            label: "Workspace overview".to_owned(),
            value: Some(match self.visibility {
                OverviewVisibility::Hidden => "hidden".to_owned(),
                OverviewVisibility::Visible => "visible".to_owned(),
            }),
            state: AccessibilityState::default(),
            children: self
                .snapshots
                .workspaces
                .iter()
                .map(|workspace| AccessibilityNode {
                    id: SemanticId::new(format!("workspace-{}", workspace.id.get())),
                    role: SemanticRole::Button,
                    label: workspace.label.clone(),
                    value: Some(format!("{} windows", workspace.windows.len())),
                    state: AccessibilityState {
                        focused: self.focus == OverviewFocus::Workspace(workspace.id),
                        selected: self.snapshots.active == workspace.id,
                        ..AccessibilityState::default()
                    },
                    children: workspace
                        .windows
                        .iter()
                        .map(|window| AccessibilityNode {
                            id: SemanticId::new(format!("window-{}", window.id.get())),
                            role: SemanticRole::Button,
                            label: window.title.clone(),
                            value: Some(window.application.clone()),
                            state: AccessibilityState {
                                focused: self.focus
                                    == OverviewFocus::Window {
                                        workspace: workspace.id,
                                        window: window.id,
                                    },
                                selected: window.focused,
                                ..AccessibilityState::default()
                            },
                            children: Vec::new(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Return a renderer-neutral visual contract using named SolKit tokens.
    #[must_use]
    pub const fn visual_tokens(&self) -> VisualTokenContract {
        VisualTokenContract {
            background: Color::Surface,
            foreground: Color::TextPrimary,
            padding: Spacing::Lg,
            radius: sol_design::radius::Radius::Md,
            metric: sol_design::metrics::ControlMetric::Toolbar,
            motion: Motion::Workspace,
            typography: sol_design::typography::FontStyle::Title,
        }
    }

    fn workspace(&self, id: WorkspaceId) -> OverviewResult<&WorkspaceSnapshot> {
        self.snapshots
            .workspaces
            .iter()
            .find(|workspace| workspace.id == id)
            .ok_or(OverviewError::UnknownWorkspace(id))
    }

    fn window(
        &self,
        workspace: WorkspaceId,
        id: OverviewWindowId,
    ) -> OverviewResult<&OverviewWindowSnapshot> {
        self.workspace(workspace)?
            .windows
            .iter()
            .find(|window| window.id == id)
            .ok_or(OverviewError::UnknownWindow {
                workspace,
                window: id,
            })
    }

    fn move_workspace_focus(&mut self, previous: bool) -> OverviewOutcome {
        let current = match self.focus {
            OverviewFocus::Workspace(workspace) => workspace,
            OverviewFocus::Window { workspace, .. } => workspace,
        };
        let current_index = self
            .snapshots
            .workspaces
            .iter()
            .position(|workspace| workspace.id == current)
            .unwrap_or(0);
        let len = self.snapshots.workspaces.len();
        let next_index = if previous {
            (current_index + len - 1) % len
        } else {
            (current_index + 1) % len
        };
        self.focus = OverviewFocus::Workspace(self.snapshots.workspaces[next_index].id);
        OverviewOutcome::FocusChanged(self.focus)
    }

    fn move_window_focus(&mut self, previous: bool) -> OverviewOutcome {
        let windows: Vec<(WorkspaceId, OverviewWindowId)> = self
            .snapshots
            .workspaces
            .iter()
            .flat_map(|workspace| {
                workspace
                    .windows
                    .iter()
                    .map(move |window| (workspace.id, window.id))
            })
            .collect();
        if windows.is_empty() {
            return OverviewOutcome::Ignored;
        }
        let current_index = match self.focus {
            OverviewFocus::Window { workspace, window } => windows
                .iter()
                .position(|candidate| *candidate == (workspace, window))
                .unwrap_or(0),
            OverviewFocus::Workspace(workspace) => windows
                .iter()
                .position(|candidate| candidate.0 == workspace)
                .unwrap_or(0),
        };
        let len = windows.len();
        let next_index = if matches!(self.focus, OverviewFocus::Workspace(_)) && !previous {
            current_index
        } else if previous {
            (current_index + len - 1) % len
        } else {
            (current_index + 1) % len
        };
        let (workspace, window) = windows[next_index];
        self.focus = OverviewFocus::Window { workspace, window };
        OverviewOutcome::FocusChanged(self.focus)
    }
}

fn validate_snapshot(snapshots: &WorkspaceSnapshotSet) -> OverviewResult<()> {
    if snapshots.workspaces.is_empty() {
        return Err(OverviewError::InvalidSnapshot(
            "at least one workspace is required",
        ));
    }
    if !snapshots
        .workspaces
        .iter()
        .any(|workspace| workspace.id == snapshots.active)
    {
        return Err(OverviewError::InvalidSnapshot(
            "active workspace does not occur in workspace list",
        ));
    }
    Ok(())
}

/// Direction of a four-finger workspace gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureDirection {
    /// Reveal the immediately preceding workspace.
    Previous,
    /// Reveal the immediately following workspace.
    Next,
}

/// State of a touchpad-driven workspace transition.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceGestureState {
    /// No gesture owns workspace transition progress.
    Idle,
    /// Finger position directly owns normalized transition progress.
    Tracking {
        /// Current source workspace.
        from: WorkspaceId,
        /// Adjacent workspace being revealed.
        to: WorkspaceId,
        /// Normalized finger-derived progress.
        progress: f32,
        /// Last normalized gesture velocity.
        velocity: f32,
    },
    /// Gesture ended or cancelled; a renderer settles using semantic motion.
    Settling(WorkspaceGestureSettle),
}

/// Settling specification handed to a renderer after a gesture releases.
#[derive(Debug, Clone)]
pub struct WorkspaceGestureSettle {
    /// Source workspace.
    pub from: WorkspaceId,
    /// Gesture target workspace.
    pub to: WorkspaceId,
    /// Final selected workspace after cancel/threshold/velocity policy.
    pub target: WorkspaceId,
    /// Progress at finger release.
    pub progress: f32,
    /// Velocity preserved from the gesture.
    pub velocity: f32,
    /// Token-resolved animation specification; zero in reduced-motion mode.
    pub motion: MotionSpec,
}

impl PartialEq for WorkspaceGestureSettle {
    fn eq(&self, other: &Self) -> bool {
        self.from == other.from
            && self.to == other.to
            && self.target == other.target
            && self.progress == other.progress
            && self.velocity == other.velocity
            && self.motion.duration_ms == other.motion.duration_ms
            && self.motion.spring == other.motion.spring
    }
}

/// Gesture state machine that translates finger progress into workspace intent.
pub struct WorkspaceGestureController {
    state: WorkspaceGestureState,
    animation: InterruptibleAnimation,
}

impl Default for WorkspaceGestureController {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceGestureController {
    /// Create an idle controller using the workspace semantic motion tier.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: WorkspaceGestureState::Idle,
            animation: InterruptibleAnimation::new(Motion::Workspace),
        }
    }

    /// Return the current gesture state for renderer projection.
    #[must_use]
    pub fn state(&self) -> &WorkspaceGestureState {
        &self.state
    }

    /// Start an interactive transition toward an adjacent workspace.
    pub fn begin(
        &mut self,
        current: WorkspaceId,
        direction: GestureDirection,
        ordered_workspaces: &[WorkspaceId],
    ) -> OverviewResult<()> {
        let index = ordered_workspaces
            .iter()
            .position(|workspace| *workspace == current)
            .ok_or(OverviewError::UnknownWorkspace(current))?;
        let target_index = match direction {
            GestureDirection::Previous => index.checked_sub(1),
            GestureDirection::Next => (index + 1 < ordered_workspaces.len()).then_some(index + 1),
        }
        .ok_or(OverviewError::NoAdjacentWorkspace)?;
        let target = ordered_workspaces[target_index];
        self.animation.update_with_progress(0.0);
        self.animation.set_velocity(0.0);
        self.state = WorkspaceGestureState::Tracking {
            from: current,
            to: target,
            progress: 0.0,
            velocity: 0.0,
        };
        Ok(())
    }

    /// Update the exact interactive progress and velocity derived by an input adapter.
    pub fn update(&mut self, progress: f32, velocity: f32) -> OverviewResult<()> {
        let WorkspaceGestureState::Tracking {
            from,
            to,
            progress: tracked_progress,
            velocity: tracked_velocity,
        } = &mut self.state
        else {
            return Err(OverviewError::GestureInactive);
        };
        *tracked_progress = progress.clamp(0.0, 1.0);
        *tracked_velocity = velocity;
        self.animation.update_with_progress(*tracked_progress);
        self.animation.set_velocity(*tracked_velocity);
        let _ = (from, to);
        Ok(())
    }

    /// Cancel the active gesture and settle back at its source workspace.
    pub fn cancel(&mut self, mode: TokenMode) -> OverviewResult<WorkspaceGestureSettle> {
        self.settle(mode, true)
    }

    /// End the active gesture using progress and velocity to choose a workspace.
    pub fn end(&mut self, mode: TokenMode) -> OverviewResult<WorkspaceGestureSettle> {
        self.settle(mode, false)
    }

    /// Finish a renderer-side settle and return the selected workspace action.
    pub fn complete_settle(&mut self) -> OverviewResult<WorkspaceAction> {
        let WorkspaceGestureState::Settling(settle) = &self.state else {
            return Err(OverviewError::GestureInactive);
        };
        let action = WorkspaceAction::SwitchWorkspace {
            workspace: settle.target,
        };
        self.state = WorkspaceGestureState::Idle;
        Ok(action)
    }

    fn settle(
        &mut self,
        mode: TokenMode,
        cancelled: bool,
    ) -> OverviewResult<WorkspaceGestureSettle> {
        let WorkspaceGestureState::Tracking {
            from,
            to,
            progress,
            velocity,
        } = self.state
        else {
            return Err(OverviewError::GestureInactive);
        };
        // A fast fling carries through even if the finger lifted before halfway;
        // a matching reverse fling returns to the origin. The threshold is
        // input policy, not a visual timing constant.
        let target = if cancelled || (progress < 0.5 && velocity < 0.6) {
            from
        } else {
            to
        };
        self.animation
            .update_with_progress(if target == to { 1.0 } else { 0.0 });
        self.animation.set_velocity(velocity);
        let settle = WorkspaceGestureSettle {
            from,
            to,
            target,
            progress,
            velocity,
            motion: mode.motion_spec(Motion::Workspace),
        };
        self.state = WorkspaceGestureState::Settling(settle.clone());
        Ok(settle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: u32, label: &str, windows: &[(u64, &str)]) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::new(id),
            label: label.to_owned(),
            windows: windows
                .iter()
                .enumerate()
                .map(|(index, (id, title))| OverviewWindowSnapshot {
                    id: OverviewWindowId::new(*id),
                    title: (*title).to_owned(),
                    application: "org.sol.fixture".to_owned(),
                    focused: index == 0,
                })
                .collect(),
        }
    }

    fn fixture() -> WorkspaceSnapshotSet {
        WorkspaceSnapshotSet {
            active: WorkspaceId::new(1),
            workspaces: vec![
                workspace(1, "Workspace 1", &[(10, "Files"), (11, "Terminal")]),
                workspace(2, "Workspace 2", &[(20, "Settings")]),
                workspace(3, "Workspace 3", &[]),
            ],
        }
    }

    #[derive(Debug)]
    struct FixtureBridge {
        snapshot: WorkspaceSnapshotSet,
        actions: Vec<WorkspaceAction>,
    }

    impl WorkspaceBridge for FixtureBridge {
        fn snapshot(&self) -> Result<WorkspaceSnapshotSet, WorkspaceBridgeError> {
            Ok(self.snapshot.clone())
        }

        fn dispatch(&mut self, action: WorkspaceAction) -> Result<(), WorkspaceBridgeError> {
            self.actions.push(action);
            Ok(())
        }
    }

    #[test]
    fn overview_projects_snapshot_keyboard_actions_and_accessibility() {
        let mut overview = OverviewModel::new(fixture()).expect("fixture should be valid");
        overview.enter();
        assert_eq!(overview.visibility(), OverviewVisibility::Visible);
        assert_eq!(
            overview.handle_key(OverviewKey::ArrowRight),
            OverviewOutcome::FocusChanged(OverviewFocus::Workspace(WorkspaceId::new(2)))
        );
        assert_eq!(
            overview.handle_key(OverviewKey::Enter),
            OverviewOutcome::Action(WorkspaceAction::SwitchWorkspace {
                workspace: WorkspaceId::new(2)
            })
        );
        assert!(matches!(
            overview.handle_key(OverviewKey::ArrowDown),
            OverviewOutcome::FocusChanged(OverviewFocus::Window { .. })
        ));
        let move_intent = overview
            .move_focused_window(WorkspaceId::new(3))
            .expect("focused window should move");
        assert_eq!(
            move_intent,
            WorkspaceAction::MoveWindow {
                window: OverviewWindowId::new(20),
                workspace: WorkspaceId::new(3)
            }
        );
        let tree = overview.accessibility_tree();
        assert_eq!(tree.label, "Workspace overview");
        assert_eq!(tree.children.len(), 3);
        assert_eq!(tree.children[0].children[0].label, "Files");
        assert_eq!(overview.visual_tokens().motion, Motion::Workspace);
    }

    #[test]
    fn overview_bridge_refresh_and_dispatch_stay_at_typed_boundary() {
        let mut overview = OverviewModel::new(fixture()).expect("fixture should be valid");
        let mut bridge = FixtureBridge {
            snapshot: fixture(),
            actions: Vec::new(),
        };
        overview
            .refresh_from(&bridge)
            .expect("fixture bridge should refresh");
        overview
            .dispatch(
                &mut bridge,
                WorkspaceAction::SwitchWorkspace {
                    workspace: WorkspaceId::new(2),
                },
            )
            .expect("fixture bridge should receive action");
        assert_eq!(bridge.actions.len(), 1);
    }

    #[test]
    fn gesture_progress_is_interruptible_cancellable_and_reduced_motion_safe() {
        let workspaces = [
            WorkspaceId::new(1),
            WorkspaceId::new(2),
            WorkspaceId::new(3),
        ];
        let mut gesture = WorkspaceGestureController::new();
        gesture
            .begin(WorkspaceId::new(1), GestureDirection::Next, &workspaces)
            .expect("next workspace should exist");
        gesture
            .update(0.35, 0.2)
            .expect("tracking update should work");
        let cancelled = gesture
            .cancel(TokenMode::light())
            .expect("cancel should settle");
        assert_eq!(cancelled.target, WorkspaceId::new(1));
        assert!(cancelled.motion.duration_ms > 0);
        assert_eq!(
            gesture
                .complete_settle()
                .expect("settle should produce action"),
            WorkspaceAction::SwitchWorkspace {
                workspace: WorkspaceId::new(1)
            }
        );

        gesture
            .begin(WorkspaceId::new(1), GestureDirection::Next, &workspaces)
            .expect("a new gesture interrupts the previous settle");
        gesture
            .update(0.2, 0.8)
            .expect("velocity should be retained");
        let committed = gesture
            .end(TokenMode::dark().reduced_motion())
            .expect("end should settle");
        assert_eq!(committed.target, WorkspaceId::new(2));
        assert_eq!(committed.motion.duration_ms, 0);
        assert!(committed.motion.spring.is_none());
        assert_eq!(
            gesture
                .complete_settle()
                .expect("settle should produce action"),
            WorkspaceAction::SwitchWorkspace {
                workspace: WorkspaceId::new(2)
            }
        );
    }

    #[test]
    fn gesture_rejects_out_of_range_transition_without_mutating_state() {
        let workspaces = [WorkspaceId::new(1), WorkspaceId::new(2)];
        let mut gesture = WorkspaceGestureController::new();
        assert_eq!(
            gesture.begin(WorkspaceId::new(1), GestureDirection::Previous, &workspaces),
            Err(OverviewError::NoAdjacentWorkspace)
        );
        assert_eq!(gesture.state(), &WorkspaceGestureState::Idle);
    }
}
