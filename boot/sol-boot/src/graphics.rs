//! Deterministic GOP mode selection and bounded boot-frame composition.

use alloc::vec;
use alloc::vec::Vec;

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
    const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            blue,
            green,
            red,
            reserved: 0,
        }
    }
}

const BACKGROUND: BootPixel = BootPixel::rgb(11, 15, 24);
const SUN: BootPixel = BootPixel::rgb(255, 184, 77);
const HORIZON: BootPixel = BootPixel::rgb(247, 116, 86);
const MAX_FRAME_PIXELS: usize = 33_554_432;

/// Composes a single complete static SOL frame with checked dimensions.
#[must_use]
pub fn render_boot_frame(width: usize, height: usize) -> Option<Vec<BootPixel>> {
    let pixel_count = width.checked_mul(height)?;
    if width == 0 || height == 0 || pixel_count > MAX_FRAME_PIXELS {
        return None;
    }
    let mut pixels = vec![BACKGROUND; pixel_count];
    let unit = width.min(height).max(64) / 16;
    let radius = unit.saturating_mul(2).max(4);
    let center_x = width / 2;
    let center_y = height / 2;
    let radius_squared = radius.saturating_mul(radius);
    let y_start = center_y.saturating_sub(radius);
    let y_end = center_y.saturating_add(radius).min(height - 1);
    let x_start = center_x.saturating_sub(radius);
    let x_end = center_x.saturating_add(radius).min(width - 1);
    for y in y_start..=y_end {
        for x in x_start..=x_end {
            let dx = x.abs_diff(center_x);
            let dy = y.abs_diff(center_y);
            if dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy)) <= radius_squared {
                pixels[y * width + x] = SUN;
            }
        }
    }
    let horizon_height = (unit / 4).max(2);
    for y in center_y..center_y.saturating_add(horizon_height).min(height) {
        for x in x_start..=x_end {
            pixels[y * width + x] = HORIZON;
        }
    }
    Some(pixels)
}
