//! Basic output management (Phase 1 M1).
//!
//! The compositor must enumerate and map the available outputs so windows
//! render onto the right screen (PRD §13 / §33 — multi-monitor is a "do not
//! defer" theme). Smithay exposes a [`smithay::output::Output`] global we
//! wrap here; each output is advertised to clients via `wl_output` (and
//! `zxdg_output` from `OutputManagerState`).
//!
//! This module is deliberately small for M1:
//!
//! - one default output (`Output::new` + a 1920×1080 @ 60Hz mode),
//! - tracking in `OutputManagerState` so the `wl_output` global is served,
//! - helpers to read an output's size/scale and to map the window manager's
//!   work area from the first output.
//!
//! Real-hotplug/gbm output wiring (per-monitor modes, position from the DRM
//! backend) lands with the udev backend (ADR-0005) in later milestones; the
//! architecture does not block it.

use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    utils::{Point, Transform},
    wayland::output::{OutputHandler, OutputManagerState},
};

/// The number of logical millimeters a 1920×1080 monitor roughly is.
const DEFAULT_PHYSICAL_MM: (u32, u32) = (530, 300);

/// A single compositor output and its manager state.
pub struct Outputs {
    /// Tracks every `wl_output`/`zxdg_output` client bound object.
    #[allow(dead_code)]
    pub manager: OutputManagerState,
    /// The primary (default) output. M1 advertises one output.
    pub primary: Output,
}

impl Outputs {
    /// Create a fresh output manager and a primary output, and advertise its
    /// global.
    pub fn new<D>(display: &smithay::reexports::wayland_server::DisplayHandle) -> Self
    where
        D: OutputHandler,
        D: smithay::wayland::compositor::CompositorHandler,
        D: smithay::reexports::wayland_server::GlobalDispatch<
                smithay::reexports::wayland_server::protocol::wl_output::WlOutput,
                smithay::wayland::output::WlOutputData,
            >,
        D: smithay::reexports::wayland_server::Dispatch<
                smithay::reexports::wayland_server::protocol::wl_output::WlOutput,
                smithay::wayland::output::OutputUserData,
            >,
        D: 'static,
    {
        let manager = OutputManagerState::new();

        let primary = Output::new(
            "output-0".into(),
            PhysicalProperties {
                size: (DEFAULT_PHYSICAL_MM.0 as i32, DEFAULT_PHYSICAL_MM.1 as i32).into(),
                subpixel: Subpixel::Unknown,
                make: "SOL".into(),
                model: "SOL Virtual Output".into(),
            },
        );

        // Advertise the output to clients and give it a 1080p@60 mode.
        primary.create_global::<D>(display);
        primary.change_current_state(
            Some(Mode {
                size: (1920, 1080).into(),
                refresh: 60_000,
            }),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some(Point::from((0, 0))),
        );
        primary.set_preferred(Mode {
            size: (1920, 1080).into(),
            refresh: 60_000,
        });
        primary.add_mode(Mode {
            size: (800, 600).into(),
            refresh: 60_000,
        });

        Outputs { manager, primary }
    }

    /// The primary output's current mode size, in logical pixels (scale 1.0).
    pub fn primary_size(&self) -> (i32, i32) {
        match self.primary.current_mode() {
            Some(Mode { size, .. }) => (size.w, size.h),
            None => (1920, 1080),
        }
    }

    /// The primary output's current scale.
    #[allow(dead_code)]
    pub fn primary_scale(&self) -> Scale {
        self.primary.current_scale()
    }
}
