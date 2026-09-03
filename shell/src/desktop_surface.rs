//! The desktop background surface.
//!
//! The bottom of the SOL desktop: a background-layer SCP surface covering the
//! whole output, beneath every application window. It is the surface that makes
//! the difference between "the compositor is running" and "there is a desktop",
//! and everything else in the Shell is placed on top of it.
//!
//! ## Why the fill is flat
//!
//! `sol-design` is the single source of truth for visual parameters (PRD §19.1),
//! and it does not yet define a wallpaper beyond a root fill. The Shell could
//! invent a gradient here and it would look better; it would also be a visual
//! decision made outside the token crate, which is exactly the drift the token
//! architecture exists to prevent. So the background resolves one token, and a
//! real wallpaper — image decoding, per-user selection, per-output fitting —
//! arrives as tokens and a Settings-owned source, not as a constant in the
//! Shell.

use sol_design::{accessibility::TokenMode, color::Color};
use sol_ui::{AccessibilityNode, AccessibilityState, LogicalSize, SemanticId, SemanticRole};

use crate::{
    overlay::LayerShellLayer,
    paint::Canvas,
    scp_host::{
        DesktopHost, DesktopHostError, HostOutput, LayerAnchor, LayerKeyboard, LayerMargin,
        LayerPlacement,
    },
};

/// Stable SCP namespace of the desktop background surface.
pub const DESKTOP_NAMESPACE: &str = "sol.desktop";

/// The token-resolved description of one desktop background frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopSurfaceContract {
    /// Output the frame was laid out against.
    pub output: HostOutput,
    /// Logical extent covered by the surface.
    pub logical_size: LogicalSize,
    /// Physical frame extent.
    pub physical_size: (u32, u32),
    /// Native placement policy.
    pub placement: LayerPlacement,
    /// Token-resolved background role.
    pub background: Color,
    /// Accessibility preferences the frame was resolved under.
    pub token_mode: TokenMode,
    /// Projected accessibility tree.
    pub accessibility: AccessibilityNode,
}

/// Errors raised before an invalid background frame reaches the compositor.
#[derive(Debug)]
pub enum DesktopSurfaceError {
    /// The compositor has not reported a usable output extent yet.
    OutputNotConfigured,
    /// The frame extent could not be allocated.
    UnpaintableExtent((u32, u32)),
    /// The native host rejected the frame.
    Host(DesktopHostError),
}

