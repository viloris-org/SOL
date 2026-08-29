//! Renderer-neutral SOL Shell models.
//!
//! The native `sol-shell` binary owns SCP layer surfaces. These modules
//! remain independently testable so service adapters can be validated without
//! duplicating shell policy in transport-specific code.

pub mod bluez;
pub mod catalog;
pub mod consent;
pub mod desktop;
pub mod desktop_surface;
pub mod dock_surface;
pub mod launcher;
pub mod launcher_surface;
pub mod networkmanager;
pub mod notification_center;
pub mod notification_surface;
pub mod overlay;
pub mod overview;
pub mod overview_surface;
pub mod paint;
pub mod pipewire_audio;
pub mod quick_settings;
pub mod scp_host;
pub mod system_status;
pub mod topbar;
pub mod topbar_surface;
pub mod upower;
