//! Data device management — clipboard and drag-and-drop.
//!
//! Handles data transfer between surfaces with security restrictions:
//! - Clipboard requires ClipboardWrite capability
//! - Drag-and-drop requires DragAndDrop capability
//! - Both require a recent interaction serial

use crate::scp::protocol::{SessionId, SurfaceId};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const SERIAL_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MIME_TYPES: usize = 32;

/// Ceiling on tracked serials, in case events outpace the timeout sweep.
const MAX_TRACKED_SERIALS: usize = 4096;

/// A serial the compositor issued, and what quoting it back proves.
#[derive(Debug, Clone, Copy)]
struct SerialRecord {
    /// The client the event carrying this serial was delivered to.
    ///
    /// Serials used to be tracked in one global pool, so any client could quote
    /// a serial minted for a *different* client's input. A serial only means
    /// anything as evidence that the user acted on *this* client.
    session: SessionId,
    at: Instant,
    /// Whether it came from a deliberate action — a press, a key, a touch —
    /// rather than from the pointer merely crossing a window.
    interactive: bool,
}

/// Selection source (clipboard owner)
#[derive(Debug, Clone)]
pub struct Selection {
    pub owner: SessionId,
    pub mime_types: Vec<String>,
    pub serial: u32,
    pub timestamp: Instant,
}

/// Active drag-and-drop operation
#[derive(Debug, Clone)]
pub struct DragOperation {
    pub source: SessionId,
    pub origin_surface: SurfaceId,
    pub icon_surface: Option<SurfaceId>,
    pub mime_types: Vec<String>,
    pub serial: u32,
    pub target: Option<SessionId>,
    pub accepted_mime: Option<String>,
}

/// Data device manager
#[derive(Debug)]
pub struct DataDevice {
    /// Current clipboard selection
    selection: Option<Selection>,
    /// Active drag operation
    drag: Option<DragOperation>,
    /// Recently issued serials, and who they were issued to.
    recent_serials: HashMap<u32, SerialRecord>,
    /// Surface the drag is currently over.
    ///
    /// Surface IDs are client-local, so the owning session is part of the
    /// identity: without it, two clients that both use surface ID 1 would look
    /// like the same drop target.
    drag_surface: Option<(SessionId, SurfaceId)>,
}

impl DataDevice {
    pub fn new() -> Self {
        Self {
            selection: None,
            drag: None,
            recent_serials: HashMap::new(),
            drag_surface: None,
        }
    }

    /// Record a serial the compositor just delivered to a client.
    ///
    /// `interactive` distinguishes a deliberate action from the pointer crossing
    /// a window: only the former may authorize taking over the clipboard or
    /// starting a drag, or moving the cursor across a window would be enough.
    pub fn record_serial(&mut self, serial: u32, session: SessionId, interactive: bool) {
        self.recent_serials.insert(
            serial,
            SerialRecord {
                session,
                at: Instant::now(),
                interactive,
            },
        );
        self.cleanup_old_serials();
    }

    /// Set clipboard selection with serial validation
    pub fn set_selection_validated(
        &mut self,
        owner: SessionId,
        mime_types: Vec<String>,
        serial: u32,
    ) -> Result<(), &'static str> {
        if mime_types.len() > MAX_MIME_TYPES {
            return Err("Too many MIME types");
        }

        if !self.is_serial_valid(serial, owner) {
            return Err("Serial is stale, invalid, or was issued to another client");
        }

        self.selection = Some(Selection {
            owner,
            mime_types,
            serial,
            timestamp: Instant::now(),
        });

