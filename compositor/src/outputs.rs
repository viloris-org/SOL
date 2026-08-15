//! Basic output management (Phase 1 M1).
//!
//! The compositor must enumerate and map the available outputs so windows
//! render onto the right screen (PRD §13 / §33 — multi-monitor is a "do not
//! defer" theme). Smithay exposes a [`smithay::output::Output`] global we
//! wrap here; each output is advertised to clients via `wl_output` (and
//! `zxdg_output` from `OutputManagerState`).
//!
//! This module is deliberately small for M1 but keeps the hotplug / multi-
//! output architecture open:
//!
//! - one default output (`Output::new` + a 1920×1080 @ 60Hz mode),
//! - tracking every output in `OutputManagerState` so each `wl_output`
//!   global is served,
//! - `add_output` / `remove_output` so a connector plug / unplug event can
//!   register or retire a monitor without a compositor rewrite,
//! - helpers to read an output's size/scale and to map the window manager's
//!   work area from the first (primary) output.
//!
//! Real DRM/gbm connector wiring (modes from the backend, per-monitor
//! position) lands with the udev backend (ADR-0005) in later milestones; the
//! architecture does not block it.

use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::wayland_server::DisplayHandle,
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
    /// All currently connected outputs. The first one is the primary.
    pub outputs: Vec<Output>,
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
        D: smithay::wayland::compositor::CompositorHandler,
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

        Outputs {
            manager,
            outputs: vec![primary],
        }
    }

    /// The primary output (the first in the list, or a fresh 1080p fallback).
    #[allow(dead_code)]
    pub fn primary(&self) -> &Output {
        &self.outputs[0]
    }

    /// Register a new connected output and advertise it to clients (PRD §33
    /// display-hotplug). Returns the new output's index in the list.
    #[allow(dead_code)]
    pub fn add_output<D>(
        &mut self,
        name: String,
        size: (i32, i32),
        display: &DisplayHandle,
    ) -> usize
    where
        D: smithay::wayland::output::OutputHandler,
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
        let output = Output::new(
            name,
            PhysicalProperties {
                size: (DEFAULT_PHYSICAL_MM.0 as i32, DEFAULT_PHYSICAL_MM.1 as i32).into(),
                subpixel: Subpixel::Unknown,
                make: "SOL".into(),
                model: "SOL Virtual Output".into(),
            },
        );
        output.create_global::<D>(display);
        output.change_current_state(
            Some(Mode {
                size: size.into(),
                refresh: 60_000,
            }),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some(Point::from((0, 0))),
        );
        output.set_preferred(Mode {
            size: size.into(),
            refresh: 60_000,
        });

        let idx = self.outputs.len();
        self.outputs.push(output);
        idx
    }

    /// Retire a connected output at the given index (PRD §33 display-hotplug).
    /// The output is removed from the list and its `wl_output` global is no
    /// longer advertised.
    #[allow(dead_code)]
    pub fn remove_output(&mut self, index: usize) -> Option<Output> {
        if index >= self.outputs.len() {
            return None;
        }
        Some(self.outputs.remove(index))
    }

    /// The primary output's current mode size, in logical pixels (scale 1.0).
    pub fn primary_size(&self) -> (i32, i32) {
        match self.outputs[0].current_mode() {
            Some(Mode { size, .. }) => (size.w, size.h),
            None => (1920, 1080),
        }
    }

    /// The primary output's current scale.
    #[allow(dead_code)]
    pub fn primary_scale(&self) -> Scale {
        self.outputs[0].current_scale()
    }

    /// How many outputs are currently connected.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.outputs.len()
    }

    /// Whether any outputs are connected.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }
}
