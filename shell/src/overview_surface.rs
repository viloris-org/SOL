//! Native overview surface projection.
//!
//! The overview model in [`crate::overview`] owns interaction policy.  This
//! module is the shell-side surface adapter: it turns a compositor snapshot
//! and optional window thumbnails into an output-sized layer-shell frame.
//! Wayland and GPU code consume [`OverviewSurfaceContract`] at the host edge;
//! no protocol types leak into the overview model.

use std::collections::BTreeMap;

use sol_design::{accessibility::TokenMode, color::Color, spacing::Spacing};
use sol_ui::{AccessibilityNode, AccessibilityState, LogicalSize, SemanticId, SemanticRole};

use crate::overview::{
    OverviewFocus, OverviewKey, OverviewModel, OverviewOutcome, OverviewWindowId, WorkspaceAction,
    WorkspaceBridge, WorkspaceBridgeError, WorkspaceId, WorkspaceSnapshot, WorkspaceSnapshotSet,
};

/// A bounded RGBA thumbnail supplied by a compositor capture adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowThumbnail {
    /// Width in logical pixels represented by the buffer.
    pub width: u32,
    /// Height in logical pixels represented by the buffer.
    pub height: u32,
    /// Premultiplied RGBA8 bytes, row-major.
    pub rgba: Vec<u8>,
}

impl WindowThumbnail {
    /// Construct a thumbnail, rejecting malformed or excessively large data.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, OverviewSurfaceError> {
        let pixels = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
            .ok_or(OverviewSurfaceError::InvalidThumbnail)?;
        let expected = pixels
            .checked_mul(4)
            .ok_or(OverviewSurfaceError::InvalidThumbnail)?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 || rgba.len() != expected {
            return Err(OverviewSurfaceError::InvalidThumbnail);
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

/// Compositor-owned source of window thumbnails.
pub trait ThumbnailProvider {
    /// Return a current thumbnail for a window, if capture is available.
    fn thumbnail(
        &mut self,
        window: OverviewWindowId,
    ) -> Result<Option<WindowThumbnail>, OverviewSurfaceError>;
}

/// Host boundary implemented by the Wayland layer-shell adapter.
pub trait OverviewSurfaceHost {
    /// Present one frame at the negotiated physical extent.
    fn present(&mut self, contract: &OverviewSurfaceContract, pixels: &[u8]);
    /// Destroy or hide the surface.
    fn dismiss(&mut self);
    /// Transfer keyboard focus to the surface.
    fn set_keyboard_focus(&mut self, focused: bool);
}

/// Output contract negotiated by the compositor and consumed by the native
/// layer-shell host.
#[derive(Debug, Clone, PartialEq)]
pub struct OverviewOutput {
    /// Stable output identity.
    pub id: String,
    /// Logical output extent.
    pub logical_size: LogicalSize,
    /// Fractional output scale.
    pub scale_factor: f32,
}

impl OverviewOutput {
    /// Validate an output contract.
    pub fn new(
        id: impl Into<String>,
        logical_size: LogicalSize,
        scale_factor: f32,
    ) -> Result<Self, OverviewSurfaceError> {
        let id = id.into();
        if id.is_empty()
            || !logical_size.width.is_finite()
            || !logical_size.height.is_finite()
            || logical_size.width <= 0.0
            || logical_size.height <= 0.0
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
        {
            return Err(OverviewSurfaceError::InvalidOutput);
        }
        Ok(Self {
            id,
            logical_size,
            scale_factor,
        })
    }
}

/// One window card in the rendered overview.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowCard {
    /// Stable window identity.
    pub window: OverviewWindowId,
    /// Owning workspace.
    pub workspace: WorkspaceId,
    /// User-visible title and application identity.
    pub title: String,
    pub application: String,
    /// Logical card rectangle `(x, y, width, height)`.
    pub rect: (f32, f32, f32, f32),
    /// Whether this card owns keyboard focus.
    pub focused: bool,
    /// Thumbnail available for this card.
    pub has_thumbnail: bool,
}

/// A complete native overview frame contract.
#[derive(Debug, Clone, PartialEq)]
pub struct OverviewSurfaceContract {
    /// Surface output and physical extent.
    pub output: OverviewOutput,
    pub physical_size: (u32, u32),
    /// Surface-wide semantic colors resolved from `sol-design`.
    pub background: Color,
    pub border: Color,
    pub accent: Color,
    /// Workspace and window card layout.
    pub workspaces: Vec<(WorkspaceId, String, (f32, f32, f32, f32), bool)>,
    pub windows: Vec<WindowCard>,
    /// Accessibility tree projected alongside the visual frame.
    pub accessibility: AccessibilityNode,
    /// Token mode used to resolve the frame.
    pub token_mode: TokenMode,
}

/// Errors at the native overview surface boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverviewSurfaceError {
    /// Snapshot or output facts are malformed.
    InvalidOutput,
    /// Thumbnail dimensions or byte count are unsafe.
    InvalidThumbnail,
    /// The compositor bridge failed.
    Bridge(String),
}

