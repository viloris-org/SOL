//! Deterministic GOP mode selection, bounded boot-frame composition, and the
//! branded Mac-classic boot splash renderer.

/// A usable firmware graphics mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsMode {
    /// Firmware mode number.
    pub index: u32,
    /// Visible horizontal pixels.
    pub width: usize,
    /// Visible vertical pixels.
    pub height: usize,
    /// Pixels per scanline.
    pub stride: usize,
}

impl GraphicsMode {
    const fn usable(self) -> bool {
        self.width > 0 && self.height > 0 && self.stride >= self.width
    }
}

/// EDID preferred physical resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferredResolution {
    /// Preferred horizontal pixels.
    pub width: usize,
    /// Preferred vertical pixels.
    pub height: usize,
}

/// Whether the adapter should preserve the current mode or make one change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsDecision {
    /// The current usable mode is the best safe choice.
    Preserve(GraphicsMode),
    /// Select this exact advertised preferred mode once.
    SetOnce(GraphicsMode),
    /// Firmware did not expose a usable current mode.
    Unavailable,
}

/// Selects an advertised GOP mode without assuming that the largest mode is native.
#[must_use]
pub fn select_graphics_mode(
    modes: &[GraphicsMode],
    current_index: u32,
    preferred: Option<PreferredResolution>,
) -> GraphicsDecision {
    let current = modes
        .iter()
        .copied()
        .find(|mode| mode.index == current_index && mode.usable());
    let Some(current) = current else {
        return GraphicsDecision::Unavailable;
    };
    let Some(preferred) = preferred else {
        return GraphicsDecision::Preserve(current);
    };
    let exact = modes.iter().copied().find(|mode| {
        mode.usable() && mode.width == preferred.width && mode.height == preferred.height
    });
    match exact {
        Some(mode) if mode.index == current.index => GraphicsDecision::Preserve(current),
        Some(mode) => GraphicsDecision::SetOnce(mode),
        None => GraphicsDecision::Preserve(current),
    }
}

/// Ease-out cubic curve over progress in `0.0..=1.0`; fast at first, still
/// near the target afterwards.
///
/// Splash animation advances between real work milestones with motion that
/// decelerates into each target instead of stopping abruptly. Non-finite and
/// out-of-range inputs clamp onto the nearest endpoint.
#[must_use]
pub fn ease_out_cubic(progress: f32) -> f32 {
    // Only NaN has no meaningful endpoint; infinities clamp naturally.
    let clamped = if progress.is_nan() {
        0.0
    } else {
        progress.clamp(0.0, 1.0)
    };
    let inverse = 1.0 - clamped;
    1.0 - inverse * inverse * inverse
}

/// Malformed or unsupported EDID base block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdidError {
    /// A complete 128-byte base block was not supplied.
    Truncated,
    /// Header or checksum validation failed.
    Invalid,
    /// The first detailed timing descriptor did not contain a preferred timing.
    NoPreferredTiming,
}

/// Parses the first detailed timing descriptor from a valid EDID base block.
///
/// # Errors
///
/// Rejects truncated blocks, invalid headers/checksums, and blocks without a
/// usable first detailed timing.
pub fn edid_preferred_mode(edid: &[u8]) -> Result<PreferredResolution, EdidError> {
    if edid.len() < 128 {
        return Err(EdidError::Truncated);
    }
    if edid[..8] != [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]
        || edid[..128]
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
            != 0
    {
        return Err(EdidError::Invalid);
    }
    let timing = &edid[54..72];
    if timing[0] == 0 && timing[1] == 0 {
        return Err(EdidError::NoPreferredTiming);
    }
    let width = usize::from(timing[2]) | (usize::from(timing[4] & 0xf0) << 4);
    let height = usize::from(timing[5]) | (usize::from(timing[7] & 0xf0) << 4);
    if width == 0 || height == 0 {
        return Err(EdidError::NoPreferredTiming);
    }
    Ok(PreferredResolution { width, height })
}

/// Portable BGRX pixel used by GOP block transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct BootPixel {
    /// Blue channel.
    pub blue: u8,
    /// Green channel.
    pub green: u8,
    /// Red channel.
    pub red: u8,
    /// Reserved channel, always zero.
    pub reserved: u8,
}

impl BootPixel {
    #[must_use]
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            blue,
            green,
            red,
            reserved: 0,
        }
    }

    /// Linear interpolation towards `target`; `alpha` 255 selects `target`.
    fn blended(self, target: Self, alpha: u8) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "each operand is at most 255, so the mixed sum stays within a byte"
        )]
        let mix = |base: u8, goal: u8| -> u8 {
            (((u16::from(base) * (255 - u16::from(alpha)))
                + (u16::from(goal) * u16::from(alpha))
                + 127)
                / 255) as u8
        };
        Self {
            blue: mix(self.blue, target.blue),
            green: mix(self.green, target.green),
            red: mix(self.red, target.red),
            reserved: 0,
        }
    }
}

mod splash;
pub use splash::{MAX_FRAME_EDGE, SplashProgress, redraw_boot_frame, render_boot_frame};
