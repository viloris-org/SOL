//! Session lock — the protocol behind the greeter and the lock screen.
//!
//! A locked session is the one moment where the compositor must be *less*
//! cooperative than usual: exactly one client may draw, only that client may
//! receive input, and nothing underneath may be observed. Layer shell cannot
//! express that. Its topmost layer is reachable by anything holding
//! [`Capability::LayerShell`](crate::scp::capability::Capability::LayerShell) —
//! `sol-shell` included — so a lock built on it would be a surface the shell
//! could cover, forge, or read around. Lock surfaces are therefore their own
//! role, stacked above every layer, and gated by a capability no shell or
//! application is granted.
//!
//! ## Lifecycle
//!
//! ```text
//! LockSession        → lock engaged immediately; input to other clients stops
//!   CreateLockSurface  (one per output)
//!   AckLockConfigure   (per surface)
//! → SessionLocked     once every output is covered by an acked surface
//!   UnlockSession    → lock released, previous focus restored
//! ```
//!
//! ## Why a crashed locker does not unlock the screen
//!
//! The lock outlives its client. If the locking process dies the lock is
//! *abandoned*, not released: [`SessionLockManager::abandon`] clears the owner
//! and drops the surfaces but keeps the session locked, leaving the compositor
//! to paint a blank fallback until a new locker adopts it. A crash must never
//! be a path back to the desktop.

use crate::scp::{
    protocol::{LockId, LockSurfaceId, OutputId, SessionId, SurfaceId},
    security::AppId,
};
use std::collections::HashMap;

/// Geometry the compositor has offered a lock surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockConfigure {
    pub serial: u32,
    pub width: i32,
    pub height: i32,
}

/// One full-output surface belonging to a session lock.
///
/// Lock surfaces have no anchors, margins, or client-chosen size: they always
/// cover their entire output. A client that could size or place its own lock
/// surface could leave a strip of the desktop visible.
#[derive(Debug, Clone)]
pub struct LockSurface {
    pub id: LockSurfaceId,
    pub lock_id: LockId,
    pub session_id: SessionId,
    pub surface_id: SurfaceId,
    /// Output this surface covers. `None` only before any output is registered.
    pub output: Option<OutputId>,
    pub size: (i32, i32),
    pub pending_configure: Option<LockConfigure>,
    /// Whether the client has accepted the geometry it was offered.
    pub acked: bool,
}

impl LockSurface {
    fn new(
        id: LockSurfaceId,
        lock_id: LockId,
        session_id: SessionId,
        surface_id: SurfaceId,
        output: Option<OutputId>,
    ) -> Self {
        Self {
            id,
            lock_id,
            session_id,
            surface_id,
            output,
            size: (0, 0),
            pending_configure: None,
            acked: false,
        }
    }

    pub fn configure(&mut self, serial: u32, width: i32, height: i32) {
        self.pending_configure = Some(LockConfigure {
            serial,
            width,
            height,
        });
        self.size = (width, height);
        self.acked = false;
    }

    pub fn ack_configure(&mut self, serial: u32) -> bool {
        if let Some(pending) = self.pending_configure
            && pending.serial == serial
        {
            self.pending_configure = None;
            self.acked = true;
            return true;
        }
        false
    }
}

/// An engaged session lock.
#[derive(Debug)]
pub struct SessionLock {
    pub id: LockId,
    /// Client driving the lock, or `None` once it has been abandoned.
    owner: Option<SessionId>,
    /// Verified identity of the client that engaged the lock.
    pub app_id: AppId,
    /// Set once every output is covered by an acknowledged lock surface.
    confirmed: bool,
    /// Keyboard focus to restore when the lock is released.
    previous_focus: Option<(SessionId, SurfaceId)>,
    surfaces: HashMap<LockSurfaceId, LockSurface>,
    /// One surface per output; the key is what makes a second surface for the
    /// same output rejectable.
    outputs: HashMap<Option<OutputId>, LockSurfaceId>,
}

impl SessionLock {
    pub const fn owner(&self) -> Option<SessionId> {
        self.owner
    }

    pub const fn is_confirmed(&self) -> bool {
        self.confirmed
    }

    /// Whether the lock has lost its client and is waiting to be adopted.
    pub const fn is_abandoned(&self) -> bool {
        self.owner.is_none()
    }

