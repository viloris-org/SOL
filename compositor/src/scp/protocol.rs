//! SCP wire protocol message definitions.
//!
//! These types define the messages exchanged between clients and the compositor.
//! In the full implementation, these would be generated from protobuf/Cap'n Proto
//! schemas. For Phase 1, we use plain Rust enums with serde for serialization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type SurfaceId = u32;
pub type SessionId = u64;
pub type ToplevelId = u32;
pub type PopupId = u32;
pub type OutputId = u32;
pub type BufferId = u32;
pub type PoolId = u32;
pub type LayerSurfaceId = u32;
pub type LockId = u32;
pub type LockSurfaceId = u32;

/// Client → Compositor messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Establish connection and authenticate
    Connect { app_id: String, pid: u32 },

    /// Create a new surface
    CreateSurface { surface_id: SurfaceId },

    /// Destroy a surface
    DestroySurface { surface_id: SurfaceId },

    /// Attach a buffer to a surface
    AttachBuffer {
        surface_id: SurfaceId,
        /// Populated by the server from SCM_RIGHTS, never from JSON.
        #[serde(skip, default = "invalid_fd")]
        buffer_fd: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: BufferFormat,
    },

    /// Commit pending surface state
    Commit {
        surface_id: SurfaceId,
        /// Optional callback ID for frame timing
        frame_callback: Option<u32>,
    },

    /// Request capability authorization
    RequestCapability {
        capability: String, // Simplified for now
        justification: String,
    },

    /// Create a toplevel window (requires WindowToplevel capability)
    CreateToplevel {
        surface_id: SurfaceId,
        capability_token: Vec<u8>, // Simplified token
        title: String,
    },

    /// Set toplevel title
    SetToplevelTitle {
        toplevel_id: ToplevelId,
        title: String,
    },

    /// Request fullscreen
    SetFullscreen {
        toplevel_id: ToplevelId,
        capability_token: Vec<u8>,
    },

    /// Acknowledge a configure event
    AckConfigure {
        toplevel_id: ToplevelId,
        serial: u32,
    },

    // ===== Buffer Management =====
    /// Create a shared memory pool
    CreateShmPool {
        pool_id: PoolId,
        #[serde(skip, default = "invalid_fd")]
        fd: i32,
        size: usize,
    },

    /// Create a buffer from a pool
    CreateBuffer {
        buffer_id: BufferId,
        pool_id: PoolId,
        offset: usize,
        width: i32,
        height: i32,
        stride: i32,
        format: ShmFormat,
    },

    /// Destroy a buffer
    DestroyBuffer { buffer_id: BufferId },

    /// Mark damaged region (optimization)
    Damage {
        surface_id: SurfaceId,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },

    /// Set input region
    SetInputRegion {
        surface_id: SurfaceId,
        rects: Vec<Rect>,
    },

    /// Set opaque region
    SetOpaqueRegion {
        surface_id: SurfaceId,
        rects: Vec<Rect>,
    },

    // ===== Popup Windows =====
    /// Create a popup window (requires WindowPopup capability).
    ///
    /// `parent_id` may name a toplevel surface or another popup's surface; the
    /// latter forms a nested popup chain.
    CreatePopup {
        surface_id: SurfaceId,
        parent_id: SurfaceId,
        positioner: PopupPositioner,
        grab: bool,
    },

    /// Dismiss a popup this client created.
    DestroyPopup { popup_id: PopupId },

    // ===== Toplevel State Management =====
    /// Request state change (maximize, minimize, fullscreen)
    SetToplevelState {
        toplevel_id: ToplevelId,
        state: ToplevelStateRequest,
    },

    /// Set toplevel app_id
    SetToplevelAppId {
        toplevel_id: ToplevelId,
        app_id: String,
    },

    /// Close a toplevel this client owns, releasing its window and popups.
    CloseToplevel { toplevel_id: ToplevelId },

    // ===== Input =====
    /// Set cursor image
    SetCursor {
        serial: u32,
        surface_id: Option<SurfaceId>,
        hotspot_x: i32,
        hotspot_y: i32,
    },

    // ===== Layer Shell =====
    /// Create a layer surface (requires LayerShell capability)
    CreateLayerSurface {
        surface_id: SurfaceId,
        capability_token: Vec<u8>,
        layer: LayerShellLayer,
        namespace: String,
        output_id: Option<OutputId>,
    },

    /// Set layer surface anchor edges
    SetLayerAnchor {
        layer_id: LayerSurfaceId,
        top: bool,
        bottom: bool,
        left: bool,
        right: bool,
    },

    /// Set layer surface exclusive zone
    SetLayerExclusiveZone { layer_id: LayerSurfaceId, zone: i32 },

    /// Set layer surface margins
    SetLayerMargin {
        layer_id: LayerSurfaceId,
        top: i32,
        right: i32,
        bottom: i32,
        left: i32,
    },

    /// Set layer surface keyboard interactivity
    SetLayerKeyboardInteractivity {
        layer_id: LayerSurfaceId,
        interactivity: LayerKeyboardInteractivity,
    },

    /// Set layer surface size
    SetLayerSize {
        layer_id: LayerSurfaceId,
        width: i32,
        height: i32,
    },

    /// Acknowledge layer surface configure
    AckLayerConfigure {
        layer_id: LayerSurfaceId,
        serial: u32,
    },

    // ===== Session Lock =====
    /// Engage the session lock (requires SessionLock capability).
    ///
    /// The lock takes effect immediately: other clients stop receiving input
    /// before this client has drawn anything. The client must then cover every
    /// output with a lock surface, after which the compositor replies
    /// `SessionLocked`.
    LockSession { capability_token: Vec<u8> },

    /// Create a lock surface covering one output.
    ///
    /// `output_id` omitted means the primary output. Lock surfaces are always
    /// output-sized; there is no anchor, margin, or client-chosen geometry.
    CreateLockSurface {
        surface_id: SurfaceId,
        lock_id: LockId,
        output_id: Option<OutputId>,
    },

    /// Acknowledge a lock surface configure
    AckLockConfigure {
        lock_surface_id: LockSurfaceId,
        serial: u32,
    },

    /// Release the session lock after successful authentication.
    ///
    /// Only the lock's owner may unlock, and only once the lock has engaged on
    /// every output.
    UnlockSession { lock_id: LockId },

    // ===== Data Transfer (Clipboard/DnD) =====
    /// Offer data to clipboard (requires ClipboardWrite capability + recent interaction)
    SetSelection {
        mime_types: Vec<String>,
        serial: u32, // Must match recent input serial
    },

    /// Read the current clipboard selection (requires ClipboardRead capability
    /// and foreground focus).
    ///
    /// The compositor creates a pipe, hands the write end to the selection owner
    /// in `RequestSelectionData`, and returns the read end here in
    /// `SelectionData`. Content flows directly between the two clients: the
    /// compositor never buffers or inspects clipboard bytes.
    RequestSelection { mime_type: String },

    /// Start drag-and-drop operation (requires DragAndDrop capability)
    StartDrag {
        surface_id: SurfaceId,
        origin_surface: SurfaceId,
        icon_surface: Option<SurfaceId>,
        mime_types: Vec<String>,
        serial: u32,
    },

    /// Read the dragged data, sent by the drop target after `Drop`.
    ///
    /// Uses the same pipe handoff as `RequestSelection`: the source receives the
    /// write end via `RequestDragData` and the target the read end via
    /// `DragData`.
    ReceiveDragData { mime_type: String },

    /// Accept a drag offer
    AcceptDrag {
        serial: u32,
        mime_type: Option<String>,
    },

    /// Finish drag operation
    FinishDrag,

    /// Cancel ongoing drag
    CancelDrag,
}

