//! SCP surface management.

use crate::scp::{
    protocol::{BufferFormat, SessionId, SurfaceId, ToplevelId},
    security::AppId,
};
use std::collections::HashMap;

/// A compositor surface — the minimal unit of client content.
#[derive(Debug)]
pub struct ScpSurface {
    pub id: SurfaceId,
    pub app_id: AppId,
    pub buffer: Option<SurfaceBuffer>,
    pub pending_buffer: Option<SurfaceBuffer>,
    pub role: SurfaceRole,
}

#[derive(Debug)]
pub struct SurfaceBuffer {
    pub fd: i32,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: BufferFormat,
}

impl Drop for SurfaceBuffer {
    fn drop(&mut self) {
        if self.fd >= 0 {
            let _ = nix::unistd::close(self.fd);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceRole {
    None,
    Toplevel(ToplevelId),
    Popup { parent: SurfaceId },
    LayerShell,
}

impl ScpSurface {
    pub fn new(id: SurfaceId, app_id: AppId) -> Self {
        Self {
            id,
            app_id,
            buffer: None,
            pending_buffer: None,
            role: SurfaceRole::None,
        }
    }

    pub fn attach_buffer(&mut self, buffer: SurfaceBuffer) {
        self.pending_buffer = Some(buffer);
    }

    pub fn commit(&mut self) {
        if let Some(buffer) = self.pending_buffer.take() {
            self.buffer = Some(buffer);
        }
    }

    pub fn assign_role(&mut self, role: SurfaceRole) -> Result<(), String> {
        if self.role != SurfaceRole::None {
            return Err("Surface already has a role".to_string());
        }
        self.role = role;
        Ok(())
    }
}

/// Toplevel window managed by the compositor.
#[derive(Debug)]
pub struct Toplevel {
    pub id: ToplevelId,
    pub surface_id: SurfaceId,
    pub app_id: AppId,
    pub session_id: SessionId,
    pub title: String,
    pub geometry: ToplevelGeometry,
    pub states: ToplevelStates,
    pub pending_configure: Option<PendingConfigure>,
}

#[derive(Debug, Clone)]
pub struct ToplevelGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Default for ToplevelGeometry {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToplevelStates {
    pub activated: bool,
    pub maximized: bool,
    pub fullscreen: bool,
}

#[derive(Debug, Clone)]
pub struct PendingConfigure {
    pub serial: u32,
    pub width: i32,
    pub height: i32,
    pub states: ToplevelStates,
}

impl Toplevel {
    pub fn new(
        id: ToplevelId,
        surface_id: SurfaceId,
        app_id: AppId,
        session_id: SessionId,
        title: String,
    ) -> Self {
        Self {
            id,
            surface_id,
            app_id,
            session_id,
            title,
            geometry: ToplevelGeometry::default(),
            states: ToplevelStates::default(),
            pending_configure: None,
        }
    }

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn set_app_id(&mut self, app_id: String) {
        // Store user-provided app_id for grouping and icon lookup
        // Note: This is different from the authenticated AppId
        tracing::debug!(toplevel_id = ?self.id, ?app_id, "Toplevel app_id updated");
    }

    pub fn configure(&mut self, serial: u32, width: i32, height: i32, states: ToplevelStates) {
        self.pending_configure = Some(PendingConfigure {
            serial,
            width,
            height,
            states: states.clone(),
        });
    }

    pub fn ack_configure(&mut self, serial: u32) -> bool {
        if let Some(pending) = &self.pending_configure
            && pending.serial == serial
        {
            self.geometry.width = pending.width;
            self.geometry.height = pending.height;
            self.states = pending.states.clone();
            self.pending_configure = None;
            return true;
        }
        false
    }
}

/// Surface manager — owns all surfaces and toplevels.
#[derive(Debug, Default)]
pub struct SurfaceManager {
    surfaces: HashMap<(SessionId, SurfaceId), ScpSurface>,
    toplevels: HashMap<ToplevelId, Toplevel>,
    next_toplevel_id: ToplevelId,
}

impl SurfaceManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_surface(
        &mut self,
        session_id: SessionId,
        id: SurfaceId,
        app_id: AppId,
    ) -> Result<(), String> {
        if self.surfaces.contains_key(&(session_id, id)) {
            return Err("Surface ID already exists".to_string());
        }
        self.surfaces
            .insert((session_id, id), ScpSurface::new(id, app_id));
        Ok(())
    }

    pub fn destroy_surface(&mut self, session_id: SessionId, id: SurfaceId) -> Result<(), String> {
        self.surfaces
            .remove(&(session_id, id))
            .ok_or_else(|| "Surface not found".to_string())?;
        self.toplevels.retain(|_, toplevel| {
            !(toplevel.session_id == session_id && toplevel.surface_id == id)
        });
        Ok(())
    }

    pub fn get_surface(&self, session_id: SessionId, id: SurfaceId) -> Option<&ScpSurface> {
        self.surfaces.get(&(session_id, id))
    }

    pub fn get_surface_mut(
        &mut self,
        session_id: SessionId,
        id: SurfaceId,
    ) -> Option<&mut ScpSurface> {
        self.surfaces.get_mut(&(session_id, id))
    }

    pub fn create_toplevel(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
        title: String,
    ) -> Result<ToplevelId, String> {
        let surface = self
            .surfaces
            .get_mut(&(session_id, surface_id))
            .ok_or_else(|| "Surface not found".to_string())?;

        let toplevel_id = self.next_toplevel_id;
        self.next_toplevel_id += 1;

        surface.assign_role(SurfaceRole::Toplevel(toplevel_id))?;

        let toplevel = Toplevel::new(
            toplevel_id,
            surface_id,
            surface.app_id.clone(),
            session_id,
            title,
        );
        self.toplevels.insert(toplevel_id, toplevel);

        Ok(toplevel_id)
    }

    pub fn get_toplevel(&self, id: ToplevelId) -> Option<&Toplevel> {
        self.toplevels.get(&id)
    }

    pub fn get_toplevel_mut(&mut self, id: ToplevelId) -> Option<&mut Toplevel> {
        self.toplevels.get_mut(&id)
    }

    pub fn remove_toplevel(&mut self, id: ToplevelId) -> Option<Toplevel> {
        self.toplevels.remove(&id)
    }

    pub fn iter_toplevels(&self) -> impl Iterator<Item = &Toplevel> {
        self.toplevels.values()
    }

    pub fn destroy_session(&mut self, session_id: SessionId) {
        self.surfaces.retain(|(owner, _), _| *owner != session_id);
        self.toplevels
            .retain(|_, toplevel| toplevel.session_id != session_id);
    }
}