    pub const fn previous_focus(&self) -> Option<(SessionId, SurfaceId)> {
        self.previous_focus
    }

    pub fn iter_surfaces(&self) -> impl Iterator<Item = &LockSurface> {
        self.surfaces.values()
    }

    pub fn get_surface(&self, id: LockSurfaceId) -> Option<&LockSurface> {
        self.surfaces.get(&id)
    }

    pub fn get_surface_mut(&mut self, id: LockSurfaceId) -> Option<&mut LockSurface> {
        self.surfaces.get_mut(&id)
    }

    /// Whether every registered output is covered by an acknowledged surface.
    ///
    /// A lock with no surface at all never confirms, so an empty output list
    /// cannot be used to declare the screen locked without drawing anything.
    fn covers_all_outputs(&self, outputs: &[OutputId]) -> bool {
        if self.surfaces.is_empty() {
            return false;
        }
        if !self.surfaces.values().all(|surface| surface.acked) {
            return false;
        }
        outputs
            .iter()
            .all(|id| self.outputs.contains_key(&Some(*id)))
    }
}

/// Owns the session's lock state, of which there is at most one.
#[derive(Debug)]
pub struct SessionLockManager {
    lock: Option<SessionLock>,
    next_lock_id: LockId,
    next_surface_id: LockSurfaceId,
}

impl Default for SessionLockManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of a client's attempt to engage the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockGrant {
    /// A fresh lock was engaged by this client.
    Engaged(LockId),
    /// An abandoned lock was taken over; it stays confirmed if it already was.
    Adopted(LockId),
}

impl LockGrant {
    pub const fn lock_id(self) -> LockId {
        match self {
            Self::Engaged(id) | Self::Adopted(id) => id,
        }
    }
}

impl SessionLockManager {
    pub fn new() -> Self {
        Self {
            lock: None,
            next_lock_id: 1,
            next_surface_id: 1,
        }
    }

    pub const fn lock(&self) -> Option<&SessionLock> {
        self.lock.as_ref()
    }

    pub const fn lock_mut(&mut self) -> Option<&mut SessionLock> {
        self.lock.as_mut()
    }

    /// Whether input and capture restrictions are in force.
    ///
    /// True from the moment a lock is engaged, not from the moment it is
    /// confirmed: the desktop must stop receiving input before the locker has
    /// finished painting, or the gap is the attack.
    pub const fn is_locked(&self) -> bool {
        self.lock.is_some()
    }

    /// Whether the lock has finished covering every output.
    pub fn is_confirmed(&self) -> bool {
        self.lock.as_ref().is_some_and(|lock| lock.confirmed)
    }

    /// Whether `session_id` is the client currently driving the lock.
    pub fn is_owner(&self, session_id: SessionId) -> bool {
        self.lock
            .as_ref()
            .is_some_and(|lock| lock.owner == Some(session_id))
    }

    /// Re-evaluate coverage after output hotplug. A newly added output must
    /// immediately make a formerly confirmed lock incomplete; otherwise the
    /// owner could unlock while that display was never covered.
    pub fn outputs_changed(&mut self, outputs: &[OutputId]) -> bool {
        let Some(lock) = self.lock.as_mut() else {
            return false;
        };
        let was_confirmed = lock.confirmed;
        lock.confirmed = lock.covers_all_outputs(outputs);
        was_confirmed != lock.confirmed
    }

    /// Engage the lock, or adopt one whose client died.
    ///
    /// `previous_focus` is remembered only for a fresh lock: adopting an
    /// abandoned lock must not overwrite the focus the original locker saved,
    /// or unlocking would restore whatever happened to be focused at crash
    /// time.
    pub fn engage(
        &mut self,
        session_id: SessionId,
        app_id: AppId,
        previous_focus: Option<(SessionId, SurfaceId)>,
    ) -> Result<LockGrant, String> {
        if let Some(lock) = &mut self.lock {
            if let Some(owner) = lock.owner {
                return Err(format!(
                    "Session is already locked by session {owner} ({})",
                    lock.app_id
                ));
            }
            lock.owner = Some(session_id);
            lock.app_id = app_id;
            tracing::warn!(
                lock_id = lock.id,
                ?session_id,
                "abandoned session lock adopted by a new client"
            );
            return Ok(LockGrant::Adopted(lock.id));
        }

        let id = self.next_lock_id;
        self.next_lock_id = self
            .next_lock_id
            .checked_add(1)
            .ok_or("Session lock ID space exhausted")?;

        self.lock = Some(SessionLock {
            id,
            owner: Some(session_id),
            app_id,
            confirmed: false,
            previous_focus,
            surfaces: HashMap::new(),
            outputs: HashMap::new(),
        });
        Ok(LockGrant::Engaged(id))
    }

