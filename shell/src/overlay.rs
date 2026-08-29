//! Renderer-neutral system overlay and layer-shell popup contracts.
//!
//! `sol-shell` owns the protocol-facing layer-shell lifecycle.  This module
//! deliberately models that lifecycle without importing SCP wire types so the
//! same contract can be exercised by a deterministic compositor fixture and
//! consumed by a future native layer-shell host.

use std::collections::BTreeMap;

use sol_design::{
    accessibility::TokenMode,
    motion::{Motion, MotionSpec},
};
use sol_ui::{
    AccessibilityNode, AccessibilityState, Button, ButtonController, ButtonFrame,
    FixtureSurfaceHost, InteractionTree, Key, KeyboardOutcome, LogicalSize, RecordingRenderer,
    SemanticControl, SemanticId, SemanticRole, present_button_for,
};

/// Stable identifier for a shell-owned overlay surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OverlayId(pub u64);

/// Stable identifier for an output supplied by the compositor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputId(String);

impl OutputId {
    /// Construct an output identifier. Empty identifiers are rejected when an
    /// [`OutputContract`] is registered.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Return the compositor-owned output name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Logical point in a compositor output's coordinate space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPoint {
    /// Horizontal logical coordinate.
    pub x: f32,
    /// Vertical logical coordinate.
    pub y: f32,
}

/// The output facts a layer-shell host must negotiate before placing an
/// overlay.  Physical output enumeration remains a compositor responsibility.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputContract {
    /// Stable compositor output identity.
    pub id: OutputId,
    /// Usable logical output extent for placement.
    pub logical_size: LogicalSize,
    /// Fractional scale advertised by the output.
    pub scale_factor: f32,
}

impl OutputContract {
    /// Validate and create an output contract.
    pub fn new(
        id: OutputId,
        logical_size: LogicalSize,
        scale_factor: f32,
    ) -> Result<Self, OverlayError> {
        if id.as_str().is_empty() {
            return Err(OverlayError::InvalidOutput("output id must not be empty"));
        }
        if !logical_size.width.is_finite()
            || !logical_size.height.is_finite()
            || logical_size.width <= 0.0
            || logical_size.height <= 0.0
        {
            return Err(OverlayError::InvalidOutput(
                "logical output extent must be finite and positive",
            ));
        }
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err(OverlayError::InvalidOutput(
                "output scale factor must be finite and positive",
            ));
        }
        Ok(Self {
            id,
            logical_size,
            scale_factor,
        })
    }
}

/// Semantic system-surface role. The mapping to a layer-shell layer is kept
/// in this shell-owned contract instead of leaking into SolUI or applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayRole {
    /// A persistent panel which may reserve output work area.
    Panel,
    /// A transient, pointer-transparent on-screen display.
    Osd,
    /// A transient keyboard-interactive menu.
    Menu,
    /// A transient keyboard-interactive contextual popover.
    Popover,
    /// A modal surface with an input-capturing scrim.
    Modal,
}

/// Layer selection requested from a native layer-shell adapter.
///
/// Ordered bottom to top, matching the compositor's own stacking order. The
/// desktop background is the one Shell surface that belongs *below* application
/// windows, which is why the enum reaches further down than the overlay roles
/// above ever need to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayerShellLayer {
    /// The desktop background: wallpaper, below every application window.
    Background,
    /// Chrome that sits on the desktop but still beneath application windows.
    Bottom,
    /// Panels are placed above regular application surfaces.
    Top,
    /// Transient system UI is placed in the overlay layer.
    Overlay,
}

impl OverlayRole {
    /// Resolve the only layer-shell layer allowed for this role.
    pub const fn layer(self) -> LayerShellLayer {
        match self {
            Self::Panel => LayerShellLayer::Top,
            Self::Osd | Self::Menu | Self::Popover | Self::Modal => LayerShellLayer::Overlay,
        }
    }
}

/// Edge or centre anchor used for compositor-independent placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// Top-left output corner.
    TopLeft,
    /// Top output edge, horizontally centred.
    Top,
    /// Top-right output corner.
    TopRight,
    /// Right output edge, vertically centred.
    Right,
    /// Bottom-right output corner.
    BottomRight,
    /// Bottom output edge, horizontally centred.
    Bottom,
    /// Bottom-left output corner.
    BottomLeft,
    /// Left output edge, vertically centred.
    Left,
    /// Output centre.
    Center,
}