/// Compositor → Client messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompositorMessage {
    /// Connection established
    Connected {
        session_id: SessionId,
        granted_capabilities: Vec<String>,
        /// Opaque tokens keyed by their stable capability name.
        capability_tokens: HashMap<String, Vec<u8>>,
    },

    /// Connection rejected
    Rejected { reason: String },

    /// Capability request decision
    CapabilityDecision {
        capability: String,
        granted: bool,
        token: Option<Vec<u8>>,
        reason: Option<String>,
        needs_user_consent: bool,
    },

    /// A malformed or invalid request. Fatal errors close the connection.
    ProtocolError {
        code: String,
        message: String,
        fatal: bool,
    },

    /// Configure a toplevel window
    ConfigureToplevel {
        toplevel_id: ToplevelId,
        serial: u32,
        width: i32,
        height: i32,
        decoration_height: i32, // Height reserved for compositor-drawn titlebar
        states: ToplevelStates,
    },

    /// Toplevel closed (user clicked close button)
    ToplevelClosed { toplevel_id: ToplevelId },

    /// Frame callback — client can submit next frame
    FrameCallback {
        surface_id: SurfaceId,
        callback_id: u32,
        timestamp_ms: u64,
    },

    /// Input event
    InputEvent {
        surface_id: SurfaceId,
        event: InputEvent,
    },

    /// Output configuration changed
    OutputChanged { width: i32, height: i32, scale: f64 },

    /// Buffer can be reused
    BufferRelease { buffer_id: BufferId },

    /// Popup configured
    ConfigurePopup {
        popup_id: PopupId,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },

    /// Popup dismissed
    PopupDismissed {
        popup_id: PopupId,
        reason: DismissReason,
    },

    // ===== Output Management =====
    /// New output added
    OutputAdded {
        output_id: OutputId,
        name: String,
        description: String,
        geometry: Rect,
        physical_size: (i32, i32),
        subpixel: SubpixelLayout,
        transform: Transform,
        scale: i32,
        modes: Vec<OutputMode>,
        current_mode: usize,
    },

    /// Output removed
    OutputRemoved { output_id: OutputId },

    /// Output geometry changed
    OutputGeometryChanged { output_id: OutputId, geometry: Rect },

    /// Output scale changed
    OutputScaleChanged { output_id: OutputId, scale: i32 },

    /// Output mode changed
    OutputModeChanged {
        output_id: OutputId,
        mode: OutputMode,
    },

    /// Surface entered output
    SurfaceEnterOutput {
        surface_id: SurfaceId,
        output_id: OutputId,
    },

    /// Surface left output
    SurfaceLeaveOutput {
        surface_id: SurfaceId,
        output_id: OutputId,
    },

    // ===== Keyboard Input =====
    /// Keyboard keymap (XKB)
    KeymapFormat {
        format: KeymapFormat,
        #[serde(skip, default = "invalid_fd")]
        fd: i32,
        size: u32,
    },

    /// Keyboard repeat info
    RepeatInfo { rate: i32, delay: i32 },

    /// Keyboard modifiers
    Modifiers {
        surface_id: SurfaceId,
        serial: u32,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
    },

    // ===== Layer Shell =====
    /// Configure layer surface
    ConfigureLayerSurface {
        layer_id: LayerSurfaceId,
        serial: u32,
        width: i32,
        height: i32,
    },

    /// Layer surface closed
    LayerSurfaceClosed { layer_id: LayerSurfaceId },

    // ===== Session Lock =====
    /// The lock is engaged and owned by this client.
    ///
    /// Sent as the direct reply to `LockSession`, before any lock surface
    /// exists: the client needs `lock_id` in order to create them. The desktop
    /// is already cut off at this point, but nothing is drawn yet.
    SessionLockEngaged { lock_id: LockId },

    /// The session is locked: every output is covered by an acknowledged lock
    /// surface. Sent only to the lock's owner.
    SessionLocked { lock_id: LockId },

    /// The compositor will not honor this client's lock, or has taken it away.
    ///
    /// The client owns no lock objects after this and must not assume the
    /// session is locked or unlocked on its behalf.
    SessionLockFinished { reason: String },

    /// Configure a lock surface. Always the full output size.
    ConfigureLockSurface {
        lock_surface_id: LockSurfaceId,
        serial: u32,
        width: i32,
        height: i32,
    },

    /// The session's lock state changed, broadcast to every other client so it
    /// can drop sensitive on-screen content while it is not visible.
    SessionLockStateChanged { locked: bool },

    // ===== Data Transfer (Clipboard/DnD) =====
    /// Selection offered (clipboard content available)
    SelectionOffer { mime_types: Vec<String> },

    /// Sent to the selection owner: write the named MIME type into `fd`, then
    /// close it. The compositor never inspects clipboard bytes.
    RequestSelectionData {
        mime_type: String,
        #[serde(skip, default = "invalid_fd")]
        fd: i32,
    },

    /// Sent to a client that called `RequestSelection`: read the clipboard
    /// content from `fd` until EOF.
    SelectionData {
        mime_type: String,
        #[serde(skip, default = "invalid_fd")]
        fd: i32,
    },

    /// Selection cleared
    SelectionCleared,

    /// Drag enter surface
    DragEnter {
        serial: u32,
        surface_id: SurfaceId,
        x: f64,
        y: f64,
        mime_types: Vec<String>,
    },

    /// Drag motion over surface
    DragMotion { x: f64, y: f64, time_ms: u32 },

    /// Drag leave surface
    DragLeave,

    /// Drop occurred
    Drop,

    /// Sent to the drag source: write the named MIME type into `fd`, then close
    /// it.
    RequestDragData {
        mime_type: String,
        #[serde(skip, default = "invalid_fd")]
        fd: i32,
    },

    /// Sent to the drop target: read the dragged content from `fd` until EOF.
    DragData {
        mime_type: String,
        #[serde(skip, default = "invalid_fd")]
        fd: i32,
    },

    /// Drag finished successfully
    DragFinished,

    /// Drag cancelled
    DragCancelled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToplevelStates {
    pub activated: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub resizing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BufferFormat {
    Argb8888,
    Xrgb8888,
    Rgba8888,
}

impl BufferFormat {
    /// Bytes one pixel occupies, which is what a stride must at minimum cover.
    pub const fn bytes_per_pixel(self) -> i32 {
        match self {
            Self::Argb8888 | Self::Xrgb8888 | Self::Rgba8888 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShmFormat {
    Argb8888,
    Xrgb8888,
    Rgb565,
}

impl ShmFormat {
    /// Bytes one pixel occupies, which is what a stride must at minimum cover.
    pub const fn bytes_per_pixel(self) -> i32 {
        match self {
            Self::Argb8888 | Self::Xrgb8888 => 4,
            Self::Rgb565 => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopupPositioner {
    pub anchor_rect: Rect,
    pub anchor_edge: Edge,
    pub gravity: Gravity,
    pub constraint: ConstraintAdjustment,
    pub offset: (i32, i32),
    pub size: (i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gravity {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintAdjustment {
    pub flip_x: bool,
    pub flip_y: bool,
    pub slide_x: bool,
    pub slide_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DismissReason {
    OutsideClick,
    ParentClosed,
    EscapeKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToplevelStateRequest {
    Maximize,
    Minimize,
    Fullscreen { output_id: Option<OutputId> },
    UnsetMaximize,
    UnsetFullscreen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputMode {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: i32, // mHz (60000 = 60Hz)
    pub preferred: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubpixelLayout {
    Unknown,
    None,
    HorizontalRgb,
    HorizontalBgr,
    VerticalRgb,
    VerticalBgr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeymapFormat {
    NoKeymap,
    XkbV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    PointerEnter {
        serial: u32,
        x: f64,
        y: f64,
    },
    PointerLeave {
        serial: u32,
    },
    PointerMotion {
        x: f64,
        y: f64,
        time_ms: u32,
    },
    PointerButton {
        serial: u32,
        button: u32,
        state: ButtonState,
        time_ms: u32,
    },
    PointerAxis {
        time_ms: u32,
        axis_source: AxisSource,
        orientation: Orientation,
        value: f64,
        discrete: i32,
    },
    PointerFrame,
    KeyboardEnter {
        serial: u32,
        keys: Vec<u32>,
    },
    KeyboardLeave {
        serial: u32,
    },
    KeyboardKey {
        serial: u32,
        key: u32,
        state: KeyState,
        time_ms: u32,
    },
    TouchDown {
        serial: u32,
        touch_id: i32,
        x: f64,
        y: f64,
        time_ms: u32,
    },
    TouchUp {
        serial: u32,
        touch_id: i32,
        time_ms: u32,
    },
    TouchMotion {
        touch_id: i32,
        x: f64,
        y: f64,
        time_ms: u32,
    },
    TouchCancel,
    TouchFrame,
    TouchShape {
        touch_id: i32,
        major: f64,
        minor: f64,
    },
    TouchOrientation {
        touch_id: i32,
        orientation: f64,
    },
    Modifiers {
        serial: u32,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisSource {
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerShellLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerKeyboardInteractivity {
    None,
    Exclusive,
    OnDemand,
}

const fn invalid_fd() -> i32 {
    -1
}
