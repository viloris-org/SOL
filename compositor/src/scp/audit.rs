//! Audit logging for capability use and security events.
//!
//! All sensitive operations (screen capture, clipboard access, file operations)
//! are logged with timestamp, app identity, and outcome. Logs are persisted
//! to disk and queryable via sol-settings UI.

use crate::scp::{capability::Capability, security::AppId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Audit log recorder.
#[derive(Clone)]
pub struct AuditLog {
    inner: Arc<RwLock<AuditState>>,
}

struct AuditState {
    /// In-memory event buffer
    events: VecDeque<AuditEvent>,
    /// Maximum in-memory events (oldest dropped)
    max_size: usize,
    /// Persistent log file
    log_file: Option<BufWriter<File>>,
    /// Path to log file
    log_path: Option<PathBuf>,
}

/// A single audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unix timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Application that performed the action
    pub app_id: String,
    /// Type of event
    pub event_type: AuditEventType,
    /// Whether the operation succeeded
    pub success: bool,
    /// Additional context
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AuditEventType {
    /// Capability requested by an app
    CapabilityRequested {
        capability: String,
        justification: String,
    },
    /// Capability granted
    CapabilityGranted {
        capability: String,
        grant_type: String,
    },
    /// Capability denied
    CapabilityDenied { capability: String, reason: String },
    /// Capability revoked by user
    CapabilityRevoked { capability: String },
    /// Capability used (e.g., screenshot taken)
    CapabilityUsed { capability: String },
    /// File accessed
    FileAccessed {
        path: String,
        mode: String, // "read", "write", "delete"
    },
    /// Portal used
    PortalUsed {
        portal_type: String, // "file_open", "file_save", "screenshot"
    },
    /// Network connection attempted
    NetworkConnection { address: String },
    /// Device accessed
    DeviceAccessed {
        device_type: String, // "camera", "microphone", "location"
    },
}

impl AuditLog {
    /// Create a new audit log with in-memory buffering.
    pub fn new(max_size: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(AuditState {
                events: VecDeque::with_capacity(max_size),
                max_size,
                log_file: None,
                log_path: None,
            })),
        }
    }

    /// Create an audit log with persistent file storage.
    pub fn with_file<P: AsRef<Path>>(max_size: usize, path: P) -> Result<Self, std::io::Error> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        Ok(Self {
            inner: Arc::new(RwLock::new(AuditState {
                events: VecDeque::with_capacity(max_size),
                max_size,
                log_file: Some(BufWriter::new(file)),
                log_path: Some(path),
            })),
        })
    }

    /// Record an audit event.
    pub fn record(&self, event: AuditEvent) {
        if let Ok(mut state) = self.inner.write() {
            // Add to in-memory buffer
            state.events.push_back(event.clone());

            // Maintain size limit
            while state.events.len() > state.max_size {
                state.events.pop_front();
            }

            // Persist to disk
            if let Some(file) = &mut state.log_file
                && let Ok(json) = serde_json::to_string(&event)
            {
                let _ = writeln!(file, "{}", json);
                let _ = file.flush();
            }
        }
    }

    /// Record a capability request.
    pub fn log_capability_requested(
        &self,
        app_id: &AppId,
        capability: &Capability,
        justification: &str,
    ) {
        self.record(AuditEvent {
            timestamp_ms: unix_timestamp_ms(),
            app_id: app_id.0.clone(),
            event_type: AuditEventType::CapabilityRequested {
                capability: capability.wire_name().to_string(),
                justification: justification.to_string(),
            },
            success: true,
            details: None,
        });
    }

    /// Record a capability grant.
    pub fn log_capability_granted(
        &self,
        app_id: &AppId,
        capability: &Capability,
        grant_type: &str,
    ) {
        self.record(AuditEvent {
            timestamp_ms: unix_timestamp_ms(),
            app_id: app_id.0.clone(),
            event_type: AuditEventType::CapabilityGranted {
                capability: capability.wire_name().to_string(),
                grant_type: grant_type.to_string(),
            },
            success: true,
            details: None,
        });
    }

    /// Record a capability denial.
    pub fn log_capability_denied(&self, app_id: &AppId, capability: &Capability, reason: &str) {
        self.record(AuditEvent {
            timestamp_ms: unix_timestamp_ms(),
            app_id: app_id.0.clone(),
            event_type: AuditEventType::CapabilityDenied {
                capability: capability.wire_name().to_string(),
                reason: reason.to_string(),
            },
            success: false,
            details: None,
        });
    }

    /// Record a capability use.
    pub fn log_capability_used(
        &self,
        app_id: &AppId,
        capability: &Capability,
        details: Option<String>,
    ) {
        self.record(AuditEvent {
            timestamp_ms: unix_timestamp_ms(),
            app_id: app_id.0.clone(),
            event_type: AuditEventType::CapabilityUsed {
                capability: capability.wire_name().to_string(),
            },
            success: true,
            details,
        });
    }

    /// Record a capability revocation.
    pub fn log_capability_revoked(&self, app_id: &AppId, capability: &Capability) {
        self.record(AuditEvent {
            timestamp_ms: unix_timestamp_ms(),
            app_id: app_id.0.clone(),
            event_type: AuditEventType::CapabilityRevoked {
                capability: capability.wire_name().to_string(),
            },
            success: true,
            details: Some("Revoked by user".to_string()),
        });
    }

    /// Query recent events for an app.
    pub fn query_app_events(&self, app_id: &AppId, limit: usize) -> Vec<AuditEvent> {
        if let Ok(state) = self.inner.read() {
            state
                .events
                .iter()
                .rev()
                .filter(|event| event.app_id == app_id.0)
                .take(limit)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Query all recent events.
    pub fn query_recent(&self, limit: usize) -> Vec<AuditEvent> {
        if let Ok(state) = self.inner.read() {
            state.events.iter().rev().take(limit).cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Count events by type for an app.
    pub fn count_events(&self, app_id: &AppId) -> EventCounts {
        if let Ok(state) = self.inner.read() {
            let mut counts = EventCounts::default();
            for event in state.events.iter() {
                if event.app_id == app_id.0 {
                    match &event.event_type {
                        AuditEventType::CapabilityRequested { .. } => counts.requested += 1,
                        AuditEventType::CapabilityGranted { .. } => counts.granted += 1,
                        AuditEventType::CapabilityDenied { .. } => counts.denied += 1,
                        AuditEventType::CapabilityRevoked { .. } => counts.revoked += 1,
                        AuditEventType::CapabilityUsed { .. } => counts.used += 1,
                        _ => {}
                    }
                }
            }
            counts
        } else {
            EventCounts::default()
        }
    }

    /// Get the log file path if configured.
    pub fn log_path(&self) -> Option<PathBuf> {
        self.inner.read().ok()?.log_path.clone()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EventCounts {
    pub requested: usize,
    pub granted: usize,
    pub denied: usize,
    pub revoked: usize,
    pub used: usize,
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_queries_events() {
        let log = AuditLog::new(100);
        let app = AppId("test-app".to_string());
        let cap = Capability::ClipboardRead;

        log.log_capability_requested(&app, &cap, "Testing");
        log.log_capability_granted(&app, &cap, "permanent");
        log.log_capability_used(&app, &cap, None);

        let events = log.query_app_events(&app, 10);
        assert_eq!(events.len(), 3);

        let counts = log.count_events(&app);
        assert_eq!(counts.requested, 1);
        assert_eq!(counts.granted, 1);
        assert_eq!(counts.used, 1);
    }

    #[test]
    fn respects_max_size() {
        let log = AuditLog::new(5);
        let app = AppId("test-app".to_string());
        let cap = Capability::WindowToplevel;

        for _ in 0..10 {
            log.log_capability_used(&app, &cap, None);
        }

        let events = log.query_recent(100);
        assert_eq!(events.len(), 5);
    }
}