/// Work-area reservation requested by a layer surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusiveZone {
    /// Do not reserve work area.
    None,
    /// Ask the compositor to derive a reservation from the surface size.
    Auto,
    /// Reserve exactly this many logical pixels.
    Fixed(u32),
}

/// Input ownership requested from the native surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRegion {
    /// The surface must not intercept pointer or keyboard input.
    PassThrough,
    /// The surface participates in input and keyboard focus lifecycle.
    Interactive,
}

/// An application-independent request to open one system overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayRequest {
    /// Caller-selected stable surface identity.
    pub id: OverlayId,
    /// Semantic surface role.
    pub role: OverlayRole,
    /// Target output.
    pub output: OutputId,
    /// Requested output anchor.
    pub anchor: Anchor,
    /// Requested logical content extent.
    pub logical_size: LogicalSize,
    /// Requested work-area reservation.
    pub exclusive_zone: ExclusiveZone,
    /// Requested input behavior.
    pub input_region: InputRegion,
    /// Whether Escape and the semantic dismiss button may close this overlay.
    pub dismissible: bool,
    /// Human-readable name projected to accessibility bridges.
    pub label: String,
}

impl OverlayRequest {
    /// Create a request with the role's safe default protocol contract.
    pub fn new(
        id: OverlayId,
        role: OverlayRole,
        output: OutputId,
        anchor: Anchor,
        logical_size: LogicalSize,
        label: impl Into<String>,
    ) -> Self {
        let (exclusive_zone, input_region, dismissible) = match role {
            OverlayRole::Panel => (ExclusiveZone::Auto, InputRegion::Interactive, false),
            OverlayRole::Osd => (ExclusiveZone::None, InputRegion::PassThrough, true),
            OverlayRole::Menu | OverlayRole::Popover => {
                (ExclusiveZone::None, InputRegion::Interactive, true)
            }
            OverlayRole::Modal => (ExclusiveZone::None, InputRegion::Interactive, true),
        };
        Self {
            id,
            role,
            output,
            anchor,
            logical_size,
            exclusive_zone,
            input_region,
            dismissible,
            label: label.into(),
        }
    }
}

/// Fully resolved native-layer contract emitted by an overlay manager.
#[derive(Debug, Clone)]
pub struct LayerShellSurfaceContract {
    /// Stable overlay identity.
    pub id: OverlayId,
    /// Semantic role retained for diagnostics and a11y projection.
    pub role: OverlayRole,
    /// Native layer selected for the role.
    pub layer: LayerShellLayer,
    /// Target compositor output.
    pub output: OutputId,
    /// Requested output anchor.
    pub anchor: Anchor,
    /// Work-area reservation contract.
    pub exclusive_zone: ExclusiveZone,
    /// Pointer/keyboard input contract.
    pub input_region: InputRegion,
    /// Resolved logical origin in the target output.
    pub logical_origin: LogicalPoint,
    /// Resolved logical extent.
    pub logical_size: LogicalSize,
    /// Physical extent computed only at the host scale boundary.
    pub physical_size: (u32, u32),
    /// Fractional scale advertised by the target output.
    pub scale_factor: f32,
    /// Whether a native host must create the input-capturing modal scrim.
    pub has_scrim: bool,
    /// Token-resolved transition policy for this surface.
    pub transition: MotionSpec,
    /// Theme and accessibility token mode supplied to the SolUI frame host.
    pub token_mode: TokenMode,
}

/// Errors returned before a native host sees an invalid surface contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayError {
    /// Output input facts were invalid.
    InvalidOutput(&'static str),
    /// Request geometry was invalid.
    InvalidGeometry,
    /// No registered compositor output matched the request.
    UnknownOutput(OutputId),
    /// The request combined a role with an unsafe protocol policy.
    InvalidRoleContract(&'static str),
    /// An overlay id was opened more than once.
    DuplicateOverlay(OverlayId),
}

/// Host callback boundary for a future native layer-shell adapter.
pub trait LayerShellOverlayHost {
    /// Create/configure the native surface from a validated contract.
    fn present_layer_surface(&mut self, surface: &LayerShellSurfaceContract);
    /// Destroy a dismissed native surface.
    fn dismiss_layer_surface(&mut self, id: OverlayId);
    /// Record the active keyboard target, if any.
    fn set_keyboard_focus(&mut self, id: Option<OverlayId>);
}

#[derive(Debug)]
struct OverlayInstance {
    request: OverlayRequest,
    contract: LayerShellSurfaceContract,
    tree: InteractionTree,
}

/// Outcome of one shell-level keyboard dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayKeyboardOutcome {
    /// No interactive overlay owned the key.
    Ignored,
    /// SolUI changed focus, selection, text, or activated an item.
    SolUi(KeyboardOutcome),
    /// Escape or the semantic dismiss control closed an overlay.
    Dismissed(OverlayId),
}

