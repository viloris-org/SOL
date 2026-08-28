//! Core SCP compositor state and message handlers.

use crate::scp::{
    buffer::BufferManager,
    capability::{
        Capability, CapabilityGrant, CapabilityToken, Decision, blocked_while_locked,
        requires_foreground, requires_recent_interaction,
    },
    data_device::DataDevice,
    event_queue::{EventRouter, OutboundEvent, SessionSink},
    input::InputState,
    keymap::{KeymapState, ModifierState, RepeatInfo},
    output::OutputManager,
    popup::{Popup, PopupManager, position_popup},
    protocol::{
        BufferId, ButtonState, ClientMessage, CompositorMessage, DismissReason, InputEvent,
        LayerSurfaceId, OutputId, PopupId, Rect, SessionId, SurfaceId, ToplevelId,
    },
    security::{AppId, AuditOutcome, SecurityCoordinator, StubSecurityCoordinator},
    session_lock::{LockGrant, SessionLockManager},
    stack::{StackEntry, StackKind, WindowStack, place_toplevel},
    surface::{Anchor, KeyboardInteractivity, Layer, Margin, SurfaceManager, SurfaceRole},
    unix_socket,
};
use std::{collections::HashMap, sync::Arc, time::Instant};

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

/// Escape, in the XKB keycode space this compositor speaks (evdev + 8).
///
/// Matches the `<ESC> = 9` mapping in [`crate::scp::keymap`].
const KEY_ESCAPE: u32 = 9;

/// Authenticated client session.
#[derive(Debug)]
pub struct ClientSession {
    pub session_id: SessionId,
    pub app_id: AppId,
    pub pid: u32,
    pub granted_capabilities: HashMap<Capability, CapabilityGrant>,
    pub connection_time: Instant,
    pub last_user_interaction: Option<Instant>,
    pub is_foreground: bool,
}