        Ok(())
    }

    /// Get current selection (returns owner session and mime types)
    pub fn get_selection(&self) -> Option<(SessionId, Vec<String>)> {
        self.selection
            .as_ref()
            .map(|s| (s.owner, s.mime_types.clone()))
    }

    /// Get full selection details
    pub fn get_selection_full(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Start drag operation with full validation
    pub fn start_drag_validated(
        &mut self,
        source: SessionId,
        origin_surface: SurfaceId,
        icon_surface: Option<SurfaceId>,
        mime_types: Vec<String>,
        serial: u32,
    ) -> Result<(), &'static str> {
        if self.drag.is_some() {
            return Err("Drag already in progress");
        }

        if mime_types.len() > MAX_MIME_TYPES {
            return Err("Too many MIME types");
        }

        if !self.is_serial_valid(serial, source) {
            return Err("Serial is stale, invalid, or was issued to another client");
        }

        self.drag = Some(DragOperation {
            source,
            origin_surface,
            icon_surface,
            mime_types,
            serial,
            target: None,
            accepted_mime: None,
        });

        Ok(())
    }

    /// Get active drag (returns source session, surface, and mime types)
    pub fn get_drag(&self) -> Option<(SessionId, SurfaceId, Vec<String>)> {
        self.drag
            .as_ref()
            .map(|d| (d.source, d.origin_surface, d.mime_types.clone()))
    }

    /// Set the surface the drag is currently over.
    pub fn set_drag_surface(&mut self, surface: Option<(SessionId, SurfaceId)>) {
        self.drag_surface = surface;
    }

    /// Check if drag is over a specific surface
    pub fn is_drag_over_surface(&self, surface: (SessionId, SurfaceId)) -> bool {
        self.drag_surface == Some(surface)
    }

    /// Get the surface the drag is currently over.
    pub fn drag_surface(&self) -> Option<(SessionId, SurfaceId)> {
        self.drag_surface
    }

    /// Clear drag operation
    pub fn clear_drag(&mut self) {
        self.drag = None;
        self.drag_surface = None;
    }

    /// Accept drag with specific MIME type
    pub fn accept_drag(&mut self, mime_type: Option<String>) -> Result<(), &'static str> {
        let drag = self.drag.as_mut().ok_or("No active drag")?;
        drag.accepted_mime = mime_type;
        Ok(())
    }

    /// Finish drag operation successfully
    pub fn finish_drag(&mut self) -> Result<(), &'static str> {
        if self.drag.is_none() {
            return Err("No active drag");
        }
        self.drag = None;
        Ok(())
    }

    /// Cancel drag operation
    pub fn cancel_drag(&mut self) -> Result<(), &'static str> {
        if self.drag.is_none() {
            return Err("No active drag");
        }
        self.drag = None;
        Ok(())
    }

    /// Get active drag
    pub fn active_drag(&self) -> Option<&DragOperation> {
        self.drag.as_ref()
    }

    /// Update drag target surface
    pub fn set_drag_target(&mut self, target: Option<SessionId>) {
        if let Some(drag) = &mut self.drag {
            drag.target = target;
        }
    }

    /// Whether `serial` is one the compositor recently delivered to `session`.
    ///
    /// Enough to prove the client is answering a real event — which is what a
    /// cursor change needs — but not that the user did anything deliberate.
    pub fn is_serial_known(&self, serial: u32, session: SessionId) -> bool {
        self.recent_serials
            .get(&serial)
            .is_some_and(|record| record.session == session && record.at.elapsed() < SERIAL_TIMEOUT)
    }

    /// Whether `serial` proves the user deliberately acted on `session`.
    fn is_serial_valid(&self, serial: u32, session: SessionId) -> bool {
        self.recent_serials.get(&serial).is_some_and(|record| {
            record.interactive && record.session == session && record.at.elapsed() < SERIAL_TIMEOUT
        })
    }

    /// Clean up old serials
    fn cleanup_old_serials(&mut self) {
        let now = Instant::now();
        self.recent_serials
            .retain(|_, record| now.duration_since(record.at) < SERIAL_TIMEOUT);

        // The sweep above is normally enough; this only matters if events arrive
        // faster than they age out. Dropping the oldest costs a client at worst
        // one refused clipboard write, which it can retry.
        while self.recent_serials.len() > MAX_TRACKED_SERIALS {
            let Some(oldest) = self
                .recent_serials
                .iter()
                .min_by_key(|(_, record)| record.at)
                .map(|(serial, _)| *serial)
            else {
                break;
            };
            self.recent_serials.remove(&oldest);
        }
    }
}