impl std::fmt::Display for OverviewSurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOutput => f.write_str("invalid overview output contract"),
            Self::InvalidThumbnail => f.write_str("invalid overview thumbnail"),
            Self::Bridge(error) => write!(f, "overview compositor bridge failed: {error}"),
        }
    }
}

impl std::error::Error for OverviewSurfaceError {}

impl From<WorkspaceBridgeError> for OverviewSurfaceError {
    fn from(value: WorkspaceBridgeError) -> Self {
        Self::Bridge(value.to_string())
    }
}

/// Native overview surface retained state.
pub struct OverviewSurface<B, T> {
    bridge: B,
    thumbnails: T,
    model: OverviewModel,
    output: OverviewOutput,
    mode: TokenMode,
    visible: bool,
    /// Last contract, useful for host adapters and deterministic tests.
    pub last_contract: Option<OverviewSurfaceContract>,
}

impl<B: WorkspaceBridge, T: ThumbnailProvider> OverviewSurface<B, T> {
    /// Read an initial compositor snapshot and prepare the surface adapter.
    pub fn new(
        bridge: B,
        mut thumbnails: T,
        output: OverviewOutput,
        mode: TokenMode,
    ) -> Result<Self, OverviewSurfaceError> {
        let snapshot = bridge.snapshot()?;
        // Probe thumbnail adapters at construction only through render; this
        // keeps a transient capture failure from preventing overview opening.
        let model = OverviewModel::new(snapshot)
            .map_err(|error| OverviewSurfaceError::Bridge(error.to_string()))?;
        let _ = &mut thumbnails;
        Ok(Self {
            bridge,
            thumbnails,
            model,
            output,
            mode,
            visible: false,
            last_contract: None,
        })
    }

    /// Refresh the model from the authoritative compositor bridge.
    pub fn refresh(&mut self) -> Result<(), OverviewSurfaceError> {
        self.model
            .refresh(self.bridge.snapshot()?)
            .map_err(|e| OverviewSurfaceError::Bridge(e.to_string()))
    }

    /// Enter overview, render, and present it through the native host.
    pub fn open(
        &mut self,
        host: &mut impl OverviewSurfaceHost,
    ) -> Result<(), OverviewSurfaceError> {
        self.refresh()?;
        self.model.enter();
        self.visible = true;
        host.set_keyboard_focus(true);
        self.present(host)
    }

    /// Hide overview and release keyboard focus.
    pub fn close(&mut self, host: &mut impl OverviewSurfaceHost) {
        self.visible = false;
        self.model.exit();
        host.set_keyboard_focus(false);
        host.dismiss();
    }

    /// Present the current model as a token-resolved native frame.
    pub fn present(
        &mut self,
        host: &mut impl OverviewSurfaceHost,
    ) -> Result<(), OverviewSurfaceError> {
        if !self.visible {
            return Ok(());
        }
        let contract = self.contract()?;
        let pixels = rasterize(&contract);
        host.present(&contract, &pixels);
        self.last_contract = Some(contract);
        Ok(())
    }

