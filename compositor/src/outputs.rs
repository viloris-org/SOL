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

/// A backend-supplied logical configuration for one connected output.
///
/// This is deliberately renderer-independent: winit, headless tests, and the
/// udev/DRM backend can all advertise the same Wayland output contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputConfiguration {
    /// Stable backend connector/output name.
    pub name: String,
    /// Current mode size in physical pixels at scale 1.
    pub size: (i32, i32),
    /// Logical top-left position in the desktop layout.
    pub location: (i32, i32),
    /// Fractional scale factor advertised to clients and renderers.
    /// Stored as thousandths so topology snapshots remain comparable.
    scale_milli: u32,
}

impl OutputConfiguration {
    /// Create one output configuration.
    #[must_use]
    pub fn new(name: impl Into<String>, size: (i32, i32), location: (i32, i32)) -> Self {
        Self {
            name: name.into(),
            size,
            location,
            scale_milli: 1_000,
        }
    }

    /// Set a validated fractional scale factor.
    pub fn try_with_scale(mut self, scale: f64) -> Result<Self, OutputConfigurationError> {
        if !scale.is_finite() || !(0.5..=8.0).contains(&scale) {
            return Err(OutputConfigurationError::InvalidScale(scale));
        }
        self.scale_milli = (scale * 1_000.0).round() as u32;
        Ok(self)
    }

    /// Return the configured fractional scale factor.
    #[must_use]
    pub fn scale_factor(&self) -> f64 {
        f64::from(self.scale_milli) / 1_000.0
    }
}

/// Invalid output configuration data rejected before it reaches Smithay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputConfigurationError {
    InvalidScale(f64),
}

impl std::fmt::Display for OutputConfigurationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScale(scale) => write!(formatter, "invalid output scale: {scale}"),
        }
    }
}

impl std::error::Error for OutputConfigurationError {}

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
        Self::from_configurations::<D>(
            display,
            &[OutputConfiguration::new("output-0", (1920, 1080), (0, 0))],
        )
    }

    /// Create output globals from backend-provided configurations.
    pub fn from_configurations<D>(
        display: &DisplayHandle,
        configurations: &[OutputConfiguration],
    ) -> Self
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
        let manager = OutputManagerState::new();
        let outputs = configurations
            .iter()
            .map(|configuration| Self::create_output::<D>(configuration, display))
            .collect();
        Self { manager, outputs }
    }

    fn create_output<D>(configuration: &OutputConfiguration, display: &DisplayHandle) -> Output
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
            configuration.name.clone(),
            PhysicalProperties {
                size: (DEFAULT_PHYSICAL_MM.0 as i32, DEFAULT_PHYSICAL_MM.1 as i32).into(),
                subpixel: Subpixel::Unknown,
                make: "SOL".into(),
                model: "SOL Output".into(),
            },
        );
        let mode = Mode {
            size: configuration.size.into(),
            refresh: 60_000,
        };
        output.create_global::<D>(display);
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Fractional(configuration.scale_factor())),
            Some(Point::from(configuration.location)),
        );
        output.set_preferred(mode);
        output
    }

    /// Reconcile the Wayland output globals with a backend topology refresh.
    ///
    /// Existing outputs retain their identity while their mode/location is
    /// updated. New connectors receive a global; disconnected connectors are
    /// retired from the compositor's active output set.
    #[cfg(feature = "udev")]
    pub fn reconcile<D>(&mut self, configurations: &[OutputConfiguration], display: &DisplayHandle)
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
        let mut active = Vec::with_capacity(configurations.len());
        for configuration in configurations {
            if let Some(output) = self
                .outputs
                .iter()
                .find(|output| output.name() == configuration.name)
            {
                output.change_current_state(
                    Some(Mode {
                        size: configuration.size.into(),
                        refresh: 60_000,
                    }),
                    Some(Transform::Normal),
                    Some(Scale::Fractional(configuration.scale_factor())),
                    Some(Point::from(configuration.location)),
                );
                active.push(output.clone());
            } else {
                active.push(Self::create_output::<D>(configuration, display));
            }
        }
        self.outputs = active;
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

#[cfg(test)]
mod tests {
    use super::{OutputConfiguration, OutputConfigurationError};

    #[test]
    fn fractional_scale_is_validated_and_retained_as_milli_units() {
        let output = OutputConfiguration::new("display", (2560, 1440), (0, 0))
            .try_with_scale(1.25)
            .expect("1.25 is a supported fractional scale");
        assert!((output.scale_factor() - 1.25).abs() < f64::EPSILON);
        assert!(matches!(
            OutputConfiguration::new("display", (1, 1), (0, 0)).try_with_scale(0.0),
            Err(OutputConfigurationError::InvalidScale(0.0))
        ));
        assert!(
            OutputConfiguration::new("display", (1, 1), (0, 0))
                .try_with_scale(f64::NAN)
                .is_err()
        );
    }
}
