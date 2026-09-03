//! Core SCP compositor state and message handlers.

use crate::scp::{
    buffer::BufferManager,
    capability::{
        Capability, CapabilityGrant, CapabilityToken, CaptureScope, Decision, blocked_while_locked,
        requires_foreground, requires_recent_interaction,
    },
    compose::{self, BufferSource, Framebuffer, Rgba8},
    data_device::DataDevice,
    dmabuf::DmabufManager,
    event_queue::{EventRouter, OutboundEvent, SessionSink},
    input::InputState,
    keymap::{KeymapState, ModifierState, RepeatInfo},
    output::OutputManager,
    popup::{Popup, PopupManager, position_popup},
    protocol::{
        BufferId, ButtonState, CURRENT_PROTOCOL_VERSION, CaptureFormat, CaptureId, CaptureTarget,
        ClientMessage, CompositorMessage, DismissReason, InputEvent, KeyBinding, LayerSurfaceId,
        MIN_PROTOCOL_VERSION, OutputId, PopupId, Rect, SessionId, ShortcutPriority, SurfaceId,
        ToplevelId,
    },
    security::{AppId, AuditOutcome, SecurityCoordinator, StubSecurityCoordinator},
    session_lock::{LockGrant, SessionLockManager},
    shortcuts::ShortcutManager,
    stack::{StackEntry, StackKind, WindowStack, place_toplevel},
    surface::{Anchor, KeyboardInteractivity, Layer, Margin, SurfaceManager, SurfaceRole},
    unix_socket,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

/// Fallback output geometry used before any real output is registered.
const FALLBACK_OUTPUT: Rect = Rect {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
};

/// Default size given to a toplevel that has not been resized yet.
const DEFAULT_TOPLEVEL_SIZE: (i32, i32) = (800, 600);

/// Height of the compositor-drawn titlebar. Clients never draw their own
/// decorations, so this is reserved on their behalf (see ADR-0027).
const DECORATION_HEIGHT: i32 = 32;

/// Guards popup parent-chain walks against a malformed nesting cycle.
const MAX_POPUP_NESTING: usize = 16;

/// What an output shows where no surface covers it.
///
/// Not a wallpaper: the desktop background is a Shell-owned background-layer
/// surface like any other. This is only what remains visible before the Shell
/// has mapped one, or in the gap left by an output no surface reaches.
const DESKTOP_CLEAR: Rgba8 = Rgba8::BLACK;

/// Escape, in the XKB keycode space this compositor speaks (evdev + 8).
///
/// Matches the `<ESC> = 9` mapping in [`crate::scp::keymap`].
const KEY_ESCAPE: u32 = 9;

/// Defense in depth for in-process users of `ScpState`; the socket transport
/// enforces the same order of magnitude before spawning a worker.
pub const MAX_SESSIONS: usize = 256;

/// Authenticated client session.
#[derive(Debug)]
pub struct ClientSession {
    pub session_id: SessionId,
    pub app_id: AppId,
    pub pid: u32,
    pub uid: u32,
    pub granted_capabilities: HashMap<Capability, CapabilityGrant>,
    pub connection_time: Instant,
    pub last_user_interaction: Option<Instant>,
    pub is_foreground: bool,
    pub protocol_version: u32,
}

impl ClientSession {
    pub fn new(session_id: SessionId, app_id: AppId, pid: u32, uid: u32) -> Self {
        Self {
            session_id,
            app_id,
            pid,
            uid,
            granted_capabilities: HashMap::new(),
            connection_time: Instant::now(),
            last_user_interaction: None,
            is_foreground: false,
            protocol_version: MIN_PROTOCOL_VERSION,
        }
    }

    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.granted_capabilities
            .get(cap)
            .is_some_and(CapabilityGrant::is_valid)
    }

    /// Record that the user just acted in this app's window.
    ///
    /// Capabilities like clipboard writes and drag initiation are only honored
    /// inside a short window after real input, so this timestamp is what stops a
    /// background app from silently hijacking the clipboard.
    pub fn record_user_interaction(&mut self) {
        self.last_user_interaction = Some(Instant::now());
    }

    /// Whether the user interacted with this app within `window`.
    pub fn interacted_within(&self, window: std::time::Duration) -> bool {
        self.last_user_interaction
            .is_some_and(|at| at.elapsed() <= window)
    }

    pub fn grant_capability(
        &mut self,
        capability: Capability,
        token: CapabilityToken,
        expires_at: Option<Instant>,
    ) {
        let grant = CapabilityGrant {
            capability: capability.clone(),
            token,
            granted_at: Instant::now(),
            expires_at,
            use_count: 0,
        };
        self.granted_capabilities.insert(capability, grant);
    }

    pub fn record_use(&mut self, capability: &Capability) {
        if let Some(grant) = self.granted_capabilities.get_mut(capability) {
            grant.use_count += 1;
        }
    }
}

/// Core SCP compositor state.
pub struct ScpState {
    security: Arc<dyn SecurityCoordinator>,
    sessions: HashMap<SessionId, ClientSession>,
    next_session_id: SessionId,
    surface_manager: SurfaceManager,
    buffer_manager: BufferManager,
    dmabuf_manager: DmabufManager,
    input_state: InputState,
    output_manager: OutputManager,
    popups: PopupManager,
    next_buffer_id: BufferId,
    /// Toplevels ordered topmost-first. Drives both Z order and focus history.
    toplevel_stack: Vec<ToplevelId>,
    /// Monotonic counter feeding cascade placement of new windows.
    toplevels_placed: u32,
    focused_surface: Option<(SessionId, SurfaceId)>,
    cursor_surface: Option<(SessionId, SurfaceId)>,
    /// Absolute pointer position in output-layout coordinates.
    pointer_position: (f64, f64),
    next_serial: u32,
    next_capture_id: CaptureId,

    // Keyboard state
    keymap_state: KeymapState,
    repeat_info: RepeatInfo,
    modifier_state: ModifierState,

    // Data device (clipboard/DnD)
    data_device: DataDevice,

    /// Conflict-arbitrated global bindings, released with their owner session.
    shortcuts: ShortcutManager,
    active_shortcut_keys: HashSet<u32>,
    /// Outputs intersected by each currently stacked surface.
    surface_outputs: HashMap<(SessionId, SurfaceId), HashSet<OutputId>>,

    /// Session lock. While engaged it takes over input and stacking entirely.
    session_lock: SessionLockManager,

    /// UID admitted by the greeter after successful authentication.
    ///
    /// The compositor is a system process and outlives user sessions, so its
    /// own UID cannot be used as the desktop-session boundary. Transport peers
    /// may connect only as the compositor itself or as this active UID.
    active_session_uid: Option<u32>,

    /// Delivers compositor-initiated events to client connections.
    events: EventRouter,
}

impl ScpState {
    pub fn new() -> Self {
        Self::with_security(Arc::new(StubSecurityCoordinator::default()))
    }

    pub fn with_security(security: Arc<dyn SecurityCoordinator>) -> Self {
        Self {
            security,
            sessions: HashMap::new(),
            next_session_id: 1,
            surface_manager: SurfaceManager::new(),
            buffer_manager: BufferManager::new(),
            dmabuf_manager: DmabufManager::new(),
            input_state: InputState::new(),
            output_manager: OutputManager::new(),
            popups: PopupManager::new(),
            next_buffer_id: 1,
            toplevel_stack: Vec::new(),
            toplevels_placed: 0,
            focused_surface: None,
            cursor_surface: None,
            pointer_position: (0.0, 0.0),
            next_serial: 1,
            next_capture_id: 1,
            keymap_state: KeymapState::new(),
            repeat_info: RepeatInfo::default(),
            modifier_state: ModifierState::new(),
            data_device: DataDevice::new(),
            shortcuts: ShortcutManager::new(),
            active_shortcut_keys: HashSet::new(),
            surface_outputs: HashMap::new(),
            session_lock: SessionLockManager::new(),
            active_session_uid: None,
            events: EventRouter::new(),
        }
    }

