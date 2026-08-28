//! SCP surface management.

use crate::scp::{
    protocol::{
        BufferFormat, BufferId, LayerSurfaceId, LockSurfaceId, OutputId, Rect, SessionId,
        SurfaceId, ToplevelId,
    },
    security::AppId,
    unix_socket,
};
use std::collections::HashMap;

/// Surfaces one client may hold at once.
///
/// Ids are client-chosen `u32`s, so without a cap a client can allocate four
/// billion of them and take the compositor down by growing its heap. Every limit
/// in this module is set well above what an application plausibly needs and far
/// below what it takes to hurt the machine.
pub const MAX_SURFACES_PER_SESSION: usize = 256;

/// Toplevel windows one client may hold at once.
pub const MAX_TOPLEVELS_PER_SESSION: usize = 64;

/// Layer surfaces one client may hold at once.
pub const MAX_LAYER_SURFACES_PER_SESSION: usize = 32;

/// Buffers a surface may retain awaiting release before the oldest is dropped.
///
/// Each one pins a descriptor. The renderer releases them after a frame; until
/// there is a renderer nothing does, so this is what keeps an attach/commit loop
/// from exhausting the compositor's descriptors.
pub const MAX_RETAINED_BUFFERS: usize = 8;

/// Damage rectangles a surface accumulates before they are coalesced.
///
/// Damage is an optimization hint, so collapsing many rectangles into their
/// bounding box costs redraw area and never correctness.
pub const MAX_DAMAGE_RECTS: usize = 64;

/// Frame callbacks a surface may have outstanding.
pub const MAX_FRAME_CALLBACKS: usize = 64;

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
            unix_socket::close_fd(self.fd);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceRole {
    None,
    Toplevel(ToplevelId),
    Popup {
        parent: SurfaceId,
    },
    LayerShell(LayerSurfaceId),
    /// Full-output surface of an engaged session lock. Owned by
    /// [`crate::scp::session_lock`], not by this module's per-role maps.
    LockSurface(LockSurfaceId),
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

    /// Register a frame callback, ignoring requests past the outstanding limit.
    ///
    /// A client that asks for callbacks it never lets fire is misbehaving; the
    /// answer is to stop recording them, not to grow without bound.
    pub fn request_frame(&mut self, callback_id: u32) {
        if self.pending_frame_callbacks.len() >= MAX_FRAME_CALLBACKS {
            return;
        }
        self.pending_frame_callbacks.push(callback_id);
    }

    pub fn attach_buffer(&mut self, buffer: SurfaceBuffer) {
        self.pending_buffer = Some(buffer);
    }

    /// Record a damaged region, coalescing once the list is full.
    ///
    /// Merging into a bounding box redraws more than strictly necessary, which
    /// is the harmless direction to be wrong in — unlike an unbounded list a
    /// client can grow by sending `Damage` without ever committing.
    pub fn add_damage(&mut self, rect: Rect) {
        self.pending_damage.push(rect);
        if self.pending_damage.len() > MAX_DAMAGE_RECTS
            && let Some(bounds) = bounding_box(&self.pending_damage)
        {
            self.pending_damage.clear();
            self.pending_damage.push(bounds);
        }
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
                // Nothing has drained these yet — the renderer that will do so
                // does not exist. Dropping the oldest closes its descriptor,
                // which is what an attach/commit loop would otherwise exhaust.
                while self.old_buffers.len() > MAX_RETAINED_BUFFERS {
                    self.old_buffers.remove(0);
                }
            }
        }
        self.damage = std::mem::take(&mut self.pending_damage);

        // Move pending frame callbacks to active
        self.frame_callbacks
            .append(&mut self.pending_frame_callbacks);
        if self.frame_callbacks.len() > MAX_FRAME_CALLBACKS {
            let excess = self.frame_callbacks.len() - MAX_FRAME_CALLBACKS;
            self.frame_callbacks.drain(..excess);
        }
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
                x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
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

    /// Whether this surface accepts pointer or touch input at a surface-local
    /// point, used to refine a window-bounds hit test.
    ///
    /// An unset input region means the whole surface is interactive, matching
    /// the caller's bounds check. An explicitly *empty* region is how a client
    /// makes an overlay click-through, so it must be honored rather than treated
    /// as "unset".
    pub fn accepts_input_at(&self, x: f64, y: f64) -> bool {
        match &self.input_region {
            Some(regions) => regions.iter().any(|rect| {
                x >= f64::from(rect.x)
                    && x < f64::from(rect.x) + f64::from(rect.width)
                    && y >= f64::from(rect.y)
                    && y < f64::from(rect.y) + f64::from(rect.height)
            }),
            None => true,
        }
    }

    /// Check if a region is fully opaque.
    pub fn is_opaque_at(&self, x: i32, y: i32) -> bool {
        if let Some(regions) = &self.opaque_region {
            regions.iter().any(|rect| {
                x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
            })
        } else {
            false
        }
    }
}