/// Retained overlay stack. It owns focus restoration and shell-level Escape
/// policy while SolUI owns traversal and component semantics inside a surface.
#[derive(Debug, Default)]
pub struct OverlayManager {
    outputs: BTreeMap<OutputId, OutputContract>,
    overlays: Vec<OverlayInstance>,
    focused: Option<OverlayId>,
}

impl OverlayManager {
    /// Register a compositor output contract.
    pub fn register_output(&mut self, output: OutputContract) {
        self.outputs.insert(output.id.clone(), output);
    }

    /// Return the active keyboard overlay, if any.
    pub const fn focused_overlay(&self) -> Option<OverlayId> {
        self.focused
    }

    /// Return resolved contracts in bottom-to-top presentation order.
    pub fn surfaces(&self) -> impl Iterator<Item = &LayerShellSurfaceContract> {
        self.overlays.iter().map(|overlay| &overlay.contract)
    }

    /// Open a validated overlay and present it through the host boundary.
    pub fn open(
        &mut self,
        host: &mut impl LayerShellOverlayHost,
        request: OverlayRequest,
        mode: TokenMode,
    ) -> Result<LayerShellSurfaceContract, OverlayError> {
        if self
            .overlays
            .iter()
            .any(|overlay| overlay.request.id == request.id)
        {
            return Err(OverlayError::DuplicateOverlay(request.id));
        }
        validate_request(&request)?;
        let output = self
            .outputs
            .get(&request.output)
            .ok_or_else(|| OverlayError::UnknownOutput(request.output.clone()))?;
        let contract = resolve_contract(&request, output, mode);
        let mut tree =
            InteractionTree::new(format!("overlay-{}", request.id.0), request.label.clone());
        if request.input_region == InputRegion::Interactive && request.dismissible {
            tree.push(SemanticControl::button(
                "dismiss",
                &Button::new().with_label("Dismiss overlay"),
            ));
        }
        host.present_layer_surface(&contract);
        self.overlays.push(OverlayInstance {
            request,
            contract: contract.clone(),
            tree,
        });
        self.restore_focus(host);
        Ok(contract)
    }

    /// Dismiss an overlay and restore keyboard focus to the next interactive
    /// surface below it.
    pub fn dismiss(&mut self, host: &mut impl LayerShellOverlayHost, id: OverlayId) -> bool {
        let Some(index) = self
            .overlays
            .iter()
            .position(|overlay| overlay.request.id == id)
        else {
            return false;
        };
        self.overlays.remove(index);
        host.dismiss_layer_surface(id);
        self.restore_focus(host);
        true
    }

    /// Dispatch a keyboard key to the top interactive surface. Escape is a
    /// shell policy: it dismisses only an explicitly dismissible surface.
    pub fn handle_key(
        &mut self,
        host: &mut impl LayerShellOverlayHost,
        key: Key,
    ) -> OverlayKeyboardOutcome {
        let Some(id) = self.focused else {
            return OverlayKeyboardOutcome::Ignored;
        };
        let Some(index) = self
            .overlays
            .iter()
            .position(|overlay| overlay.request.id == id)
        else {
            self.restore_focus(host);
            return OverlayKeyboardOutcome::Ignored;
        };
        if key == Key::Escape {
            if self.overlays[index].request.dismissible {
                self.dismiss(host, id);
                return OverlayKeyboardOutcome::Dismissed(id);
            }
            return OverlayKeyboardOutcome::Ignored;
        }
        let outcome = self.overlays[index].tree.handle_key(key);
        if matches!(&outcome, KeyboardOutcome::Activated(control) if control.as_str() == "dismiss")
        {
            self.dismiss(host, id);
            OverlayKeyboardOutcome::Dismissed(id)
        } else {
            OverlayKeyboardOutcome::SolUi(outcome)
        }
    }

