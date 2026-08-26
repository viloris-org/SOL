//! SCP surface management.

use crate::scp::{
    protocol::{BufferFormat, BufferId, Rect, SessionId, SurfaceId, ToplevelId, OutputId, LayerSurfaceId},
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
    pub damage: Vec<Rect>,
    pub pending_damage: Vec<Rect>,
    pub input_region: Option<Vec<Rect>>,
    pub opaque_region: Option<Vec<Rect>>,
    pub frame_callbacks: Vec<u32>,
    pub pending_frame_callbacks: Vec<u32>,
    pub old_buffers: Vec<SurfaceBuffer>,
}

#[derive(Debug)]
pub struct SurfaceBuffer {
    pub buffer_id: BufferId,
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
    LayerShell(LayerSurfaceId),
}

impl ScpSurface {
    pub fn new(id: SurfaceId, app_id: AppId) -> Self {
        Self {
            id,
            app_id,
            buffer: None,
            pending_buffer: None,
            role: SurfaceRole::None,
            damage: Vec::new(),
            pending_damage: Vec::new(),
            input_region: None,
            opaque_region: None,
            frame_callbacks: Vec::new(),
            pending_frame_callbacks: Vec::new(),
            old_buffers: Vec::new(),
        }
    }

    pub fn request_frame(&mut self, callback_id: u32) {
        self.pending_frame_callbacks.push(callback_id);
    }

    pub fn attach_buffer(&mut self, buffer: SurfaceBuffer) {
        self.pending_buffer = Some(buffer);
    }

    pub fn add_damage(&mut self, rect: Rect) {
        self.pending_damage.push(rect);
    }

    pub fn set_input_region(&mut self, rects: Vec<Rect>) {
        self.input_region = Some(rects);
    }

    pub fn set_opaque_region(&mut self, rects: Vec<Rect>) {
        self.opaque_region = Some(rects);
    }

    pub fn commit(&mut self) {
        if let Some(buffer) = self.pending_buffer.take() {
            // Keep old buffer for release after render
            if let Some(old) = self.buffer.replace(buffer) {
                self.old_buffers.push(old);
            }
        }
        self.damage = std::mem::take(&mut self.pending_damage);

        // Move pending frame callbacks to active
        self.frame_callbacks.append(&mut self.pending_frame_callbacks);
    }

    /// Take frame callbacks that should fire after this frame renders.
    pub fn take_frame_callbacks(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.frame_callbacks)
    }

    /// Take old buffers that can now be released back to the client.
    pub fn take_old_buffers(&mut self) -> Vec<SurfaceBuffer> {
        std::mem::take(&mut self.old_buffers)
    }

    pub fn assign_role(&mut self, role: SurfaceRole) -> Result<(), String> {
        if self.role != SurfaceRole::None {
            return Err("Surface already has a role".to_string());
        }
        self.role = role;
        Ok(())
    }

    /// Check if a point is inside the input region.
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        if let Some(regions) = &self.input_region {
            regions.iter().any(|rect| {
                x >= rect.x
                    && x < rect.x + rect.width
                    && y >= rect.y
                    && y < rect.y + rect.height
            })
        } else {
            // Default: entire surface is input region
            if let Some(buffer) = &self.buffer {
                x >= 0 && x < buffer.width && y >= 0 && y < buffer.height
            } else {
                false
            }
        }
    }

    /// Check if a region is fully opaque.
    pub fn is_opaque_at(&self, x: i32, y: i32) -> bool {
        if let Some(regions) = &self.opaque_region {
            regions.iter().any(|rect| {
                x >= rect.x
                    && x < rect.x + rect.width
                    && y >= rect.y
                    && y < rect.y + rect.height
            })
        } else {
            false
        }
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
    pub parent: Option<ToplevelId>,
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
    pub minimized: bool,
    pub resizing: bool,
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
            parent: None,
        }
    }

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    pub fn set_app_id(&mut self, app_id: String) {
        tracing::debug!(toplevel_id = ?self.id, ?app_id, "Toplevel app_id updated");
    }

    pub fn set_parent(&mut self, parent: Option<ToplevelId>) {
        self.parent = parent;
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

    pub fn set_maximized(&mut self, maximized: bool) {
        self.states.maximized = maximized;
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.states.fullscreen = fullscreen;
    }

    pub fn set_minimized(&mut self, minimized: bool) {
        self.states.minimized = minimized;
    }

    pub fn set_activated(&mut self, activated: bool) {
        self.states.activated = activated;
    }
}