/// Smallest rectangle covering every input rectangle.
fn bounding_box(rects: &[Rect]) -> Option<Rect> {
    let mut iter = rects.iter();
    let first = iter.next()?;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x.saturating_add(first.width);
    let mut bottom = first.y.saturating_add(first.height);

    for rect in iter {
        left = left.min(rect.x);
        top = top.min(rect.y);
        right = right.max(rect.x.saturating_add(rect.width));
        bottom = bottom.max(rect.y.saturating_add(rect.height));
    }

    Some(Rect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    })
}

/// Toplevel window managed by the compositor.
#[derive(Debug)]
pub struct Toplevel {
    pub id: ToplevelId,
    pub surface_id: SurfaceId,
    /// Verified identity of the owning application, assigned at connect time.
    pub app_id: AppId,
    /// Self-declared grouping hint from `SetToplevelAppId`.
    ///
    /// Kept separate from [`Self::app_id`]: a client may name its window group
    /// freely, but it must never be able to overwrite the identity the security
    /// coordinator verified.
    pub declared_app_id: Option<String>,
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
            declared_app_id: None,
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

    /// Record the client's declared app_id grouping hint.
    pub fn set_app_id(&mut self, app_id: String) {
        tracing::debug!(toplevel_id = ?self.id, %app_id, "toplevel declared app_id updated");
        self.declared_app_id = Some(app_id);
    }

