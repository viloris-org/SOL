//! Core SCP compositor state and message handlers.

use crate::scp::{
    buffer::BufferManager,
    capability::{Capability, CapabilityGrant, CapabilityToken, Decision},
    input::InputState,
    output::OutputManager,
    protocol::{ClientMessage, CompositorMessage, SessionId, SurfaceId},
    security::{AppId, AuditOutcome, SecurityCoordinator, StubSecurityCoordinator},
    surface::SurfaceManager,
};
use std::{collections::HashMap, sync::Arc, time::Instant};

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
    focused_surface: Option<(SessionId, SurfaceId)>,
    next_serial: u32,
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
            focused_surface: None,
            next_serial: 1,
        }
    }

    pub fn next_serial(&mut self) -> u32 {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);
        serial
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
                    let _ = nix::unistd::close(fd);
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
            let _ = nix::unistd::close(fd);
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
                let surface = self
                    .surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?;
                surface.attach_buffer(crate::scp::surface::SurfaceBuffer {
                    fd: buffer_fd,
                    width,
                    height,
                    stride,
                    format,
                });
                Ok(vec![])
            }
            ClientMessage::Commit { surface_id } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                self.surface_manager
                    .get_surface_mut(session_id, surface_id)
                    .ok_or("Surface not found")?
                    .commit();
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
                // TODO: Track damage regions for optimization
                tracing::trace!(?surface_id, ?x, ?y, ?width, ?height, "Surface damage");
                Ok(vec![])
            }
            ClientMessage::SetInputRegion { surface_id, rects } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                // TODO: Implement input region management
                tracing::debug!(?surface_id, num_rects = ?rects.len(), "SetInputRegion - not yet implemented");
                Ok(vec![])
            }
            ClientMessage::SetOpaqueRegion { surface_id, rects } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                // TODO: Implement opaque region management for rendering optimization
                tracing::debug!(?surface_id, num_rects = ?rects.len(), "SetOpaqueRegion - not yet implemented");
                Ok(vec![])
            }

            // Popup windows
            ClientMessage::CreatePopup {
                surface_id,
                parent_id,
                positioner: _,
                grab,
            } => {
                self.verify_surface_ownership(session_id, surface_id)?;
                self.verify_surface_ownership(session_id, parent_id)?;
                // TODO: Implement popup positioning algorithm
                tracing::debug!(
                    ?surface_id,
                    ?parent_id,
                    ?grab,
                    "CreatePopup - not yet implemented"
                );
                Ok(vec![])
            }

            // Toplevel state management
            ClientMessage::SetToplevelState { toplevel_id, state } => {
                self.verify_toplevel_ownership(session_id, toplevel_id)?;
                // TODO: Implement state change handling (maximize, minimize, fullscreen)
                tracing::debug!(
                    ?toplevel_id,
                    ?state,
                    "SetToplevelState - not yet implemented"
                );
                Ok(vec![])
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
                // TODO: Implement cursor image setting
                tracing::trace!(
                    ?serial,
                    ?surface_id,
                    ?hotspot_x,
                    ?hotspot_y,
                    "SetCursor - not yet implemented"
                );
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
        self.surface_manager.destroy_session(session_id);
        if self
            .focused_surface
            .is_some_and(|(owner, _)| owner == session_id)
        {
            self.focused_surface = None;
        }
    }

    pub fn iter_toplevels(&self) -> impl Iterator<Item = &crate::scp::surface::Toplevel> {
        self.surface_manager.iter_toplevels()
    }

    pub const fn get_focused_surface(&self) -> Option<(SessionId, SurfaceId)> {
        self.focused_surface
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