    /// Dispatch one overview action through the compositor bridge.
    pub fn dispatch(&mut self, action: WorkspaceAction) -> Result<(), OverviewSurfaceError> {
        self.bridge.dispatch(action)?;
        self.refresh()
    }

    /// Route a semantic overview key through the model, compositor bridge, and
    /// native host. Actions are followed by a fresh authoritative snapshot;
    /// Escape releases the layer surface without mutating compositor state.
    pub fn handle_key(
        &mut self,
        key: OverviewKey,
        host: &mut impl OverviewSurfaceHost,
    ) -> Result<OverviewOutcome, OverviewSurfaceError> {
        let outcome = self.model.handle_key(key);
        match &outcome {
            OverviewOutcome::Action(action) => {
                self.dispatch(action.clone())?;
                self.present(host)?;
            }
            OverviewOutcome::Hidden => self.close(host),
            OverviewOutcome::FocusChanged(_) => self.present(host)?,
            OverviewOutcome::Ignored => {}
        }
        Ok(outcome)
    }

    /// Build the current visual and accessibility contract.
    pub fn contract(&mut self) -> Result<OverviewSurfaceContract, OverviewSurfaceError> {
        let snapshots = self.model.snapshots();
        let margin = Spacing::Xl.px();
        let gap = Spacing::Md.px();
        let width = snapshots.workspaces.len().max(1) as f32;
        let workspace_width =
            ((self.output.logical_size.width - margin * 2.0 - gap * (width - 1.0)) / width)
                .max(1.0);
        let workspace_height = (self.output.logical_size.height * 0.82).max(1.0);
        let mut workspaces = Vec::with_capacity(snapshots.workspaces.len());
        let mut windows = Vec::new();
        for (index, workspace) in snapshots.workspaces.iter().enumerate() {
            let x = margin + index as f32 * (workspace_width + gap);
            let rect = (x, margin, workspace_width, workspace_height);
            let selected = self.model.focus() == OverviewFocus::Workspace(workspace.id);
            workspaces.push((workspace.id, workspace.label.clone(), rect, selected));
            windows.extend(layout_windows(
                workspace,
                self.model.focus(),
                rect,
                &mut self.thumbnails,
            ));
        }
        let accessibility =
            accessibility_tree(&snapshots, &self.model.focus(), &workspaces, &windows);
        Ok(OverviewSurfaceContract {
            output: self.output.clone(),
            physical_size: self
                .output
                .logical_size
                .physical_pixels(self.output.scale_factor),
            background: Color::Surface,
            border: Color::Border,
            accent: Color::Accent,
            workspaces,
            windows,
            accessibility,
            token_mode: self.mode,
        })
    }
}

fn layout_windows<T: ThumbnailProvider>(
    workspace: &WorkspaceSnapshot,
    focus: OverviewFocus,
    rect: (f32, f32, f32, f32),
    thumbnails: &mut T,
) -> Vec<WindowCard> {
    let padding = Spacing::Md.px();
    let content_width = (rect.2 - padding * 2.0).max(1.0);
    let card_height =
        ((rect.3 - padding * 2.0) / workspace.windows.len().max(1) as f32 - padding).max(1.0);
    workspace
        .windows
        .iter()
        .enumerate()
        .map(|(index, window)| {
            let y = rect.1 + padding + index as f32 * (card_height + padding);
            let has_thumbnail = thumbnails.thumbnail(window.id).ok().flatten().is_some();
            WindowCard {
                window: window.id,
                workspace: workspace.id,
                title: window.title.clone(),
                application: window.application.clone(),
                rect: (rect.0 + padding, y, content_width, card_height),
                focused: focus
                    == (OverviewFocus::Window {
                        workspace: workspace.id,
                        window: window.id,
                    }),
                has_thumbnail,
            }
        })
        .collect()
}