    /// Build the semantic tree exported to a platform accessibility bridge.
    pub fn accessibility_tree(&self) -> AccessibilityNode {
        AccessibilityNode {
            id: SemanticId::new("system-overlays"),
            role: SemanticRole::Group,
            label: "System overlays".to_owned(),
            value: None,
            state: AccessibilityState::default(),
            children: self
                .overlays
                .iter()
                .flat_map(|overlay| {
                    let mut nodes = Vec::new();
                    if overlay.contract.has_scrim {
                        nodes.push(AccessibilityNode {
                            id: SemanticId::new(format!("overlay-{}.scrim", overlay.request.id.0)),
                            role: SemanticRole::Group,
                            label: "Modal scrim".to_owned(),
                            value: None,
                            state: AccessibilityState::default(),
                            children: Vec::new(),
                        });
                    }
                    nodes.push(overlay.tree.accessibility_tree());
                    nodes
                })
                .collect(),
        }
    }

    fn restore_focus(&mut self, host: &mut impl LayerShellOverlayHost) {
        self.focused = self
            .overlays
            .iter()
            .rev()
            .find(|overlay| overlay.request.input_region == InputRegion::Interactive)
            .map(|overlay| overlay.request.id);
        host.set_keyboard_focus(self.focused);
    }
}

fn validate_request(request: &OverlayRequest) -> Result<(), OverlayError> {
    if !request.logical_size.width.is_finite()
        || !request.logical_size.height.is_finite()
        || request.logical_size.width <= 0.0
        || request.logical_size.height <= 0.0
    {
        return Err(OverlayError::InvalidGeometry);
    }
    match request.role {
        OverlayRole::Panel if request.exclusive_zone == ExclusiveZone::None => Err(
            OverlayError::InvalidRoleContract("a panel must reserve an exclusive zone"),
        ),
        OverlayRole::Osd if request.input_region != InputRegion::PassThrough => Err(
            OverlayError::InvalidRoleContract("an OSD must be pointer and keyboard transparent"),
        ),
        OverlayRole::Osd if request.exclusive_zone != ExclusiveZone::None => Err(
            OverlayError::InvalidRoleContract("an OSD must not reserve output work area"),
        ),
        OverlayRole::Menu | OverlayRole::Popover | OverlayRole::Modal
            if request.input_region != InputRegion::Interactive =>
        {
            Err(OverlayError::InvalidRoleContract(
                "transient interactive surfaces require input ownership",
            ))
        }
        OverlayRole::Menu | OverlayRole::Popover | OverlayRole::Modal
            if request.exclusive_zone != ExclusiveZone::None =>
        {
            Err(OverlayError::InvalidRoleContract(
                "transient surfaces must not reserve output work area",
            ))
        }
        _ => Ok(()),
    }
}

fn resolve_contract(
    request: &OverlayRequest,
    output: &OutputContract,
    mode: TokenMode,
) -> LayerShellSurfaceContract {
    let logical_size = LogicalSize::new(
        request.logical_size.width.min(output.logical_size.width),
        request.logical_size.height.min(output.logical_size.height),
    );
    let origin = placement(request.anchor, output.logical_size, logical_size);
    LayerShellSurfaceContract {
        id: request.id,
        role: request.role,
        layer: request.role.layer(),
        output: output.id.clone(),
        anchor: request.anchor,
        exclusive_zone: request.exclusive_zone,
        input_region: request.input_region,
        logical_origin: origin,
        logical_size,
        physical_size: logical_size.physical_pixels(output.scale_factor),
        scale_factor: output.scale_factor,
        has_scrim: request.role == OverlayRole::Modal,
        transition: mode.motion_spec(Motion::Panel),
        token_mode: mode,
    }
}

