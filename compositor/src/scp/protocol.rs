//! SCP wire protocol message definitions.
//!
//! These types define the messages exchanged between clients and the compositor.
//! The ergonomic domain types here are translated to the generated Protobuf
//! contract by [`super::wire`]. Serde remains derived for diagnostics only; it
//! is not used by the SCP socket transport.

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
pub type CaptureId = u64;
pub type ShortcutId = u64;

/// Old clients speak version 1 through `Connect`; version-aware clients use
/// `ConnectVersioned` and negotiate the highest mutually supported version.
pub const MIN_PROTOCOL_VERSION: u32 = 1;
pub const CURRENT_PROTOCOL_VERSION: u32 = 2;

/// Client → Compositor messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Establish connection and authenticate
    Connect {
        app_id: String,
        pid: u32,
    },

    /// Establish a connection while explicitly negotiating the wire version.
    ///
    /// `Connect` remains the version-1 compatibility handshake.
    ConnectVersioned {
        app_id: String,
        pid: u32,
        min_version: u32,
        max_version: u32,
    },

    /// Create a new surface
    CreateSurface {
        surface_id: SurfaceId,
    },

    /// Destroy a surface
    DestroySurface {
        surface_id: SurfaceId,
    },

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

    /// Import a Linux DMA-BUF image. Descriptors are supplied by SCM_RIGHTS;
    /// fd_index in each plane addresses that out-of-band descriptor array.
    CreateDmabufBuffer {
        buffer_id: BufferId,
        width: i32,
        height: i32,
        format: DmabufFormat,
        modifier: u64,
        planes: Vec<DmabufPlane>,
        #[serde(skip, default)]
        fds: Vec<i32>,
    },

    /// Attach a previously-created shared-memory buffer to a surface.
    ///
    /// Unlike the legacy descriptor-per-frame `AttachBuffer`, this request
    /// gives the buffer a stable client-owned id. The compositor sends
    /// `BufferRelease` with that id once the previous commit is no longer being
    /// read, allowing bounded double- or triple-buffering.
    AttachShmBuffer {
        surface_id: SurfaceId,
        buffer_id: BufferId,
    },

    /// Attach a previously imported DMA-BUF to a surface.
    AttachDmabufBuffer {
        surface_id: SurfaceId,
        buffer_id: BufferId,
    },

    /// Atomically unmap a surface on its next commit.
    DetachBuffer {
        surface_id: SurfaceId,
    },

    /// Destroy a buffer
    DestroyBuffer {
        buffer_id: BufferId,
    },

    /// Destroy a shared-memory pool after all buffers created from it are gone.
    DestroyShmPool {
        pool_id: PoolId,
    },

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
    DestroyPopup {
        popup_id: PopupId,
    },

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
    CloseToplevel {
        toplevel_id: ToplevelId,
    },

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
    SetLayerExclusiveZone {
        layer_id: LayerSurfaceId,
        zone: i32,
    },

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
    LockSession {
        capability_token: Vec<u8>,
    },

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
    UnlockSession {
        lock_id: LockId,
    },

    /// Admit processes belonging to the authenticated user to this compositor.
    ///
    /// Only the owner of a fully engaged session lock may do this. The
    /// authorization is installed before the user's Shell starts, so the
    /// desktop can prepare underneath the still-protected lock surface.
    AuthorizeSessionUser {
        lock_id: LockId,
        uid: u32,
    },

    /// Remove the cross-UID admission installed for a completed user session.
    ///
    /// The session-lock capability is required because this is valid while the
    /// greeter is between its unlocked and re-locked states.
    RevokeSessionUser {
        uid: u32,
        capability_token: Vec<u8>,
    },

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
    RequestSelection {
        mime_type: String,
    },

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
    ReceiveDragData {
        mime_type: String,
    },

    /// Accept a drag offer
    AcceptDrag {
        serial: u32,
        mime_type: Option<String>,
    },

    /// Finish drag operation
    FinishDrag,

    /// Cancel ongoing drag
    CancelDrag,

    /// Negotiate the operation performed by the active drag.
    SetDragActions {
        actions: Vec<DragAction>,
        preferred: Option<DragAction>,
    },

    // ===== Screen capture =====
    /// Capture one frame. The capability token is consumed after the frame has
    /// been queued, so every subsequent capture requires a fresh grant.
    RequestCapture {
        target: CaptureTarget,
        cursor_mode: CursorMode,
        capability_token: Vec<u8>,
    },

    // ===== Global shortcuts =====
    RegisterShortcut {
        binding: KeyBinding,
        justification: String,
        capability_token: Vec<u8>,
    },

    UnregisterShortcut {
        shortcut_id: ShortcutId,
    },
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

    /// Result of a version-aware handshake. It is sent before `Connected`.
    ProtocolVersion { version: u32, features: Vec<String> },

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

    /// A previously granted capability stopped being valid at runtime.
    CapabilityRevoked { capability: String, reason: String },

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

    /// The authenticated UID may now connect clients to this compositor.
    SessionUserAuthorized { uid: u32 },

    /// The completed user session's compositor admission has been removed.
    SessionUserRevoked { uid: u32 },

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

    /// Final operation selected for the active drag.
    DragActionSelected { action: DragAction },

    /// A capture frame in tightly-packed RGBA8888 form. `fd` contains exactly
    /// `stride * height` immutable bytes and is delivered through SCM_RIGHTS.
    CaptureGranted {
        capture_id: CaptureId,
        width: u32,
        height: u32,
        stride: u32,
        format: CaptureFormat,
        cursor_mode: CursorMode,
        #[serde(skip, default = "invalid_fd")]
        fd: i32,
    },

    ShortcutGranted {
        shortcut_id: ShortcutId,
        binding: KeyBinding,
        priority: ShortcutPriority,
    },

    ShortcutRevoked {
        shortcut_id: ShortcutId,
        reason: String,
    },

    ShortcutActivated {
        shortcut_id: ShortcutId,
        timestamp_ms: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DragAction {
    Copy,
    Move,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureTarget {
    Window(ToplevelId),
    Output(OutputId),
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorMode {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureFormat {
    Rgba8888,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyBinding {
    /// XKB keycode (evdev code + 8), matching `InputEvent::KeyboardKey`.
    pub keycode: u32,
    /// XKB-compatible modifier mask from `scp::keymap::modifiers`.
    pub modifiers: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShortcutPriority {
    App,
    Shell,
    System,
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
    Rgb565,
}

/// Concrete DRM formats accepted by the SCP DMA-BUF import path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DmabufFormat {
    Argb8888,
    Xrgb8888,
    Abgr8888,
    Xbgr8888,
    Rgb565,
    Nv12,
}

impl DmabufFormat {
    pub const fn plane_count(self) -> usize {
        match self {
            Self::Nv12 => 2,
            _ => 1,
        }
    }
}

/// One DMA-BUF image plane. fd_index addresses the SCM_RIGHTS descriptor
/// array rather than trusting a process-local integer from the wire payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmabufPlane {
    pub fd_index: u32,
    pub offset: u32,
    pub stride: u32,
}

pub const DRM_FORMAT_MOD_LINEAR: u64 = 0;
pub const DRM_FORMAT_MOD_INVALID: u64 = u64::MAX;

impl BufferFormat {
    /// Bytes one pixel occupies, which is what a stride must at minimum cover.
    pub const fn bytes_per_pixel(self) -> i32 {
        match self {
            Self::Argb8888 | Self::Xrgb8888 | Self::Rgba8888 => 4,
            Self::Rgb565 => 2,
        }
    }
}

impl From<ShmFormat> for BufferFormat {
    fn from(format: ShmFormat) -> Self {
        match format {
            ShmFormat::Argb8888 => Self::Argb8888,
            ShmFormat::Xrgb8888 => Self::Xrgb8888,
            ShmFormat::Rgb565 => Self::Rgb565,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::wire::WireMessage;

    #[test]
    fn v1_and_v2_handshakes_round_trip() {
        for message in [
            ClientMessage::Connect {
                app_id: "org.sol.test".to_string(),
                pid: 42,
            },
            ClientMessage::ConnectVersioned {
                app_id: "org.sol.test".to_string(),
                pid: 42,
                min_version: 1,
                max_version: 2,
            },
        ] {
            let encoded = message.encode_wire().unwrap();
            let decoded = ClientMessage::decode_wire(&encoded).unwrap();
            assert_eq!(
                std::mem::discriminant(&decoded),
                std::mem::discriminant(&message)
            );
        }
    }

    #[test]
    fn descriptor_numbers_never_enter_capture_wire_payload() {
        let message = CompositorMessage::CaptureGranted {
            capture_id: 7,
            width: 10,
            height: 8,
            stride: 40,
            format: CaptureFormat::Rgba8888,
            cursor_mode: CursorMode::Exclude,
            fd: 123_456,
        };
        let encoded = message.encode_wire().unwrap();
        let decoded = CompositorMessage::decode_wire(&encoded).unwrap();
        assert!(matches!(
            decoded,
            CompositorMessage::CaptureGranted { fd: -1, .. }
        ));
    }
}
