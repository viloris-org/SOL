//! Renderer-neutral SOL Shell models.
//!
//! The native `sol-shell` binary owns SCP layer surfaces. These modules
//! remain independently testable so service adapters can be validated without
//! duplicating shell policy in transport-specific code.

pub mod bluez;
pub mod consent;
pub mod launcher;
pub mod networkmanager;
pub mod notification_center;
pub mod notification_surface;
pub mod overlay;
pub mod overview;
pub mod overview_surface;
pub mod pipewire_audio;
pub mod quick_settings;
pub mod topbar;
pub mod upower;