fn accessibility_tree(
    snapshots: &WorkspaceSnapshotSet,
    focus: &OverviewFocus,
    workspaces: &[(WorkspaceId, String, (f32, f32, f32, f32), bool)],
    windows: &[WindowCard],
) -> AccessibilityNode {
    let mut children = Vec::new();
    for (id, label, _, selected) in workspaces {
        let mut ws_windows = Vec::new();
        for window in windows.iter().filter(|window| window.workspace == *id) {
            ws_windows.push(AccessibilityNode {
                id: SemanticId::new(format!("overview.window.{}", window.window.get())),
                role: SemanticRole::Button,
                label: format!("{} - {}", window.title, window.application),
                value: None,
                state: AccessibilityState {
                    focused: window.focused,
                    selected: window.focused,
                    disabled: false,
                    editable: false,
                },
                children: Vec::new(),
            });
        }
        children.push(AccessibilityNode {
            id: SemanticId::new(format!("overview.workspace.{}", id.get())),
            role: SemanticRole::Group,
            label: label.clone(),
            value: None,
            state: AccessibilityState {
                focused: *selected,
                selected: *selected,
                disabled: false,
                editable: false,
            },
            children: ws_windows,
        });
    }
    AccessibilityNode {
        id: SemanticId::new("overview.surface"),
        role: SemanticRole::Group,
        label: format!("Workspace overview ({})", snapshots.workspaces.len()),
        value: Some(match focus {
            OverviewFocus::Workspace(id) => id.to_string(),
            OverviewFocus::Window { window, .. } => window.to_string(),
        }),
        state: AccessibilityState::default(),
        children,
    }
}

fn rasterize(contract: &OverviewSurfaceContract) -> Vec<u8> {
    let (width, height) = contract.physical_size;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    fill(&mut pixels, width, height, contract.background.rgba());
    for (_, _, rect, selected) in &contract.workspaces {
        fill_rect(
            &mut pixels,
            width,
            height,
            *rect,
            if *selected {
                contract.accent.rgba()
            } else {
                contract.border.rgba()
            },
            contract.output.scale_factor,
        );
    }
    for card in &contract.windows {
        fill_rect(
            &mut pixels,
            width,
            height,
            card.rect,
            Color::Elevated.rgba(),
            contract.output.scale_factor,
        );
    }
    pixels
}

fn fill(pixels: &mut [u8], width: u32, height: u32, color: sol_design::color::Rgba) {
    let value = [
        (color.0 * 255.0) as u8,
        (color.1 * 255.0) as u8,
        (color.2 * 255.0) as u8,
        (color.3 * 255.0) as u8,
    ];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&value);
    }
    let _ = (width, height);
}

