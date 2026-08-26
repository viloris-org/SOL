//! Capability definitions and validation logic.

use std::time::{Duration, Instant};

/// A compositor capability that requires authorization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Create toplevel windows (granted to all non-shell apps by default)
    WindowToplevel,
    /// Create popup windows (requires parent window)
    WindowPopup,
    /// Capture screen content (requires user consent per capture)
    ScreenCapture { scope: CaptureScope },
    /// Register global keyboard shortcuts
    GlobalShortcuts,
    /// Read clipboard content (requires foreground focus)
    ClipboardRead,
    /// Write to clipboard (requires recent user interaction)
    ClipboardWrite,
    /// Initiate drag-and-drop operations
    DragAndDrop,
    /// Access layer-shell (reserved for sol-shell only)
    LayerShell,
    /// Fullscreen mode
    Fullscreen,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CaptureScope {
    SingleWindow,
    Output,
    Workspace,
}

/// Authorization decision from sol-securityd.
#[derive(Debug, Clone)]
pub enum Decision {
    /// Capability granted immediately
    Granted {
        token: CapabilityToken,
        expires_at: Option<Instant>,
    },
    /// Denied by policy
    Denied { reason: String },
    /// Requires user consent via Shell dialog
    NeedsUserConsent { dialog_id: u64 },
}

/// A signed token proving capability authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityToken {
    /// Opaque token data (HMAC-signed by sol-securityd)
    pub data: Vec<u8>,
    /// When this token expires
    pub expires_at: Option<Instant>,
    /// Single-use token (consumed on first use)
    pub one_time: bool,
}

impl CapabilityToken {
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Instant::now() >= exp)
    }
}

/// Active capability grant tracked by the compositor.
#[derive(Debug, Clone)]
pub struct CapabilityGrant {
    pub capability: Capability,
    pub token: CapabilityToken,
    pub granted_at: Instant,
    pub expires_at: Option<Instant>,
    pub use_count: u64,
}

impl Capability {
    /// Stable protocol spelling. Debug output is not a wire format.
    pub const fn wire_name(&self) -> &'static str {
        match self {
            Self::WindowToplevel => "window-toplevel",
            Self::WindowPopup => "window-popup",
            Self::ScreenCapture { .. } => "screen-capture",
            Self::GlobalShortcuts => "global-shortcuts",
            Self::ClipboardRead => "clipboard-read",
            Self::ClipboardWrite => "clipboard-write",
            Self::DragAndDrop => "drag-and-drop",
            Self::LayerShell => "layer-shell",
            Self::Fullscreen => "fullscreen",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        match name {
            "window-toplevel" => Some(Self::WindowToplevel),
            "window-popup" => Some(Self::WindowPopup),
            "screen-capture-window" => Some(Self::ScreenCapture {
                scope: CaptureScope::SingleWindow,
            }),
            "screen-capture-output" => Some(Self::ScreenCapture {
                scope: CaptureScope::Output,
            }),
            "screen-capture-workspace" => Some(Self::ScreenCapture {
                scope: CaptureScope::Workspace,
            }),
            "global-shortcuts" => Some(Self::GlobalShortcuts),
            "clipboard-read" => Some(Self::ClipboardRead),
            "clipboard-write" => Some(Self::ClipboardWrite),
            "drag-and-drop" => Some(Self::DragAndDrop),
            "layer-shell" => Some(Self::LayerShell),
            "fullscreen" => Some(Self::Fullscreen),
            _ => None,
        }
    }
}

impl CapabilityGrant {
    pub fn is_valid(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Instant::now() < expires_at
        } else {
            true
        }
    }
}

/// Default capability set for normal applications.
pub fn default_app_capabilities() -> Vec<Capability> {
    vec![
        Capability::WindowToplevel,
        Capability::WindowPopup,
        Capability::ClipboardWrite,
        Capability::DragAndDrop,
    ]
}

/// Capabilities reserved exclusively for sol-shell.
pub fn shell_only_capabilities() -> Vec<Capability> {
    vec![Capability::LayerShell]
}

/// Check if a capability requires user interaction recency.
pub fn requires_recent_interaction(cap: &Capability) -> Option<Duration> {
    match cap {
        Capability::ClipboardWrite => Some(Duration::from_millis(500)),
        Capability::DragAndDrop => Some(Duration::from_millis(200)),
        _ => None,
    }
}

/// Check if a capability requires foreground focus.
pub fn requires_foreground(cap: &Capability) -> bool {
    matches!(cap, Capability::ClipboardRead | Capability::GlobalShortcuts)
}
