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
    /// Recently valid serials (for validation)
    recent_serials: HashMap<u32, Instant>,
    /// Current drag target surface (if any)
    drag_surface: Option<SurfaceId>,
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

    /// Record an input serial for later validation
    pub fn record_serial(&mut self, serial: u32) {
        self.recent_serials.insert(serial, Instant::now());
        self.cleanup_old_serials();
    }

    /// Set clipboard selection (simplified version without serial validation)
    pub fn set_selection(&mut self, owner: SessionId, mime_types: Vec<String>) {
        self.selection = Some(Selection {
            owner,
            mime_types,
            serial: 0,
            timestamp: Instant::now(),
        });
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

        if !self.is_serial_valid(serial) {
            return Err("Serial is stale or invalid");
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
        self.selection.as_ref().map(|s| (s.owner, s.mime_types.clone()))
    }

    /// Get full selection details
    pub fn get_selection_full(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Start drag operation (simplified version)
    pub fn start_drag(
        &mut self,
        source: SessionId,
        origin_surface: SurfaceId,
        mime_types: Vec<String>,
    ) {
        self.drag = Some(DragOperation {
            source,
            origin_surface,
            icon_surface: None,
            mime_types,
            serial: 0,
            target: None,
            accepted_mime: None,
        });
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

        if !self.is_serial_valid(serial) {
            return Err("Serial is stale or invalid");
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
        self.drag.as_ref().map(|d| (d.source, d.origin_surface, d.mime_types.clone()))
    }

    /// Set drag target surface
    pub fn set_drag_surface(&mut self, surface: SurfaceId) {
        self.drag_surface = Some(surface);
    }

    /// Check if drag is over a specific surface
    pub fn is_drag_over_surface(&self, surface: SurfaceId) -> bool {
        self.drag_surface == Some(surface)
    }

    /// Get current drag target surface
    pub fn drag_surface(&self) -> Option<SurfaceId> {
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

    /// Check if serial is recent and valid
    fn is_serial_valid(&self, serial: u32) -> bool {
        self.recent_serials
            .get(&serial)
            .is_some_and(|t| t.elapsed() < SERIAL_TIMEOUT)
    }

    /// Clean up old serials
    fn cleanup_old_serials(&mut self) {
        let now = Instant::now();
        self.recent_serials
            .retain(|_, timestamp| now.duration_since(*timestamp) < SERIAL_TIMEOUT);
    }
}

impl Default for DataDevice {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate MIME type string
pub fn is_valid_mime_type(mime: &str) -> bool {
    // Basic validation: type/subtype format
    if mime.is_empty() || mime.len() > 256 {
        return false;
    }

    let parts: Vec<&str> = mime.split('/').collect();
    if parts.len() != 2 {
        return false;
    }

    let [type_part, subtype] = [parts[0], parts[1]];

    // Type and subtype must be non-empty and contain valid characters
    !type_part.is_empty()
        && !subtype.is_empty()
        && type_part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '.')
        && subtype
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '.')
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
    fn test_selection() {
        let mut device = DataDevice::new();

        device.set_selection(1, vec!["text/plain".to_string()]);
        assert!(device.get_selection().is_some());
        assert_eq!(device.get_selection().unwrap().0, 1);

        device.clear_selection();
        assert!(device.get_selection().is_none());
    }

    #[test]
    fn test_drag() {
        let mut device = DataDevice::new();

        device.start_drag(1, 100, vec!["text/plain".to_string()]);
        assert!(device.active_drag().is_some());

        device.finish_drag();
        assert!(device.active_drag().is_none());
    }

    #[test]
    fn test_validated_operations() {
        let mut device = DataDevice::new();
        let serial = 1;
        device.record_serial(serial);

        // Valid serial should work
        assert!(device
            .set_selection_validated(1, vec!["text/plain".to_string()], serial)
            .is_ok());

        // Invalid serial should fail
        assert!(device
            .set_selection_validated(2, vec!["text/plain".to_string()], 999)
            .is_err());
    }
}