impl Default for DataDevice {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate MIME type string.
///
/// Accepts `type/subtype` with optional parameters, so a client can offer
/// `text/plain;charset=utf-8` — the form text is most often published in.
/// Rejecting it left clients unable to say what encoding their clipboard content
/// was in.
pub fn is_valid_mime_type(mime: &str) -> bool {
    if mime.is_empty() || mime.len() > 256 {
        return false;
    }

    // Parameters are validated only for shape: they are opaque to the
    // compositor, which never interprets clipboard content.
    let (essence, parameters) = mime
        .split_once(';')
        .map_or((mime, None), |(essence, rest)| (essence, Some(rest)));

    let Some((type_part, subtype)) = essence.split_once('/') else {
        return false;
    };
    if subtype.contains('/') {
        return false;
    }

    let is_token = |value: &str| {
        !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '.' | '_'))
    };

    if !is_token(type_part) || !is_token(subtype) {
        return false;
    }

    parameters.is_none_or(|parameters| {
        parameters.split(';').all(|parameter| {
            parameter
                .split_once('=')
                .is_some_and(|(name, value)| is_token(name.trim()) && is_token(value.trim()))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_validation() {
        assert!(is_valid_mime_type("text/plain"));
        assert!(is_valid_mime_type("image/png"));
        assert!(is_valid_mime_type("application/json"));
        assert!(is_valid_mime_type("text/html"));

        assert!(!is_valid_mime_type(""));
        assert!(!is_valid_mime_type("text"));
        assert!(!is_valid_mime_type("text/"));
        assert!(!is_valid_mime_type("/plain"));
        assert!(!is_valid_mime_type("text/plain/extra"));
        assert!(!is_valid_mime_type("text plain"));
    }

    #[test]
    fn accepts_the_parameters_text_is_actually_published_with() {
        assert!(is_valid_mime_type("text/plain;charset=utf-8"));
        assert!(is_valid_mime_type("text/html;charset=utf-8"));
        assert!(is_valid_mime_type("application/json; charset = utf-8"));

        // Shape is still checked: a parameter has to be name=value.
        assert!(!is_valid_mime_type("text/plain;charset"));
        assert!(!is_valid_mime_type("text/plain;=utf-8"));
        assert!(!is_valid_mime_type("text/plain;charset="));
    }

    #[test]
    fn a_serial_issued_to_one_client_does_not_authorize_another() {
        let mut device = DataDevice::new();
        // The user clicked in session 1's window.
        device.record_serial(7, 1, true);

        assert!(
            device
                .set_selection_validated(2, vec!["text/plain".to_string()], 7)
                .is_err(),
            "session 2 must not borrow session 1's proof that the user acted"
        );
        device
            .set_selection_validated(1, vec!["text/plain".to_string()], 7)
            .expect("the client the serial was issued to may use it");
    }

    #[test]
    fn passive_pointer_motion_does_not_authorize_a_clipboard_write() {
        let mut device = DataDevice::new();
        // A pointer enter carries a serial, but crossing a window is not the
        // user asking for anything.
        device.record_serial(7, 1, false);

        assert!(
            device
                .set_selection_validated(1, vec!["text/plain".to_string()], 7)
                .is_err(),
            "only a deliberate action authorizes taking over the clipboard"
        );
        assert!(
            device.is_serial_known(7, 1),
            "the serial is still quotable for things like a cursor change"
        );
    }

    #[test]
    fn test_selection() {
        let mut device = DataDevice::new();
        device.record_serial(1, 1, true);

        device
            .set_selection_validated(1, vec!["text/plain".to_string()], 1)
            .expect("set selection");
        assert_eq!(
            device.get_selection().expect("selection is set").0,
            1,
            "the setting session owns the selection"
        );

        device.clear_selection();
        assert!(device.get_selection().is_none());
    }

    #[test]
    fn test_drag() {
        let mut device = DataDevice::new();
        device.record_serial(1, 1, true);

        device
            .start_drag_validated(1, 100, None, vec!["text/plain".to_string()], 1)
            .expect("start drag");
        assert!(device.active_drag().is_some());

        device.finish_drag().expect("finish drag");
        assert!(device.active_drag().is_none());
    }

    #[test]
    fn drag_target_identity_includes_the_session() {
        let mut device = DataDevice::new();
        device.record_serial(1, 1, true);
        device
            .start_drag_validated(1, 100, None, vec!["text/plain".to_string()], 1)
            .expect("start drag");

        device.set_drag_surface(Some((2, 1)));

        assert!(device.is_drag_over_surface((2, 1)));
        // Same client-local surface ID, different client: not the same target.
        assert!(!device.is_drag_over_surface((3, 1)));
    }

    #[test]
    fn rejects_a_second_concurrent_drag() {
        let mut device = DataDevice::new();
        device.record_serial(1, 1, true);
        device
            .start_drag_validated(1, 100, None, vec!["text/plain".to_string()], 1)
            .expect("start first drag");

        assert!(
            device
                .start_drag_validated(2, 200, None, vec!["text/plain".to_string()], 1)
                .is_err(),
            "a drag already in progress must block another"
        );
    }

    #[test]
    fn test_validated_operations() {
        let mut device = DataDevice::new();
        let serial = 1;
        device.record_serial(serial, 1, true);

        // Valid serial should work
        assert!(
            device
                .set_selection_validated(1, vec!["text/plain".to_string()], serial)
                .is_ok()
        );

        // Invalid serial should fail
        assert!(
            device
                .set_selection_validated(2, vec!["text/plain".to_string()], 999)
                .is_err()
        );
    }
}