    /// Create a lock surface covering `output`.
    ///
    /// `output` must already be normalized to the resolved output id by the
    /// caller, so a client cannot cover one output twice by naming it once
    /// explicitly and once by omission.
    pub fn create_surface(
        &mut self,
        session_id: SessionId,
        lock_id: LockId,
        surface_id: SurfaceId,
        output: Option<OutputId>,
    ) -> Result<LockSurfaceId, String> {
        let next_surface_id = self.next_surface_id;
        let lock = self.lock.as_mut().ok_or("Session is not locked")?;

        if lock.id != lock_id {
            return Err("Lock ID does not match the active session lock".to_string());
        }
        if lock.owner != Some(session_id) {
            return Err("Session does not own the session lock".to_string());
        }
        if lock.outputs.contains_key(&output) {
            return Err("Output already has a lock surface".to_string());
        }

        self.next_surface_id = next_surface_id
            .checked_add(1)
            .ok_or("Lock surface ID space exhausted")?;

        lock.surfaces.insert(
            next_surface_id,
            LockSurface::new(next_surface_id, lock_id, session_id, surface_id, output),
        );
        lock.outputs.insert(output, next_surface_id);
        Ok(next_surface_id)
    }

    /// Mark a surface's configure acknowledged, returning whether the lock
    /// became confirmed as a result.
    pub fn ack_configure(
        &mut self,
        session_id: SessionId,
        lock_surface_id: LockSurfaceId,
        serial: u32,
        outputs: &[OutputId],
    ) -> Result<bool, String> {
        let lock = self.lock.as_mut().ok_or("Session is not locked")?;
        if lock.owner != Some(session_id) {
            return Err("Session does not own the session lock".to_string());
        }

        let surface = lock
            .surfaces
            .get_mut(&lock_surface_id)
            .ok_or("Lock surface not found")?;
        if !surface.ack_configure(serial) {
            return Err("Configure serial is stale or unknown".to_string());
        }

        if lock.confirmed || !lock.covers_all_outputs(outputs) {
            return Ok(false);
        }
        lock.confirmed = true;
        tracing::info!(lock_id = lock.id, "session lock confirmed on every output");
        Ok(true)
    }

    /// Release the lock on the owner's request, returning the focus to restore.
    ///
    /// A lock that never confirmed cannot be released: engaging and immediately
    /// releasing would otherwise be a way to blank the desktop and hand input
    /// back without ever showing an authentication prompt.
    pub fn release(
        &mut self,
        session_id: SessionId,
        lock_id: LockId,
    ) -> Result<Option<(SessionId, SurfaceId)>, String> {
        let lock = self.lock.as_ref().ok_or("Session is not locked")?;
        if lock.id != lock_id {
            return Err("Lock ID does not match the active session lock".to_string());
        }
        if lock.owner != Some(session_id) {
            return Err("Session does not own the session lock".to_string());
        }
        if !lock.confirmed {
            return Err("Session lock has not engaged on every output yet".to_string());
        }

        let previous_focus = lock.previous_focus;
        self.lock = None;
        tracing::info!(lock_id, "session lock released");
        Ok(previous_focus)
    }

    /// Give up the lock's client without unlocking the session.
    ///
    /// Returns whether `session_id` owned the lock. The surfaces are dropped
    /// with their client, but the lock itself stays engaged so the desktop is
    /// not revealed by a crash.
    pub fn abandon(&mut self, session_id: SessionId) -> bool {
        let Some(lock) = &mut self.lock else {
            return false;
        };
        if lock.owner != Some(session_id) {
            return false;
        }

        lock.owner = None;
        lock.confirmed = false;
        lock.surfaces.clear();
        lock.outputs.clear();
        tracing::warn!(
            lock_id = lock.id,
            ?session_id,
            "session lock client disconnected; session stays locked"
        );
        true
    }

