//! Optional best-effort static boot frame in the current GOP mode.
//!
//! ADR-0026 permits (but does not require) one bounded static SOL frame through
//! the currently active GOP mode. This module implements that optional capability
//! with strict constraints:
//!
//! - Never read EDID or infer native/preferred resolution
//! - Never enumerate modes or call `SetMode()`
//! - Preserve current width, height, stride, and pixel format
//! - Draw only a solid background and aspect-correct centered mark
//! - No animation, interactive menu, or routine text output
//! - Ignore missing GOP, unsupported pixel formats, and rendering failure
//! - Never let graphics affect verification, retry, fallback, or recovery

use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
use uefi::table::boot::ScopedProtocol;

/// SOL brand color (blue-purple).
const BRAND_COLOR: BltPixel = BltPixel::new(103, 80, 164); // #6750A4

/// Background color (very dark, near black).
const BACKGROUND_COLOR: BltPixel = BltPixel::new(16, 16, 20);

/// Centered mark dimensions (logical pixels).
const MARK_SIZE: usize = 120;

/// Draws one static SOL brand mark in the current GOP mode.
///
/// This function is best-effort and never returns an error. Missing GOP,
/// unsupported pixel formats, or rendering failures are silently ignored.
/// The call has no effect on boot policy, verification, or recovery paths.
pub fn draw_optional_boot_frame(gop: &mut ScopedProtocol<GraphicsOutput>) {
    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();

    // Silently skip if the mode is too small for the mark.
    if width < MARK_SIZE || height < MARK_SIZE {
        return;
    }

    // Fill background (ignore failure).
    let _ = gop.blt(BltOp::VideoFill {
        color: BACKGROUND_COLOR,
        dest: (0, 0),
        dims: (width, height),
    });

    // Draw centered circular mark using scanline algorithm (efficient).
    let center_x = width / 2;
    let center_y = height / 2;
    let radius = (MARK_SIZE / 2) as isize;

    // Midpoint circle algorithm - draw horizontal spans for each y coordinate.
    // This reduces blit calls from ~11,000 to ~120 (100x improvement).
    for dy in -radius..=radius {
        let y = (center_y as isize + dy) as usize;
        if y >= height {
            continue;
        }

        // Calculate horizontal span width at this y using circle equation:
        // x² + y² = r²  =>  x = sqrt(r² - y²)
        let dy_sq = dy * dy;
        let radius_sq = radius * radius;
        if dy_sq > radius_sq {
            continue;
        }

        let dx = ((radius_sq - dy_sq) as f64).sqrt() as isize;
        let x_start = ((center_x as isize) - dx).max(0) as usize;
        let x_end = ((center_x as isize) + dx).min(width as isize - 1) as usize;
        let span_width = (x_end - x_start + 1).min(width - x_start);

        if span_width > 0 {
            let _ = gop.blt(BltOp::VideoFill {
                color: BRAND_COLOR,
                dest: (x_start, y),
                dims: (span_width, 1),
            });
        }
    }
}
