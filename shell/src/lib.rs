//! Renderer-neutral SOL Shell models.
//!
//! The native `sol-shell` binary owns Wayland layer surfaces. These modules
//! remain independently testable so service adapters can be validated without
//! duplicating shell policy in transport-specific code.

pub mod consent;
pub mod launcher;
pub mod notification_center;
pub mod overlay;
pub mod overview;
pub mod quick_settings;
pub mod topbar;