impl ClientSession {
    pub fn new(session_id: SessionId, app_id: AppId, pid: u32) -> Self {
        Self {
            session_id,
            app_id,
            pid,
            granted_capabilities: HashMap::new(),
            connection_time: Instant::now(),
            last_user_interaction: None,
            is_foreground: false,
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

    // Keyboard state
    keymap_state: KeymapState,
    repeat_info: RepeatInfo,
    modifier_state: ModifierState,

    // Data device (clipboard/DnD)
    data_device: DataDevice,

    /// Session lock. While engaged it takes over input and stacking entirely.
    session_lock: SessionLockManager,

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
            keymap_state: KeymapState::new(),
            repeat_info: RepeatInfo::default(),
            modifier_state: ModifierState::new(),
            data_device: DataDevice::new(),
            session_lock: SessionLockManager::new(),
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
        self.next_buffer_id += 1;
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

    /// Event router used to push compositor-initiated messages to clients.
    pub fn events(&self) -> &EventRouter {
        &self.events
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
    /// While the session is locked the stack contains *only* lock surfaces. That
    /// single rule is what enforces the lock across every consumer at once:
    /// hit-testing cannot reach an application window, so pointer and touch
    /// input have nowhere else to go and nothing else can be composited.
    pub fn build_stack(&self) -> WindowStack {
        let mut stack = WindowStack::new();

        if self.session_lock.is_locked() {
            // Only lock surfaces, and nothing else. Popups are not admitted at
            // all: they cannot be parented to a lock surface, and a popup left
            // over from before the lock must not stay reachable.
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
            return stack;
        }

        self.push_layer_entries(&mut stack, Layer::Overlay);
        self.push_layer_entries(&mut stack, Layer::Top);

        self.push_popup_entries(&mut stack);

        for &toplevel_id in &self.toplevel_stack {
            let Some(toplevel) = self.surface_manager.get_toplevel(toplevel_id) else {
                continue;
            };
            // A minimized window is off-screen: it must not swallow input.
            if toplevel.states.minimized {
                continue;
            }
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

        self.push_layer_entries(&mut stack, Layer::Bottom);
        self.push_layer_entries(&mut stack, Layer::Background);

        stack
    }

    /// Push popups above the windows they belong to, innermost first.
    fn push_popup_entries(&self, stack: &mut WindowStack) {
        // The grab chain is ordered outermost-first, so reverse it to put
        // submenus above the menus that opened them. Grabless popups (tooltips)
        // follow in arbitrary order; nothing depends on their relative Z.
        let mut popup_ids: Vec<PopupId> = self.popups.grab_chain().iter().rev().copied().collect();
        for popup in self.popups.iter() {
            if !popup_ids.contains(&popup.id) {
                popup_ids.push(popup.id);
            }
        }

        for popup_id in popup_ids {
            let Some(popup) = self.popups.get(popup_id) else {
                continue;
            };
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

    fn push_layer_entries(&self, stack: &mut WindowStack, layer: Layer) {
        for layer_surface in self.surface_manager.iter_layer_surfaces() {
            if layer_surface.layer != layer {
                continue;
            }
            let Some(rect) =
                self.absolute_surface_rect(layer_surface.session_id, layer_surface.surface_id)
            else {
                continue;
            };
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
        match (session_id, message) {
            (None, ClientMessage::Connect { app_id, pid }) => self.handle_connect(app_id, pid),
            (Some(_), ClientMessage::Connect { .. }) => Err("Already connected".to_string()),
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
        received_fd: Option<i32>,
    ) -> Result<Vec<CompositorMessage>, String> {
        match &mut message {
            ClientMessage::AttachBuffer { buffer_fd, .. } => {
                *buffer_fd =
                    received_fd.ok_or("AttachBuffer requires exactly one file descriptor")?;
            }
            _ if received_fd.is_some() => {
                if let Some(fd) = received_fd {
                    unix_socket::close_fd(fd);
                }
                return Err("File descriptor is only valid with AttachBuffer".to_string());
            }
            _ => {}
        }

        let attached_fd = match &message {
            ClientMessage::AttachBuffer { buffer_fd, .. } => Some(*buffer_fd),
            _ => None,
        };
        let result = self.handle_message(session_id, message);
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

        let session_id = self.next_session_id;
        self.next_session_id = self
            .next_session_id
            .checked_add(1)
            .ok_or("Session ID space exhausted")?;
        let mut session = ClientSession::new(session_id, verified_app_id.clone(), pid);

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
        let app_id = self
            .sessions
            .get(&session_id)
            .ok_or("Invalid session")?
            .app_id
            .clone();

        match message {
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
                surface.attach_buffer(crate::scp::surface::SurfaceBuffer {
                    buffer_id,
                    fd: buffer_fd,
                    width,
                    height,
                    stride,
                    format,
                });
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

                surface.commit();
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
                let serial = self.next_serial();
                if let Some(toplevel) = self.surface_manager.get_toplevel_mut(toplevel_id) {
                    let states = crate::scp::surface::ToplevelStates {
                        fullscreen: true,
                        ..Default::default()
                    };
                    toplevel.configure(serial, 1920, 1080, states);
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
                    width: 1920,
                    height: 1080,
                    decoration_height: 0,
                    states: crate::scp::protocol::ToplevelStates {
                        fullscreen: true,
                        ..Default::default()
                    },
                }])
            }
            ClientMessage::Connect { .. } => {
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
            } => self
                .buffer_manager
                .create_buffer(
                    session_id, buffer_id, pool_id, offset, width, height, stride, format,
                )
                .map(|_| vec![]),
            ClientMessage::DestroyBuffer { buffer_id } => self
                .buffer_manager
                .destroy_buffer(session_id, buffer_id)
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
                // Verify serial is valid (from a recent enter/button event)
                if serial > self.next_serial {
                    return Err("Invalid cursor serial".to_string());
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
                if serial > self.next_serial {
                    return Err("Invalid drag serial".to_string());
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
        }
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
                self.sessions
                    .get_mut(&session_id)
                    .ok_or("Invalid session")?
                    .grant_capability(capability.clone(), token, expires_at);
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
    /// Geometry checking is shared with the SHM path so the two cannot drift into
    /// disagreeing about what a legal buffer is.
    fn validate_buffer(
        &self,
        fd: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: crate::scp::protocol::BufferFormat,
    ) -> Result<(), String> {
        if fd < 0 {
            return Err("Invalid buffer file descriptor".to_string());
        }
        crate::scp::buffer::validate_geometry(width, height, stride, format.bytes_per_pixel())
            .map(|_| ())
    }

    // ===== Session lifetime =====

    pub fn disconnect(&mut self, session_id: SessionId) {
        self.events.unregister(session_id);
        self.sessions.remove(&session_id);

        // A locker that dies abandons its lock but does not release it: a crash
        // must never be a way back to the desktop.
        if self.session_lock.abandon(session_id) {
            self.focused_surface = None;
            self.input_state.set_keyboard_focus(None);
            self.input_state.set_pointer_focus(None);
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
    }

    /// Remove a surface and finish the teardown its role implies.
    fn destroy_surface_and_role(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Result<(), String> {
        // Child popups must go first: once the parent surface is gone their
        // geometry can no longer be resolved.
        let orphaned = self
            .popups
            .dismiss_children_of_surface(session_id, surface_id);
        self.finish_popup_dismissal(&orphaned, DismissReason::ParentClosed);

        let role = self
            .surface_manager
            .destroy_surface(session_id, surface_id)?;

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
            self.authorize_serial(serial);
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
            self.authorize_serial(serial);
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
    fn authorize_serial(&mut self, serial: u32) {
        self.data_device.record_serial(serial);
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
        self.authorize_serial(serial);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::{
        capability::CapabilityToken,
        protocol::CompositorMessage,
        security::{AuditOutcome, SecurityCoordinator},
    };
    use std::sync::Mutex;

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
}