fn fill_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rect: (f32, f32, f32, f32),
    color: sol_design::color::Rgba,
    scale: f32,
) {
    let left = (rect.0 * scale).max(0.0) as u32;
    let top = (rect.1 * scale).max(0.0) as u32;
    let right = ((rect.0 + rect.2) * scale).min(width as f32).max(0.0) as u32;
    let bottom = ((rect.1 + rect.3) * scale).min(height as f32).max(0.0) as u32;
    let value = [
        (color.0 * 255.0) as u8,
        (color.1 * 255.0) as u8,
        (color.2 * 255.0) as u8,
        (color.3 * 255.0) as u8,
    ];
    for y in top..bottom {
        for x in left..right {
            let index = ((y * width + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&value);
        }
    }
}

/// Small deterministic thumbnail provider useful for shell integration tests.
#[derive(Debug, Default)]
pub struct MemoryThumbnailProvider(pub BTreeMap<OverviewWindowId, WindowThumbnail>);

impl ThumbnailProvider for MemoryThumbnailProvider {
    fn thumbnail(
        &mut self,
        window: OverviewWindowId,
    ) -> Result<Option<WindowThumbnail>, OverviewSurfaceError> {
        Ok(self.0.get(&window).cloned())
    }
}

/// Bridge fixture that records typed compositor intents.
#[derive(Debug, Clone)]
pub struct MemoryWorkspaceBridge {
    /// Current authoritative snapshot.
    pub state: WorkspaceSnapshotSet,
    /// Actions sent by the native surface.
    pub actions: Vec<WorkspaceAction>,
}

impl WorkspaceBridge for MemoryWorkspaceBridge {
    fn snapshot(&self) -> Result<WorkspaceSnapshotSet, WorkspaceBridgeError> {
        Ok(self.state.clone())
    }
    fn dispatch(&mut self, action: WorkspaceAction) -> Result<(), WorkspaceBridgeError> {
        self.actions.push(action);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overview::{OverviewWindowSnapshot, WorkspaceSnapshot};

    #[derive(Default)]
    struct Host {
        presented: Vec<(OverviewSurfaceContract, usize)>,
        focused: Vec<bool>,
        dismissed: usize,
    }
    impl OverviewSurfaceHost for Host {
        fn present(&mut self, contract: &OverviewSurfaceContract, pixels: &[u8]) {
            self.presented.push((contract.clone(), pixels.len()));
        }
        fn dismiss(&mut self) {
            self.dismissed += 1;
        }
        fn set_keyboard_focus(&mut self, focused: bool) {
            self.focused.push(focused);
        }
    }

    fn fixture() -> WorkspaceSnapshotSet {
        WorkspaceSnapshotSet {
            active: WorkspaceId::new(1),
            workspaces: vec![
                WorkspaceSnapshot {
                    id: WorkspaceId::new(1),
                    label: "Main".into(),
                    windows: vec![OverviewWindowSnapshot {
                        id: OverviewWindowId::new(7),
                        title: "Files".into(),
                        application: "org.sol.files".into(),
                        focused: true,
                    }],
                },
                WorkspaceSnapshot {
                    id: WorkspaceId::new(2),
                    label: "Code".into(),
                    windows: vec![],
                },
            ],
        }
    }

    #[test]
    fn native_surface_projects_scaled_cards_and_accessibility() {
        let bridge = MemoryWorkspaceBridge {
            state: fixture(),
            actions: Vec::new(),
        };
        let output = OverviewOutput::new("eDP-1", LogicalSize::new(800.0, 600.0), 1.25).unwrap();
        let mut surface = OverviewSurface::new(
            bridge,
            MemoryThumbnailProvider::default(),
            output,
            TokenMode::dark(),
        )
        .unwrap();
        let mut host = Host::default();
        surface.open(&mut host).unwrap();
        assert_eq!(host.presented.len(), 1);
        assert_eq!(host.presented[0].0.physical_size, (1000, 750));
        assert_eq!(host.presented[0].1, 1000 * 750 * 4);
        assert_eq!(host.presented[0].0.windows[0].title, "Files");
        assert_eq!(host.presented[0].0.accessibility.children.len(), 2);
        surface.close(&mut host);
        assert_eq!(host.dismissed, 1);
    }

    #[test]
    fn malformed_thumbnail_is_rejected_before_allocation() {
        assert_eq!(
            WindowThumbnail::new(2, 2, vec![0; 3]),
            Err(OverviewSurfaceError::InvalidThumbnail)
        );
    }

    #[test]
    fn keyboard_actions_cross_the_bridge_and_escape_dismisses_the_host() {
        let bridge = MemoryWorkspaceBridge {
            state: fixture(),
            actions: Vec::new(),
        };
        let output = OverviewOutput::new("eDP-1", LogicalSize::new(800.0, 600.0), 1.0).unwrap();
        let mut surface = OverviewSurface::new(
            bridge,
            MemoryThumbnailProvider::default(),
            output,
            TokenMode::dark(),
        )
        .unwrap();
        let mut host = Host::default();
        surface.open(&mut host).unwrap();
        assert_eq!(
            surface.handle_key(OverviewKey::Enter, &mut host).unwrap(),
            OverviewOutcome::Action(WorkspaceAction::SwitchWorkspace {
                workspace: WorkspaceId::new(1)
            })
        );
        assert_eq!(surface.bridge.actions.len(), 1);
        assert_eq!(
            surface.handle_key(OverviewKey::Escape, &mut host).unwrap(),
            OverviewOutcome::Hidden
        );
        assert_eq!(host.dismissed, 1);
    }
}
