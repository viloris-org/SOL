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
    /// Engage the session lock — the greeter and lock screen (reserved for
    /// sol-logind only).
    ///
    /// Strictly stronger than [`Self::LayerShell`]: it grants a surface above
    /// every layer, exclusive input, and immunity from capture. The shell must
    /// not hold it, or it could forge an authentication prompt.
    SessionLock,
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
    ///
    /// Every distinct capability — capture scopes included — needs a distinct
    /// name. The name is what keys `Connected.capability_tokens`, so two
    /// capabilities sharing one would collapse into a single token entry, and it
    /// must round-trip through [`Self::from_wire_name`] for a client to be able
    /// to name back what it was granted.
    pub const fn wire_name(&self) -> &'static str {
        match self {
            Self::WindowToplevel => "window-toplevel",
            Self::WindowPopup => "window-popup",
            Self::ScreenCapture {
                scope: CaptureScope::SingleWindow,
            } => "screen-capture-window",
            Self::ScreenCapture {
                scope: CaptureScope::Output,
            } => "screen-capture-output",
            Self::ScreenCapture {
                scope: CaptureScope::Workspace,
            } => "screen-capture-workspace",
            Self::GlobalShortcuts => "global-shortcuts",
            Self::ClipboardRead => "clipboard-read",
            Self::ClipboardWrite => "clipboard-write",
            Self::DragAndDrop => "drag-and-drop",
            Self::LayerShell => "layer-shell",
            Self::Fullscreen => "fullscreen",
            Self::SessionLock => "session-lock",
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
            "session-lock" => Some(Self::SessionLock),
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

/// Capabilities reserved exclusively for the login/lock service.
///
/// Deliberately disjoint from [`shell_only_capabilities`]: the shell is a
/// trusted component, but it is not the authentication surface, and the two
/// trust boundaries must not collapse into one.
pub fn lock_only_capabilities() -> Vec<Capability> {
    vec![Capability::SessionLock]
}

/// Whether a capability is refused while the session is locked.
///
/// A locked session must not be observable or reachable through side channels,
/// so anything that reads the screen, intercepts keys globally, or moves data
/// in or out is refused for the duration — including for the lock client
/// itself, which needs none of it to authenticate a user.
pub fn blocked_while_locked(cap: &Capability) -> bool {
    matches!(
        cap,
        Capability::ScreenCapture { .. }
            | Capability::GlobalShortcuts
            | Capability::ClipboardRead
            | Capability::ClipboardWrite
            | Capability::DragAndDrop
    )
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
    matches!(
        cap,
        Capability::ClipboardRead | Capability::GlobalShortcuts | Capability::ScreenCapture { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every capability the protocol can express.
    ///
    /// Adding a variant without adding it here makes the tests below stop
    /// covering it, so keep this exhaustive.
    fn every_capability() -> Vec<Capability> {
        vec![
            Capability::WindowToplevel,
            Capability::WindowPopup,
            Capability::ScreenCapture {
                scope: CaptureScope::SingleWindow,
            },
            Capability::ScreenCapture {
                scope: CaptureScope::Output,
            },
            Capability::ScreenCapture {
                scope: CaptureScope::Workspace,
            },
            Capability::GlobalShortcuts,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::DragAndDrop,
            Capability::LayerShell,
            Capability::Fullscreen,
            Capability::SessionLock,
        ]
    }

    #[test]
    fn wire_names_round_trip() {
        for capability in every_capability() {
            let name = capability.wire_name();
            assert_eq!(
                Capability::from_wire_name(name),
                Some(capability.clone()),
                "'{name}' must parse back to the capability it names"
            );
        }
    }

    #[test]
    fn wire_names_are_unique() {
        let capabilities = every_capability();
        let names: HashSet<&str> = capabilities
            .iter()
            .map(|capability| capability.wire_name())
            .collect();
        assert_eq!(
            names.len(),
            capabilities.len(),
            "two capabilities share a wire name, which would collapse them into \
             one entry in Connected.capability_tokens"
        );
    }

    #[test]
    fn unknown_wire_names_are_rejected() {
        for name in [
            "",
            "screen-capture",
            "window",
            "session-lock ",
            "SessionLock",
        ] {
            assert_eq!(
                Capability::from_wire_name(name),
                None,
                "'{name}' must not resolve to a capability"
            );
        }
    }

    #[test]
    fn privileged_capabilities_are_not_granted_by_default() {
        let defaults = default_app_capabilities();
        for reserved in shell_only_capabilities()
            .into_iter()
            .chain(lock_only_capabilities())
        {
            assert!(
                !defaults.contains(&reserved),
                "{reserved:?} must not be in the default set"
            );
        }
    }

    #[test]
    fn the_shell_and_lock_trust_boundaries_stay_disjoint() {
        let shell = shell_only_capabilities();
        for locked in lock_only_capabilities() {
            assert!(
                !shell.contains(&locked),
                "{locked:?} must not be reachable through the shell's capability set"
            );
        }
    }
}