    pub fn next_serial(&mut self) -> u32 {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);
        serial
    }

    fn next_buffer_id(&mut self) -> BufferId {
        let id = self.next_buffer_id;
        self.next_buffer_id = self.next_buffer_id.wrapping_add(1);
        id
    }

    pub fn input_state(&self) -> &InputState {
        &self.input_state
    }

    pub fn input_state_mut(&mut self) -> &mut InputState {
        &mut self.input_state
    }

    pub fn output_manager(&self) -> &OutputManager {
        &self.output_manager
    }

    pub fn output_manager_mut(&mut self) -> &mut OutputManager {
        &mut self.output_manager
    }

    /// Register a backend output and notify every connected client.
    pub fn add_output(
        &mut self,
        name: String,
        description: String,
        width: i32,
        height: i32,
        refresh_rate: i32,
    ) -> Result<OutputId, String> {
        if width <= 0
            || height <= 0
            || width > compose::MAX_OUTPUT_DIMENSION
            || height > compose::MAX_OUTPUT_DIMENSION
            || refresh_rate <= 0
        {
            return Err("Invalid output mode".to_string());
        }
        let id = self
            .output_manager
            .add_output(name, description, width, height, refresh_rate);
        let output = self
            .output_manager
            .get_output(id)
            .cloned()
            .ok_or("Output manager lost the output it just inserted")?;
        let message = output_added(&output);
        let sessions: Vec<_> = self.events.session_ids().collect();
        self.events.send_all(
            sessions
                .into_iter()
                .map(|session| (session, message.clone())),
        );
        let outputs = self.output_ids();
        self.session_lock.outputs_changed(&outputs);
        self.sync_surface_outputs();
        Ok(id)
    }

    /// Remove a backend output and invalidate lock confirmation if topology no
    /// longer matches the surfaces acknowledged by the locker.
    pub fn remove_output(&mut self, output_id: OutputId) -> Result<(), String> {
        self.output_manager
            .remove_output(output_id)
            .ok_or("Output not found")?;
        self.sync_surface_outputs();
        let sessions: Vec<_> = self.events.session_ids().collect();
        self.events.send_all(
            sessions
                .into_iter()
                .map(|session| (session, CompositorMessage::OutputRemoved { output_id })),
        );
        let outputs = self.output_ids();
        self.session_lock.outputs_changed(&outputs);
        Ok(())
    }

    pub fn popups(&self) -> &PopupManager {
        &self.popups
    }

    pub fn session_lock(&self) -> &SessionLockManager {
        &self.session_lock
    }

    /// Whether the session is locked and ordinary clients are cut off.
    pub const fn is_locked(&self) -> bool {
        self.session_lock.is_locked()
    }

    /// Whether a transport peer UID belongs to the system compositor or to the
    /// user authenticated by the current greeter.
    pub fn peer_uid_is_admitted(&self, uid: u32) -> bool {
        // SAFETY: geteuid has no preconditions and does not mutate memory.
        uid == unsafe { libc::geteuid() } || self.active_session_uid == Some(uid)
    }

    /// UID currently admitted for the desktop session, if any.
    #[must_use]
    pub const fn active_session_uid(&self) -> Option<u32> {
        self.active_session_uid
    }

    /// Event router used to push compositor-initiated messages to clients.
    pub fn events(&self) -> &EventRouter {
        &self.events
    }

    /// Number of authenticated sessions, exposed for health reporting.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Attach a connection's outbound queue to its session.
    ///
    /// The transport must call this while still holding the state lock that
    /// produced the `Connected` reply, so no other thread can try to route an
    /// event to the session before its sink exists.
    pub fn register_session_sink(&mut self, session_id: SessionId, sink: Arc<SessionSink>) {
        self.events.register(session_id, sink);
    }

    // ===== Geometry and stacking =====

    /// Geometry of the output new windows are placed on.
    fn primary_output_rect(&self) -> Rect {
        self.output_manager
            .primary_output()
            .map_or(FALLBACK_OUTPUT, |output| output.geometry)
    }

    fn output_rect_or_primary(&self, output_id: Option<OutputId>) -> Rect {
        output_id
            .and_then(|id| self.output_manager.get_output(id))
            .map_or_else(|| self.primary_output_rect(), |output| output.geometry)
    }

    /// Normalize an optional output reference to a concrete output ID.
    ///
    /// Callers that key state by output must agree on identity, so "omitted"
    /// resolves to the primary output rather than staying a distinct `None`.
    fn resolve_output_id(&self, output_id: Option<OutputId>) -> Option<OutputId> {
        match output_id {
            Some(id) if self.output_manager.get_output(id).is_some() => Some(id),
            // An unknown ID falls back to the primary output rather than being
            // silently accepted as its own slot.
            _ => self.output_manager.primary_output().map(|output| output.id),
        }
    }

    fn output_ids(&self) -> Vec<OutputId> {
        self.output_manager
            .outputs()
            .iter()
            .map(|output| output.id)
            .collect()
    }

    /// Absolute geometry of a surface, resolved through its role.
    ///
    /// Popups are positioned relative to their parent, so this walks the parent
    /// chain to accumulate offsets. The walk is depth-limited: a corrupted chain
    /// must not be able to hang the compositor.
    fn absolute_surface_rect(&self, session_id: SessionId, surface_id: SurfaceId) -> Option<Rect> {
        let mut offset_x = 0;
        let mut offset_y = 0;
        let mut current = surface_id;

        for _ in 0..MAX_POPUP_NESTING {
            let surface = self.surface_manager.get_surface(session_id, current)?;

            match &surface.role {
                SurfaceRole::Toplevel(toplevel_id) => {
                    let toplevel = self.surface_manager.get_toplevel(*toplevel_id)?;
                    return Some(Rect {
                        x: toplevel.geometry.x + offset_x,
                        y: toplevel.geometry.y + offset_y,
                        width: toplevel.geometry.width,
                        height: toplevel.geometry.height,
                    });
                }
                SurfaceRole::LayerShell(layer_id) => {
                    let layer = self.surface_manager.get_layer_surface(*layer_id)?;
                    let output = self.output_rect_or_primary(layer.output);
                    let local = layer.calculate_geometry(output.width, output.height);
                    return Some(Rect {
                        x: output.x + local.x + offset_x,
                        y: output.y + local.y + offset_y,
                        width: local.width,
                        height: local.height,
                    });
                }
                SurfaceRole::LockSurface(lock_surface_id) => {
                    let lock_surface = self.session_lock.lock()?.get_surface(*lock_surface_id)?;
                    let output = self.output_rect_or_primary(lock_surface.output);
                    // Lock surfaces always cover their whole output; the client
                    // has no say in geometry.
                    return Some(output);
                }
                SurfaceRole::Popup { parent } => {
                    let popup_id = self.popups.find_by_surface(session_id, current)?;
                    let popup = self.popups.get(popup_id)?;
                    offset_x += popup.geometry.x;
                    offset_y += popup.geometry.y;
                    current = *parent;
                }
                // A surface without a role has no position of its own.
                SurfaceRole::None => return None,
            }
        }

        tracing::warn!(
            ?session_id,
            ?surface_id,
            "popup parent chain exceeded the nesting limit"
        );
        None
    }

    /// Flatten all windows into a topmost-first stack for input and rendering.
    ///
    /// Before authentication, a locked stack contains only lock surfaces. Once
    /// the greeter admits an authenticated UID, its prepared desktop may be
    /// composed underneath those full-output surfaces so an alpha handoff can
    /// reveal it. Input and capture remain locked until `UnlockSession`.
    pub fn build_stack(&self) -> WindowStack {
        let mut stack = WindowStack::new();

        if self.session_lock.is_locked() {
            // Lock surfaces stay first (topmost) and cover the full outputs, so
            // hit-testing still cannot reach anything underneath them.
            for lock_surface in self.session_lock.iter_surfaces() {
                let rect = self.output_rect_or_primary(lock_surface.output);
                stack.push(StackEntry {
                    session_id: lock_surface.session_id,
                    surface_id: lock_surface.surface_id,
                    kind: StackKind::LockSurface(lock_surface.id),
                    rect,
                    accepts_keyboard: true,
                });
            }
            if self.active_session_uid.is_some() {
                self.push_desktop_entries(&mut stack);
            }
            return stack;
        }

        self.push_desktop_entries(&mut stack);
        stack
    }

    fn push_desktop_entries(&self, stack: &mut WindowStack) {
        let popup_order = self.popup_order();

        self.push_layer_entries(stack, Layer::Overlay, &popup_order);
        self.push_layer_entries(stack, Layer::Top, &popup_order);

        for &toplevel_id in &self.toplevel_stack {
            let Some(toplevel) = self.surface_manager.get_toplevel(toplevel_id) else {
                continue;
            };
            if !self.desktop_session_is_visible(toplevel.session_id) {
                continue;
            }
            // A minimized window is off-screen: it must not swallow input, and
            // neither may the menus hanging off it.
            if toplevel.states.minimized {
                continue;
            }
            let owner = (toplevel.session_id, toplevel.surface_id);
            self.push_popups_for(stack, &popup_order, owner);
            stack.push(StackEntry {
                session_id: toplevel.session_id,
                surface_id: toplevel.surface_id,
                kind: StackKind::Toplevel(toplevel_id),
                rect: Rect {
                    x: toplevel.geometry.x,
                    y: toplevel.geometry.y,
                    width: toplevel.geometry.width,
                    height: toplevel.geometry.height,
                },
                accepts_keyboard: true,
            });
        }

        self.push_layer_entries(stack, Layer::Bottom, &popup_order);
        self.push_layer_entries(stack, Layer::Background, &popup_order);
    }

    fn desktop_session_is_visible(&self, session_id: SessionId) -> bool {
        let Some(active_uid) = self
            .active_session_uid
            .filter(|_| self.session_lock.is_locked())
        else {
            return true;
        };
        self.sessions
            .get(&session_id)
            .is_some_and(|session| session.uid == active_uid)
    }

    /// Every popup, innermost first.
    ///
    /// The grab chain is ordered outermost-first, so reversing it puts submenus
    /// above the menus that opened them. Grabless popups (tooltips) follow in
    /// arbitrary order; nothing depends on their relative Z.
    fn popup_order(&self) -> Vec<PopupId> {
        let mut popup_ids: Vec<PopupId> = self.popups.grab_chain().iter().rev().copied().collect();
        for popup in self.popups.iter() {
            if !popup_ids.contains(&popup.id) {
                popup_ids.push(popup.id);
            }
        }
        popup_ids
    }

    /// The non-popup surface a popup chain ultimately hangs off.
    ///
    /// Returns `None` for a chain that is broken or nested past
    /// [`MAX_POPUP_NESTING`], which is the same thing as far as stacking is
    /// concerned: a popup nobody owns is not shown.
    fn popup_root(&self, popup_id: PopupId) -> Option<(SessionId, SurfaceId)> {
        let mut current = self.popups.get(popup_id)?;
        for _ in 0..MAX_POPUP_NESTING {
            match current.parent_popup {
                Some(parent) => current = self.popups.get(parent)?,
                None => return Some((current.session_id, current.parent_id)),
            }
        }
        tracing::warn!(?popup_id, "popup parent chain exceeded the nesting limit");
        None
    }

    /// Push the popups anchored to one window, directly above it.
    ///
    /// A popup belongs to the window it hangs off, not to the desktop. Stacking
    /// every popup above every window — which is what this used to do — let any
    /// application put a surface over the whole screen, above other clients'
    /// windows, and win hit-testing there: an overlay the compositor's own
    /// server-side chrome could not protect against.
    fn push_popups_for(
        &self,
        stack: &mut WindowStack,
        popup_order: &[PopupId],
        owner: (SessionId, SurfaceId),
    ) {
        for &popup_id in popup_order {
            let Some(popup) = self.popups.get(popup_id) else {
                continue;
            };
            if !self.desktop_session_is_visible(popup.session_id) {
                continue;
            }
            if self.popup_root(popup_id) != Some(owner) {
                continue;
            }
            let Some(rect) = self.absolute_surface_rect(popup.session_id, popup.surface_id) else {
                continue;
            };
            stack.push(StackEntry {
                session_id: popup.session_id,
                surface_id: popup.surface_id,
                kind: StackKind::Popup(popup_id),
                rect: Rect {
                    x: rect.x,
                    y: rect.y,
                    width: popup.geometry.width,
                    height: popup.geometry.height,
                },
                accepts_keyboard: popup.grab,
            });
        }
    }

    fn push_layer_entries(&self, stack: &mut WindowStack, layer: Layer, popup_order: &[PopupId]) {
        for layer_surface in self.surface_manager.iter_layer_surfaces() {
            if layer_surface.layer != layer {
                continue;
            }
            if !self.desktop_session_is_visible(layer_surface.session_id) {
                continue;
            }
            let Some(rect) =
                self.absolute_surface_rect(layer_surface.session_id, layer_surface.surface_id)
            else {
                continue;
            };
            let owner = (layer_surface.session_id, layer_surface.surface_id);
            self.push_popups_for(stack, popup_order, owner);
            stack.push(StackEntry {
                session_id: layer_surface.session_id,
                surface_id: layer_surface.surface_id,
                kind: StackKind::LayerSurface(layer_surface.id),
                rect,
                accepts_keyboard: layer_surface.keyboard_interactivity
                    != KeyboardInteractivity::None,
            });
        }
    }

    /// Topmost window accepting input at an absolute point.
    pub fn hit_test(&self, x: f64, y: f64) -> Option<StackEntry> {
        self.hit_test_in(&self.build_stack(), x, y)
    }

    fn hit_test_in(&self, stack: &WindowStack, x: f64, y: f64) -> Option<StackEntry> {
        stack.hit_test(x, y, |entry, local_x, local_y| {
            self.surface_manager
                .get_surface(entry.session_id, entry.surface_id)
                .is_some_and(|surface| surface.accepts_input_at(local_x, local_y))
        })
    }

    /// Raise a toplevel to the top of the stack.
    fn raise_toplevel(&mut self, toplevel_id: ToplevelId) {
        self.toplevel_stack.retain(|&id| id != toplevel_id);
        self.toplevel_stack.insert(0, toplevel_id);
    }

    pub fn handle_message(
        &mut self,
        session_id: Option<SessionId>,
        message: ClientMessage,
    ) -> Result<Vec<CompositorMessage>, String> {
        // Direct callers are in-process fixtures. The real transport supplies
        // the kernel-authenticated peer UID through `handle_transport_message`.
        let uid = unsafe { libc::geteuid() };
        self.handle_message_from_uid(session_id, message, uid)
    }

    fn handle_message_from_uid(
        &mut self,
        session_id: Option<SessionId>,
        message: ClientMessage,
        uid: u32,
    ) -> Result<Vec<CompositorMessage>, String> {
        match (session_id, message) {
            (None, ClientMessage::Connect { app_id, pid }) => self.handle_connect(app_id, pid, uid),
            (
                None,
                ClientMessage::ConnectVersioned {
                    app_id,
                    pid,
                    min_version,
                    max_version,
                },
            ) => {
                let version = negotiate_version(min_version, max_version)?;
                let mut responses = self.handle_connect(app_id, pid, uid)?;
                if !matches!(responses.first(), Some(CompositorMessage::Rejected { .. })) {
                    let connected_session = responses.iter().find_map(|response| match response {
                        CompositorMessage::Connected { session_id, .. } => Some(*session_id),
                        _ => None,
                    });
                    if let Some(session_id) = connected_session
                        && let Some(session) = self.sessions.get_mut(&session_id)
                    {
                        session.protocol_version = version;
                    }
                    responses.insert(
                        0,
                        CompositorMessage::ProtocolVersion {
                            version,
                            features: protocol_features(version),
                        },
                    );
                    responses.extend(self.output_manager.outputs().iter().map(output_added));
                }
                Ok(responses)
            }
            (Some(_), ClientMessage::Connect { .. } | ClientMessage::ConnectVersioned { .. }) => {
                Err("Already connected".to_string())
            }
            (None, _) => Err("Not connected".to_string()),
            (Some(session_id), message) => self.handle_authenticated_message(session_id, message),
        }
    }

    /// Binds the descriptor received via SCM_RIGHTS to AttachBuffer. File
    /// descriptor integers embedded in JSON are never trusted.
    pub fn handle_transport_message(
        &mut self,
        session_id: Option<SessionId>,
        mut message: ClientMessage,
        mut received_fds: Vec<i32>,
        peer_uid: u32,
    ) -> Result<Vec<CompositorMessage>, String> {
        match &mut message {
            ClientMessage::AttachBuffer { buffer_fd, .. } => {
                *buffer_fd = received_fds
                    .pop()
                    .ok_or("AttachBuffer requires exactly one file descriptor")?;
            }
            ClientMessage::CreateShmPool { fd, .. } => {
                *fd = received_fds
                    .pop()
                    .ok_or("CreateShmPool requires exactly one file descriptor")?;
            }
            ClientMessage::CreateDmabufBuffer { fds, .. } => {
                if session_id.is_none() {
                    for fd in received_fds {
                        unix_socket::close_fd(fd);
                    }
                    return Err("Not connected".to_string());
                }
                *fds = std::mem::take(&mut received_fds);
            }
            _ if !received_fds.is_empty() => {
                for fd in received_fds {
                    unix_socket::close_fd(fd);
                }
                return Err(
                    "File descriptors are only valid with buffer import requests".to_string(),
                );
            }
            _ => {}
        }

        // Only AttachBuffer needs unwinding here: its handler transfers the
        // descriptor to a `SurfaceBuffer` on success and leaves it untouched on
        // failure. `CreateShmPool` is deliberately absent — `create_pool` takes
        // ownership unconditionally and closes on every failure path of its own,
        // so closing it a second time here would hand back a descriptor number
        // another thread may already have reused.
        let attached_fd = match &message {
            ClientMessage::AttachBuffer { buffer_fd, .. } => Some(*buffer_fd),
            _ => None,
        };
        let result = self.handle_message_from_uid(session_id, message, peer_uid);
        if result.is_err()
            && let Some(fd) = attached_fd
        {
            unix_socket::close_fd(fd);
        }
        result
    }

    fn handle_connect(
        &mut self,
        app_id: String,
        pid: u32,
        uid: u32,
    ) -> Result<Vec<CompositorMessage>, String> {
        let verified_app_id = self
            .security
            .verify_app_identity(pid)
            .ok_or_else(|| format!("Failed to verify app identity for PID {pid}"))?;

        if verified_app_id.0 != app_id {
            return Ok(vec![CompositorMessage::Rejected {
                reason: format!(
                    "App ID mismatch: claimed '{app_id}', verified '{}'",
                    verified_app_id.0
                ),
            }]);
        }

        if self.sessions.len() >= MAX_SESSIONS {
            return Ok(vec![CompositorMessage::Rejected {
                reason: "SCP session limit reached; retry later".to_string(),
            }]);
        }

        let session_id = self.next_session_id;
        self.next_session_id = self
            .next_session_id
            .checked_add(1)
            .ok_or("Session ID space exhausted")?;
        let mut session = ClientSession::new(session_id, verified_app_id.clone(), pid, uid);

        for capability in crate::scp::capability::default_app_capabilities() {
            if let Decision::Granted { token, expires_at } = self
                .security
                .evaluate_capability(&verified_app_id, &capability)
            {
                session.grant_capability(capability.clone(), token, expires_at);
                self.security.audit_capability_use(
                    &verified_app_id,
                    &capability,
                    AuditOutcome::Granted,
                );
            }
        }

        let granted_capabilities = session
            .granted_capabilities
            .keys()
            .map(|capability| capability.wire_name().to_string())
            .collect();
        let capability_tokens = session
            .granted_capabilities
            .iter()
            .map(|(capability, grant)| {
                (capability.wire_name().to_string(), grant.token.data.clone())
            })
            .collect();

        self.sessions.insert(session_id, session);
        tracing::info!(?session_id, ?verified_app_id, "SCP client connected");

        Ok(vec![CompositorMessage::Connected {
            session_id,
            granted_capabilities,
            capability_tokens,
        }])
    }

    fn handle_authenticated_message(
        &mut self,
        session_id: SessionId,
        message: ClientMessage,
    ) -> Result<Vec<CompositorMessage>, String> {
        self.cleanup_expired_capabilities(session_id);
        let app_id = self
            .sessions
            .get(&session_id)
            .ok_or("Invalid session")?
            .app_id
            .clone();

        let result = match message {
            ClientMessage::CreateSurface { surface_id } => {
                self.surface_manager
                    .create_surface(session_id, surface_id, app_id)?;
                Ok(vec![])
            }
            ClientMessage::DestroySurface { surface_id } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                self.destroy_surface_and_role(session_id, surface_id)?;
                Ok(vec![])
            }
            ClientMessage::AttachBuffer {
                surface_id,
                buffer_fd,
                width,
                height,
                stride,
                format,
            } => {
                self.validate_buffer(buffer_fd, width, height, stride, format)?;
                self.verify_surface_ownership(session_id, surface_id)?;
                let buffer_id = self.next_buffer_id();
                let surface = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?;
                let replaced = surface.attach_buffer(crate::scp::surface::SurfaceBuffer {
                    buffer_id,
                    offset: 0,
                    managed: false,
                    kind: crate::scp::surface::SurfaceBufferKind::Shm,
                    fd: buffer_fd,
                    width,
                    height,
                    stride,
                    format,
                });
                if let Some(replaced) = replaced.filter(|buffer| buffer.managed) {
                    self.release_managed_buffer(session_id, replaced.buffer_id);
                }
                Ok(vec![])
            }
            ClientMessage::Commit {
                surface_id,
                frame_callback,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;

                let surface = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?;

                // Register frame callback if requested
                if let Some(callback_id) = frame_callback {
                    surface.request_frame(callback_id);
                }

                let evicted = surface.commit();
                for buffer in evicted.into_iter().filter(|buffer| buffer.managed) {
                    self.release_managed_buffer(session_id, buffer.buffer_id);
                }
                Ok(vec![])
            }
            ClientMessage::RequestCapability {
                capability,
                justification,
            } => self.handle_capability_request(session_id, app_id, capability, justification),
            ClientMessage::CreateToplevel {
                surface_id,
                capability_token,
                title,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                self.verify_capability_token(
                    session_id,
                    &Capability::WindowToplevel,
                    &capability_token,
                )?;

                let toplevel_id = self
                    .surface_manager
                    .create_toplevel(session_id, surface_id, title)?;

                let (width, height) = DEFAULT_TOPLEVEL_SIZE;
                let output = self.primary_output_rect();
                let (x, y) = place_toplevel(&output, width, height, self.toplevels_placed);
                self.toplevels_placed = self.toplevels_placed.wrapping_add(1);

                let serial = self.next_serial();
                if let Some(toplevel) = self.surface_manager.get_toplevel_mut(toplevel_id) {
                    // Seed the geometry immediately so hit-testing works before
                    // the client acknowledges the first configure.
                    toplevel.set_position(x, y);
                    toplevel.geometry.width = width;
                    toplevel.geometry.height = height;
                    toplevel.configure(
                        serial,
                        width,
                        height,
                        crate::scp::surface::ToplevelStates {
                            activated: true,
                            ..Default::default()
                        },
                    );
                }
                self.raise_toplevel(toplevel_id);

                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.record_use(&Capability::WindowToplevel);
                }
                self.set_keyboard_focus(Some((session_id, surface_id)));
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::WindowToplevel,
                    AuditOutcome::Used,
                );

                Ok(vec![CompositorMessage::ConfigureToplevel {
                    toplevel_id,
                    serial,
                    width,
                    height,
                    decoration_height: DECORATION_HEIGHT,
                    states: crate::scp::protocol::ToplevelStates {
                        activated: true,
                        ..Default::default()
                    },
                }])
            }
            ClientMessage::SetToplevelTitle { toplevel_id, title } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;
                self.surface_manager
                    .get_toplevel_mut(toplevel_id)
                    .ok_or("Toplevel not found")?
                    .set_title(title);
                Ok(vec![])
            }
            ClientMessage::AckConfigure {
                toplevel_id,
                serial,
            } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;
                if !self
                    .surface_manager
                    .get_toplevel_mut(toplevel_id)
                    .ok_or("Toplevel not found")?
                    .ack_configure(serial)
                {
                    return Err("Configure serial is stale or unknown".to_string());
                }
                Ok(vec![])
            }
            ClientMessage::SetFullscreen {
                toplevel_id,
                capability_token,
            } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;
                self.verify_capability_token(
                    session_id,
                    &Capability::Fullscreen,
                    &capability_token,
                )?;
                // Use the output's real geometry. The hardcoded 1920×1080 here
                // disagreed with the SetToplevelState::Fullscreen path, so the
                // same request answered differently depending on how it was
                // spelled — and was simply wrong on any other display.
                let output = self.primary_output_rect();
                let serial = self.next_serial();
                if let Some(toplevel) = self.surface_manager.get_toplevel_mut(toplevel_id) {
                    let states = crate::scp::surface::ToplevelStates {
                        fullscreen: true,
                        ..Default::default()
                    };
                    toplevel.set_position(output.x, output.y);
                    toplevel.geometry.width = output.width;
                    toplevel.geometry.height = output.height;
                    toplevel.configure(serial, output.width, output.height, states);
                }
                self.sessions
                    .get_mut(&session_id)
                    .ok_or("Invalid session")?
                    .record_use(&Capability::Fullscreen);
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::Fullscreen,
                    AuditOutcome::Used,
                );
                Ok(vec![CompositorMessage::ConfigureToplevel {
                    toplevel_id,
                    serial,
                    width: output.width,
                    height: output.height,
                    decoration_height: 0,
                    states: crate::scp::protocol::ToplevelStates {
                        fullscreen: true,
                        ..Default::default()
                    },
                }])
            }
            ClientMessage::Connect { .. } | ClientMessage::ConnectVersioned { .. } => {
                unreachable!("connect is routed before authenticated messages")
            }

            // Buffer management. Pool and buffer ids are client-chosen, so every
            // one of these is scoped to the calling session: a shared pool is a
            // shared mapping, which makes a global id namespace a cross-client
            // memory read rather than just a naming collision.
            ClientMessage::CreateShmPool { pool_id, fd, size } => self
                .buffer_manager
                .create_pool(session_id, pool_id, fd, size)
                .map(|_| vec![]),
            ClientMessage::CreateBuffer {
                buffer_id,
                pool_id,
                offset,
                width,
                height,
                stride,
                format,
            } => {
                if self.dmabuf_manager.contains(session_id, buffer_id) {
                    return Err("Buffer ID already exists as a DMA-BUF".to_string());
                }
                self.buffer_manager
                    .create_buffer(
                        session_id, buffer_id, pool_id, offset, width, height, stride, format,
                    )
                    .map(|_| vec![])
            }
            ClientMessage::CreateDmabufBuffer {
                buffer_id,
                width,
                height,
                format,
                modifier,
                planes,
                fds,
            } => {
                if self
                    .buffer_manager
                    .get_buffer(session_id, buffer_id)
                    .is_some()
                {
                    for fd in fds {
                        unix_socket::close_fd(fd);
                    }
                    return Err("Buffer ID already exists as an SHM buffer".to_string());
                }
                self.dmabuf_manager
                    .create_buffer(
                        session_id, buffer_id, width, height, format, modifier, planes, fds,
                    )
                    .map(|_| vec![])
            }
            ClientMessage::AttachShmBuffer {
                surface_id,
                buffer_id,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                let buffer = self
                    .buffer_manager
                    .acquire_surface_buffer(session_id, buffer_id)?;
                let replaced = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?
                    .attach_buffer(buffer);
                if let Some(replaced) = replaced.filter(|buffer| buffer.managed) {
                    self.release_managed_buffer(session_id, replaced.buffer_id);
                }
                Ok(vec![])
            }
            ClientMessage::AttachDmabufBuffer {
                surface_id,
                buffer_id,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                let buffer = self
                    .dmabuf_manager
                    .acquire_surface_buffer(session_id, buffer_id)?;
                let replaced = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?
                    .attach_buffer(buffer);
                if let Some(replaced) = replaced.filter(|buffer| buffer.managed) {
                    self.release_managed_buffer(session_id, replaced.buffer_id);
                }
                Ok(vec![])
            }
            ClientMessage::DetachBuffer { surface_id } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                let replaced = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?
                    .detach_buffer();
                if let Some(replaced) = replaced.filter(|buffer| buffer.managed) {
                    self.release_managed_buffer(session_id, replaced.buffer_id);
                }
                Ok(vec![])
            }
            ClientMessage::DestroyBuffer { buffer_id } => {
                if self
                    .buffer_manager
                    .get_buffer(session_id, buffer_id)
                    .is_some()
                {
                    self.buffer_manager
                        .destroy_buffer(session_id, buffer_id)
                        .map(|_| vec![])
                } else {
                    self.dmabuf_manager
                        .destroy_buffer(session_id, buffer_id)
                        .map(|_| vec![])
                }
            }
            ClientMessage::DestroyShmPool { pool_id } => self
                .buffer_manager
                .destroy_pool(session_id, pool_id)
                .map(|_| vec![]),
            ClientMessage::Damage {
                surface_id,
                x,
                y,
                width,
                height,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                if let Some(surface) = self.surface_manager.get_surface_mut(session_id, surface_id)
                {
                    surface.add_damage(Rect {
                        x,
                        y,
                        width,
                        height,
                    });
                }
                Ok(vec![])
            }
            ClientMessage::SetInputRegion { surface_id, rects } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                if let Some(surface) = self.surface_manager.get_surface_mut(session_id, surface_id)
                {
                    surface.set_input_region(rects);
                }
                Ok(vec![])
            }
            ClientMessage::SetOpaqueRegion { surface_id, rects } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                if let Some(surface) = self.surface_manager.get_surface_mut(session_id, surface_id)
                {
                    surface.set_opaque_region(rects);
                }
                Ok(vec![])
            }

            // Popup windows
            ClientMessage::CreatePopup {
                surface_id,
                parent_id,
                positioner,
                grab,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                self.verify_surface_ownership(session_id, parent_id)?;
                self.verify_capability(session_id, &Capability::WindowPopup)?;

                if surface_id == parent_id {
                    return Err("Popup cannot be its own parent".to_string());
                }
                if positioner.size.0 <= 0 || positioner.size.1 <= 0 {
                    return Err("Popup size must be positive".to_string());
                }

                // A popup must hang off a positioned surface. Refusing an
                // unroled parent keeps the parent chain resolvable, which is
                // what makes absolute geometry and dismissal cascades work.
                let parent_popup = match &self
                    .surface_manager
                    .get_surface(session_id, parent_id)
                    .ok_or("Parent surface not found")?
                    .role
                {
                    SurfaceRole::Toplevel(_) | SurfaceRole::LayerShell(_) => None,
                    SurfaceRole::Popup { .. } => Some(
                        self.popups
                            .find_by_surface(session_id, parent_id)
                            .ok_or("Parent popup not found")?,
                    ),
                    // The lock screen draws its own menus. A popup parented
                    // to a lock surface would stack below it — invisible, but
                    // still a surface the lock does not cover.
                    SurfaceRole::LockSurface(_) => {
                        return Err("Popups cannot be parented to a lock surface".to_string());
                    }
                    SurfaceRole::None => {
                        return Err(
                            "Popup parent must be a toplevel, layer surface, or popup".to_string()
                        );
                    }
                };

                // The positioner's anchor rect is in parent-local coordinates,
                // but constraint resolution needs to know where the parent
                // actually sits so it can flip or slide against the output edge.
                let parent_rect = self
                    .absolute_surface_rect(session_id, parent_id)
                    .ok_or("Parent surface has no resolved geometry")?;
                let output_rect = self.primary_output_rect();
                let local_output = Rect {
                    x: output_rect.x - parent_rect.x,
                    y: output_rect.y - parent_rect.y,
                    width: output_rect.width,
                    height: output_rect.height,
                };
                let geometry = position_popup(&positioner, &local_output);

                let popup_id =
                    self.popups
                        .create(session_id, surface_id, parent_id, parent_popup, grab)?;
                if let Some(popup) = self.popups.get_mut(popup_id) {
                    popup.geometry = geometry;
                }

                // Assign the role only once the popup is registered, so a
                // rejected popup never leaves a surface with a dangling role.
                if let Err(error) = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?
                    .assign_role(SurfaceRole::Popup { parent: parent_id })
                {
                    self.popups.dismiss_subtree(popup_id);
                    return Err(error);
                }

                tracing::debug!(
                    ?popup_id,
                    ?surface_id,
                    ?parent_id,
                    ?parent_popup,
                    grab,
                    x = geometry.x,
                    y = geometry.y,
                    "popup created"
                );

                Ok(vec![CompositorMessage::ConfigurePopup {
                    popup_id,
                    x: geometry.x,
                    y: geometry.y,
                    width: geometry.width,
                    height: geometry.height,
                }])
            }

            ClientMessage::DestroyPopup { popup_id } => {
                let popup = self.popups.get(popup_id).ok_or("Popup not found")?;
                if popup.session_id != session_id {
                    return Err("Popup does not belong to this session".to_string());
                }
                let dismissed = self.popups.dismiss_subtree(popup_id);
                self.finish_popup_dismissal(&dismissed, DismissReason::ParentClosed);
                Ok(vec![])
            }

            // Toplevel state management
            ClientMessage::SetToplevelState { toplevel_id, state } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;

                use crate::scp::protocol::ToplevelStateRequest;

                // Fullscreen hides the compositor's chrome, so it stays behind
                // an explicit capability rather than being reachable through a
                // plain state request (ADR-0027).
                if matches!(state, ToplevelStateRequest::Fullscreen { .. }) {
                    self.verify_capability(session_id, &Capability::Fullscreen)?;
                }

                // Resolve output geometry before taking the mutable borrow.
                let target_output = match &state {
                    ToplevelStateRequest::Fullscreen { output_id } => {
                        self.output_rect_or_primary(*output_id)
                    }
                    _ => self.primary_output_rect(),
                };
                let serial = self.next_serial();

                let toplevel = self
                    .surface_manager
                    .get_toplevel_mut(toplevel_id)
                    .ok_or("Toplevel not found")?;
                let previous = toplevel.states.clone();

                // Position matters now that hit-testing is real: a maximized or
                // fullscreen window must actually sit on its output.
                let (width, height, position, states) = match state {
                    ToplevelStateRequest::Maximize => (
                        target_output.width,
                        target_output.height,
                        Some((target_output.x, target_output.y)),
                        crate::scp::surface::ToplevelStates {
                            activated: previous.activated,
                            maximized: true,
                            fullscreen: false,
                            minimized: false,
                            resizing: false,
                        },
                    ),
                    ToplevelStateRequest::Minimize => (
                        toplevel.geometry.width,
                        toplevel.geometry.height,
                        None,
                        crate::scp::surface::ToplevelStates {
                            activated: false,
                            maximized: previous.maximized,
                            fullscreen: previous.fullscreen,
                            minimized: true,
                            resizing: false,
                        },
                    ),
                    ToplevelStateRequest::Fullscreen { .. } => (
                        target_output.width,
                        target_output.height,
                        Some((target_output.x, target_output.y)),
                        crate::scp::surface::ToplevelStates {
                            activated: previous.activated,
                            maximized: false,
                            fullscreen: true,
                            minimized: false,
                            resizing: false,
                        },
                    ),
                    ToplevelStateRequest::UnsetMaximize => {
                        let (width, height) = DEFAULT_TOPLEVEL_SIZE;
                        (
                            width,
                            height,
                            Some(place_toplevel(&target_output, width, height, 0)),
                            crate::scp::surface::ToplevelStates {
                                activated: previous.activated,
                                maximized: false,
                                fullscreen: previous.fullscreen,
                                minimized: previous.minimized,
                                resizing: false,
                            },
                        )
                    }
                    ToplevelStateRequest::UnsetFullscreen => {
                        let (width, height) = DEFAULT_TOPLEVEL_SIZE;
                        (
                            width,
                            height,
                            Some(place_toplevel(&target_output, width, height, 0)),
                            crate::scp::surface::ToplevelStates {
                                activated: previous.activated,
                                maximized: previous.maximized,
                                fullscreen: false,
                                minimized: previous.minimized,
                                resizing: false,
                            },
                        )
                    }
                };

                toplevel.states = states.clone();
                toplevel.geometry.width = width;
                toplevel.geometry.height = height;
                if let Some((x, y)) = position {
                    toplevel.set_position(x, y);
                }
                toplevel.configure(serial, width, height, states.clone());

                // A minimized window is off-screen and cannot keep focus.
                if states.minimized && self.focused_surface_is_toplevel(toplevel_id) {
                    self.set_keyboard_focus(None);
                }

                Ok(vec![CompositorMessage::ConfigureToplevel {
                    toplevel_id,
                    serial,
                    width,
                    height,
                    decoration_height: if states.fullscreen {
                        0
                    } else {
                        DECORATION_HEIGHT
                    },
                    states: crate::scp::protocol::ToplevelStates {
                        activated: states.activated,
                        maximized: states.maximized,
                        fullscreen: states.fullscreen,
                        resizing: states.resizing,
                    },
                }])
            }
            ClientMessage::SetToplevelAppId {
                toplevel_id,
                app_id: new_app_id,
            } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;
                self.surface_manager
                    .get_toplevel_mut(toplevel_id)
                    .ok_or("Toplevel not found")?
                    .set_app_id(new_app_id);
                Ok(vec![])
            }
            ClientMessage::CloseToplevel { toplevel_id } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;
                self.close_toplevel(toplevel_id);
                Ok(vec![])
            }

            // Input
            ClientMessage::SetCursor {
                serial,
                surface_id,
                hotspot_x,
                hotspot_y,
            } => {
                // The serial must be one this client was actually sent. The
                // old check compared against `next_serial`, a counter that
                // wraps — which made it true for almost any value, and outright
                // meaningless after the first wrap.
                if !self.data_device.is_serial_known(serial, session_id) {
                    return Err(
                        "Cursor serial is stale or was issued to another client".to_string()
                    );
                }

                if let Some(surface_id) = surface_id {
                    self.verify_surface_ownership(session_id, surface_id)?;
                    self.cursor_surface = Some((session_id, surface_id));
                    tracing::trace!(?surface_id, ?hotspot_x, ?hotspot_y, "Cursor surface set");
                } else {
                    // Hide cursor
                    self.cursor_surface = None;
                    tracing::trace!("Cursor hidden");
                }
                Ok(vec![])
            }

            // Layer Shell
            ClientMessage::CreateLayerSurface {
                surface_id,
                capability_token,
                layer,
                namespace,
                output_id,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                self.verify_capability_token(
                    session_id,
                    &Capability::LayerShell,
                    &capability_token,
                )?;

                let layer_enum = match layer {
                    crate::scp::protocol::LayerShellLayer::Background => Layer::Background,
                    crate::scp::protocol::LayerShellLayer::Bottom => Layer::Bottom,
                    crate::scp::protocol::LayerShellLayer::Top => Layer::Top,
                    crate::scp::protocol::LayerShellLayer::Overlay => Layer::Overlay,
                };

                let layer_id = self
                    .surface_manager
                    .create_layer_surface(session_id, surface_id, layer_enum, namespace)?;

                if let Some(layer_surface) = self.surface_manager.get_layer_surface_mut(layer_id) {
                    layer_surface.output = output_id;
                }

                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.record_use(&Capability::LayerShell);
                }
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::LayerShell,
                    AuditOutcome::Used,
                );

                // Send initial configure
                let serial = self.next_serial();
                let output = output_id
                    .and_then(|id| self.output_manager.get_output(id))
                    .or_else(|| self.output_manager.primary_output());

                let (width, height) = if let Some(output) = output {
                    (output.geometry.width, output.geometry.height)
                } else {
                    (1920, 1080)
                };

                if let Some(layer_surface) = self.surface_manager.get_layer_surface_mut(layer_id) {
                    layer_surface.configure(serial, width, height);
                }

                Ok(vec![CompositorMessage::ConfigureLayerSurface {
                    layer_id,
                    serial,
                    width,
                    height,
                }])
            }

            ClientMessage::SetLayerAnchor {
                layer_id,
                top,
                bottom,
                left,
                right,
            } => {
                self.verify_layer_ownership(session_id, layer_id)?;
                if let Some(layer_surface) = self.surface_manager.get_layer_surface_mut(layer_id) {
                    layer_surface.anchor = Anchor {
                        top,
                        bottom,
                        left,
                        right,
                    };
                }
                Ok(vec![])
            }

            ClientMessage::SetLayerExclusiveZone { layer_id, zone } => {
                self.verify_layer_ownership(session_id, layer_id)?;
                if let Some(layer_surface) = self.surface_manager.get_layer_surface_mut(layer_id) {
                    layer_surface.exclusive_zone = zone;
                }
                Ok(vec![])
            }

            ClientMessage::SetLayerMargin {
                layer_id,
                top,
                right,
                bottom,
                left,
            } => {
                self.verify_layer_ownership(session_id, layer_id)?;
                if let Some(layer_surface) = self.surface_manager.get_layer_surface_mut(layer_id) {
                    layer_surface.margin = Margin {
                        top,
                        right,
                        bottom,
                        left,
                    };
                }
                Ok(vec![])
            }

            ClientMessage::SetLayerKeyboardInteractivity {
                layer_id,
                interactivity,
            } => {
                self.verify_layer_ownership(session_id, layer_id)?;
                if let Some(layer_surface) = self.surface_manager.get_layer_surface_mut(layer_id) {
                    layer_surface.keyboard_interactivity = match interactivity {
                        crate::scp::protocol::LayerKeyboardInteractivity::None => {
                            KeyboardInteractivity::None
                        }
                        crate::scp::protocol::LayerKeyboardInteractivity::Exclusive => {
                            KeyboardInteractivity::Exclusive
                        }
                        crate::scp::protocol::LayerKeyboardInteractivity::OnDemand => {
                            KeyboardInteractivity::OnDemand
                        }
                    };
                }
                Ok(vec![])
            }

            ClientMessage::SetLayerSize {
                layer_id,
                width,
                height,
            } => {
                self.verify_layer_ownership(session_id, layer_id)?;
                if let Some(layer_surface) = self.surface_manager.get_layer_surface_mut(layer_id) {
                    layer_surface.size = (width, height);
                }
                Ok(vec![])
            }

            ClientMessage::AckLayerConfigure { layer_id, serial } => {
                self.verify_layer_ownership(session_id, layer_id)?;
                if !self
                    .surface_manager
                    .get_layer_surface_mut(layer_id)
                    .ok_or("Layer surface not found")?
                    .ack_configure(serial)
                {
                    return Err("Configure serial is stale or unknown".to_string());
                }
                Ok(vec![])
            }

            // ===== Session Lock =====
            ClientMessage::LockSession { capability_token } => {
                self.verify_capability_token(
                    session_id,
                    &Capability::SessionLock,
                    &capability_token,
                )?;

                // The focus to restore is captured before the lock engages,
                // because engaging drops focus from whatever had it.
                let previous_focus = self.focused_surface;
                let grant =
                    match self
                        .session_lock
                        .engage(session_id, app_id.clone(), previous_focus)
                    {
                        Ok(grant) => grant,
                        // Another live client already holds the lock. Two lockers
                        // racing is not a malformed request, so the loser is told to
                        // drop its objects rather than killed with a protocol error
                        // — and it learns nothing about the session it did not lock.
                        Err(reason) => {
                            tracing::warn!(?session_id, %reason, "session lock request refused");
                            return Ok(vec![CompositorMessage::SessionLockFinished { reason }]);
                        }
                    };

                // Cut the desktop off immediately, before the locker has drawn
                // anything: the interval between request and first frame is
                // exactly when a screen must not still be live.
                self.set_keyboard_focus(None);
                self.input_state.set_pointer_focus(None);
                let dismissed = self.popups.dismiss_grab_chain();
                self.finish_popup_dismissal(&dismissed, DismissReason::ParentClosed);

                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.record_use(&Capability::SessionLock);
                }
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::SessionLock,
                    AuditOutcome::Used,
                );

                if matches!(grant, LockGrant::Engaged(_)) {
                    self.events.broadcast_except(
                        session_id,
                        &CompositorMessage::SessionLockStateChanged { locked: true },
                    );
                }

                tracing::info!(?session_id, ?grant, "session lock engaged");
                Ok(vec![CompositorMessage::SessionLockEngaged {
                    lock_id: grant.lock_id(),
                }])
            }

            ClientMessage::CreateLockSurface {
                surface_id,
                lock_id,
                output_id,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;

                // Resolve the output before registering, so naming an output
                // explicitly and omitting it cannot cover the same output twice.
                let resolved_output = self.resolve_output_id(output_id);
                let lock_surface_id = self.session_lock.create_surface(
                    session_id,
                    lock_id,
                    surface_id,
                    resolved_output,
                )?;

                if let Err(error) = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?
                    .assign_role(SurfaceRole::LockSurface(lock_surface_id))
                {
                    self.session_lock.remove_surface(session_id, surface_id);
                    return Err(error);
                }

                let output = self.output_rect_or_primary(resolved_output);
                let serial = self.next_serial();
                if let Some(lock_surface) = self
                    .session_lock
                    .lock_mut()
                    .and_then(|lock| lock.get_surface_mut(lock_surface_id))
                {
                    lock_surface.configure(serial, output.width, output.height);
                }

                Ok(vec![CompositorMessage::ConfigureLockSurface {
                    lock_surface_id,
                    serial,
                    width: output.width,
                    height: output.height,
                }])
            }

            ClientMessage::AckLockConfigure {
                lock_surface_id,
                serial,
            } => {
                let outputs = self.output_ids();
                let confirmed = self.session_lock.ack_configure(
                    session_id,
                    lock_surface_id,
                    serial,
                    &outputs,
                )?;
                if !confirmed {
                    return Ok(vec![]);
                }

                let lock_id = self
                    .session_lock
                    .lock()
                    .map(|lock| lock.id)
                    .ok_or("Session is not locked")?;

                // Every output is covered, so hand keyboard focus to the locker
                // and let it accept a password.
                if let Some(target) = self.primary_lock_focus() {
                    self.set_keyboard_focus(Some(target));
                }

                Ok(vec![CompositorMessage::SessionLocked { lock_id }])
            }

            ClientMessage::AuthorizeSessionUser { lock_id, uid } => {
                let lock = self.session_lock.lock().ok_or("Session is not locked")?;
                if lock.id != lock_id {
                    return Err("Lock ID does not match the active session lock".to_string());
                }
                if lock.owner() != Some(session_id) {
                    return Err("Session does not own the session lock".to_string());
                }
                if !lock.is_confirmed() {
                    return Err("Session lock has not engaged on every output yet".to_string());
                }
                if self.active_session_uid.is_some_and(|active| active != uid) {
                    return Err("Another user session is already authorized".to_string());
                }

                self.active_session_uid = Some(uid);
                tracing::info!(uid, ?session_id, "desktop session UID authorized");
                Ok(vec![CompositorMessage::SessionUserAuthorized { uid }])
            }

            ClientMessage::UnlockSession { lock_id } => {
                let previous_focus = self.session_lock.release(session_id, lock_id)?;

                self.events.broadcast_except(
                    session_id,
                    &CompositorMessage::SessionLockStateChanged { locked: false },
                );

                // Restore focus only if the window that had it still exists.
                let restored = previous_focus.filter(|&(owner, surface_id)| {
                    self.surface_manager
                        .get_surface(owner, surface_id)
                        .is_some()
                });
                self.set_keyboard_focus(restored);

                tracing::info!(?session_id, ?lock_id, "session lock released");
                Ok(vec![])
            }

            ClientMessage::RevokeSessionUser {
                uid,
                capability_token,
            } => {
                self.verify_capability_token(
                    session_id,
                    &Capability::SessionLock,
                    &capability_token,
                )?;
                if self.active_session_uid != Some(uid) {
                    return Err("User session UID is not currently authorized".to_string());
                }
                self.disconnect_uid_sessions(uid, Some(session_id));
                self.active_session_uid = None;
                tracing::info!(uid, ?session_id, "desktop session UID revoked");
                Ok(vec![CompositorMessage::SessionUserRevoked { uid }])
            }

            // ===== Data Transfer (Clipboard/DnD) =====
            ClientMessage::SetSelection { mime_types, serial } => {
                // The capability alone is not enough: clipboard writes are only
                // honored shortly after real user input, so a background app
                // cannot silently take over the clipboard.
                self.verify_interactive_capability(session_id, &Capability::ClipboardWrite)?;

                if mime_types.is_empty() {
                    return Err("Selection must offer at least one MIME type".to_string());
                }
                for mime in &mime_types {
                    if !crate::scp::data_device::is_valid_mime_type(mime) {
                        return Err(format!("Invalid MIME type: {mime}"));
                    }
                }

                self.data_device
                    .set_selection_validated(session_id, mime_types.clone(), serial)
                    .map_err(str::to_string)?;

                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.record_use(&Capability::ClipboardWrite);
                }
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::ClipboardWrite,
                    AuditOutcome::Used,
                );

                // Every other client learns that new content is available; only
                // clients that go on to call RequestSelection see any bytes.
                self.events.broadcast_except(
                    session_id,
                    &CompositorMessage::SelectionOffer { mime_types },
                );
                Ok(vec![])
            }

            ClientMessage::RequestSelection { mime_type } => {
                self.verify_interactive_capability(session_id, &Capability::ClipboardRead)?;

                let (owner, available) = self
                    .data_device
                    .get_selection()
                    .ok_or("No selection available")?;
                if owner == session_id {
                    return Err("Client already owns the selection".to_string());
                }
                if !available.contains(&mime_type) {
                    return Err(format!("Selection does not offer MIME type: {mime_type}"));
                }

                self.transfer_via_pipe(
                    owner,
                    session_id,
                    |mime_type, fd| CompositorMessage::RequestSelectionData { mime_type, fd },
                    |mime_type, fd| CompositorMessage::SelectionData { mime_type, fd },
                    mime_type,
                )?;

                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.record_use(&Capability::ClipboardRead);
                }
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::ClipboardRead,
                    AuditOutcome::Used,
                );
                Ok(vec![])
            }

            ClientMessage::StartDrag {
                surface_id,
                origin_surface,
                icon_surface,
                mime_types,
                serial,
            } => {
                // Dragging must follow a real gesture, not arrive unprompted.
                self.verify_interactive_capability(session_id, &Capability::DragAndDrop)?;
                self.verify_surface_ownership(session_id, surface_id)?;
                self.verify_surface_ownership(session_id, origin_surface)?;
                if let Some(icon) = icon_surface {
                    self.verify_surface_ownership(session_id, icon)?;
                }

                if mime_types.is_empty() {
                    return Err("Drag must offer at least one MIME type".to_string());
                }
                for mime in &mime_types {
                    if !crate::scp::data_device::is_valid_mime_type(mime) {
                        return Err(format!("Invalid MIME type: {mime}"));
                    }
                }

                self.data_device
                    .start_drag_validated(
                        session_id,
                        origin_surface,
                        icon_surface,
                        mime_types,
                        serial,
                    )
                    .map_err(str::to_string)?;

                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.record_use(&Capability::DragAndDrop);
                }
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::DragAndDrop,
                    AuditOutcome::Used,
                );
                Ok(vec![])
            }

            ClientMessage::AcceptDrag { serial, mime_type } => {
                if !self.data_device.is_serial_known(serial, session_id) {
                    return Err("Drag serial is stale or was issued to another client".to_string());
                }
                // Only the surface the drag is currently over may accept it.
                if !self
                    .data_device
                    .active_drag()
                    .is_some_and(|drag| drag.target == Some(session_id))
                {
                    return Err("Client is not the current drop target".to_string());
                }
                if let Some(mime) = &mime_type
                    && !self
                        .data_device
                        .active_drag()
                        .is_some_and(|drag| drag.mime_types.contains(mime))
                {
                    return Err(format!("Drag does not offer MIME type: {mime}"));
                }
                self.data_device
                    .accept_drag(mime_type)
                    .map_err(str::to_string)?;
                Ok(vec![])
            }

            ClientMessage::ReceiveDragData { mime_type } => {
                let drag = self
                    .data_device
                    .active_drag()
                    .ok_or("No active drag")?
                    .clone();
                if drag.target != Some(session_id) {
                    return Err("Client is not the current drop target".to_string());
                }
                if !drag.mime_types.contains(&mime_type) {
                    return Err(format!("Drag does not offer MIME type: {mime_type}"));
                }

                self.transfer_via_pipe(
                    drag.source,
                    session_id,
                    |mime_type, fd| CompositorMessage::RequestDragData { mime_type, fd },
                    |mime_type, fd| CompositorMessage::DragData { mime_type, fd },
                    mime_type,
                )?;
                Ok(vec![])
            }

            ClientMessage::FinishDrag => {
                let source = self
                    .data_device
                    .active_drag()
                    .ok_or("No active drag")?
                    .source;
                if source != session_id {
                    return Err("Only the drag source may finish a drag".to_string());
                }
                self.data_device.finish_drag().map_err(str::to_string)?;
                self.data_device.clear_drag();
                Ok(vec![CompositorMessage::DragFinished])
            }

            ClientMessage::CancelDrag => {
                let drag = self
                    .data_device
                    .active_drag()
                    .ok_or("No active drag")?
                    .clone();
                if drag.source != session_id {
                    return Err("Only the drag source may cancel a drag".to_string());
                }
                self.data_device.cancel_drag().map_err(str::to_string)?;
                self.data_device.clear_drag();

                // Tell the target the offer is gone so it can drop its state.
                if let Some(target) = drag.target {
                    self.events
                        .send_logged(target, CompositorMessage::DragLeave);
                }
                Ok(vec![CompositorMessage::DragCancelled])
            }

            ClientMessage::SetDragActions { actions, preferred } => {
                let action = self
                    .data_device
                    .set_drag_actions(session_id, actions, preferred)
                    .map_err(str::to_string)?;
                let source = self
                    .data_device
                    .active_drag()
                    .ok_or("No active drag")?
                    .source;
                self.events
                    .send_logged(source, CompositorMessage::DragActionSelected { action });
                Ok(vec![CompositorMessage::DragActionSelected { action }])
            }

            ClientMessage::RequestCapture {
                target,
                cursor_mode,
                capability_token,
            } => {
                let capability = capture_capability(target);
                self.verify_capability_token(session_id, &capability, &capability_token)?;
                self.verify_interactive_capability(session_id, &capability)?;

                let frame = self.capture_target(target)?;
                let width = frame.width();
                let height = frame.height();
                let stride = width.checked_mul(4).ok_or("Capture stride overflow")?;
                let fd = crate::scp::capture::export_frame(&frame)
                    .map_err(|error| format!("Failed to export capture: {error}"))?;
                let capture_id = self.next_capture_id;
                self.next_capture_id = self
                    .next_capture_id
                    .checked_add(1)
                    .ok_or("Capture ID space exhausted")?;
                let raw_fd = std::os::fd::AsRawFd::as_raw_fd(&fd);
                let event = OutboundEvent::with_fd(
                    CompositorMessage::CaptureGranted {
                        capture_id,
                        width,
                        height,
                        stride,
                        format: CaptureFormat::Rgba8888,
                        cursor_mode,
                        fd: raw_fd,
                    },
                    fd,
                );
                self.events
                    .send_event(session_id, event)
                    .map_err(|error| format!("Failed to queue capture frame: {error:?}"))?;

                self.consume_capability(session_id, &capability);
                self.security
                    .audit_capability_use(&app_id, &capability, AuditOutcome::Used);
                Ok(vec![])
            }

            ClientMessage::RegisterShortcut {
                binding,
                justification,
                capability_token,
            } => {
                let capability = Capability::GlobalShortcuts;
                self.verify_capability_token(session_id, &capability, &capability_token)?;
                self.verify_interactive_capability(session_id, &capability)?;
                let priority = if app_id.0 == crate::scp::security::SHELL_APP_ID {
                    ShortcutPriority::Shell
                } else {
                    ShortcutPriority::App
                };
                let (registration, displaced) =
                    self.shortcuts
                        .register(session_id, binding, priority, &justification)?;
                if let Some(displaced) = displaced {
                    self.events.send_logged(
                        displaced.owner,
                        CompositorMessage::ShortcutRevoked {
                            shortcut_id: displaced.id,
                            reason: "Displaced by a higher-priority binding".to_string(),
                        },
                    );
                }
                self.security
                    .audit_capability_use(&app_id, &capability, AuditOutcome::Used);
                Ok(vec![CompositorMessage::ShortcutGranted {
                    shortcut_id: registration.id,
                    binding,
                    priority,
                }])
            }

            ClientMessage::UnregisterShortcut { shortcut_id } => {
                self.shortcuts.unregister(session_id, shortcut_id)?;
                Ok(vec![])
            }
        };
        if result.is_ok() {
            self.sync_surface_outputs();
        }
        result
    }

    /// Hand a readable/writable pipe pair to two clients so content flows
    /// directly between them.
    ///
    /// The compositor mediates *authorization*, never the bytes: it creates the
    /// pipe, gives the producer the write end and the consumer the read end, and
    /// keeps neither. If either handoff fails, both ends are released with it.
    fn transfer_via_pipe(
        &mut self,
        producer: SessionId,
        consumer: SessionId,
        request: impl FnOnce(String, i32) -> CompositorMessage,
        deliver: impl FnOnce(String, i32) -> CompositorMessage,
        mime_type: String,
    ) -> Result<(), String> {
        let (read_fd, write_fd) = unix_socket::create_pipe()
            .map_err(|error| format!("Failed to create transfer pipe: {error}"))?;

        // SAFETY: create_pipe just handed us both ends and nothing else holds
        // them, so OutboundEvent may take ownership. Dropping an event closes
        // its descriptor, which the consumer observes as EOF.
        let request_event =
            unsafe { OutboundEvent::from_raw_fd(request(mime_type.clone(), write_fd), write_fd) };
        let deliver_event =
            unsafe { OutboundEvent::from_raw_fd(deliver(mime_type, read_fd), read_fd) };

        self.events
            .send_event(producer, request_event)
            .map_err(|error| format!("Data owner is unreachable: {error:?}"))?;
        self.events
            .send_event(consumer, deliver_event)
            .map_err(|error| format!("Requesting client is unreachable: {error:?}"))?;
        Ok(())
    }

    /// Compose one privacy-filtered capture target.
    fn capture_target(&self, target: CaptureTarget) -> Result<Framebuffer, String> {
        match target {
            CaptureTarget::Output(output_id) => self
                .compose_capture_output(output_id)
                .ok_or_else(|| "Capture output is unavailable".to_string()),
            CaptureTarget::Workspace => {
                let mut outputs = self.output_manager.outputs().iter();
                let first = outputs.next().ok_or("No output is available")?;
                let mut bounds = first.geometry;
                for output in outputs {
                    let right = bounds
                        .x
                        .saturating_add(bounds.width)
                        .max(output.geometry.x.saturating_add(output.geometry.width));
                    let bottom = bounds
                        .y
                        .saturating_add(bounds.height)
                        .max(output.geometry.y.saturating_add(output.geometry.height));
                    bounds.x = bounds.x.min(output.geometry.x);
                    bounds.y = bounds.y.min(output.geometry.y);
                    bounds.width = right.saturating_sub(bounds.x);
                    bounds.height = bottom.saturating_sub(bounds.y);
                }
                compose::compose_for(
                    crate::scp::compose::RenderPurpose::Capture,
                    bounds,
                    DESKTOP_CLEAR,
                    &self.build_stack(),
                    self,
                )
                .ok_or_else(|| "Workspace capture is too large".to_string())
            }
            CaptureTarget::Window(toplevel_id) => {
                let toplevel = self
                    .surface_manager
                    .get_toplevel(toplevel_id)
                    .ok_or("Capture window does not exist")?;
                let window = Rect {
                    x: toplevel.geometry.x,
                    y: toplevel.geometry.y,
                    width: toplevel.geometry.width,
                    height: toplevel.geometry.height,
                };
                let output = self
                    .output_manager
                    .outputs()
                    .iter()
                    .max_by_key(|output| intersection_area(window, output.geometry))
                    .filter(|output| intersection_area(window, output.geometry) > 0)
                    .ok_or("Capture window is not visible on an output")?;
                let local = Rect {
                    x: window.x.saturating_sub(output.geometry.x),
                    y: window.y.saturating_sub(output.geometry.y),
                    width: window.width,
                    height: window.height,
                };
                self.compose_capture_output(output.id)
                    .and_then(|frame| frame.cropped(local))
                    .ok_or_else(|| "Capture window has no visible pixels".to_string())
            }
        }
    }

    /// Reconcile each stacked surface's output membership and emit only the
    /// transitions. Repeated commits therefore do not produce duplicate events.
    fn sync_surface_outputs(&mut self) {
        let stack = self.build_stack();
        let mut desired: HashMap<(SessionId, SurfaceId), HashSet<OutputId>> = HashMap::new();
        for entry in stack.iter_bottom_up() {
            let outputs = desired
                .entry((entry.session_id, entry.surface_id))
                .or_default();
            for output in self.output_manager.outputs() {
                if intersection_area(entry.rect, output.geometry) > 0 {
                    outputs.insert(output.id);
                }
            }
        }

        let mut transitions = Vec::new();
        for (&(session_id, surface_id), outputs) in &desired {
            let previous = self.surface_outputs.get(&(session_id, surface_id));
            for &output_id in outputs {
                if previous.is_none_or(|previous| !previous.contains(&output_id)) {
                    transitions.push((
                        session_id,
                        CompositorMessage::SurfaceEnterOutput {
                            surface_id,
                            output_id,
                        },
                    ));
                }
            }
        }
        for (&(session_id, surface_id), outputs) in &self.surface_outputs {
            let current = desired.get(&(session_id, surface_id));
            for &output_id in outputs {
                if current.is_none_or(|current| !current.contains(&output_id)) {
                    transitions.push((
                        session_id,
                        CompositorMessage::SurfaceLeaveOutput {
                            surface_id,
                            output_id,
                        },
                    ));
                }
            }
        }
        self.surface_outputs = desired;
        self.events.send_all(transitions);
    }

    /// Remove a single-use grant after its operation has been successfully
    /// queued. Releasing it from the coordinator also prevents token replay.
    fn consume_capability(&mut self, session_id: SessionId, capability: &Capability) {
        let token = self
            .sessions
            .get_mut(&session_id)
            .and_then(|session| session.granted_capabilities.remove(capability))
            .map(|grant| grant.token);
        if let Some(token) = token {
            self.security.release_tokens(&[token]);
        }
    }

    fn handle_capability_request(
        &mut self,
        session_id: SessionId,
        app_id: AppId,
        capability_name: String,
        justification: String,
    ) -> Result<Vec<CompositorMessage>, String> {
        tracing::debug!(
            ?app_id,
            ?capability_name,
            ?justification,
            "capability requested"
        );
        let Some(capability) = Capability::from_wire_name(&capability_name) else {
            return Ok(vec![CompositorMessage::CapabilityDecision {
                capability: capability_name,
                granted: false,
                token: None,
                reason: Some("Unknown capability".to_string()),
                needs_user_consent: false,
            }]);
        };

        // Refuse before consulting policy at all: a locked session must not be
        // able to hand out capture or clipboard access, and a client must not
        // learn what it *would* have been granted either.
        if self.session_lock.is_locked() && blocked_while_locked(&capability) {
            self.security
                .audit_capability_use(&app_id, &capability, AuditOutcome::Denied);
            return Ok(vec![CompositorMessage::CapabilityDecision {
                capability: capability_name,
                granted: false,
                token: None,
                reason: Some("Refused while the session is locked".to_string()),
                needs_user_consent: false,
            }]);
        }

        match self.security.evaluate_capability(&app_id, &capability) {
            Decision::Granted { token, expires_at } => {
                let token_data = token.data.clone();
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .ok_or("Invalid session")?;
                let replaced = session.granted_capabilities.remove(&capability);
                session.grant_capability(capability.clone(), token, expires_at);
                if let Some(replaced) = replaced {
                    self.security.release_tokens(&[replaced.token]);
                }
                self.security
                    .audit_capability_use(&app_id, &capability, AuditOutcome::Granted);
                Ok(vec![CompositorMessage::CapabilityDecision {
                    capability: capability_name,
                    granted: true,
                    token: Some(token_data),
                    reason: None,
                    needs_user_consent: false,
                }])
            }
            Decision::Denied { reason } => {
                self.security
                    .audit_capability_use(&app_id, &capability, AuditOutcome::Denied);
                Ok(vec![CompositorMessage::CapabilityDecision {
                    capability: capability_name,
                    granted: false,
                    token: None,
                    reason: Some(reason),
                    needs_user_consent: false,
                }])
            }
            Decision::NeedsUserConsent { .. } => Ok(vec![CompositorMessage::CapabilityDecision {
                capability: capability_name,
                granted: false,
                token: None,
                reason: None,
                needs_user_consent: true,
            }]),
        }
    }

    /// Lock surface that should hold keyboard focus while the session is locked.
    ///
    /// The surface covering the primary output is preferred, so the prompt lands
    /// where the user is looking. The fallback is the lowest lock-surface id
    /// rather than whatever the surface map yields first, so the choice is
    /// deterministic across runs.
    fn primary_lock_focus(&self) -> Option<(SessionId, SurfaceId)> {
        let primary = self.output_manager.primary_output().map(|output| output.id);
        let mut candidates: Vec<_> = self.session_lock.iter_surfaces().collect();
        candidates.sort_by_key(|surface| surface.id);
        candidates
            .iter()
            .find(|surface| primary.is_some() && surface.output == primary)
            .or_else(|| candidates.first())
            .map(|surface| (surface.session_id, surface.surface_id))
    }

    fn verify_surface_ownership(
        &self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Result<(), String> {
        self.surface_manager
            .get_surface(session_id, surface_id)
            .map(|_| ())
            .ok_or_else(|| "Surface not found for this session".to_string())
    }

    fn cleanup_expired_capabilities(&mut self, session_id: SessionId) {
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return;
        };
        let expired: Vec<_> = session
            .granted_capabilities
            .iter()
            .filter_map(|(capability, grant)| (!grant.is_valid()).then_some(capability.clone()))
            .collect();
        let shortcuts_expired = expired.contains(&Capability::GlobalShortcuts);
        let expired_names: Vec<_> = expired
            .iter()
            .map(|capability| capability.wire_name().to_string())
            .collect();
        let tokens: Vec<_> = expired
            .into_iter()
            .filter_map(|capability| session.granted_capabilities.remove(&capability))
            .map(|grant| grant.token)
            .collect();
        self.security.release_tokens(&tokens);
        if shortcuts_expired {
            self.shortcuts.remove_session(session_id);
        }
        for capability in expired_names {
            self.events.send_logged(
                session_id,
                CompositorMessage::CapabilityRevoked {
                    capability,
                    reason: "Capability grant expired".to_string(),
                },
            );
        }
    }

    /// Apply an asynchronous policy revocation from the security service.
    /// Existing protocol objects remain valid unless they embody the revoked
    /// authority; global bindings, clipboard ownership, and active drags are
    /// withdrawn immediately.
    pub fn revoke_capability_for_app(
        &mut self,
        app_id: &AppId,
        capability: &Capability,
        reason: impl Into<String>,
    ) -> usize {
        let reason = reason.into();
        let affected: Vec<_> = self
            .sessions
            .iter()
            .filter_map(|(&session_id, session)| {
                (&session.app_id == app_id && session.granted_capabilities.contains_key(capability))
                    .then_some(session_id)
            })
            .collect();

        for session_id in &affected {
            if let Some(grant) = self
                .sessions
                .get_mut(session_id)
                .and_then(|session| session.granted_capabilities.remove(capability))
            {
                self.security.release_tokens(&[grant.token]);
            }
            if *capability == Capability::GlobalShortcuts {
                self.shortcuts.remove_session(*session_id);
            }
            if *capability == Capability::ClipboardWrite
                && self
                    .data_device
                    .get_selection_full()
                    .is_some_and(|selection| selection.owner == *session_id)
            {
                self.data_device.clear_selection();
                self.events
                    .broadcast_except(*session_id, &CompositorMessage::SelectionCleared);
            }
            if *capability == Capability::DragAndDrop
                && let Some(drag) = self.data_device.active_drag().cloned()
                && drag.source == *session_id
            {
                self.data_device.clear_drag();
                if let Some(target) = drag.target {
                    self.events
                        .send_logged(target, CompositorMessage::DragCancelled);
                }
            }
            self.events.send_logged(
                *session_id,
                CompositorMessage::CapabilityRevoked {
                    capability: capability.wire_name().to_string(),
                    reason: reason.clone(),
                },
            );
            self.security
                .audit_capability_use(app_id, capability, AuditOutcome::Denied);
        }
        affected.len()
    }

    fn verify_toplevel_ownership(
        &self,
        session_id: SessionId,
        toplevel_id: u32,
    ) -> Result<(), String> {
        let toplevel = self
            .surface_manager
            .get_toplevel(toplevel_id)
            .ok_or("Toplevel not found")?;
        if toplevel.session_id == session_id {
            Ok(())
        } else {
            Err("Toplevel does not belong to this session".to_string())
        }
    }

    /// Refuse a capability that a locked session must not honor.
    ///
    /// Grants outlive the moment they were issued, so a client that obtained
    /// clipboard or capture access before the lock still holds a valid token.
    /// The block therefore has to sit at the point of *use*, not only at the
    /// point of request, and it applies to the lock client too: authenticating a
    /// user needs none of these.
    fn reject_if_locked(&self, capability: &Capability) -> Result<(), String> {
        if self.session_lock.is_locked() && blocked_while_locked(capability) {
            return Err(format!(
                "Capability '{}' is refused while the session is locked",
                capability.wire_name()
            ));
        }
        Ok(())
    }

    fn verify_capability_token(
        &self,
        session_id: SessionId,
        capability: &Capability,
        token_data: &[u8],
    ) -> Result<(), String> {
        self.reject_if_locked(capability)?;
        let session = self.sessions.get(&session_id).ok_or("Invalid session")?;
        let grant = session
            .granted_capabilities
            .get(capability)
            .filter(|grant| grant.is_valid())
            .ok_or_else(|| format!("Capability '{}' is not granted", capability.wire_name()))?;
        if grant.token.data != token_data {
            return Err("Capability token does not match this session".to_string());
        }
        let (verified_app, verified_capability) = self
            .security
            .verify_token(&grant.token)
            .ok_or("Capability token is invalid or expired")?;
        if verified_app != session.app_id || verified_capability != *capability {
            return Err("Capability token identity or scope mismatch".to_string());
        }
        Ok(())
    }

    fn verify_capability(
        &self,
        session_id: SessionId,
        capability: &Capability,
    ) -> Result<(), String> {
        self.reject_if_locked(capability)?;
        self.sessions
            .get(&session_id)
            .ok_or("Invalid session")?
            .has_capability(capability)
            .then_some(())
            .ok_or_else(|| format!("Missing required capability: {}", capability.wire_name()))
    }

    /// Verify a capability whose grant is not sufficient on its own.
    ///
    /// Privacy-sensitive capabilities carry contextual preconditions: clipboard
    /// writes and drag initiation must follow recent user input, and clipboard
    /// reads require the app to actually be focused. A long-lived grant plus
    /// these checks is what prevents a backgrounded app from quietly reading or
    /// rewriting the clipboard.
    fn verify_interactive_capability(
        &self,
        session_id: SessionId,
        capability: &Capability,
    ) -> Result<(), String> {
        self.reject_if_locked(capability)?;
        let session = self.sessions.get(&session_id).ok_or("Invalid session")?;

        if !session.has_capability(capability) {
            return Err(format!(
                "Missing required capability: {}",
                capability.wire_name()
            ));
        }

        if requires_foreground(capability) && !session.is_foreground {
            return Err(format!(
                "Capability '{}' requires foreground focus",
                capability.wire_name()
            ));
        }

        if let Some(window) = requires_recent_interaction(capability)
            && !session.interacted_within(window)
        {
            return Err(format!(
                "Capability '{}' requires user interaction within {}ms",
                capability.wire_name(),
                window.as_millis()
            ));
        }

        Ok(())
    }

    fn verify_layer_ownership(
        &self,
        session_id: SessionId,
        layer_id: LayerSurfaceId,
    ) -> Result<(), String> {
        let layer = self
            .surface_manager
            .get_layer_surface(layer_id)
            .ok_or("Layer surface not found")?;
        if layer.session_id == session_id {
            Ok(())
        } else {
            Err("Layer surface does not belong to this session".to_string())
        }
    }

    /// Validate a directly attached buffer.
    ///
    /// Both halves of the check are shared with the SHM pool path, so the two
    /// cannot drift into disagreeing about what a legal buffer is: the geometry
    /// has to be arithmetically sound, and the descriptor has to actually back
    /// the bytes that geometry implies — for as long as the compositor will be
    /// reading them.
    fn validate_buffer(
        &self,
        fd: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: crate::scp::protocol::BufferFormat,
    ) -> Result<(), String> {
        let required =
            crate::scp::buffer::validate_geometry(width, height, stride, format.bytes_per_pixel())?;
        crate::scp::buffer::validate_descriptor(fd, required)
    }

    // ===== Session lifetime =====

    fn disconnect_uid_sessions(&mut self, uid: u32, except: Option<SessionId>) {
        let user_sessions: Vec<SessionId> = self
            .sessions
            .iter()
            .filter_map(|(&id, session)| (Some(id) != except && session.uid == uid).then_some(id))
            .collect();
        for user_session in user_sessions {
            self.disconnect(user_session);
        }
    }

    pub fn disconnect(&mut self, session_id: SessionId) {
        self.events.unregister(session_id);
        self.shortcuts.remove_session(session_id);

        // Grants die with the connection that holds them, so hand the tokens
        // back rather than leaving the coordinator tracking them forever.
        if let Some(session) = self.sessions.remove(&session_id) {
            let tokens: Vec<CapabilityToken> = session
                .granted_capabilities
                .into_values()
                .map(|grant| grant.token)
                .collect();
            self.security.release_tokens(&tokens);
        }

        // A locker that dies abandons its lock but does not release it: a crash
        // must never be a way back to the desktop.
        if self.session_lock.abandon(session_id) {
            self.focused_surface = None;
            self.input_state.set_keyboard_focus(None);
            self.input_state.set_pointer_focus(None);
            if let Some(uid) = self.active_session_uid.take() {
                self.disconnect_uid_sessions(uid, None);
            }
        }

        // A disconnecting client's popups are gone, but no one is left to
        // notify: drop them without emitting dismissal events.
        self.popups.remove_session(session_id);

        self.toplevel_stack.retain(|&toplevel_id| {
            self.surface_manager
                .get_toplevel(toplevel_id)
                .is_some_and(|toplevel| toplevel.session_id != session_id)
        });

        self.surface_manager.destroy_session(session_id);

        // Pools own their descriptors, so releasing them here is what keeps a
        // disconnect from leaking one.
        self.buffer_manager.destroy_session(session_id);
        self.dmabuf_manager.destroy_session(session_id);

        // Clipboard content is owned by a live client. Once it exits there is
        // nothing to read, so the offer must not outlive it.
        if self
            .data_device
            .get_selection_full()
            .is_some_and(|selection| selection.owner == session_id)
        {
            self.data_device.clear_selection();
            self.events
                .broadcast_except(session_id, &CompositorMessage::SelectionCleared);
        }

        if let Some(drag) = self.data_device.active_drag().cloned()
            && (drag.source == session_id || drag.target == Some(session_id))
        {
            self.data_device.clear_drag();
            let survivor = if drag.source == session_id {
                drag.target
            } else {
                Some(drag.source)
            };
            if let Some(survivor) = survivor.filter(|&id| id != session_id) {
                self.events
                    .send_logged(survivor, CompositorMessage::DragCancelled);
            }
        }

        if self
            .focused_surface
            .is_some_and(|(owner, _)| owner == session_id)
        {
            self.focused_surface = None;
            self.input_state.set_keyboard_focus(None);
        }
        if self
            .input_state
            .pointer_focus()
            .is_some_and(|(owner, _)| owner == session_id)
        {
            self.input_state.set_pointer_focus(None);
        }
        if self
            .cursor_surface
            .is_some_and(|(owner, _)| owner == session_id)
        {
            self.cursor_surface = None;
        }
        self.surface_outputs
            .retain(|(owner, _), _| *owner != session_id);
    }

    /// Remove a surface and finish the teardown its role implies.
    fn destroy_surface_and_role(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Result<(), String> {
        let managed_buffers = self
            .surface_manager
            .get_surface(session_id, surface_id)
            .ok_or("Surface not found")?
            .managed_buffer_ids();

        // Child popups must go first: once the parent surface is gone their
        // geometry can no longer be resolved.
        let orphaned = self
            .popups
            .dismiss_children_of_surface(session_id, surface_id);
        self.finish_popup_dismissal(&orphaned, DismissReason::ParentClosed);

        let role = self
            .surface_manager
            .destroy_surface(session_id, surface_id)?;

        // Destruction is also a presentation retirement point: the compositor
        // cannot read any of this surface's buffers again. Release every stage,
        // including a buffer attached but never committed.
        for buffer_id in managed_buffers {
            self.release_managed_buffer(session_id, buffer_id);
        }

        match role {
            SurfaceRole::Toplevel(toplevel_id) => {
                self.toplevel_stack.retain(|&id| id != toplevel_id);
            }
            SurfaceRole::Popup { .. } => {
                if let Some(popup_id) = self.popups.find_by_surface(session_id, surface_id) {
                    let dismissed = self.popups.dismiss_subtree(popup_id);
                    self.finish_popup_dismissal(&dismissed, DismissReason::ParentClosed);
                }
            }
            SurfaceRole::LockSurface(_) => {
                // Dropping a lock surface uncovers an output, so the lock must
                // re-confirm before it can be released again.
                self.session_lock.remove_surface(session_id, surface_id);
            }
            SurfaceRole::LayerShell(_) | SurfaceRole::None => {}
        }

        if self.focused_surface == Some((session_id, surface_id)) {
            self.focused_surface = None;
            self.input_state.set_keyboard_focus(None);
        }
        if self.input_state.pointer_focus() == Some((session_id, surface_id)) {
            self.input_state.set_pointer_focus(None);
        }
        if self.cursor_surface == Some((session_id, surface_id)) {
            self.cursor_surface = None;
        }
        Ok(())
    }

    /// Close a toplevel and tell its client the window is gone.
    ///
    /// Used both for client-initiated closes and for the compositor's own titlebar
    /// close button.
    pub fn close_toplevel(&mut self, toplevel_id: ToplevelId) {
        let Some(toplevel) = self.surface_manager.get_toplevel(toplevel_id) else {
            return;
        };
        let session_id = toplevel.session_id;
        let surface_id = toplevel.surface_id;

        let dismissed = self
            .popups
            .dismiss_children_of_surface(session_id, surface_id);
        self.finish_popup_dismissal(&dismissed, DismissReason::ParentClosed);

        self.surface_manager.remove_toplevel(toplevel_id);
        self.toplevel_stack.retain(|&id| id != toplevel_id);

        if self.focused_surface == Some((session_id, surface_id)) {
            self.focused_surface = None;
            self.input_state.set_keyboard_focus(None);
            if let Some(session) = self.sessions.get_mut(&session_id) {
                session.is_foreground = false;
            }
        }

        self.events.send_logged(
            session_id,
            CompositorMessage::ToplevelClosed { toplevel_id },
        );

        // Hand focus to whatever is now on top so the session does not end up
        // with no focused window at all.
        if self.focused_surface.is_none()
            && let Some(next) = self
                .toplevel_stack
                .first()
                .and_then(|&id| self.surface_manager.get_toplevel(id))
                .map(|toplevel| (toplevel.session_id, toplevel.surface_id))
        {
            self.set_keyboard_focus(Some(next));
        }
        self.sync_surface_outputs();
    }

    fn focused_surface_is_toplevel(&self, toplevel_id: ToplevelId) -> bool {
        self.surface_manager
            .get_toplevel(toplevel_id)
            .is_some_and(|toplevel| {
                self.focused_surface == Some((toplevel.session_id, toplevel.surface_id))
            })
    }

    // ===== Renderer-facing views =====

    pub fn iter_toplevels(&self) -> impl Iterator<Item = &crate::scp::surface::Toplevel> {
        self.surface_manager.iter_toplevels()
    }

    pub fn iter_layer_surfaces(&self) -> impl Iterator<Item = &crate::scp::surface::LayerSurface> {
        self.surface_manager.iter_layer_surfaces()
    }

    pub fn iter_layer_surfaces_sorted(&self) -> Vec<&crate::scp::surface::LayerSurface> {
        self.surface_manager.iter_layer_surfaces_sorted()
    }

    /// Resolve complete plane and modifier metadata for native GPU import.
    pub fn committed_dmabuf(
        &self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Option<&crate::scp::dmabuf::Dmabuf> {
        let buffer = self
            .surface_manager
            .get_surface(session_id, surface_id)?
            .buffer
            .as_ref()?;
        if buffer.kind != crate::scp::surface::SurfaceBufferKind::Dmabuf {
            return None;
        }
        self.dmabuf_manager.get_buffer(session_id, buffer.buffer_id)
    }

    /// Deliver frame callbacks for every surface that requested one.
    ///
    /// Called by the renderer after presenting a frame. Returns the number of
    /// callbacks queued.
    pub fn send_frame_callbacks(&mut self, timestamp_ms: u64) -> usize {
        let callbacks = self.surface_manager.take_frame_callbacks();
        let count = callbacks.len();

        for (session_id, surface_id, callback_id) in callbacks {
            self.events.send_logged(
                session_id,
                CompositorMessage::FrameCallback {
                    surface_id,
                    callback_id,
                    timestamp_ms,
                },
            );
        }
        count
    }

    // ===== Presentation =====

    /// Compose one output's current image without mutating protocol state.
    ///
    /// Read-only on purpose: a screenshot, a test assertion, or a diagnostic
    /// dump must be able to look at the desktop without telling clients a frame
    /// was presented. [`Self::present_frame`] is the pass that has that effect.
    pub fn compose_output(&self, output_id: OutputId) -> Option<Framebuffer> {
        let output = self.output_manager.get_output(output_id)?;
        compose::compose(output.geometry, DESKTOP_CLEAR, &self.build_stack(), self)
    }

    /// Compose the primary output, or `None` when no output is configured.
    pub fn compose_primary_output(&self) -> Option<Framebuffer> {
        let output_id = self.output_manager.primary_output()?.id;
        self.compose_output(output_id)
    }

    /// Compose the only framebuffer that may be exported outside the
    /// compositor. Protected surfaces are replaced before their buffers are
    /// read, while unrelated regions remain intact.
    pub fn compose_capture_output(&self, output_id: OutputId) -> Option<Framebuffer> {
        let output = self.output_manager.get_output(output_id)?;
        compose::compose_for(
            compose::RenderPurpose::Capture,
            output.geometry,
            DESKTOP_CLEAR,
            &self.build_stack(),
            self,
        )
    }

    /// Capture-safe composition of the primary output.
    pub fn compose_capture_primary_output(&self) -> Option<Framebuffer> {
        let output_id = self.output_manager.primary_output()?.id;
        self.compose_capture_output(output_id)
    }

    /// Apply a capture policy issued by a trusted in-process broker adapter.
    ///
    /// This is intentionally an API on compositor state rather than an SCP
    /// client message. The caller must authenticate a protected-media,
    /// privacy, or authentication grant before reaching this method; ordinary
    /// applications cannot self-assert capture immunity over the wire.
    pub fn set_broker_capture_policy(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
        policy: crate::scp::surface::CapturePolicy,
    ) -> Result<(), String> {
        let surface = self
            .surface_manager
            .get_surface_mut(session_id, surface_id)
            .ok_or("Surface not found")?;
        surface.set_capture_policy(policy);
        Ok(())
    }

    /// Compose one output and complete the frame for its clients.
    ///
    /// The order matters: buffers are released and callbacks fire only *after*
    /// composition has finished reading, because a client is entitled to draw
    /// into a buffer the moment it is released. A frame that fails to compose
    /// releases nothing and fires nothing, so the client's next commit still
    /// finds its previous buffer intact.
    pub fn present_frame(&mut self, output_id: OutputId, timestamp_ms: u64) -> Option<Framebuffer> {
        let framebuffer = self.compose_output(output_id)?;
        self.finish_presented_frame(timestamp_ms);
        Some(framebuffer)
    }

    /// Complete a frame after a backend has accepted it for presentation.
    ///
    /// Hardware backends use this two-phase API (`compose_output`, scanout,
    /// then `finish_presented_frame`) so a failed page flip cannot falsely tell
    /// clients that their frame was displayed. The headless backend uses
    /// `present_frame`, whose in-memory presentation cannot fail after compose.
    pub fn finish_presented_frame(&mut self, timestamp_ms: u64) {
        self.release_presented_buffers();
        self.send_frame_callbacks(timestamp_ms);
    }

    /// Hand back every buffer a later commit replaced. Returns how many.
    pub fn release_presented_buffers(&mut self) -> usize {
        let released = self.surface_manager.take_released_buffers();
        let count = released.len();
        for (session_id, buffer_id) in released {
            self.release_managed_buffer(session_id, buffer_id);
        }
        count
    }

    fn release_managed_buffer(&mut self, session_id: SessionId, buffer_id: BufferId) {
        let released = if self
            .buffer_manager
            .get_buffer(session_id, buffer_id)
            .is_some()
        {
            self.buffer_manager
                .mark_buffer_released(session_id, buffer_id)
        } else {
            self.dmabuf_manager
                .mark_buffer_released(session_id, buffer_id)
        };
        match released {
            Ok(()) => self
                .events
                .send_logged(session_id, CompositorMessage::BufferRelease { buffer_id }),
            Err(error) => tracing::warn!(
                session_id,
                buffer_id,
                %error,
                "managed SCP buffer disappeared before release"
            ),
        }
    }

    pub const fn get_focused_surface(&self) -> Option<(SessionId, SurfaceId)> {
        self.focused_surface
    }

    pub const fn get_cursor_surface(&self) -> Option<(SessionId, SurfaceId)> {
        self.cursor_surface
    }

    pub const fn pointer_position(&self) -> (f64, f64) {
        self.pointer_position
    }

    pub fn get_popup(&self, popup_id: PopupId) -> Option<&Popup> {
        self.popups.get(popup_id)
    }

    // ===== Focus =====

    /// Move keyboard focus, emitting the leave/enter transition.
    ///
    /// Focus is what gates privacy-sensitive capabilities like clipboard reads,
    /// so `is_foreground` is maintained here rather than by callers.
    pub fn set_keyboard_focus(&mut self, target: Option<(SessionId, SurfaceId)>) {
        if self.focused_surface == target {
            return;
        }

        // While locked, focus may only rest on a lock surface or nowhere.
        // Refusing here rather than only at key delivery matters because
        // focusing a surface also marks its session foreground, which is a
        // precondition some capabilities check — a window closing behind the
        // lock screen must not be able to inherit that.
        if self.session_lock.is_locked()
            && let Some((owner, surface_id)) = target
            && !self.session_lock.is_lock_surface(owner, surface_id)
        {
            tracing::warn!(
                ?owner,
                ?surface_id,
                "refused keyboard focus to a non-lock surface while the session is locked"
            );
            return;
        }

        if let Some((session_id, surface_id)) = self.focused_surface.take() {
            let serial = self.next_serial();
            self.events.send_logged(
                session_id,
                CompositorMessage::InputEvent {
                    surface_id,
                    event: InputEvent::KeyboardLeave { serial },
                },
            );
            if let Some(session) = self.sessions.get_mut(&session_id) {
                session.is_foreground = false;
            }
            self.set_toplevel_activation(session_id, surface_id, false);
        }

        self.focused_surface = target;
        self.input_state.set_keyboard_focus(target);

        let Some((session_id, surface_id)) = target else {
            return;
        };

        // The keymap has to arrive before any key event, or the client cannot
        // interpret the keycodes it is about to receive.
        match self.keymap_state.create_memfd() {
            Ok(fd) => {
                let message = CompositorMessage::KeymapFormat {
                    format: match self.keymap_state.format() {
                        crate::scp::keymap::KeymapFormat::NoKeymap => {
                            crate::scp::protocol::KeymapFormat::NoKeymap
                        }
                        crate::scp::keymap::KeymapFormat::XkbV1 => {
                            crate::scp::protocol::KeymapFormat::XkbV1
                        }
                    },
                    fd,
                    size: self.keymap_state.size(),
                };
                // SAFETY: create_memfd returned a fresh owned descriptor that
                // nothing else holds, so the event may take ownership of it.
                let event = unsafe { OutboundEvent::from_raw_fd(message, fd) };
                if let Err(error) = self.events.send_event(session_id, event) {
                    tracing::debug!(?session_id, ?error, "failed to deliver keymap");
                }
            }
            Err(error) => tracing::warn!(?error, "failed to publish keymap"),
        }

        let repeat_info = CompositorMessage::RepeatInfo {
            rate: self.repeat_info.rate,
            delay: self.repeat_info.delay,
        };
        self.events.send_logged(session_id, repeat_info);

        let serial = self.next_serial();
        let keys = self.modifier_state.pressed_keys();
        self.record_delivered_serial(serial, session_id);
        self.events.send_logged(
            session_id,
            CompositorMessage::InputEvent {
                surface_id,
                event: InputEvent::KeyboardEnter { serial, keys },
            },
        );
        self.send_modifiers(session_id, surface_id, serial);

        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.is_foreground = true;
        }
        self.set_toplevel_activation(session_id, surface_id, true);
    }

    /// Update and re-publish a toplevel's activated state after a focus change.
    fn set_toplevel_activation(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
        activated: bool,
    ) {
        let Some(toplevel_id) = self
            .surface_manager
            .iter_toplevels()
            .find(|toplevel| toplevel.session_id == session_id && toplevel.surface_id == surface_id)
            .map(|toplevel| toplevel.id)
        else {
            return;
        };

        let serial = self.next_serial();
        let Some(toplevel) = self.surface_manager.get_toplevel_mut(toplevel_id) else {
            return;
        };
        if toplevel.states.activated == activated {
            return;
        }
        toplevel.set_activated(activated);

        let states = toplevel.states.clone();
        let (width, height) = (toplevel.geometry.width, toplevel.geometry.height);
        toplevel.configure(serial, width, height, states.clone());

        self.events.send_logged(
            session_id,
            CompositorMessage::ConfigureToplevel {
                toplevel_id,
                serial,
                width,
                height,
                decoration_height: if states.fullscreen {
                    0
                } else {
                    DECORATION_HEIGHT
                },
                states: crate::scp::protocol::ToplevelStates {
                    activated: states.activated,
                    maximized: states.maximized,
                    fullscreen: states.fullscreen,
                    resizing: states.resizing,
                },
            },
        );
    }

    // ===== Popup dismissal =====

    /// Notify clients about dismissed popups and release their surface roles.
    fn finish_popup_dismissal(&mut self, dismissed: &[Popup], reason: DismissReason) {
        for popup in dismissed {
            if self.focused_surface == Some(popup.key()) {
                self.focused_surface = None;
                self.input_state.set_keyboard_focus(None);
            }
            if self.input_state.pointer_focus() == Some(popup.key()) {
                self.input_state.set_pointer_focus(None);
            }

            self.events.send_logged(
                popup.session_id,
                CompositorMessage::PopupDismissed {
                    popup_id: popup.id,
                    reason,
                },
            );
        }
    }

    /// Dismiss a popup and its descendants, reporting the reason to the client.
    ///
    /// Returns the number of popups dismissed.
    pub fn dismiss_popup(&mut self, popup_id: PopupId, reason: DismissReason) -> usize {
        let dismissed = self.popups.dismiss_subtree(popup_id);
        let count = dismissed.len();
        self.finish_popup_dismissal(&dismissed, reason);
        self.sync_surface_outputs();
        count
    }

    // ===== Keyboard input =====

    /// Publish repeat settings to a client.
    pub const fn send_repeat_info(&self) -> CompositorMessage {
        CompositorMessage::RepeatInfo {
            rate: self.repeat_info.rate,
            delay: self.repeat_info.delay,
        }
    }

    /// Route a key event from the input backend to the focused surface.
    ///
    /// Escape is intercepted while a popup grab is active so menus close
    /// predictably even if the focused client ignores the key.
    pub fn handle_key(
        &mut self,
        keycode: u32,
        state: crate::scp::protocol::KeyState,
        time_ms: u32,
    ) {
        use crate::scp::protocol::KeyState;

        match state {
            KeyState::Pressed => self.modifier_state.key_pressed(keycode),
            KeyState::Released => self.modifier_state.key_released(keycode),
        }

        // A global binding consumes both halves of its key sequence. Delivering
        // only the release to the focused application would leave its keyboard
        // state inconsistent.
        if state == KeyState::Released && self.active_shortcut_keys.remove(&keycode) {
            return;
        }
        if state == KeyState::Pressed && !self.session_lock.is_locked() {
            let modifiers = self.modifier_state.mods_depressed
                | self.modifier_state.mods_latched
                | self.modifier_state.mods_locked;
            if let Some(shortcut) = self.shortcuts.matching(KeyBinding { keycode, modifiers }) {
                self.active_shortcut_keys.insert(keycode);
                self.events.send_logged(
                    shortcut.owner,
                    CompositorMessage::ShortcutActivated {
                        shortcut_id: shortcut.id,
                        timestamp_ms: time_ms,
                    },
                );
                return;
            }
        }

        if state == KeyState::Pressed
            && keycode == KEY_ESCAPE
            && let Some(innermost) = self.popups.innermost_grab()
        {
            let dismissed = self.popups.dismiss_subtree(innermost);
            self.finish_popup_dismissal(&dismissed, DismissReason::EscapeKey);
            return;
        }

        let Some((session_id, surface_id)) = self.focused_surface else {
            return;
        };

        // Pointer and touch are confined by the window stack, but keyboard
        // focus is held in a field that could predate the lock. Check the role
        // explicitly so keystrokes — a typed password among them — cannot reach
        // a window that is no longer on screen.
        if self.session_lock.is_locked()
            && !self.session_lock.is_lock_surface(session_id, surface_id)
        {
            return;
        }

        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.record_user_interaction();
        }

        let serial = self.next_serial();
        if state == KeyState::Pressed {
            self.authorize_serial(serial, session_id);
        }
        self.events.send_logged(
            session_id,
            CompositorMessage::InputEvent {
                surface_id,
                event: InputEvent::KeyboardKey {
                    serial,
                    key: keycode,
                    state,
                    time_ms,
                },
            },
        );
        self.send_modifiers(session_id, surface_id, serial);
    }

    fn send_modifiers(&self, session_id: SessionId, surface_id: SurfaceId, serial: u32) {
        self.events.send_logged(
            session_id,
            CompositorMessage::Modifiers {
                surface_id,
                serial,
                mods_depressed: self.modifier_state.mods_depressed,
                mods_latched: self.modifier_state.mods_latched,
                mods_locked: self.modifier_state.mods_locked,
                group: self.modifier_state.group,
            },
        );
    }

    /// Reset modifier state, e.g. when the compositor session loses the VT.
    pub fn reset_keyboard_state(&mut self) {
        self.modifier_state.reset();
        self.active_shortcut_keys.clear();
    }

    pub const fn modifier_state(&self) -> &ModifierState {
        &self.modifier_state
    }

    // ===== Pointer input =====

    /// Route pointer motion, updating pointer focus as the cursor crosses
    /// window boundaries.
    ///
    /// The backend is expected to call [`Self::handle_pointer_frame`] once it has
    /// finished submitting a batch of pointer events.
    pub fn handle_pointer_motion(&mut self, x: f64, y: f64, time_ms: u32) {
        self.pointer_position = (x, y);
        self.input_state.set_pointer_position((x, y));

        let hit = self.hit_test(x, y);

        // A drag in flight replaces normal motion with drag events: the target
        // is offered data, not raw pointer input.
        if self.data_device.active_drag().is_some() {
            self.dispatch_drag_motion(hit, x, y, time_ms);
            return;
        }

        let changed = self.sync_pointer_focus(hit, x, y);
        if !changed && let Some(entry) = hit {
            let (local_x, local_y) = entry.to_local(x, y);
            self.events.send_logged(
                entry.session_id,
                CompositorMessage::InputEvent {
                    surface_id: entry.surface_id,
                    event: InputEvent::PointerMotion {
                        x: local_x,
                        y: local_y,
                        time_ms,
                    },
                },
            );
        }
    }

    /// Bring pointer focus in line with the window under the cursor.
    ///
    /// Returns whether focus changed, which tells the caller an enter event was
    /// already sent with the current position and plain motion is redundant.
    fn sync_pointer_focus(&mut self, hit: Option<StackEntry>, x: f64, y: f64) -> bool {
        let target = hit.map(|entry| (entry.session_id, entry.surface_id));
        if self.input_state.pointer_focus() == target {
            return false;
        }

        if let Some((session_id, surface_id)) = self.input_state.pointer_focus() {
            let serial = self.next_serial();
            self.events.send_logged(
                session_id,
                CompositorMessage::InputEvent {
                    surface_id,
                    event: InputEvent::PointerLeave { serial },
                },
            );
        }

        self.input_state.set_pointer_focus(target);

        if let Some(entry) = hit {
            let (local_x, local_y) = entry.to_local(x, y);
            let serial = self.next_serial();
            // A cursor change answers this event, so the client has to be able
            // to quote the serial back — without it authorizing anything more.
            self.record_delivered_serial(serial, entry.session_id);
            self.events.send_logged(
                entry.session_id,
                CompositorMessage::InputEvent {
                    surface_id: entry.surface_id,
                    event: InputEvent::PointerEnter {
                        serial,
                        x: local_x,
                        y: local_y,
                    },
                },
            );
        }
        true
    }

    /// Route a pointer button, applying popup grab and focus-follows-click rules.
    pub fn handle_pointer_button(&mut self, button: u32, state: ButtonState, time_ms: u32) {
        let (x, y) = self.pointer_position;
        let hit = self.hit_test(x, y);
        let hit_surface = hit.map(|entry| (entry.session_id, entry.surface_id));

        if state == ButtonState::Pressed && self.popups.has_grab() {
            let hit_popup = hit_surface
                .and_then(|(session_id, surface_id)| {
                    self.popups.find_by_surface(session_id, surface_id)
                })
                .filter(|id| self.popups.grab_chain().contains(id));

            match hit_popup {
                // Clicking an outer menu collapses the submenus it opened, then
                // the click is delivered normally.
                Some(popup_id) => {
                    let dismissed = self.popups.dismiss_descendants(popup_id);
                    self.finish_popup_dismissal(&dismissed, DismissReason::OutsideClick);
                }
                // Clicking outside the chain closes it. The grab consumes the
                // click: that is the point of a grab, and it keeps a stray click
                // from also activating whatever was behind the menu.
                None => {
                    let dismissed = self.popups.dismiss_grab_chain();
                    self.finish_popup_dismissal(&dismissed, DismissReason::OutsideClick);
                    return;
                }
            }
        }

        // A drop ends the drag rather than delivering a button to the target.
        if state == ButtonState::Released && self.data_device.active_drag().is_some() {
            self.dispatch_drop();
            return;
        }

        self.sync_pointer_focus(hit, x, y);

        let Some(entry) = hit else {
            // A click on empty desktop clears focus so no window keeps the
            // privileges that focus confers.
            if state == ButtonState::Pressed {
                self.set_keyboard_focus(None);
            }
            return;
        };

        if state == ButtonState::Pressed {
            if let Some(session) = self.sessions.get_mut(&entry.session_id) {
                session.record_user_interaction();
            }
            if entry.accepts_keyboard {
                if let StackKind::Toplevel(toplevel_id) = entry.kind {
                    self.raise_toplevel(toplevel_id);
                }
                self.set_keyboard_focus(Some((entry.session_id, entry.surface_id)));
            }
        }

        let serial = self.next_serial();
        if state == ButtonState::Pressed {
            self.authorize_serial(serial, entry.session_id);
        }
        self.events.send_logged(
            entry.session_id,
            CompositorMessage::InputEvent {
                surface_id: entry.surface_id,
                event: InputEvent::PointerButton {
                    serial,
                    button,
                    state,
                    time_ms,
                },
            },
        );
    }

    /// Mark a serial as one a client may quote to authorize a privileged action.
    ///
    /// Clipboard writes and drag initiation must cite the serial of a real input
    /// event. Only deliberate actions — a button press, a key press, a touch —
    /// mint one: passive pointer motion must not be enough to authorize taking
    /// over the clipboard, or merely moving the cursor across a window would.
    fn authorize_serial(&mut self, serial: u32, session_id: SessionId) {
        self.data_device.record_serial(serial, session_id, true);
    }

    /// Note a serial the compositor sent a client without authorizing anything.
    ///
    /// Enter and leave events carry serials a client may legitimately quote back
    /// — a cursor change answers a pointer enter — but crossing a window is not
    /// the user asking for anything, so these do not authorize privileged
    /// actions.
    fn record_delivered_serial(&mut self, serial: u32, session_id: SessionId) {
        self.data_device.record_serial(serial, session_id, false);
    }

    /// Route a scroll event to the surface under the cursor.
    pub fn handle_pointer_axis(
        &mut self,
        axis_source: crate::scp::protocol::AxisSource,
        orientation: crate::scp::protocol::Orientation,
        value: f64,
        discrete: i32,
        time_ms: u32,
    ) {
        let Some((session_id, surface_id)) = self.input_state.pointer_focus() else {
            return;
        };
        self.events.send_logged(
            session_id,
            CompositorMessage::InputEvent {
                surface_id,
                event: InputEvent::PointerAxis {
                    time_ms,
                    axis_source,
                    orientation,
                    value,
                    discrete,
                },
            },
        );
    }

    /// Mark the end of a batch of pointer events.
    pub fn handle_pointer_frame(&mut self) {
        let Some((session_id, surface_id)) = self.input_state.pointer_focus() else {
            return;
        };
        self.events.send_logged(
            session_id,
            CompositorMessage::InputEvent {
                surface_id,
                event: InputEvent::PointerFrame,
            },
        );
    }

    // ===== Touch input =====

    /// Route a new touch point, focusing the window it lands on.
    pub fn handle_touch_down(&mut self, touch_id: i32, x: f64, y: f64, time_ms: u32) {
        let Some(entry) = self.hit_test(x, y) else {
            return;
        };

        // A touch is a user interaction and a focus request, just like a click.
        if let Some(session) = self.sessions.get_mut(&entry.session_id) {
            session.record_user_interaction();
        }
        if entry.accepts_keyboard {
            if let StackKind::Toplevel(toplevel_id) = entry.kind {
                self.raise_toplevel(toplevel_id);
            }
            self.set_keyboard_focus(Some((entry.session_id, entry.surface_id)));
        }

        let (local_x, local_y) = entry.to_local(x, y);
        let serial = self.next_serial();
        // A touch is as deliberate as a click, so it authorizes the same actions.
        self.authorize_serial(serial, entry.session_id);
        let event = self.input_state.dispatch_touch_down(
            (entry.session_id, entry.surface_id),
            touch_id,
            local_x,
            local_y,
            time_ms,
            serial,
        );
        self.events.send_logged(
            entry.session_id,
            CompositorMessage::InputEvent {
                surface_id: entry.surface_id,
                event,
            },
        );
    }

    /// Route touch movement to the surface the point started on.
    ///
    /// Touch sequences are sticky: a point stays with its original surface even
    /// if it slides outside, so a drag that leaves a button still ends there.
    pub fn handle_touch_motion(&mut self, touch_id: i32, x: f64, y: f64, time_ms: u32) {
        let Some((session_id, surface_id)) = self.input_state.touch_surface(touch_id) else {
            return;
        };
        let Some(rect) = self.absolute_surface_rect(session_id, surface_id) else {
            return;
        };

        let local_x = x - f64::from(rect.x);
        let local_y = y - f64::from(rect.y);
        let Some(event) = self
            .input_state
            .dispatch_touch_motion(touch_id, local_x, local_y, time_ms)
        else {
            return;
        };
        self.events.send_logged(
            session_id,
            CompositorMessage::InputEvent { surface_id, event },
        );
    }

    /// Route the release of a touch point.
    pub fn handle_touch_up(&mut self, touch_id: i32, time_ms: u32) {
        let Some((session_id, surface_id)) = self.input_state.touch_surface(touch_id) else {
            return;
        };
        let serial = self.next_serial();
        let Some(event) = self
            .input_state
            .dispatch_touch_up(touch_id, time_ms, serial)
        else {
            return;
        };
        self.events.send_logged(
            session_id,
            CompositorMessage::InputEvent { surface_id, event },
        );
    }

    /// Cancel every active touch point, e.g. when a system gesture takes over.
    pub fn handle_touch_cancel(&mut self) {
        let affected = self.input_state.touch_surfaces();
        self.input_state.dispatch_touch_cancel();

        for (session_id, surface_id) in affected {
            self.events.send_logged(
                session_id,
                CompositorMessage::InputEvent {
                    surface_id,
                    event: InputEvent::TouchCancel,
                },
            );
        }
    }

    /// Mark the end of a batch of touch events.
    pub fn handle_touch_frame(&mut self) {
        for (session_id, surface_id) in self.input_state.touch_surfaces() {
            self.events.send_logged(
                session_id,
                CompositorMessage::InputEvent {
                    surface_id,
                    event: InputEvent::TouchFrame,
                },
            );
        }
    }

    // ===== Drag-and-drop routing =====

    /// Move the drag over the window under the cursor, emitting enter/leave.
    fn dispatch_drag_motion(&mut self, hit: Option<StackEntry>, x: f64, y: f64, time_ms: u32) {
        let Some(drag) = self.data_device.active_drag().cloned() else {
            return;
        };

        // The drag source's own surfaces are still valid drop targets, but a
        // surface that rejects input is not.
        let target = hit.map(|entry| (entry.session_id, entry.surface_id));
        let previous = self.data_device.drag_surface();

        if previous == target {
            if let Some((session_id, _)) = target {
                let (local_x, local_y) = hit.map(|entry| entry.to_local(x, y)).unwrap_or((x, y));
                self.events.send_logged(
                    session_id,
                    CompositorMessage::DragMotion {
                        x: local_x,
                        y: local_y,
                        time_ms,
                    },
                );
            }
            return;
        }

        if let Some((session_id, _)) = previous {
            self.events
                .send_logged(session_id, CompositorMessage::DragLeave);
        }

        self.data_device.set_drag_surface(target);
        self.data_device
            .set_drag_target(target.map(|(session_id, _)| session_id));

        if let Some(entry) = hit {
            let (local_x, local_y) = entry.to_local(x, y);
            let serial = self.next_serial();
            // AcceptDrag quotes this serial back.
            self.record_delivered_serial(serial, entry.session_id);
            self.events.send_logged(
                entry.session_id,
                CompositorMessage::DragEnter {
                    serial,
                    surface_id: entry.surface_id,
                    x: local_x,
                    y: local_y,
                    mime_types: drag.mime_types,
                },
            );
        }
    }

    /// Complete a drag: tell the target to drop, or cancel if there is none.
    fn dispatch_drop(&mut self) {
        let Some(drag) = self.data_device.active_drag().cloned() else {
            return;
        };

        match drag.target {
            // The target now asks for the data with ReceiveDragData, so the drag
            // stays alive until it finishes or is cancelled.
            Some(target) => self.events.send_logged(target, CompositorMessage::Drop),
            None => {
                self.data_device.clear_drag();
                self.events
                    .send_logged(drag.source, CompositorMessage::DragCancelled);
            }
        }
    }

    // ===== Clipboard =====

    /// Clear the clipboard and tell every client the offer is gone.
    pub fn clear_selection(&mut self) {
        let owner = self
            .data_device
            .get_selection_full()
            .map(|selection| selection.owner);
        self.data_device.clear_selection();

        for session_id in self.events.session_ids().collect::<Vec<_>>() {
            if Some(session_id) == owner {
                continue;
            }
            self.events
                .send_logged(session_id, CompositorMessage::SelectionCleared);
        }
    }
}