impl std::fmt::Display for DesktopSurfaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputNotConfigured => {
                formatter.write_str("no output extent has been configured yet")
            }
            Self::UnpaintableExtent((width, height)) => {
                write!(formatter, "cannot paint a {width}x{height} desktop frame")
            }
            Self::Host(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DesktopSurfaceError {}

impl From<DesktopHostError> for DesktopSurfaceError {
    fn from(error: DesktopHostError) -> Self {
        Self::Host(error)
    }
}

/// Retained desktop background surface.
#[derive(Debug, Clone)]
pub struct DesktopSurface {
    output: HostOutput,
    mode: TokenMode,
    /// Last frame presented, retained for inspection and native hosts.
    pub last_contract: Option<DesktopSurfaceContract>,
}

impl DesktopSurface {
    /// Create the background surface for an output.
    #[must_use]
    pub const fn new(output: HostOutput, mode: TokenMode) -> Self {
        Self {
            output,
            mode,
            last_contract: None,
        }
    }

    /// Adopt a new output extent after a mode change or hotplug.
    pub fn set_output(&mut self, output: HostOutput) {
        self.output = output;
    }

    /// Adopt new accessibility and theme preferences.
    pub fn set_token_mode(&mut self, mode: TokenMode) {
        self.mode = mode;
    }

    /// Build the current frame contract without painting it.
    pub fn contract(&self) -> Result<DesktopSurfaceContract, DesktopSurfaceError> {
        if !self.output.is_configured() {
            return Err(DesktopSurfaceError::OutputNotConfigured);
        }
        let (width, height) = self.output.size;
        Ok(DesktopSurfaceContract {
            output: self.output,
            logical_size: self.output.logical_size(),
            physical_size: (width.max(0) as u32, height.max(0) as u32),
            placement: LayerPlacement {
                namespace: DESKTOP_NAMESPACE.to_owned(),
                // Background, not Bottom: application windows belong above the
                // desktop, and a wallpaper that can cover them is a wallpaper
                // that can hide the window a user is looking for.
                layer: LayerShellLayer::Background,
                anchor: LayerAnchor::FULL,
                margin: LayerMargin::default(),
                size: (width, height),
                // The desktop reserves nothing: it is what other surfaces are
                // placed on, not something they must avoid.
                exclusive_zone: 0,
                keyboard: LayerKeyboard::None,
            },
            background: Color::Surface,
            token_mode: self.mode,
            accessibility: AccessibilityNode {
                id: SemanticId::new("desktop-surface"),
                role: SemanticRole::Group,
                label: "Desktop".to_owned(),
                value: None,
                state: AccessibilityState::default(),
                children: Vec::new(),
            },
        })
    }

    /// Paint and present the background.
    pub fn present(&mut self, host: &mut impl DesktopHost) -> Result<(), DesktopSurfaceError> {
        let contract = self.contract()?;
        let pixels = rasterize(&contract)?;
        host.present(&contract.placement, &pixels)?;
        self.last_contract = Some(contract);
        Ok(())
    }
}

/// Paint one background frame.
fn rasterize(contract: &DesktopSurfaceContract) -> Result<Vec<u8>, DesktopSurfaceError> {
    let (width, height) = contract.physical_size;
    let mut canvas = Canvas::new(width, height)
        .ok_or(DesktopSurfaceError::UnpaintableExtent((width, height)))?;
    canvas.clear(contract.token_mode.color(contract.background));
    Ok(canvas.into_pixels())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp_host::RecordingDesktopHost;

    fn surface() -> DesktopSurface {
        DesktopSurface::new(HostOutput::new(64, 32, 1.0), TokenMode::dark())
    }

    #[test]
    fn the_background_covers_the_whole_output_from_the_background_layer() {
        let contract = surface().contract().expect("contract");

        assert_eq!(contract.placement.layer, LayerShellLayer::Background);
        assert_eq!(contract.placement.anchor, LayerAnchor::FULL);
        assert_eq!(contract.placement.size, (64, 32));
        assert_eq!(contract.physical_size, (64, 32));
        assert_eq!(contract.placement.exclusive_zone, 0);
    }

    #[test]
    fn every_pixel_is_the_resolved_background_token() {
        let mut host = RecordingDesktopHost::default();
        let mut surface = surface();
        surface.present(&mut host).expect("present");

        let (placement, pixels) = host.last_frame(DESKTOP_NAMESPACE).expect("frame");
        assert_eq!(placement.size, (64, 32));
        assert_eq!(pixels.len(), 64 * 32 * 4);

        let expected = TokenMode::dark().color(Color::Surface);
        let first = &pixels[..4];
        assert_eq!(first[3], 255, "the desktop background must be opaque");
        assert_eq!(first[0], (expected.0 * 255.0 + 0.5) as u8);
        assert!(
            pixels.as_chunks::<4>().0.iter().all(|pixel| pixel == first),
            "the background is one flat token fill"
        );
    }

    #[test]
    fn theme_changes_the_fill_without_changing_the_layout() {
        let light = DesktopSurface::new(HostOutput::new(8, 8, 1.0), TokenMode::light());
        let dark = DesktopSurface::new(HostOutput::new(8, 8, 1.0), TokenMode::dark());

        let light_contract = light.contract().expect("contract");
        let dark_contract = dark.contract().expect("contract");
        assert_eq!(light_contract.placement, dark_contract.placement);
        assert_ne!(
            rasterize(&light_contract).expect("paint"),
            rasterize(&dark_contract).expect("paint")
        );
    }

    #[test]
    fn a_scaled_output_keeps_physical_pixels_and_reports_logical_layout() {
        let surface = DesktopSurface::new(HostOutput::new(3840, 2160, 2.0), TokenMode::dark());
        let contract = surface.contract().expect("contract");

        assert_eq!(contract.physical_size, (3840, 2160));
        assert_eq!(contract.logical_size.width, 1920.0);
        assert_eq!(contract.logical_size.height, 1080.0);
    }

    #[test]
    fn an_unconfigured_output_refuses_to_produce_a_frame() {
        let surface = DesktopSurface::new(HostOutput::new(0, 0, 1.0), TokenMode::dark());
        assert!(matches!(
            surface.contract(),
            Err(DesktopSurfaceError::OutputNotConfigured)
        ));
    }

    #[test]
    fn a_new_output_extent_is_adopted_on_the_next_frame() {
        let mut host = RecordingDesktopHost::default();
        let mut surface = surface();
        surface.present(&mut host).expect("first frame");
        surface.set_output(HostOutput::new(128, 16, 1.0));
        surface.present(&mut host).expect("second frame");

        let (placement, pixels) = host.last_frame(DESKTOP_NAMESPACE).expect("frame");
        assert_eq!(placement.size, (128, 16));
        assert_eq!(pixels.len(), 128 * 16 * 4);
    }
}
