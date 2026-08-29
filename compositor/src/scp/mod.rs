//! SOL Compositor Protocol (SCP) — capability-based minimal-privilege compositor.
//!
//! This module implements the native SOL compositor protocol, designed to
//! replace Wayland with a security-first approach where every sensitive
//! capability requires explicit authorization from `sol-securityd`.
//!
//! ## Architecture
//!
//! ```text
//! App → sol-runtime ABI → SCP Core → Capability Check → sol-securityd
//!                                          ↓
//!                                    ScpState (compositor)
//! ```
//!
//! ## Key differences from Wayland
//!
//! 1. **Identity-first**: Every connection is authenticated (PID → AppId)
//! 2. **Capability tokens**: Sensitive operations require signed tokens
//! 3. **No client decorations**: Compositor owns all chrome (anti-phishing)
//! 4. **Explicit grants**: Screen capture, shortcuts, clipboard need approval
//! 5. **Audit trail**: All capability use logged to `sol-securityd`
//!
//! ## Permission System Components
//!
//! - [`manifest`]: TOML-based app manifest parsing
//! - [`audit`]: Persistent audit logging of capability use
//! - [`revocation`]: Runtime capability revocation registry
//! - [`permission_manager`]: Unified permission management

pub mod audit;
pub mod buffer;
pub mod capability;
pub mod compose;
pub mod data_device;
pub mod event_queue;
pub mod input;
pub mod keymap;
pub mod layer;
pub mod manifest;
pub mod memfd;
pub mod output;
pub mod permission_manager;
pub mod popup;
pub mod protocol;
pub mod random;
pub mod revocation;
pub mod security;
pub mod session_lock;
pub mod stack;
pub mod state;
pub mod surface;
pub mod toml_parser;
pub mod transport;
pub mod unix_socket;

pub use compose::{CAPTURE_REDACTION, Framebuffer, RenderPurpose, Rgba8};
pub use event_queue::{EventRouter, OutboundEvent, SessionSink};
pub use permission_manager::PermissionManager;
pub use session_lock::{LockSurface, SessionLock, SessionLockManager};
pub use stack::{StackEntry, StackKind, WindowStack};
pub use state::ScpState;
pub use surface::{CapturePolicy, Layer, LayerSurface, ProtectionReason};
pub use transport::{ScpServer, resolve_socket_path};
