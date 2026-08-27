//! Core SCP compositor state and message handlers.

use crate::scp::{
    buffer::BufferManager,
    capability::{Capability, CapabilityGrant, CapabilityToken, Decision},
    data_device::DataDevice,
    input::InputState,
    keymap::{KeymapState, ModifierState, RepeatInfo},
    output::OutputManager,
    popup::{Popup, position_popup},
    protocol::{
        BufferId, ClientMessage, CompositorMessage, LayerSurfaceId, PopupId, Rect, SessionId,
        SurfaceId,
    },
    security::{AppId, AuditOutcome, SecurityCoordinator, StubSecurityCoordinator},
    surface::{Anchor, KeyboardInteractivity, Layer, Margin, SurfaceManager},
    unix_socket,
};
use std::{collections::HashMap, os::unix::io::IntoRawFd, sync::Arc, time::Instant};

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
    popups: HashMap<PopupId, Popup>,
    next_popup_id: PopupId,
    next_buffer_id: BufferId,
    focused_surface: Option<(SessionId, SurfaceId)>,
    cursor_surface: Option<(SessionId, SurfaceId)>,
    next_serial: u32,

    // Keyboard state
    keymap_state: KeymapState,
    repeat_info: RepeatInfo,
    modifier_state: ModifierState,

    // Data device (clipboard/DnD)
    data_device: DataDevice,
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
            popups: HashMap::new(),
            next_popup_id: 1,
            next_buffer_id: 1,
            focused_surface: None,
            cursor_surface: None,
            next_serial: 1,
            keymap_state: KeymapState::new(),
            repeat_info: RepeatInfo::default(),
            modifier_state: ModifierState::new(),
            data_device: DataDevice::new(),
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
                self.surface_manager
                    .destroy_surface(session_id, surface_id)?;
                if self.focused_surface == Some((session_id, surface_id)) {
                    self.focused_surface = None;
                }
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
                self.validate_buffer(buffer_fd, width, height, stride)?;
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
                let serial = self.next_serial();
                if let Some(toplevel) = self.surface_manager.get_toplevel_mut(toplevel_id) {
                    let states = crate::scp::surface::ToplevelStates {
                        activated: true,
                        ..Default::default()
                    };
                    toplevel.configure(serial, 800, 600, states);
                }

                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.is_foreground = true;
                    session.record_use(&Capability::WindowToplevel);
                }
                self.focused_surface = Some((session_id, surface_id));
                self.security.audit_capability_use(
                    &app_id,
                    &Capability::WindowToplevel,
                    AuditOutcome::Used,
                );

                Ok(vec![CompositorMessage::ConfigureToplevel {
                    toplevel_id,
                    serial,
                    width: 800,
                    height: 600,
                    decoration_height: 32,
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

            // Buffer management
            ClientMessage::CreateShmPool { pool_id, fd, size } => self
                .buffer_manager
                .create_pool(pool_id, fd, size)
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
                .create_buffer(buffer_id, pool_id, offset, width, height, stride, format)
                .map(|_| vec![]),
            ClientMessage::DestroyBuffer { buffer_id } => self
                .buffer_manager
                .destroy_buffer(buffer_id)
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

                // Get parent surface geometry for positioning
                let parent_surface = self
                    .surface_manager
                    .get_surface(session_id, parent_id)
                    .ok_or("Parent surface not found")?;

                let parent_rect = if let Some(buffer) = &parent_surface.buffer {
                    Rect {
                        x: 0,
                        y: 0,
                        width: buffer.width,
                        height: buffer.height,
                    }
                } else {
                    Rect {
                        x: 0,
                        y: 0,
                        width: 800,
                        height: 600,
                    }
                };

                // Get output bounds for constraint resolution
                let output_rect = self
                    .output_manager
                    .primary_output()
                    .map(|output| output.geometry.clone())
                    .unwrap_or(Rect {
                        x: 0,
                        y: 0,
                        width: 1920,
                        height: 1080,
                    });

                // Calculate popup position
                let geometry = position_popup(&positioner, &parent_rect, &output_rect);

                // Create popup
                let popup_id = self.next_popup_id;
                self.next_popup_id = self
                    .next_popup_id
                    .checked_add(1)
                    .ok_or("Popup ID space exhausted")?;

                let mut popup = Popup::new(surface_id, parent_id, grab);
                popup.geometry = geometry.clone();
                self.popups.insert(popup_id, popup);

                tracing::debug!(
                    ?popup_id,
                    ?surface_id,
                    ?parent_id,
                    x = geometry.x,
                    y = geometry.y,
                    "Popup created"
                );

                Ok(vec![CompositorMessage::ConfigurePopup {
                    popup_id,
                    x: geometry.x,
                    y: geometry.y,
                    width: geometry.width,
                    height: geometry.height,
                }])
            }

            // Toplevel state management
            ClientMessage::SetToplevelState { toplevel_id, state } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;

                use crate::scp::protocol::ToplevelStateRequest;

                let serial = self.next_serial();

                let toplevel = self
                    .surface_manager
                    .get_toplevel_mut(toplevel_id)
                    .ok_or("Toplevel not found")?;

                let (width, height, states) = match state {
                    ToplevelStateRequest::Maximize => {
                        toplevel.set_maximized(true);
                        // Use primary output dimensions
                        let output = self.output_manager.primary_output();
                        let (w, h) = if let Some(output) = output {
                            (output.geometry.width, output.geometry.height)
                        } else {
                            (1920, 1080)
                        };
                        (
                            w,
                            h,
                            crate::scp::surface::ToplevelStates {
                                activated: toplevel.states.activated,
                                maximized: true,
                                fullscreen: false,
                                minimized: false,
                                resizing: false,
                            },
                        )
                    }
                    ToplevelStateRequest::Minimize => {
                        toplevel.set_minimized(true);
                        // Keep current dimensions, just mark as minimized
                        (
                            toplevel.geometry.width,
                            toplevel.geometry.height,
                            crate::scp::surface::ToplevelStates {
                                activated: false,
                                maximized: toplevel.states.maximized,
                                fullscreen: toplevel.states.fullscreen,
                                minimized: true,
                                resizing: false,
                            },
                        )
                    }
                    ToplevelStateRequest::Fullscreen { output_id } => {
                        toplevel.set_fullscreen(true);
                        let output = if let Some(id) = output_id {
                            self.output_manager.get_output(id)
                        } else {
                            self.output_manager.primary_output()
                        };
                        let (w, h) = if let Some(output) = output {
                            (output.geometry.width, output.geometry.height)
                        } else {
                            (1920, 1080)
                        };
                        (
                            w,
                            h,
                            crate::scp::surface::ToplevelStates {
                                activated: toplevel.states.activated,
                                maximized: false,
                                fullscreen: true,
                                minimized: false,
                                resizing: false,
                            },
                        )
                    }
                    ToplevelStateRequest::UnsetMaximize => {
                        toplevel.set_maximized(false);
                        (
                            800,
                            600,
                            crate::scp::surface::ToplevelStates {
                                activated: toplevel.states.activated,
                                maximized: false,
                                fullscreen: toplevel.states.fullscreen,
                                minimized: toplevel.states.minimized,
                                resizing: false,
                            },
                        )
                    }
                    ToplevelStateRequest::UnsetFullscreen => {
                        toplevel.set_fullscreen(false);
                        (
                            toplevel.geometry.width,
                            toplevel.geometry.height,
                            crate::scp::surface::ToplevelStates {
                                activated: toplevel.states.activated,
                                maximized: toplevel.states.maximized,
                                fullscreen: false,
                                minimized: toplevel.states.minimized,
                                resizing: false,
                            },
                        )
                    }
                };

                toplevel.configure(serial, width, height, states.clone());

                Ok(vec![CompositorMessage::ConfigureToplevel {
                    toplevel_id,
                    serial,
                    width,
                    height,
                    decoration_height: if states.fullscreen { 0 } else { 32 },
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

            // ===== Data Transfer (Clipboard/DnD) =====
            ClientMessage::SetSelection { mime_types, serial } => {
                // Verify client has ClipboardWrite capability
                self.verify_capability(session_id, &Capability::ClipboardWrite)?;

                // Validate MIME types
                for mime in &mime_types {
                    if !crate::scp::data_device::is_valid_mime_type(mime) {
                        return Err(format!("Invalid MIME type: {}", mime));
                    }
                }

                // Record serial and set selection
                self.data_device.record_serial(serial);
                self.data_device
                    .set_selection(session_id, mime_types.clone());

                // Notify other clients
                Ok(vec![CompositorMessage::SelectionOffer { mime_types }])
            }

            ClientMessage::SendSelectionData { mime_type: _, fd } => {
                // Verify this session owns the selection
                if !self
                    .data_device
                    .get_selection_full()
                    .is_some_and(|s| s.owner == session_id)
                {
                    return Err("Not selection owner".to_string());
                }

                // In real implementation, read from fd and send to requesting client
                // For now, just acknowledge
                unix_socket::close_fd(fd);
                Ok(vec![])
            }

            ClientMessage::StartDrag {
                surface_id,
                origin_surface,
                icon_surface,
                mime_types,
                serial,
            } => {
                self.verify_capability(session_id, &Capability::DragAndDrop)?;
                self.verify_surface_ownership(session_id, surface_id)?;
                self.verify_surface_ownership(session_id, origin_surface)?;
                if let Some(icon) = icon_surface {
                    self.verify_surface_ownership(session_id, icon)?;
                }

                // Validate MIME types
                for mime in &mime_types {
                    if !crate::scp::data_device::is_valid_mime_type(mime) {
                        return Err(format!("Invalid MIME type: {}", mime));
                    }
                }

                self.data_device.record_serial(serial);
                self.data_device
                    .start_drag_validated(
                        session_id,
                        origin_surface,
                        icon_surface,
                        mime_types,
                        serial,
                    )
                    .map_err(|e| e.to_string())?;

                Ok(vec![])
            }

            ClientMessage::AcceptDrag { serial, mime_type } => {
                if serial > self.next_serial {
                    return Err("Invalid drag serial".to_string());
                }
                self.data_device
                    .accept_drag(mime_type)
                    .map_err(|e| e.to_string())?;
                Ok(vec![])
            }

            ClientMessage::FinishDrag => {
                self.data_device.finish_drag().map_err(|e| e.to_string())?;
                Ok(vec![CompositorMessage::DragFinished])
            }

            ClientMessage::CancelDrag => {
                self.data_device.cancel_drag().map_err(|e| e.to_string())?;
                Ok(vec![CompositorMessage::DragCancelled])
            }

            ClientMessage::SendDragData { mime_type: _, fd } => {
                // Verify this session is the drag source
                if !self
                    .data_device
                    .active_drag()
                    .is_some_and(|d| d.source == session_id)
                {
                    return Err("Not drag source".to_string());
                }

                // In real implementation, read from fd and send to drop target
                unix_socket::close_fd(fd);
                Ok(vec![])
            }
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

    fn verify_capability_token(
        &self,
        session_id: SessionId,
        capability: &Capability,
        token_data: &[u8],
    ) -> Result<(), String> {
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
        self.sessions
            .get(&session_id)
            .ok_or("Invalid session")?
            .has_capability(capability)
            .then_some(())
            .ok_or_else(|| format!("Missing required capability: {}", capability.wire_name()))
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

    fn validate_buffer(&self, fd: i32, width: i32, height: i32, stride: i32) -> Result<(), String> {
        if fd < 0 {
            return Err("Invalid buffer file descriptor".to_string());
        }
        if width <= 0 || height <= 0 || stride <= 0 {
            return Err("Buffer dimensions and stride must be positive".to_string());
        }
        let minimum_stride = width
            .checked_mul(4)
            .ok_or("Buffer width overflows stride calculation")?;
        if stride < minimum_stride {
            return Err("Buffer stride is too small for a 32-bit format".to_string());
        }
        Ok(())
    }

    pub fn disconnect(&mut self, session_id: SessionId) {
        self.sessions.remove(&session_id);

        // Remove popups owned by surfaces from this session
        let mut popups_to_remove = Vec::new();
        for (popup_id, popup) in &self.popups {
            if let Some(_surface) = self
                .surface_manager
                .get_surface(session_id, popup.surface_id)
            {
                // This popup belongs to the disconnecting session
                popups_to_remove.push(*popup_id);
            }
        }
        for popup_id in popups_to_remove {
            self.popups.remove(&popup_id);
        }

        self.surface_manager.destroy_session(session_id);

        if self
            .focused_surface
            .is_some_and(|(owner, _)| owner == session_id)
        {
            self.focused_surface = None;
        }
        if self
            .cursor_surface
            .is_some_and(|(owner, _)| owner == session_id)
        {
            self.cursor_surface = None;
        }
    }

    pub fn iter_toplevels(&self) -> impl Iterator<Item = &crate::scp::surface::Toplevel> {
        self.surface_manager.iter_toplevels()
    }

    pub fn iter_layer_surfaces(&self) -> impl Iterator<Item = &crate::scp::surface::LayerSurface> {
        self.surface_manager.iter_layer_surfaces()
    }

    pub fn iter_layer_surfaces_sorted(&self) -> Vec<&crate::scp::surface::LayerSurface> {
        self.surface_manager.iter_layer_surfaces_sorted()
    }

    /// Send frame callbacks to all surfaces that requested them.
    /// Called after rendering a frame.
    pub fn send_frame_callbacks(
        &mut self,
        timestamp_ms: u64,
    ) -> Vec<(SessionId, CompositorMessage)> {
        let mut messages = Vec::new();

        for (session_id, surface_id, callback_id) in self.surface_manager.take_frame_callbacks() {
            messages.push((
                session_id,
                CompositorMessage::FrameCallback {
                    surface_id,
                    callback_id,
                    timestamp_ms,
                },
            ));
        }

        messages
    }

    pub const fn get_focused_surface(&self) -> Option<(SessionId, SurfaceId)> {
        self.focused_surface
    }

    pub const fn get_cursor_surface(&self) -> Option<(SessionId, SurfaceId)> {
        self.cursor_surface
    }

    pub fn get_popup(&self, popup_id: PopupId) -> Option<&Popup> {
        self.popups.get(&popup_id)
    }

    pub fn dismiss_popup(&mut self, popup_id: PopupId) -> Option<Popup> {
        self.popups.remove(&popup_id)
    }

    // ===== Keyboard Input =====

    /// Send keymap to a newly focused surface
    pub fn send_keymap(&self, _session_id: SessionId) -> Result<CompositorMessage, std::io::Error> {
        let fd = self.keymap_state.create_memfd()?;
        Ok(CompositorMessage::KeymapFormat {
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
        })
    }

    /// Send repeat info to client
    pub fn send_repeat_info(&self) -> CompositorMessage {
        CompositorMessage::RepeatInfo {
            rate: self.repeat_info.rate,
            delay: self.repeat_info.delay,
        }
    }

    /// Process a key press event
    pub fn handle_key_press(
        &mut self,
        keycode: u32,
        time_ms: u32,
    ) -> Vec<(SessionId, CompositorMessage)> {
        let mut messages = Vec::new();

        // Update modifier state
        self.modifier_state.key_pressed(keycode);

        // Send key event to focused surface
        if let Some((session_id, surface_id)) = self.focused_surface {
            let serial = self.next_serial();

            // Send key event
            messages.push((
                session_id,
                CompositorMessage::InputEvent {
                    surface_id,
                    event: crate::scp::protocol::InputEvent::KeyboardKey {
                        serial,
                        key: keycode,
                        state: crate::scp::protocol::KeyState::Pressed,
                        time_ms,
                    },
                },
            ));

            // Send modifier update
            messages.push((
                session_id,
                CompositorMessage::Modifiers {
                    surface_id,
                    serial,
                    mods_depressed: self.modifier_state.mods_depressed,
                    mods_latched: self.modifier_state.mods_latched,
                    mods_locked: self.modifier_state.mods_locked,
                    group: self.modifier_state.group,
                },
            ));
        }

        messages
    }

    /// Process a key release event
    pub fn handle_key_release(
        &mut self,
        keycode: u32,
        time_ms: u32,
    ) -> Vec<(SessionId, CompositorMessage)> {
        let mut messages = Vec::new();

        // Update modifier state
        self.modifier_state.key_released(keycode);

        // Send key event to focused surface
        if let Some((session_id, surface_id)) = self.focused_surface {
            let serial = self.next_serial();

            // Send key event
            messages.push((
                session_id,
                CompositorMessage::InputEvent {
                    surface_id,
                    event: crate::scp::protocol::InputEvent::KeyboardKey {
                        serial,
                        key: keycode,
                        state: crate::scp::protocol::KeyState::Released,
                        time_ms,
                    },
                },
            ));

            // Send modifier update
            messages.push((
                session_id,
                CompositorMessage::Modifiers {
                    surface_id,
                    serial,
                    mods_depressed: self.modifier_state.mods_depressed,
                    mods_latched: self.modifier_state.mods_latched,
                    mods_locked: self.modifier_state.mods_locked,
                    group: self.modifier_state.group,
                },
            ));
        }

        messages
    }

    /// Send keyboard enter event when surface gains focus
    pub fn send_keyboard_enter(
        &mut self,
        _session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Vec<CompositorMessage> {
        let serial = self.next_serial();
        let pressed_keys = self.modifier_state.pressed_keys();

        vec![
            CompositorMessage::InputEvent {
                surface_id,
                event: crate::scp::protocol::InputEvent::KeyboardEnter {
                    serial,
                    keys: pressed_keys,
                },
            },
            CompositorMessage::Modifiers {
                surface_id,
                serial,
                mods_depressed: self.modifier_state.mods_depressed,
                mods_latched: self.modifier_state.mods_latched,
                mods_locked: self.modifier_state.mods_locked,
                group: self.modifier_state.group,
            },
        ]
    }

    /// Send keyboard leave event when surface loses focus
    pub fn send_keyboard_leave(
        &mut self,
        _session_id: SessionId,
        surface_id: SurfaceId,
    ) -> CompositorMessage {
        let serial = self.next_serial();
        CompositorMessage::InputEvent {
            surface_id,
            event: crate::scp::protocol::InputEvent::KeyboardLeave { serial },
        }
    }

    /// Reset modifier state (e.g., when compositor loses focus)
    pub fn reset_keyboard_state(&mut self) {
        self.modifier_state.reset();
    }

    /// Get current modifier state
    pub fn modifier_state(&self) -> &ModifierState {
        &self.modifier_state
    }

    // ===== Data Transfer (Clipboard/DnD) =====

    /// Handle SetSelection request (clipboard write)
    pub fn handle_set_selection(
        &mut self,
        session_id: SessionId,
        mime_types: Vec<String>,
        _serial: u32,
    ) -> Result<Vec<CompositorMessage>, String> {
        // Verify this is a recent input serial (anti-clipboard-hijacking)
        // In production, track recent input serials per session

        // Check ClipboardWrite capability
        let _session = self.sessions.get(&session_id).ok_or("Invalid session")?;

        // Store selection offer
        self.data_device
            .set_selection(session_id, mime_types.clone());

        // Notify all other clients about new selection
        let mut messages = Vec::new();
        for &other_session_id in self.sessions.keys() {
            if other_session_id != session_id {
                messages.push(CompositorMessage::SelectionOffer {
                    mime_types: mime_types.clone(),
                });
            }
        }

        Ok(messages)
    }

    /// Handle clipboard data request from client
    pub fn handle_request_selection_data(
        &mut self,
        _session_id: SessionId,
        mime_type: String,
    ) -> Result<(SessionId, CompositorMessage), String> {
        // Get the selection owner
        let (owner_session, available_mimes) = self
            .data_device
            .get_selection()
            .ok_or("No selection available")?;

        // Verify requested mime type is available
        if !available_mimes.contains(&mime_type) {
            return Err("Requested mime type not available".to_string());
        }

        // Create pipe for data transfer
        let (_read_fd, write_fd) =
            unix_socket::create_pipe().map_err(|e| format!("Failed to create pipe: {}", e))?;

        // Request data from selection owner
        Ok((
            owner_session,
            CompositorMessage::RequestSelectionData {
                mime_type,
                fd: write_fd.into_raw_fd(),
            },
        ))
    }

    /// Handle drag-and-drop start
    pub fn handle_start_drag(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
        mime_types: Vec<String>,
        _serial: u32,
    ) -> Result<Vec<CompositorMessage>, String> {
        // Verify DragAndDrop capability and recent input serial

        // Start drag operation
        self.data_device
            .start_drag(session_id, surface_id, mime_types.clone());

        Ok(vec![])
    }

    /// Handle drag motion (update target surface)
    pub fn handle_drag_motion(
        &mut self,
        x: f64,
        y: f64,
        time_ms: u32,
    ) -> Vec<(SessionId, CompositorMessage)> {
        let mut messages = Vec::new();

        // Find surface under pointer
        // This is simplified - real implementation uses hit-testing
        if let Some((session_id, surface_id)) = self.focused_surface
            && let Some((_drag_session, _drag_surface, mime_types)) = self.data_device.get_drag()
        {
            // If this is a new surface, send DragEnter
            if !self.data_device.is_drag_over_surface(surface_id) {
                let serial = self.next_serial();
                messages.push((
                    session_id,
                    CompositorMessage::DragEnter {
                        serial,
                        surface_id,
                        x,
                        y,
                        mime_types,
                    },
                ));
                self.data_device.set_drag_surface(surface_id);
            } else {
                // Send motion update
                messages.push((session_id, CompositorMessage::DragMotion { x, y, time_ms }));
            }
        }

        messages
    }

    /// Handle drag drop
    pub fn handle_drag_drop(&mut self) -> Vec<(SessionId, CompositorMessage)> {
        let mut messages = Vec::new();

        if let Some(_drag_surface_id) = self.data_device.drag_surface()
            && let Some((session_id, _)) = self.focused_surface
        {
            messages.push((session_id, CompositorMessage::Drop));
        }

        messages
    }

    /// Clear clipboard selection
    pub fn clear_selection(&mut self) -> Vec<CompositorMessage> {
        self.data_device.clear_selection();
        vec![CompositorMessage::SelectionCleared]
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