/// Layer shell surface for desktop panels/overlays.
#[derive(Debug, Clone)]
pub struct LayerSurface {
    pub id: LayerSurfaceId,
    pub surface_id: SurfaceId,
    pub session_id: SessionId,
    pub app_id: AppId,
    pub layer: Layer,
    pub namespace: String,
    pub output: Option<OutputId>,
    pub anchor: Anchor,
    pub exclusive_zone: i32,
    pub margin: Margin,
    pub keyboard_interactivity: KeyboardInteractivity,
    pub size: (i32, i32),
    pub configured_size: (i32, i32),
    pub pending_configure: Option<LayerConfigure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Background = 0,
    Bottom = 1,
    Top = 2,
    Overlay = 3,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Anchor {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Anchor {
    pub fn is_horizontal_stretch(&self) -> bool {
        self.left && self.right
    }

    pub fn is_vertical_stretch(&self) -> bool {
        self.top && self.bottom
    }

    pub fn is_corner(&self) -> bool {
        (self.top || self.bottom) && (self.left || self.right)
            && !(self.is_horizontal_stretch() && self.is_vertical_stretch())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Margin {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardInteractivity {
    None,
    Exclusive,
    OnDemand,
}

#[derive(Debug, Clone)]
pub struct LayerConfigure {
    pub serial: u32,
    pub width: i32,
    pub height: i32,
}

impl LayerSurface {
    pub fn new(
        id: LayerSurfaceId,
        surface_id: SurfaceId,
        session_id: SessionId,
        app_id: AppId,
        layer: Layer,
        namespace: String,
    ) -> Self {
        Self {
            id,
            surface_id,
            session_id,
            app_id,
            layer,
            namespace,
            output: None,
            anchor: Anchor::default(),
            exclusive_zone: 0,
            margin: Margin::default(),
            keyboard_interactivity: KeyboardInteractivity::None,
            size: (0, 0),
            configured_size: (0, 0),
            pending_configure: None,
        }
    }

    pub fn configure(&mut self, serial: u32, width: i32, height: i32) {
        self.pending_configure = Some(LayerConfigure {
            serial,
            width,
            height,
        });
        self.configured_size = (width, height);
    }

    pub fn ack_configure(&mut self, serial: u32) -> bool {
        if let Some(pending) = &self.pending_configure
            && pending.serial == serial
        {
            self.pending_configure = None;
            return true;
        }
        false
    }

    /// Calculate the actual geometry based on anchor, size, and output bounds.
    pub fn calculate_geometry(&self, output_width: i32, output_height: i32) -> Rect {
        let (desired_width, desired_height) = self.size;

        // Determine actual width
        let width = if self.anchor.is_horizontal_stretch() {
            output_width - self.margin.left - self.margin.right
        } else if desired_width > 0 {
            desired_width
        } else {
            self.configured_size.0
        };

        // Determine actual height
        let height = if self.anchor.is_vertical_stretch() {
            output_height - self.margin.top - self.margin.bottom
        } else if desired_height > 0 {
            desired_height
        } else {
            self.configured_size.1
        };

        // Calculate position based on anchors
        let x = if self.anchor.left && !self.anchor.right {
            self.margin.left
        } else if self.anchor.right && !self.anchor.left {
            output_width - width - self.margin.right
        } else {
            // Centered or stretched
            self.margin.left
        };

        let y = if self.anchor.top && !self.anchor.bottom {
            self.margin.top
        } else if self.anchor.bottom && !self.anchor.top {
            output_height - height - self.margin.bottom
        } else {
            // Centered or stretched
            self.margin.top
        };

        Rect {
            x,
            y,
            width,
            height,
        }
    }
}

/// Surface manager — owns all surfaces, toplevels, and layer surfaces.
#[derive(Debug, Default)]
pub struct SurfaceManager {
    surfaces: HashMap<(SessionId, SurfaceId), ScpSurface>,
    toplevels: HashMap<ToplevelId, Toplevel>,
    layer_surfaces: HashMap<LayerSurfaceId, LayerSurface>,
    next_toplevel_id: ToplevelId,
    next_layer_id: LayerSurfaceId,
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

    pub fn create_layer_surface(
        &mut self,
        session_id: SessionId,
        surface_id: SurfaceId,
        layer: Layer,
        namespace: String,
    ) -> Result<LayerSurfaceId, String> {
        let surface = self
            .surfaces
            .get_mut(&(session_id, surface_id))
            .ok_or_else(|| "Surface not found".to_string())?;

        let layer_id = self.next_layer_id;
        self.next_layer_id = self
            .next_layer_id
            .checked_add(1)
            .ok_or("Layer surface ID space exhausted")?;

        surface.assign_role(SurfaceRole::LayerShell(layer_id))?;

        let layer_surface = LayerSurface::new(
            layer_id,
            surface_id,
            session_id,
            surface.app_id.clone(),
            layer,
            namespace,
        );
        self.layer_surfaces.insert(layer_id, layer_surface);

        Ok(layer_id)
    }

    pub fn get_layer_surface(&self, id: LayerSurfaceId) -> Option<&LayerSurface> {
        self.layer_surfaces.get(&id)
    }

    pub fn get_layer_surface_mut(&mut self, id: LayerSurfaceId) -> Option<&mut LayerSurface> {
        self.layer_surfaces.get_mut(&id)
    }

    pub fn iter_layer_surfaces(&self) -> impl Iterator<Item = &LayerSurface> {
        self.layer_surfaces.values()
    }

    /// Get layer surfaces sorted by layer (background → overlay).
    pub fn iter_layer_surfaces_sorted(&self) -> Vec<&LayerSurface> {
        let mut surfaces: Vec<_> = self.layer_surfaces.values().collect();
        surfaces.sort_by_key(|s| s.layer);
        surfaces
    }

    pub fn destroy_session(&mut self, session_id: SessionId) {
        self.surfaces.retain(|(owner, _), _| *owner != session_id);
        self.toplevels
            .retain(|_, toplevel| toplevel.session_id != session_id);
        self.layer_surfaces
            .retain(|_, layer| layer.session_id != session_id);
    }

    /// Collect and clear all pending frame callbacks from all surfaces.
    /// Returns (session_id, surface_id, callback_id) tuples.
    pub fn take_frame_callbacks(&mut self) -> Vec<(SessionId, SurfaceId, u32)> {
        let mut callbacks = Vec::new();
        for ((session_id, surface_id), surface) in &mut self.surfaces {
            for callback_id in surface.frame_callbacks.drain(..) {
                callbacks.push((*session_id, *surface_id, callback_id));
            }
        }
        callbacks
    }
}