fn placement(anchor: Anchor, output: LogicalSize, surface: LogicalSize) -> LogicalPoint {
    let right = (output.width - surface.width).max(0.0);
    let bottom = (output.height - surface.height).max(0.0);
    let centre_x = right / 2.0;
    let centre_y = bottom / 2.0;
    match anchor {
        Anchor::TopLeft => LogicalPoint { x: 0.0, y: 0.0 },
        Anchor::Top => LogicalPoint {
            x: centre_x,
            y: 0.0,
        },
        Anchor::TopRight => LogicalPoint { x: right, y: 0.0 },
        Anchor::Right => LogicalPoint {
            x: right,
            y: centre_y,
        },
        Anchor::BottomRight => LogicalPoint {
            x: right,
            y: bottom,
        },
        Anchor::Bottom => LogicalPoint {
            x: centre_x,
            y: bottom,
        },
        Anchor::BottomLeft => LogicalPoint { x: 0.0, y: bottom },
        Anchor::Left => LogicalPoint {
            x: 0.0,
            y: centre_y,
        },
        Anchor::Center => LogicalPoint {
            x: centre_x,
            y: centre_y,
        },
    }
}

/// Deterministic stand-in for a compositor layer-shell host. It validates the
/// SolUI hand-off (semantic frame + fractional scale) without claiming native
/// popup, GPU, or accessibility-bridge coverage.
#[derive(Debug, Default)]
pub struct HeadlessLayerShellFixture {
    /// Surfaces presented in lifecycle order.
    pub presented: Vec<LayerShellSurfaceContract>,
    /// Surface ids dismissed in lifecycle order.
    pub dismissed: Vec<OverlayId>,
    /// Last keyboard target selected by the manager.
    pub focus_history: Vec<Option<OverlayId>>,
    /// SolUI frames scheduled at the host edge with token-resolved state.
    pub solui_frames: Vec<SolUiFrameRecord>,
}

/// Deterministic record of one SolUI frame presented for an overlay contract.
#[derive(Debug, Clone, PartialEq)]
pub struct SolUiFrameRecord {
    /// The associated overlay surface.
    pub id: OverlayId,
    /// Physical extent at the output scale boundary.
    pub physical_size: (u32, u32),
    /// Number of frame callbacks requested from the fixture host.
    pub requested_frames: u32,
    /// Token-resolved retained SolUI frame.
    pub frame: ButtonFrame,
}

impl LayerShellOverlayHost for HeadlessLayerShellFixture {
    fn present_layer_surface(&mut self, surface: &LayerShellSurfaceContract) {
        let mut host = FixtureSurfaceHost::new(surface.logical_size, surface.scale_factor);
        let mut renderer = RecordingRenderer::default();
        let button = ButtonController::new(Button::new().with_label("Overlay frame"));
        present_button_for(&mut host, &mut renderer, &button, surface.token_mode);
        let frame = renderer.frames.pop().expect("one recorded SolUI frame");
        self.solui_frames.push(SolUiFrameRecord {
            id: surface.id,
            physical_size: surface.physical_size,
            requested_frames: host.requested_frames,
            frame,
        });
        self.presented.push(surface.clone());
    }

    fn dismiss_layer_surface(&mut self, id: OverlayId) {
        self.dismissed.push(id);
    }