    /// Drop one lock surface, keeping the lock engaged.
    pub fn remove_surface(&mut self, session_id: SessionId, surface_id: SurfaceId) -> bool {
        let Some(lock) = &mut self.lock else {
            return false;
        };
        let Some(&lock_surface_id) = lock.surfaces.iter().find_map(|(id, surface)| {
            (surface.session_id == session_id && surface.surface_id == surface_id).then_some(id)
        }) else {
            return false;
        };

        if let Some(surface) = lock.surfaces.remove(&lock_surface_id) {
            lock.outputs.remove(&surface.output);
        }
        // An output is no longer covered, so the lock is no longer fully
        // engaged and must re-confirm before it can be released.
        lock.confirmed = false;
        true
    }

    /// Whether a surface is a lock surface of the active lock.
    ///
    /// Input routing consults this on every keystroke: a focus left over from
    /// before the lock must never be treated as lockscreen focus.
    pub fn is_lock_surface(&self, session_id: SessionId, surface_id: SurfaceId) -> bool {
        self.lock.as_ref().is_some_and(|lock| {
            lock.surfaces
                .values()
                .any(|surface| surface.session_id == session_id && surface.surface_id == surface_id)
        })
    }

    pub fn find_by_surface(
        &self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Option<LockSurfaceId> {
        self.lock.as_ref().and_then(|lock| {
            lock.surfaces.iter().find_map(|(id, surface)| {
                (surface.session_id == session_id && surface.surface_id == surface_id)
                    .then_some(*id)
            })
        })
    }

    /// Lock surfaces of the active lock, in unspecified order.
    pub fn iter_surfaces(&self) -> impl Iterator<Item = &LockSurface> {
        self.lock.iter().flat_map(SessionLock::iter_surfaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::{
        capability::{Capability, Decision},
        security::{LOCK_SERVICE_APP_ID, SecurityCoordinator, StubSecurityCoordinator},
    };

    #[test]
    fn only_the_login_service_may_engage_the_lock() {
        let coordinator = StubSecurityCoordinator::default();

        // The shell is trusted, but it is not the authentication surface: if it
        // could engage the lock it could draw a password prompt of its own.
        for denied in ["sol-shell", "sol-files", "attacker"] {
            assert!(
                matches!(
                    coordinator
                        .evaluate_capability(&AppId(denied.to_string()), &Capability::SessionLock),
                    Decision::Denied { .. }
                ),
                "{denied} must not be granted session-lock"
            );
        }

        assert!(matches!(
            coordinator.evaluate_capability(
                &AppId(LOCK_SERVICE_APP_ID.to_string()),
                &Capability::SessionLock,
            ),
            Decision::Granted { .. }
        ));
    }

    fn app() -> AppId {
        AppId("sol-logind".to_string())
    }

    fn engaged() -> SessionLockManager {
        let mut manager = SessionLockManager::new();
        manager
            .engage(1, app(), Some((9, 42)))
            .expect("lock engages");
        manager
    }

    #[test]
    fn engaging_locks_before_any_surface_exists() {
        let manager = engaged();
        assert!(manager.is_locked());
        assert!(!manager.is_confirmed());
    }

    #[test]
    fn second_client_cannot_take_a_live_lock() {
        let mut manager = engaged();
        let error = manager
            .engage(2, AppId("evil".to_string()), None)
            .expect_err("second lock rejected");
        assert!(error.contains("already locked"));
        assert!(manager.is_owner(1));
    }

    #[test]
    fn one_surface_per_output() {
        let mut manager = engaged();
        manager
            .create_surface(1, 1, 10, Some(0))
            .expect("first surface");
        let error = manager
            .create_surface(1, 1, 11, Some(0))
            .expect_err("duplicate output rejected");
        assert!(error.contains("already has a lock surface"));
    }

    #[test]
    fn non_owner_cannot_create_surfaces() {
        let mut manager = engaged();
        let error = manager
            .create_surface(2, 1, 10, Some(0))
            .expect_err("non-owner rejected");
        assert!(error.contains("does not own"));
    }

    #[test]
    fn confirms_only_once_every_output_is_acked() {
        let mut manager = engaged();
        let outputs = [0, 1];

        let first = manager
            .create_surface(1, 1, 10, Some(0))
            .expect("first surface");
        let second = manager
            .create_surface(1, 1, 11, Some(1))
            .expect("second surface");

        let lock = manager.lock.as_mut().expect("lock exists");
        lock.get_surface_mut(first)
            .expect("surface")
            .configure(100, 1920, 1080);
        lock.get_surface_mut(second)
            .expect("surface")
            .configure(101, 1280, 720);

        assert!(
            !manager
                .ack_configure(1, first, 100, &outputs)
                .expect("first ack"),
            "one output covered is not a locked session"
        );
        assert!(
            manager
                .ack_configure(1, second, 101, &outputs)
                .expect("second ack"),
            "lock confirms once every output is acked"
        );
        assert!(manager.is_confirmed());
    }

    #[test]
    fn stale_configure_serial_is_rejected() {
        let mut manager = engaged();
        let surface = manager.create_surface(1, 1, 10, Some(0)).expect("surface");
        manager
            .lock
            .as_mut()
            .expect("lock")
            .get_surface_mut(surface)
            .expect("surface")
            .configure(7, 1920, 1080);

        let error = manager
            .ack_configure(1, surface, 6, &[0])
            .expect_err("stale serial rejected");
        assert!(error.contains("stale"));
    }

    #[test]
    fn unconfirmed_lock_cannot_be_released() {
        let mut manager = engaged();
        let error = manager.release(1, 1).expect_err("release rejected");
        assert!(error.contains("has not engaged"));
        assert!(manager.is_locked());
    }

    #[test]
    fn release_restores_the_saved_focus() {
        let mut manager = confirmed_single_output();
        let restored = manager.release(1, 1).expect("release succeeds");
        assert_eq!(restored, Some((9, 42)));
        assert!(!manager.is_locked());
    }

    #[test]
    fn non_owner_cannot_release() {
        let mut manager = confirmed_single_output();
        let error = manager.release(2, 1).expect_err("non-owner rejected");
        assert!(error.contains("does not own"));
        assert!(manager.is_locked());
    }

    #[test]
    fn abandoned_lock_keeps_the_session_locked() {
        let mut manager = confirmed_single_output();
        assert!(manager.abandon(1));

        assert!(manager.is_locked(), "a crashed locker must not unlock");
        assert!(!manager.is_confirmed());
        assert!(
            manager.lock().expect("lock").is_abandoned(),
            "lock is waiting to be adopted"
        );
        assert_eq!(manager.iter_surfaces().count(), 0);
    }

    #[test]
    fn abandoned_lock_is_adopted_with_its_original_focus() {
        let mut manager = confirmed_single_output();
        manager.abandon(1);

        let grant = manager
            .engage(5, app(), Some((1, 1)))
            .expect("adoption succeeds");
        assert_eq!(grant, LockGrant::Adopted(1));
        assert!(manager.is_owner(5));
        assert_eq!(
            manager.lock().expect("lock").previous_focus(),
            Some((9, 42)),
            "adoption must not overwrite the focus saved before the lock"
        );
    }

    #[test]
    fn losing_a_surface_unconfirms_the_lock() {
        let mut manager = confirmed_single_output();
        assert!(manager.remove_surface(1, 10));
        assert!(manager.is_locked());
        assert!(
            !manager.is_confirmed(),
            "an uncovered output must re-confirm before unlock"
        );
    }

    #[test]
    fn hotplugged_output_immediately_invalidates_lock_confirmation() {
        let mut manager = confirmed_single_output();
        assert!(manager.outputs_changed(&[0, 1]));
        assert!(manager.is_locked());
        assert!(!manager.is_confirmed());
    }

    #[test]
    fn only_real_lock_surfaces_are_recognized() {
        let manager = confirmed_single_output();
        assert!(manager.is_lock_surface(1, 10));
        assert!(!manager.is_lock_surface(1, 11));
        assert!(!manager.is_lock_surface(2, 10));
    }

    fn confirmed_single_output() -> SessionLockManager {
        let mut manager = engaged();
        let surface = manager.create_surface(1, 1, 10, Some(0)).expect("surface");
        manager
            .lock
            .as_mut()
            .expect("lock")
            .get_surface_mut(surface)
            .expect("surface")
            .configure(1, 1920, 1080);
        manager
            .ack_configure(1, surface, 1, &[0])
            .expect("ack confirms the lock");
        manager
    }
}