impl Default for ScpState {
    fn default() -> Self {
        Self::new()
    }
}

fn negotiate_version(minimum: u32, maximum: u32) -> Result<u32, String> {
    if minimum > maximum {
        return Err("Protocol version range is inverted".to_string());
    }
    let selected = maximum.min(CURRENT_PROTOCOL_VERSION);
    if selected < minimum || selected < MIN_PROTOCOL_VERSION {
        return Err(format!(
            "No compatible SCP version (server supports {MIN_PROTOCOL_VERSION}..={CURRENT_PROTOCOL_VERSION})"
        ));
    }
    Ok(selected)
}

fn protocol_features(version: u32) -> Vec<String> {
    if version < 2 {
        return Vec::new();
    }
    [
        "capture-fd",
        "capability-revocation",
        "drag-actions",
        "dmabuf-v1",
        "global-shortcuts",
        "output-hotplug",
        "shm-buffer-objects",
        "version-negotiation",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn capture_capability(target: CaptureTarget) -> Capability {
    Capability::ScreenCapture {
        scope: match target {
            CaptureTarget::Window(_) => CaptureScope::SingleWindow,
            CaptureTarget::Output(_) => CaptureScope::Output,
            CaptureTarget::Workspace => CaptureScope::Workspace,
        },
    }
}

fn intersection_area(left: Rect, right: Rect) -> i64 {
    let width = left
        .x
        .saturating_add(left.width)
        .min(right.x.saturating_add(right.width))
        .saturating_sub(left.x.max(right.x))
        .max(0);
    let height = left
        .y
        .saturating_add(left.height)
        .min(right.y.saturating_add(right.height))
        .saturating_sub(left.y.max(right.y))
        .max(0);
    i64::from(width) * i64::from(height)
}

fn output_added(output: &crate::scp::output::Output) -> CompositorMessage {
    CompositorMessage::OutputAdded {
        output_id: output.id,
        name: output.name.clone(),
        description: output.description.clone(),
        geometry: output.geometry,
        physical_size: output.physical_size,
        subpixel: output.subpixel,
        transform: output.transform,
        scale: output.scale,
        modes: output.modes.clone(),
        current_mode: output.current_mode,
    }
}

/// Composition reads surface content through this narrow view rather than
/// through the whole of [`ScpState`], so a rendering pass cannot reach protocol
/// state it has no business touching.
impl BufferSource for ScpState {
    fn committed_buffer(
        &self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Option<&crate::scp::surface::SurfaceBuffer> {
        self.surface_manager
            .get_surface(session_id, surface_id)?
            .buffer
            .as_ref()
    }

    fn capture_policy(
        &self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> crate::scp::surface::CapturePolicy {
        self.surface_manager
            .get_surface(session_id, surface_id)
            .map_or(crate::scp::surface::CapturePolicy::Allowed, |surface| {
                surface.capture_policy
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::{
        capability::CapabilityToken,
        protocol::CompositorMessage,
        security::{AuditOutcome, SecurityCoordinator},
    };
    use std::sync::Mutex;

    fn shrink_sealed_memfd(bytes: usize) -> i32 {
        use std::os::fd::IntoRawFd;

        let file = crate::scp::memfd::create_file("scp-state-buffer-test").unwrap();
        file.set_len(bytes as u64).unwrap();
        let fd = file.into_raw_fd();
        crate::scp::memfd::add_seals(fd, crate::scp::memfd::F_SEAL_SHRINK).unwrap();
        fd
    }

    #[derive(Default)]
    struct TestSecurity {
        tokens: Mutex<HashMap<Vec<u8>, (AppId, Capability)>>,
    }

    impl SecurityCoordinator for TestSecurity {
        fn verify_app_identity(&self, pid: u32) -> Option<AppId> {
            Some(AppId(format!("app-{pid}")))
        }

        fn evaluate_capability(&self, app_id: &AppId, cap: &Capability) -> Decision {
            Decision::Granted {
                token: self.issue_token(app_id, cap),
                expires_at: None,
            }
        }

        fn issue_token(&self, app_id: &AppId, cap: &Capability) -> CapabilityToken {
            let data = format!("{}:{}", app_id.0, cap.wire_name()).into_bytes();
            self.tokens
                .lock()
                .expect("test token lock")
                .insert(data.clone(), (app_id.clone(), cap.clone()));
            CapabilityToken {
                data,
                expires_at: None,
                one_time: false,
            }
        }

        fn verify_token(&self, token: &CapabilityToken) -> Option<(AppId, Capability)> {
            self.tokens
                .lock()
                .expect("test token lock")
                .get(&token.data)
                .cloned()
        }

        fn audit_capability_use(&self, _app_id: &AppId, _cap: &Capability, _outcome: AuditOutcome) {
        }
    }

    fn connect(state: &mut ScpState, pid: u32) -> (SessionId, Vec<u8>) {
        let responses = state
            .handle_message(
                None,
                ClientMessage::Connect {
                    app_id: format!("app-{pid}"),
                    pid,
                },
            )
            .expect("connect succeeds");
        match &responses[0] {
            CompositorMessage::Connected {
                session_id,
                capability_tokens,
                ..
            } => (*session_id, capability_tokens["window-toplevel"].clone()),
            response => panic!("unexpected response: {response:?}"),
        }
    }

    #[test]
    fn managed_shm_buffers_form_a_reusable_release_cycle() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (session_id, _) = connect(&mut state, 901);
        let sink = SessionSink::new().unwrap();
        state.register_session_sink(session_id, Arc::clone(&sink));

        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateShmPool {
                    pool_id: 3,
                    fd: shrink_sealed_memfd(128),
                    size: 128,
                },
            )
            .unwrap();
        for (buffer_id, offset) in [(7, 0), (8, 64)] {
            state
                .handle_message(
                    Some(session_id),
                    ClientMessage::CreateBuffer {
                        buffer_id,
                        pool_id: 3,
                        offset,
                        width: 4,
                        height: 4,
                        stride: 16,
                        format: crate::scp::protocol::ShmFormat::Argb8888,
                    },
                )
                .unwrap();
        }
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 5 },
            )
            .unwrap();

        for buffer_id in [7, 8] {
            state
                .handle_message(
                    Some(session_id),
                    ClientMessage::AttachShmBuffer {
                        surface_id: 5,
                        buffer_id,
                    },
                )
                .unwrap();
            state
                .handle_message(
                    Some(session_id),
                    ClientMessage::Commit {
                        surface_id: 5,
                        frame_callback: None,
                    },
                )
                .unwrap();
        }

        assert!(
            state
                .handle_message(
                    Some(session_id),
                    ClientMessage::DestroyBuffer { buffer_id: 7 },
                )
                .is_err(),
            "the compositor still owns the first buffer before presentation"
        );
        assert_eq!(state.release_presented_buffers(), 1);
        assert!(sink.drain().into_iter().any(|event| matches!(
            event.message,
            CompositorMessage::BufferRelease { buffer_id: 7 }
        )));
        state
            .handle_message(
                Some(session_id),
                ClientMessage::DestroyBuffer { buffer_id: 7 },
            )
            .expect("a released buffer is reusable or destroyable");

        state
            .handle_message(
                Some(session_id),
                ClientMessage::DetachBuffer { surface_id: 5 },
            )
            .unwrap();
        state
            .handle_message(
                Some(session_id),
                ClientMessage::Commit {
                    surface_id: 5,
                    frame_callback: None,
                },
            )
            .unwrap();
        assert_eq!(state.release_presented_buffers(), 1);
        assert!(sink.drain().into_iter().any(|event| matches!(
            event.message,
            CompositorMessage::BufferRelease { buffer_id: 8 }
        )));
        state
            .handle_message(
                Some(session_id),
                ClientMessage::DestroyBuffer { buffer_id: 8 },
            )
            .unwrap();
        state
            .handle_message(
                Some(session_id),
                ClientMessage::DestroyShmPool { pool_id: 3 },
            )
            .unwrap();
    }

    /// Grant `session_id` the session-lock capability and return its token.
    fn lock_capability(state: &mut ScpState, session_id: SessionId) -> Vec<u8> {
        let responses = state
            .handle_message(
                Some(session_id),
                ClientMessage::RequestCapability {
                    capability: "session-lock".to_string(),
                    justification: "test locker".to_string(),
                },
            )
            .expect("capability request succeeds");
        match &responses[0] {
            CompositorMessage::CapabilityDecision {
                granted: true,
                token: Some(token),
                ..
            } => token.clone(),
            response => panic!("unexpected response: {response:?}"),
        }
    }

    /// Engage the lock and return the locker's session and lock id.
    fn engage_lock(state: &mut ScpState, pid: u32) -> (SessionId, u32) {
        let (session_id, _) = connect(state, pid);
        let token = lock_capability(state, session_id);
        let responses = state
            .handle_message(
                Some(session_id),
                ClientMessage::LockSession {
                    capability_token: token,
                },
            )
            .expect("lock engages");
        match responses[0] {
            CompositorMessage::SessionLockEngaged { lock_id } => (session_id, lock_id),
            ref response => panic!("unexpected response: {response:?}"),
        }
    }

    /// Engage the lock and cover the implicit primary output, the state a real
    /// greeter reaches before it prompts for a password.
    fn confirmed_lock(state: &mut ScpState, pid: u32) -> (SessionId, u32) {
        let (session_id, lock_id) = engage_lock(state, pid);
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 1 },
            )
            .expect("lock surface created");

        let responses = state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateLockSurface {
                    surface_id: 1,
                    lock_id,
                    output_id: None,
                },
            )
            .expect("lock surface accepted");
        let (lock_surface_id, serial) = match responses[0] {
            CompositorMessage::ConfigureLockSurface {
                lock_surface_id,
                serial,
                ..
            } => (lock_surface_id, serial),
            ref response => panic!("unexpected response: {response:?}"),
        };

        let responses = state
            .handle_message(
                Some(session_id),
                ClientMessage::AckLockConfigure {
                    lock_surface_id,
                    serial,
                },
            )
            .expect("configure acknowledged");
        assert!(matches!(
            responses[0],
            CompositorMessage::SessionLocked { .. }
        ));
        (session_id, lock_id)
    }

    /// A focused application window, as the desktop looks before a lock.
    fn app_with_focused_window(state: &mut ScpState, pid: u32) -> (SessionId, SurfaceId) {
        let (session_id, token) = connect(state, pid);
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 1 },
            )
            .expect("surface created");
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateToplevel {
                    surface_id: 1,
                    capability_token: token,
                    title: "app".to_string(),
                },
            )
            .expect("toplevel created");
        assert_eq!(state.get_focused_surface(), Some((session_id, 1)));
        (session_id, 1)
    }

    #[test]
    fn engaging_the_lock_drops_desktop_focus_immediately() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        app_with_focused_window(&mut state, 301);

        engage_lock(&mut state, 302);

        assert!(state.is_locked());
        assert_eq!(
            state.get_focused_surface(),
            None,
            "focus must leave the desktop before the locker has drawn anything"
        );
    }

    #[test]
    fn a_locked_stack_holds_nothing_but_lock_surfaces() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        app_with_focused_window(&mut state, 303);
        let (locker, _) = confirmed_lock(&mut state, 304);

        let stack = state.build_stack();
        assert_eq!(stack.len(), 1);
        let entry = stack.entries()[0];
        assert!(matches!(entry.kind, StackKind::LockSurface(_)));
        assert_eq!(entry.session_id, locker);
    }

    #[test]
    fn an_authorized_desktop_is_composed_below_the_input_owning_lock() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (desktop, _) = app_with_focused_window(&mut state, 305);
        let (locker, lock_id) = confirmed_lock(&mut state, 306);
        let uid = unsafe { libc::geteuid() };
        state
            .handle_message(
                Some(locker),
                ClientMessage::AuthorizeSessionUser { lock_id, uid },
            )
            .expect("authenticated desktop admitted");

        let stack = state.build_stack();
        assert_eq!(stack.len(), 2);
        assert!(matches!(stack.entries()[0].kind, StackKind::LockSurface(_)));
        assert_eq!(stack.entries()[1].session_id, desktop);
        assert!(matches!(stack.entries()[1].kind, StackKind::Toplevel(_)));
        assert!(matches!(
            state.hit_test(10.0, 10.0).map(|entry| entry.kind),
            Some(StackKind::LockSurface(_))
        ));
    }

    #[test]
    fn pointer_input_cannot_reach_a_window_behind_the_lock() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        app_with_focused_window(&mut state, 305);

        let window = state
            .build_stack()
            .entries()
            .first()
            .copied()
            .expect("the window is on the stack");
        let (x, y) = (
            f64::from(window.rect.x) + 10.0,
            f64::from(window.rect.y) + 10.0,
        );
        assert!(
            state.hit_test(x, y).is_some(),
            "window is hittable unlocked"
        );

        confirmed_lock(&mut state, 306);

        let hit = state.hit_test(x, y).expect("the lock surface covers it");
        assert!(
            matches!(hit.kind, StackKind::LockSurface(_)),
            "input must land on the lock, never the window underneath"
        );
    }

    #[test]
    fn focus_cannot_be_moved_to_a_window_while_locked() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (app, surface) = app_with_focused_window(&mut state, 307);
        confirmed_lock(&mut state, 308);

        state.set_keyboard_focus(Some((app, surface)));

        assert_ne!(
            state.get_focused_surface(),
            Some((app, surface)),
            "a window must not regain focus — or foreground status — behind the lock"
        );
    }

    #[test]
    fn clipboard_writes_are_refused_while_locked() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (app, _) = app_with_focused_window(&mut state, 309);
        confirmed_lock(&mut state, 310);

        let error = state
            .handle_message(
                Some(app),
                ClientMessage::SetSelection {
                    mime_types: vec!["text/plain".to_string()],
                    serial: 1,
                },
            )
            .expect_err("clipboard write rejected");
        assert!(
            error.contains("refused while the session is locked"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn screen_capture_is_refused_while_locked() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (app, _) = connect(&mut state, 311);
        confirmed_lock(&mut state, 312);

        let responses = state
            .handle_message(
                Some(app),
                ClientMessage::RequestCapability {
                    capability: "screen-capture-output".to_string(),
                    justification: "record the screen".to_string(),
                },
            )
            .expect("request is answered, not fatal");
        match &responses[0] {
            CompositorMessage::CapabilityDecision {
                granted, reason, ..
            } => {
                assert!(!granted, "capture must not be granted behind the lock");
                assert!(
                    reason
                        .as_deref()
                        .is_some_and(|reason| reason.contains("locked")),
                    "unexpected reason: {reason:?}"
                );
            }
            response => panic!("unexpected response: {response:?}"),
        }
    }

    #[test]
    fn a_second_client_cannot_take_a_live_lock() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        confirmed_lock(&mut state, 313);

        let (attacker, _) = connect(&mut state, 314);
        let token = lock_capability(&mut state, attacker);
        let responses = state
            .handle_message(
                Some(attacker),
                ClientMessage::LockSession {
                    capability_token: token,
                },
            )
            .expect("the loser is told to back off, not killed");
        assert!(matches!(
            responses[0],
            CompositorMessage::SessionLockFinished { .. }
        ));
    }

    #[test]
    fn the_session_stays_locked_when_the_locker_dies() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        app_with_focused_window(&mut state, 315);
        let (locker, _) = confirmed_lock(&mut state, 316);

        state.disconnect(locker);

        assert!(
            state.is_locked(),
            "a crashed locker must not reveal the desktop"
        );
        assert_eq!(state.get_focused_surface(), None);
        assert!(
            state.build_stack().is_empty(),
            "nothing is drawn or hittable"
        );
    }

    #[test]
    fn unlocking_restores_the_focus_the_lock_took() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (app, surface) = app_with_focused_window(&mut state, 317);
        let (locker, lock_id) = confirmed_lock(&mut state, 318);

        state
            .handle_message(Some(locker), ClientMessage::UnlockSession { lock_id })
            .expect("unlock succeeds");

        assert!(!state.is_locked());
        assert_eq!(state.get_focused_surface(), Some((app, surface)));
    }

    #[test]
    fn greeter_authorizes_and_revokes_one_cross_uid_session() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (greeter, lock_id) = confirmed_lock(&mut state, 410);
        // Different from the test process while remaining a valid uid_t value.
        let session_uid = (unsafe { libc::geteuid() }) ^ 1;

        let responses = state
            .handle_message(
                Some(greeter),
                ClientMessage::AuthorizeSessionUser {
                    lock_id,
                    uid: session_uid,
                },
            )
            .expect("confirmed lock owner authorizes the user");
        assert!(matches!(
            responses.as_slice(),
            [CompositorMessage::SessionUserAuthorized { uid }] if *uid == session_uid
        ));
        assert_eq!(state.active_session_uid(), Some(session_uid));
        assert!(state.peer_uid_is_admitted(session_uid));

        let connected = state
            .handle_message_from_uid(
                None,
                ClientMessage::Connect {
                    app_id: "app-411".to_string(),
                    pid: 411,
                },
                session_uid,
            )
            .expect("authorized UID connects");
        let user_session = match connected[0] {
            CompositorMessage::Connected { session_id, .. } => session_id,
            ref response => panic!("unexpected response: {response:?}"),
        };

        let token = state.sessions[&greeter].granted_capabilities[&Capability::SessionLock]
            .token
            .data
            .clone();
        let responses = state
            .handle_message(
                Some(greeter),
                ClientMessage::RevokeSessionUser {
                    uid: session_uid,
                    capability_token: token,
                },
            )
            .expect("lock-capable greeter revokes the completed user");
        assert!(matches!(
            responses.as_slice(),
            [CompositorMessage::SessionUserRevoked { uid }] if *uid == session_uid
        ));
        assert_eq!(state.active_session_uid(), None);
        assert!(!state.peer_uid_is_admitted(session_uid));
        assert!(
            !state.sessions.contains_key(&user_session),
            "revocation disconnects any surviving process from the completed session"
        );
    }

    #[test]
    fn greeter_crash_during_handoff_revokes_the_prepared_user() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (greeter, lock_id) = confirmed_lock(&mut state, 420);
        let uid = (unsafe { libc::geteuid() }) ^ 1;
        state
            .handle_message(
                Some(greeter),
                ClientMessage::AuthorizeSessionUser { lock_id, uid },
            )
            .expect("user authorized");
        let connected = state
            .handle_message_from_uid(
                None,
                ClientMessage::Connect {
                    app_id: "app-421".to_string(),
                    pid: 421,
                },
                uid,
            )
            .expect("prepared user connects");
        let user_session = match connected[0] {
            CompositorMessage::Connected { session_id, .. } => session_id,
            ref response => panic!("unexpected response: {response:?}"),
        };

        state.disconnect(greeter);

        assert!(state.is_locked(), "greeter crash remains fail-closed");
        assert_eq!(state.active_session_uid(), None);
        assert!(!state.sessions.contains_key(&user_session));
    }

    #[test]
    fn only_the_owner_can_unlock() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (_, lock_id) = confirmed_lock(&mut state, 319);
        let (attacker, _) = connect(&mut state, 320);

        let error = state
            .handle_message(Some(attacker), ClientMessage::UnlockSession { lock_id })
            .expect_err("unlock rejected");
        assert!(error.contains("does not own"), "unexpected error: {error}");
        assert!(state.is_locked());
    }

    #[test]
    fn popups_cannot_be_parented_to_a_lock_surface() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (locker, _) = confirmed_lock(&mut state, 321);

        state
            .handle_message(Some(locker), ClientMessage::CreateSurface { surface_id: 2 })
            .expect("surface created");

        let error = state
            .handle_message(
                Some(locker),
                ClientMessage::CreatePopup {
                    surface_id: 2,
                    parent_id: 1,
                    positioner: crate::scp::protocol::PopupPositioner {
                        anchor_rect: Rect {
                            x: 0,
                            y: 0,
                            width: 10,
                            height: 10,
                        },
                        anchor_edge: crate::scp::protocol::Edge::Bottom,
                        gravity: crate::scp::protocol::Gravity::BottomRight,
                        constraint: crate::scp::protocol::ConstraintAdjustment {
                            flip_x: false,
                            flip_y: false,
                            slide_x: false,
                            slide_y: false,
                            resize_x: false,
                            resize_y: false,
                        },
                        offset: (0, 0),
                        size: (100, 100),
                    },
                    grab: true,
                },
            )
            .expect_err("popup on a lock surface rejected");
        assert!(error.contains("lock surface"), "unexpected error: {error}");
    }

    #[test]
    fn destroying_a_lock_surface_does_not_unlock() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (locker, lock_id) = confirmed_lock(&mut state, 322);

        state
            .handle_message(
                Some(locker),
                ClientMessage::DestroySurface { surface_id: 1 },
            )
            .expect("surface destroyed");

        assert!(state.is_locked(), "the lock outlives its surfaces");
        let error = state
            .handle_message(Some(locker), ClientMessage::UnlockSession { lock_id })
            .expect_err("an uncovered lock cannot be released");
        assert!(
            error.contains("has not engaged"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_forged_capability_token() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (session_id, _) = connect(&mut state, 101);
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 1 },
            )
            .expect("surface creation succeeds");

        let error = state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateToplevel {
                    surface_id: 1,
                    capability_token: b"forged".to_vec(),
                    title: "forged".to_string(),
                },
            )
            .expect_err("forged token must be rejected");
        assert!(error.contains("does not match this session"));
    }

    #[test]
    fn surface_ids_are_isolated_per_session() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let (first_session, first_token) = connect(&mut state, 201);
        let (second_session, second_token) = connect(&mut state, 202);

        for session_id in [first_session, second_session] {
            state
                .handle_message(
                    Some(session_id),
                    ClientMessage::CreateSurface { surface_id: 1 },
                )
                .expect("same client-local surface ID is valid in separate sessions");
        }

        let first = state
            .handle_message(
                Some(first_session),
                ClientMessage::CreateToplevel {
                    surface_id: 1,
                    capability_token: first_token,
                    title: "first".to_string(),
                },
            )
            .expect("first toplevel succeeds");
        state
            .handle_message(
                Some(second_session),
                ClientMessage::CreateToplevel {
                    surface_id: 1,
                    capability_token: second_token,
                    title: "second".to_string(),
                },
            )
            .expect("second toplevel succeeds");

        let first_toplevel = match first[0] {
            CompositorMessage::ConfigureToplevel { toplevel_id, .. } => toplevel_id,
            ref response => panic!("unexpected response: {response:?}"),
        };
        let error = state
            .handle_message(
                Some(second_session),
                ClientMessage::SetToplevelTitle {
                    toplevel_id: first_toplevel,
                    title: "hijacked".to_string(),
                },
            )
            .expect_err("cross-session toplevel access must fail");
        assert!(error.contains("does not belong"));
    }

    #[test]
    fn version_aware_clients_negotiate_v2_without_breaking_v1() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let responses = state
            .handle_message(
                None,
                ClientMessage::ConnectVersioned {
                    app_id: "app-501".to_string(),
                    pid: 501,
                    min_version: 1,
                    max_version: 99,
                },
            )
            .expect("version range overlaps");
        assert!(matches!(
            &responses[0],
            CompositorMessage::ProtocolVersion { version: 2, features }
                if features.contains(&"capture-fd".to_string())
        ));
        assert!(matches!(responses[1], CompositorMessage::Connected { .. }));

        let error = state
            .handle_message(
                None,
                ClientMessage::ConnectVersioned {
                    app_id: "app-502".to_string(),
                    pid: 502,
                    min_version: CURRENT_PROTOCOL_VERSION + 1,
                    max_version: CURRENT_PROTOCOL_VERSION + 2,
                },
            )
            .expect_err("future-only client must be refused");
        assert!(error.contains("No compatible SCP version"));
    }

    #[test]
    fn capture_exports_one_immutable_frame_and_consumes_the_grant() {
        use std::io::Read;

        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let output = state
            .add_output("test".to_string(), "test".to_string(), 4, 3, 60_000)
            .unwrap();
        let (session_id, _) = app_with_focused_window(&mut state, 601);
        let sink = SessionSink::new().unwrap();
        state.register_session_sink(session_id, Arc::clone(&sink));

        let grant = state
            .handle_message(
                Some(session_id),
                ClientMessage::RequestCapability {
                    capability: "screen-capture-output".to_string(),
                    justification: "test capture".to_string(),
                },
            )
            .unwrap();
        let token = match &grant[0] {
            CompositorMessage::CapabilityDecision {
                granted: true,
                token: Some(token),
                ..
            } => token.clone(),
            other => panic!("unexpected grant: {other:?}"),
        };
        state
            .handle_message(
                Some(session_id),
                ClientMessage::RequestCapture {
                    target: CaptureTarget::Output(output),
                    cursor_mode: crate::scp::protocol::CursorMode::Exclude,
                    capability_token: token.clone(),
                },
            )
            .unwrap();

        let event = sink
            .drain()
            .into_iter()
            .find(|event| matches!(event.message, CompositorMessage::CaptureGranted { .. }))
            .expect("capture event queued");
        assert!(matches!(
            event.message,
            CompositorMessage::CaptureGranted {
                width: 4,
                height: 3,
                stride: 16,
                ..
            }
        ));
        let mut bytes = Vec::new();
        std::fs::File::from(event.fd.expect("capture descriptor"))
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes.len(), 4 * 3 * 4);

        let replay = state
            .handle_message(
                Some(session_id),
                ClientMessage::RequestCapture {
                    target: CaptureTarget::Output(output),
                    cursor_mode: crate::scp::protocol::CursorMode::Exclude,
                    capability_token: token,
                },
            )
            .expect_err("a capture grant is one-shot");
        assert!(replay.contains("not granted"));
    }

    #[test]
    fn registered_global_shortcut_consumes_key_press_and_release() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        state
            .add_output("test".to_string(), "test".to_string(), 800, 600, 60_000)
            .unwrap();
        let (session_id, _) = app_with_focused_window(&mut state, 701);
        let sink = SessionSink::new().unwrap();
        state.register_session_sink(session_id, Arc::clone(&sink));

        let grant = state
            .handle_message(
                Some(session_id),
                ClientMessage::RequestCapability {
                    capability: "global-shortcuts".to_string(),
                    justification: "test shortcut".to_string(),
                },
            )
            .unwrap();
        let token = match &grant[0] {
            CompositorMessage::CapabilityDecision {
                token: Some(token), ..
            } => token.clone(),
            other => panic!("unexpected grant: {other:?}"),
        };
        let binding = KeyBinding {
            keycode: 24,
            modifiers: crate::scp::keymap::modifiers::SUPER,
        };
        let granted = state
            .handle_message(
                Some(session_id),
                ClientMessage::RegisterShortcut {
                    binding,
                    justification: "Open the test app".to_string(),
                    capability_token: token,
                },
            )
            .unwrap();
        let shortcut_id = match granted[0] {
            CompositorMessage::ShortcutGranted { shortcut_id, .. } => shortcut_id,
            ref other => panic!("unexpected shortcut response: {other:?}"),
        };

        use crate::scp::protocol::KeyState;
        state.handle_key(133, KeyState::Pressed, 1);
        sink.drain();
        state.handle_key(24, KeyState::Pressed, 2);
        state.handle_key(24, KeyState::Released, 3);
        let messages: Vec<_> = sink
            .drain()
            .into_iter()
            .map(|event| event.message)
            .collect();
        assert!(messages.iter().any(|message| matches!(
            message,
            CompositorMessage::ShortcutActivated {
                shortcut_id: id,
                timestamp_ms: 2
            } if *id == shortcut_id
        )));
        assert!(!messages.iter().any(|message| matches!(
            message,
            CompositorMessage::InputEvent {
                event: InputEvent::KeyboardKey { key: 24, .. },
                ..
            }
        )));

        assert_eq!(
            state.revoke_capability_for_app(
                &AppId("app-701".to_string()),
                &Capability::GlobalShortcuts,
                "test policy change"
            ),
            1
        );
        sink.drain();
        state.handle_key(24, KeyState::Pressed, 4);
        let messages: Vec<_> = sink
            .drain()
            .into_iter()
            .map(|event| event.message)
            .collect();
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, CompositorMessage::ShortcutActivated { .. }))
        );
        assert!(messages.iter().any(|message| matches!(
            message,
            CompositorMessage::InputEvent {
                event: InputEvent::KeyboardKey { key: 24, .. },
                ..
            }
        )));
    }

    #[test]
    fn surface_output_membership_tracks_role_creation_and_hot_unplug() {
        let mut state = ScpState::with_security(Arc::new(TestSecurity::default()));
        let output = state
            .add_output("test".to_string(), "test".to_string(), 800, 600, 60_000)
            .unwrap();
        let (session_id, token) = connect(&mut state, 801);
        let sink = SessionSink::new().unwrap();
        state.register_session_sink(session_id, Arc::clone(&sink));
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateSurface { surface_id: 1 },
            )
            .unwrap();
        state
            .handle_message(
                Some(session_id),
                ClientMessage::CreateToplevel {
                    surface_id: 1,
                    capability_token: token,
                    title: "membership".to_string(),
                },
            )
            .unwrap();
        let entered = sink.drain().into_iter().any(|event| {
            matches!(
                event.message,
                CompositorMessage::SurfaceEnterOutput {
                    surface_id: 1,
                    output_id
                } if output_id == output
            )
        });
        assert!(entered);

        state.remove_output(output).unwrap();
        let messages: Vec<_> = sink
            .drain()
            .into_iter()
            .map(|event| event.message)
            .collect();
        assert!(messages.iter().any(|message| matches!(
            message,
            CompositorMessage::SurfaceLeaveOutput {
                surface_id: 1,
                output_id
            } if *output_id == output
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            CompositorMessage::OutputRemoved { output_id } if *output_id == output
        )));
    }
}