    fn set_keyboard_focus(&mut self, id: Option<OverlayId>) {
        self.focus_history.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(id: &str, size: LogicalSize, scale: f32) -> OutputContract {
        OutputContract::new(OutputId::new(id), size, scale).unwrap()
    }

    fn popover(id: u64, output: &str) -> OverlayRequest {
        OverlayRequest::new(
            OverlayId(id),
            OverlayRole::Popover,
            OutputId::new(output),
            Anchor::BottomRight,
            LogicalSize::new(320.0, 48.0),
            "Quick settings",
        )
    }

    #[test]
    fn headless_compositor_solui_fixture_validates_popup_placement_scale_and_lifecycle() {
        let mut manager = OverlayManager::default();
        manager.register_output(output("main", LogicalSize::new(1920.0, 1080.0), 1.0));
        manager.register_output(output("aux", LogicalSize::new(1280.0, 720.0), 1.25));
        let mut host = HeadlessLayerShellFixture::default();

        let contract = manager
            .open(
                &mut host,
                popover(7, "aux"),
                TokenMode::dark().high_contrast(),
            )
            .unwrap();

        assert_eq!(contract.layer, LayerShellLayer::Overlay);
        assert_eq!(contract.logical_origin, LogicalPoint { x: 960.0, y: 672.0 });
        assert_eq!(contract.physical_size, (400, 60));
        assert_eq!(contract.input_region, InputRegion::Interactive);
        assert_eq!(manager.focused_overlay(), Some(OverlayId(7)));
        assert_eq!(host.solui_frames.len(), 1);
        assert_eq!(host.solui_frames[0].id, OverlayId(7));
        assert_eq!(host.solui_frames[0].physical_size, (400, 60));
        assert_eq!(host.solui_frames[0].requested_frames, 1);
        assert_eq!(
            host.solui_frames[0].frame.background,
            TokenMode::dark()
                .high_contrast()
                .color(sol_design::color::Color::Elevated)
        );
        assert!(matches!(
            manager.handle_key(&mut host, Key::Tab),
            OverlayKeyboardOutcome::SolUi(KeyboardOutcome::FocusMoved(_))
        ));
        assert_eq!(
            manager.handle_key(&mut host, Key::Escape),
            OverlayKeyboardOutcome::Dismissed(OverlayId(7))
        );
        assert_eq!(host.dismissed, vec![OverlayId(7)]);
        assert_eq!(host.focus_history.last(), Some(&None));
    }

    #[test]
    fn modal_scrim_has_semantic_projection_and_restores_underlying_focus() {
        let mut manager = OverlayManager::default();
        manager.register_output(output("main", LogicalSize::new(800.0, 600.0), 1.0));
        let mut host = HeadlessLayerShellFixture::default();
        manager
            .open(&mut host, popover(1, "main"), TokenMode::light())
            .unwrap();
        let modal = OverlayRequest::new(
            OverlayId(2),
            OverlayRole::Modal,
            OutputId::new("main"),
            Anchor::Center,
            LogicalSize::new(400.0, 300.0),
            "Confirm action",
        );
        let contract = manager.open(&mut host, modal, TokenMode::light()).unwrap();

        assert!(contract.has_scrim);
        assert_eq!(manager.focused_overlay(), Some(OverlayId(2)));
        let a11y = manager.accessibility_tree();
        assert!(a11y.children.iter().any(|node| node.label == "Modal scrim"));
        assert_eq!(
            manager.handle_key(&mut host, Key::Escape),
            OverlayKeyboardOutcome::Dismissed(OverlayId(2))
        );
        assert_eq!(manager.focused_overlay(), Some(OverlayId(1)));
    }

    #[test]
    fn osd_is_pass_through_and_never_takes_keyboard_focus() {
        let mut manager = OverlayManager::default();
        manager.register_output(output("main", LogicalSize::new(800.0, 600.0), 1.0));
        let mut host = HeadlessLayerShellFixture::default();
        manager
            .open(&mut host, popover(1, "main"), TokenMode::light())
            .unwrap();
        let osd = OverlayRequest::new(
            OverlayId(2),
            OverlayRole::Osd,
            OutputId::new("main"),
            Anchor::Top,
            LogicalSize::new(180.0, 48.0),
            "Volume",
        );
        let contract = manager.open(&mut host, osd, TokenMode::light()).unwrap();

        assert_eq!(contract.input_region, InputRegion::PassThrough);
        assert_eq!(contract.exclusive_zone, ExclusiveZone::None);
        assert_eq!(manager.focused_overlay(), Some(OverlayId(1)));
    }

    #[test]
    fn invalid_role_policy_and_unknown_output_are_rejected_before_host_presentation() {
        let mut manager = OverlayManager::default();
        manager.register_output(output("main", LogicalSize::new(800.0, 600.0), 1.0));
        let mut host = HeadlessLayerShellFixture::default();
        let mut invalid = popover(3, "main");
        invalid.exclusive_zone = ExclusiveZone::Fixed(20);
        assert!(matches!(
            manager.open(&mut host, invalid, TokenMode::light()),
            Err(OverlayError::InvalidRoleContract(
                "transient surfaces must not reserve output work area"
            ))
        ));
        assert!(matches!(
            manager.open(&mut host, popover(4, "missing"), TokenMode::light()),
            Err(OverlayError::UnknownOutput(_))
        ));
        assert!(host.presented.is_empty());
    }

    #[test]
    fn reduced_motion_is_resolved_only_from_accessibility_tokens() {
        let mut manager = OverlayManager::default();
        manager.register_output(output("main", LogicalSize::new(800.0, 600.0), 1.0));
        let mut host = HeadlessLayerShellFixture::default();
        let surface = manager
            .open(
                &mut host,
                popover(3, "main"),
                TokenMode::dark().reduced_motion(),
            )
            .unwrap();

        assert_eq!(surface.transition.duration_ms, 0);
        assert!(surface.transition.spring.is_none());
    }
}