    /// Move the window without changing its size.
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.geometry.x = x;
        self.geometry.y = y;
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
        (self.top || self.bottom)
            && (self.left || self.right)
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
    ///
    /// Sizes and margins are client-chosen `i32`s, so every step saturates and
    /// the result is clamped to the output. Plain arithmetic here used to panic
    /// on a hostile margin — and this runs inside `build_stack`, on the input
    /// path, while the compositor state lock is held.
    pub fn calculate_geometry(&self, output_width: i32, output_height: i32) -> Rect {
        let (desired_width, desired_height) = self.size;

        // Determine actual width
        let width = if self.anchor.is_horizontal_stretch() {
            output_width
                .saturating_sub(self.margin.left)
                .saturating_sub(self.margin.right)
        } else if desired_width > 0 {
            desired_width
        } else {
            self.configured_size.0
        };

        // Determine actual height
        let height = if self.anchor.is_vertical_stretch() {
            output_height
                .saturating_sub(self.margin.top)
                .saturating_sub(self.margin.bottom)
        } else if desired_height > 0 {
            desired_height
        } else {
            self.configured_size.1
        };

        // A layer surface is chrome on one output; it cannot be larger than the
        // output it is anchored to, however it was configured.
        let width = width.clamp(0, output_width.max(0));
        let height = height.clamp(0, output_height.max(0));

        // Calculate position based on anchors
        let x = if self.anchor.right && !self.anchor.left {
            output_width
                .saturating_sub(width)
                .saturating_sub(self.margin.right)
        } else {
            // Left-anchored, centered, or stretched.
            self.margin.left
        };

        let y = if self.anchor.bottom && !self.anchor.top {
            output_height
                .saturating_sub(height)
                .saturating_sub(self.margin.bottom)
        } else {
            // Top-anchored, centered, or stretched.
            self.margin.top
        };

        Rect {
            x: x.clamp(0, output_width.saturating_sub(width).max(0)),
            y: y.clamp(0, output_height.saturating_sub(height).max(0)),
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
        if self.session_surfaces(session_id) >= MAX_SURFACES_PER_SESSION {
            return Err(format!(
                "A session may hold at most {MAX_SURFACES_PER_SESSION} surfaces"
            ));
        }
        self.surfaces
            .insert((session_id, id), ScpSurface::new(id, app_id));
        Ok(())
    }

    /// Surfaces currently held by one client.
    pub fn session_surfaces(&self, session_id: SessionId) -> usize {
        self.surfaces
            .keys()
            .filter(|(owner, _)| *owner == session_id)
            .count()
    }

    /// Toplevels currently held by one client.
    pub fn session_toplevels(&self, session_id: SessionId) -> usize {
        self.toplevels
            .values()
            .filter(|toplevel| toplevel.session_id == session_id)
            .count()
    }

    /// Layer surfaces currently held by one client.
    pub fn session_layer_surfaces(&self, session_id: SessionId) -> usize {
        self.layer_surfaces
            .values()
            .filter(|layer| layer.session_id == session_id)
            .count()
    }

    /// Remove a surface and the window state attached to its role.
    ///
    /// Returns the role the surface held so the caller can finish the teardown
    /// it owns — dismissing child popups, dropping focus, notifying clients.
    pub fn destroy_surface(
        &mut self,
        session_id: SessionId,
        id: SurfaceId,
    ) -> Result<SurfaceRole, String> {
        let surface = self
            .surfaces
            .remove(&(session_id, id))
            .ok_or_else(|| "Surface not found".to_string())?;

        let role = surface.role.clone();
        match &role {
            SurfaceRole::Toplevel(toplevel_id) => {
                self.toplevels.remove(toplevel_id);
            }
            SurfaceRole::LayerShell(layer_id) => {
                self.layer_surfaces.remove(layer_id);
            }
            // A lock surface's bookkeeping lives in the session lock, which
            // must outlive the surface: the caller drops it there so that
            // losing a surface cannot silently unlock the session.
            SurfaceRole::LockSurface(_) | SurfaceRole::Popup { .. } | SurfaceRole::None => {}
        }
        Ok(role)
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
        if self.session_toplevels(session_id) >= MAX_TOPLEVELS_PER_SESSION {
            return Err(format!(
                "A session may hold at most {MAX_TOPLEVELS_PER_SESSION} toplevel windows"
            ));
        }

        let toplevel_id = self.next_toplevel_id;
        // Wrapping would let a later toplevel take an id an earlier one still
        // holds, and `toplevels.insert` would then evict another client's window
        // from the map. Every other id space here is already checked.
        let next_toplevel_id = self
            .next_toplevel_id
            .checked_add(1)
            .ok_or("Toplevel ID space exhausted")?;

        let surface = self
            .surfaces
            .get_mut(&(session_id, surface_id))
            .ok_or_else(|| "Surface not found".to_string())?;
        surface.assign_role(SurfaceRole::Toplevel(toplevel_id))?;
        self.next_toplevel_id = next_toplevel_id;

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
        if self.session_layer_surfaces(session_id) >= MAX_LAYER_SURFACES_PER_SESSION {
            return Err(format!(
                "A session may hold at most {MAX_LAYER_SURFACES_PER_SESSION} layer surfaces"
            ));
        }

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

    pub fn remove_layer_surface(&mut self, id: LayerSurfaceId) -> Option<LayerSurface> {
        self.layer_surfaces.remove(&id)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> AppId {
        AppId("org.sol.test".to_string())
    }

    fn layer_surface() -> LayerSurface {
        LayerSurface::new(1, 1, 1, app(), Layer::Top, "panel".to_string())
    }

    #[test]
    fn a_hostile_margin_does_not_overflow() {
        // Margins are client-chosen i32s, and this runs on the input path with
        // the compositor state lock held: plain subtraction here used to panic.
        let mut surface = layer_surface();
        surface.anchor = Anchor {
            top: true,
            bottom: true,
            left: true,
            right: true,
        };
        surface.margin = Margin {
            top: i32::MIN,
            right: i32::MIN,
            bottom: i32::MAX,
            left: i32::MIN,
        };

        let geometry = surface.calculate_geometry(1920, 1080);
        assert!(geometry.width <= 1920 && geometry.height <= 1080);
        assert!(geometry.x >= 0 && geometry.y >= 0);
    }

    #[test]
    fn a_layer_surface_cannot_exceed_its_output() {
        let mut surface = layer_surface();
        surface.size = (10_000, 10_000);

        let geometry = surface.calculate_geometry(1920, 1080);
        assert_eq!((geometry.width, geometry.height), (1920, 1080));
        assert_eq!((geometry.x, geometry.y), (0, 0));
    }

    #[test]
    fn surfaces_are_capped_per_session() {
        let mut manager = SurfaceManager::new();
        for id in 0..MAX_SURFACES_PER_SESSION {
            manager
                .create_surface(1, id as SurfaceId, app())
                .expect("surfaces within the limit");
        }

        let error = manager
            .create_surface(1, MAX_SURFACES_PER_SESSION as SurfaceId, app())
            .expect_err("the limit must hold");
        assert!(error.contains("at most"), "unexpected: {error}");

        manager
            .create_surface(2, 0, app())
            .expect("the cap is per session, not global");
    }

    #[test]
    fn toplevels_are_capped_per_session() {
        let mut manager = SurfaceManager::new();
        for id in 0..=MAX_TOPLEVELS_PER_SESSION {
            manager
                .create_surface(1, id as SurfaceId, app())
                .expect("create surface");
        }

        for id in 0..MAX_TOPLEVELS_PER_SESSION {
            manager
                .create_toplevel(1, id as SurfaceId, "window".to_string())
                .expect("toplevels within the limit");
        }

        let error = manager
            .create_toplevel(
                1,
                MAX_TOPLEVELS_PER_SESSION as SurfaceId,
                "one too many".to_string(),
            )
            .expect_err("the limit must hold");
        assert!(error.contains("at most"), "unexpected: {error}");
    }

    #[test]
    fn a_refused_toplevel_leaves_the_surface_roleless() {
        let mut manager = SurfaceManager::new();
        manager.create_surface(1, 1, app()).expect("create surface");
        manager
            .create_toplevel(1, 1, "first".to_string())
            .expect("first role");

        // A second role on the same surface must fail without having consumed an
        // id, or the id space drifts ahead of the windows that exist.
        manager
            .create_toplevel(1, 1, "second".to_string())
            .expect_err("a surface takes one role");

        manager.create_surface(1, 2, app()).expect("create surface");
        let next = manager
            .create_toplevel(1, 2, "next".to_string())
            .expect("create toplevel");
        assert_eq!(next, 1, "the refused attempt must not have burned an id");
    }

    #[test]
    fn retained_buffers_do_not_grow_without_bound() {
        let mut surface = ScpSurface::new(1, app());
        for id in 0..(MAX_RETAINED_BUFFERS as u32 * 4) {
            surface.attach_buffer(SurfaceBuffer {
                buffer_id: id,
                // -1 so dropping these closes nothing real.
                fd: -1,
                width: 16,
                height: 16,
                stride: 64,
                format: BufferFormat::Argb8888,
            });
            surface.commit();
        }

        assert!(
            surface.old_buffers.len() <= MAX_RETAINED_BUFFERS,
            "an attach/commit loop must not pin descriptors without bound: {}",
            surface.old_buffers.len()
        );
    }

    #[test]
    fn damage_coalesces_instead_of_growing() {
        let mut surface = ScpSurface::new(1, app());
        for index in 0..(MAX_DAMAGE_RECTS as i32 * 4) {
            surface.add_damage(Rect {
                x: index,
                y: index,
                width: 1,
                height: 1,
            });
        }

        assert!(
            surface.pending_damage.len() <= MAX_DAMAGE_RECTS,
            "damage must coalesce: {}",
            surface.pending_damage.len()
        );
        // Coalescing may only ever grow the redrawn area, never shrink it.
        let covered = bounding_box(&surface.pending_damage).expect("damage is present");
        assert!(covered.width >= MAX_DAMAGE_RECTS as i32);
    }

    #[test]
    fn frame_callbacks_do_not_grow_without_bound() {
        let mut surface = ScpSurface::new(1, app());
        for id in 0..(MAX_FRAME_CALLBACKS as u32 * 4) {
            surface.request_frame(id);
            surface.commit();
        }

        assert!(
            surface.frame_callbacks.len() <= MAX_FRAME_CALLBACKS,
            "uncollected callbacks must not accumulate: {}",
            surface.frame_callbacks.len()
        );
    }
}
