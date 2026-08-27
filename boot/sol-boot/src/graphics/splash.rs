//! Mac-classic boot splash: a black field, the centered brand mark, and a
//! rounded progress capsule revealing itself left-to-right.
//!
//! Frames compose `assets/branding/sol-mark.svg` (parsed at build time, see
//! `build.rs`) using pure integer fixed-point geometry, keeping rendering
//! deterministic and host-testable. Anti-aliasing derives coverage from
//! distance ramps measured in sixteenths of a pixel; no floating-point math
//! participates in rasterization itself.

use super::BootPixel;
use alloc::vec;
use alloc::vec::Vec;

/// Classic boot backdrop; the mark's background variable resolves into it.
const BACKGROUND: BootPixel = BootPixel::rgb(0, 0, 0);
/// Brand foreground (`--sol-mark-foreground`).
const MARK_FOREGROUND: BootPixel = BootPixel::rgb(255, 255, 255);
/// Progress capsule track, macOS-style dark gray.
const TRACK: BootPixel = BootPixel::rgb(60, 60, 67);
/// Progress capsule fill; pure white for macOS-like appearance.
const BAR_FILL: BootPixel = BootPixel::rgb(255, 255, 255);

/// Splash mark share of the smaller screen dimension.
const MARK_SIZE_NUMERATOR: usize = 20;
const MARK_SIZE_DENOMINATOR: usize = 100;
/// Floor for legibility on very small panels.
const MARK_SIZE_MIN: usize = 48;
/// The mark never approaches any screen edge closer than this.
const SCREEN_MARGIN: usize = 8;
/// The mark rides optically above true center; the capsule takes the gap.
const MARK_CENTER_Y_NUMERATOR: usize = 46;
const MARK_CENTER_Y_DENOMINATOR: usize = 100;
/// Vertical space between mark box bottom and capsule top.
const CAPSULE_GAP_NUMERATOR: usize = 18;
const CAPSULE_GAP_DENOMINATOR: usize = 100;
/// Capsule thickness relative to its width (about two and a half percent).
const CAPSULE_THICKNESS_NUMERATOR: usize = 25;
const CAPSULE_THICKNESS_DENOMINATOR: usize = 1_000;
/// Minimum drawable thickness in whole pixels.
const CAPSULE_THICKNESS_MIN: usize = 4;

/// Fixed-point resolution: ticks per device pixel.
const TICKS_PER_PIXEL: i64 = 16;
/// Sampling offset placing each probe at the containing pixel's center.
const PIXEL_CENTER_TICKS: i64 = TICKS_PER_PIXEL / 2;
/// Half-width of the anti-alias band, so zero coverage lands half a pixel
/// outside the geometric edge and full coverage half a pixel inside.
const HALF_BAND_TICKS: i64 = PIXEL_CENTER_TICKS;
/// Panels wider or taller than this are rejected outright.
pub const MAX_FRAME_EDGE: usize = 16_384;
/// Largest supported frame area shared with the mode-selection policy.
const MAX_FRAME_PIXELS: usize = 33_554_432;

/// Coverage table indexed by an estimated covered fraction in sixteenths.
#[allow(clippy::cast_possible_truncation)]
const COVERAGE_RAMP: [u8; 17] = {
    let mut ramp = [0_u8; 17];
    let mut index = 0;
    while index < 17 {
        // Indexes reach at most 16, so the product always fits a byte.
        ramp[index] = (((index * 255) + 8) / 16) as u8;
        index += 1;
    }
    ramp
};

/// Splash composition stage handed to [`render_boot_frame`].
///
/// Classic sequencing draws the brand mark first and reveals the capsule as
/// loading milestones complete.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplashProgress {
    /// Brand mark only; no capsule is drawn.
    Hidden,
    /// Capsule track plus a fill covering the given share of its length.
    /// Values clamp into `0.0..=1.0`; non-finite values draw empty.
    Fraction(f32),
}

impl SplashProgress {
    /// Fill-length denominator: sixteenths fixed point squared.
    const DENOMINATOR: u64 = 65_536;

    /// Returns the clamped fill numerator, or `None` for [`SplashProgress::Hidden`].
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "the operand is clamped into 0.0..=1.0, so truncation saturates and sign is fixed"
    )]
    fn quantized(self) -> Option<u64> {
        let Self::Fraction(fraction) = self else {
            return None;
        };
        let clamped = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let scaled = (clamped * Self::DENOMINATOR as f32) as u64;
        Some(scaled.min(Self::DENOMINATOR))
    }
}

/// Generated description of `assets/branding/sol-mark.svg`.
mod brand {
    include!(concat!(env!("OUT_DIR"), "/sol_mark.rs"));
}

/// Converts viewport units along one axis into absolute device ticks. Both
/// operands are bounded far below [`i64::MAX`] by build-time parse limits.
#[allow(clippy::cast_possible_wrap)]
#[must_use]
const fn units_to_ticks(units: u64, size_px: usize) -> i64 {
    let product = (units as i64).saturating_mul((size_px as i64).saturating_mul(TICKS_PER_PIXEL));
    product / (brand::BRAND_VIEWBOX as i64)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
#[must_use]
const fn px_to_ticks(pixel: usize) -> i64 {
    (pixel as i64).saturating_mul(TICKS_PER_PIXEL)
}

/// Floor square root over non-negative `i64` inputs; Heron iteration seeded
/// above the root terminates on the exact floor value.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
#[must_use]
const fn integer_sqrt(value: i64) -> i64 {
    if value <= 0 {
        return 0;
    }
    let magnitude = value as u64;
    let high_bit = u64::BITS - magnitude.leading_zeros();
    let mut current: u64 = 1_u64 << high_bit.div_ceil(2);
    let mut next = current.midpoint(magnitude / current);
    while next < current {
        current = next;
        next = current.midpoint(magnitude / current);
    }
    // Heron from an above-root seed cannot exceed 2^33 here, but cap anyway
    // without `Ord::min`, which is not const-callable yet.
    let capped = if current > i64::MAX as u64 {
        i64::MAX as u64
    } else {
        current
    };
    capped as i64
}

/// Maps a signed tick distance (positive inside the shape) onto coverage.
#[must_use]
const fn coverage_ramp(signed_inside_ticks: i64) -> u8 {
    let stepped = signed_inside_ticks.saturating_add(HALF_BAND_TICKS);
    match stepped {
        ..=0 => 0,
        // `stepped` is positive here, so widening loses nothing.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        1..=TICKS_PER_PIXEL => COVERAGE_RAMP[stepped as usize],
        _ => u8::MAX,
    }
}

/// One anti-aliased filled circle positioned in tick space.
#[derive(Debug, Clone, Copy)]
struct DeviceDisc {
    cx: i64,
    cy: i64,
    r: i64,
}

impl DeviceDisc {
    const fn coverage_at(&self, x_ticks: i64, y_ticks: i64) -> u8 {
        let dx = x_ticks.saturating_sub(self.cx);
        let dy = y_ticks.saturating_sub(self.cy);
        let square = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
        // Solid shortcut skips Heron for every interior pixel of large marks.
        let solid_square = self
            .r
            .saturating_sub(TICKS_PER_PIXEL)
            .saturating_mul(self.r.saturating_sub(TICKS_PER_PIXEL));
        if square <= solid_square {
            return u8::MAX;
        }
        if square >= self.r.saturating_mul(self.r) {
            return 0;
        }
        coverage_ramp(self.r - integer_sqrt(square))
    }

    fn extend(&self, extent: &mut BoundingBox) {
        extent.grow(self.cx - self.r, self.cy - self.r);
        extent.grow(self.cx + self.r, self.cy + self.r);
    }
}

/// Sharp-cornered rectangle forming the straight body between capsule caps.
#[derive(Debug, Clone, Copy)]
struct DeviceRect {
    x0: i64,
    x1: i64,
    y0: i64,
    y1: i64,
}

impl DeviceRect {
    /// Negative outside, zero deep inside, ramping across one pixel.
    fn signed_distance_at(&self, x_ticks: i64, y_ticks: i64) -> i64 {
        signed_axis_distance(x_ticks, self.x0, self.x1)
            .min(signed_axis_distance(y_ticks, self.y0, self.y1))
    }

    fn extend(&self, extent: &mut BoundingBox) {
        extent.grow(self.x0, self.y0);
        extent.grow(self.x1, self.y1);
    }
}

/// Positive inward depth on an axis band (zero exactly at either boundary,
/// negative outside) so combined-axis minimum yields anti-aliasable boxes.
#[must_use]
const fn signed_axis_distance(value: i64, low: i64, high: i64) -> i64 {
    let half_span = (high.saturating_sub(low)) / 2;
    let center = low + half_span;
    let offset = if value >= center {
        value - center
    } else {
        center - value
    };
    half_span - offset
}

enum Prim {
    Disc(DeviceDisc),
    Rect(DeviceRect),
}

impl Prim {
    fn coverage_at(&self, x_ticks: i64, y_ticks: i64) -> u8 {
        match self {
            Self::Disc(disc) => disc.coverage_at(x_ticks, y_ticks),
            Self::Rect(rect) => coverage_ramp(rect.signed_distance_at(x_ticks, y_ticks)),
        }
    }

    fn extend(&self, extent: &mut BoundingBox) {
        match self {
            Self::Disc(disc) => disc.extend(extent),
            Self::Rect(rect) => rect.extend(extent),
        }
    }
}

/// Tick-space axis-aligned bound under construction.
#[derive(Debug, Clone, Copy)]
struct BoundingBox {
    x_low: i64,
    y_low: i64,
    x_high: i64,
    y_high: i64,
}

impl BoundingBox {
    const fn empty() -> Self {
        Self {
            x_low: i64::MAX,
            y_low: i64::MAX,
            x_high: i64::MIN,
            y_high: i64::MIN,
        }
    }

    fn grow(&mut self, x: i64, y: i64) {
        self.x_low = self.x_low.min(x);
        self.y_low = self.y_low.min(y);
        self.x_high = self.x_high.max(x);
        self.y_high = self.y_high.max(y);
    }
}

/// Horizontal capsule spanning pixel columns `[left, right_exclusive)` centered
/// on row-space height `cy`, diameter `thickness`. All inputs are whole pixels.
const fn capsule_prims(
    left: usize,
    right_exclusive: usize,
    cy: usize,
    thickness: usize,
) -> [Prim; 3] {
    let x_edge_low = px_to_ticks(left);
    let x_edge_high = px_to_ticks(right_exclusive);
    let cy_ticks = px_to_ticks(cy);
    let radius = px_to_ticks(thickness / 2);
    let cap_a = x_edge_low + radius;
    let cap_b = x_edge_high - radius;
    [
        Prim::Disc(DeviceDisc {
            cx: cap_a,
            cy: cy_ticks,
            r: radius,
        }),
        Prim::Rect(DeviceRect {
            x0: cap_a,
            x1: cap_b,
            y0: cy_ticks - radius,
            y1: cy_ticks + radius,
        }),
        Prim::Disc(DeviceDisc {
            cx: cap_b,
            cy: cy_ticks,
            r: radius,
        }),
    ]
}

/// Blends one color layer onto the framebuffer wherever its primitives cover.
/// Dimensions are bounded by [`MAX_FRAME_EDGE`], so narrowing casts are safe.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "frame dimensions are bounded by MAX_FRAME_EDGE and tick probes stay non-negative"
)]
fn paint_layer(
    buffer: &mut [BootPixel],
    width: usize,
    height: usize,
    color: BootPixel,
    prims: &[Prim],
) {
    if buffer.is_empty() || prims.is_empty() {
        return;
    }
    let mut extent = BoundingBox::empty();
    for prim in prims {
        prim.extend(&mut extent);
    }
    // Clip in tick space first, then map onto a half-open pixel window.
    let width_ticks = (width as i64).saturating_mul(TICKS_PER_PIXEL);
    let height_ticks = (height as i64).saturating_mul(TICKS_PER_PIXEL);
    let tick = i64::from(TICKS_PER_PIXEL as u8);
    let first_column = (extent.x_low.max(0) as usize) / TICKS_PER_PIXEL as usize;
    let last_column =
        ((extent.x_high.max(0).min(width_ticks.saturating_sub(1))) / tick + 1) as usize;
    let first_row = (extent.y_low.max(0) as usize) / TICKS_PER_PIXEL as usize;
    let last_row = ((extent.y_high.max(0).min(height_ticks.saturating_sub(1))) / tick + 1) as usize;
    if first_column >= last_column || first_row >= last_row {
        return;
    }
    for py in first_row..last_row {
        let y_probe = px_to_ticks(py) + PIXEL_CENTER_TICKS;
        for px in first_column..last_column {
            let mut alpha = 0_u8;
            for prim in prims {
                alpha = alpha.max(prim.coverage_at(px_to_ticks(px) + PIXEL_CENTER_TICKS, y_probe));
                if alpha == u8::MAX {
                    break;
                }
            }
            if alpha > 0 {
                let index = py * width + px;
                buffer[index] = buffer[index].blended(color, alpha);
            }
        }
    }
}

/// Mark placement resolved in whole pixels.
#[derive(Debug, Clone, Copy)]
struct MarkBox {
    origin_x: usize,
    origin_y: usize,
    size: usize,
}

/// Squares the mark into available screen space; rejects frames too small to
/// honor [`SCREEN_MARGIN`] around even the minimum mark.
fn fit_mark(width: usize, height: usize) -> Option<MarkBox> {
    let unit = width.min(height);
    let wanted = unit
        .checked_mul(MARK_SIZE_NUMERATOR)?
        .checked_div(MARK_SIZE_DENOMINATOR)?;
    let room = unit.saturating_sub(SCREEN_MARGIN.saturating_mul(2));
    let size = wanted.max(MARK_SIZE_MIN).min(room);
    let bounded = size.checked_add(SCREEN_MARGIN)?;
    if width < bounded || height < bounded || size == 0 {
        return None;
    }
    let anchor_x = width / 2;
    let anchor_y = height
        .checked_mul(MARK_CENTER_Y_NUMERATOR)?
        .checked_div(MARK_CENTER_Y_DENOMINATOR)?;
    let max_origin = |anchor: usize, total: usize| {
        anchor
            .saturating_sub(size / 2)
            .clamp(SCREEN_MARGIN, total.saturating_sub(bounded))
    };
    Some(MarkBox {
        origin_x: max_origin(anchor_x, width),
        origin_y: max_origin(anchor_y, height),
        size,
    })
}

/// Visible art width approximation: widest foreground disc's full diameter.
///
/// The progress rail matches rendered ink rather than the layout box. The
/// multiplication is bounded by `MAX_FRAME_EDGE`, so truncating casts stay
/// safe.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
fn visual_art_width(mark_size: usize) -> usize {
    let widest_diameter = brand::BRAND_CIRCLES
        .iter()
        .filter(|circle| matches!(circle.fill, brand::BrandFill::Foreground))
        .map(|circle| circle.r.saturating_mul(2))
        .max()
        .unwrap_or(brand::BRAND_VIEWBOX / 2);
    let product = (mark_size as u64).saturating_mul(widest_diameter);
    (product / brand::BRAND_VIEWBOX) as usize
}

#[allow(clippy::cast_possible_truncation)]
fn usize_from_u128(value: u128) -> usize {
    value.try_into().unwrap_or(usize::MAX)
}

/// Mac-style rail below the mark: track plus optional partial fill.
fn paint_capsule_row(
    buffer: &mut [BootPixel],
    width: usize,
    height: usize,
    mark: &MarkBox,
    fill_numerator: Option<u64>,
) {
    let Some(fill_numerator) = fill_numerator else {
        return;
    };
    let bar_width = visual_art_width(mark.size).max(CAPSULE_THICKNESS_MIN * 4);
    let thickness = (bar_width * CAPSULE_THICKNESS_NUMERATOR / CAPSULE_THICKNESS_DENOMINATOR)
        .max(CAPSULE_THICKNESS_MIN);
    let gap = mark.size * CAPSULE_GAP_NUMERATOR / CAPSULE_GAP_DENOMINATOR;
    let bar_top = mark.origin_y + mark.size + gap;
    let bottom_limit = height.saturating_sub(thickness);
    let bar_top = bar_top.min(bottom_limit);
    let cy = bar_top + thickness / 2;
    let left = width.saturating_sub(bar_width) / 2;
    let right_exclusive = left + bar_width.min(width.saturating_sub(left));
    paint_layer(
        buffer,
        width,
        height,
        TRACK,
        &capsule_prims(left, right_exclusive, cy, thickness),
    );
    if fill_numerator == 0 {
        return;
    }
    // Fill is its own rounded capsule sliding along the interior: minimum
    // width equals the cap diameter and grows by the traveled distance.
    let travel = right_exclusive
        .saturating_sub(left)
        .saturating_sub(thickness);
    let fill_width =
        thickness + usize_from_u128(((travel as u128) * u128::from(fill_numerator)) >> 16);
    let fill_right = left.saturating_add(fill_width).min(right_exclusive);
    paint_layer(
        buffer,
        width,
        height,
        BAR_FILL,
        &capsule_prims(left, fill_right, cy, thickness),
    );
}

/// Shared admission policy for both composition entry points, yielding the
/// exact expected frame area when the dimensions qualify.
fn admitted_pixels(width: usize, height: usize) -> Option<usize> {
    let pixel_count = width.checked_mul(height)?;
    (width > 0
        && height > 0
        && width <= MAX_FRAME_EDGE
        && height <= MAX_FRAME_EDGE
        && pixel_count <= MAX_FRAME_PIXELS)
        .then_some(pixel_count)
}

/// Fills `buffer` with the complete splash frame described by `progress`,
/// reusing existing storage. Dimensions must agree with the buffer length.
///
/// Returns `None` without touching the buffer for degenerate dimensions,
/// oversized frames, or a buffer whose length disagrees with the dimensions.
pub fn redraw_boot_frame(
    buffer: &mut [BootPixel],
    width: usize,
    height: usize,
    progress: SplashProgress,
) -> Option<()> {
    if admitted_pixels(width, height)? != buffer.len() {
        return None;
    }
    buffer.fill(BACKGROUND);
    let Some(mark) = fit_mark(width, height) else {
        return Some(());
    };
    let origin_ticks_x = px_to_ticks(mark.origin_x);
    let origin_ticks_y = px_to_ticks(mark.origin_y);
    for circle in brand::BRAND_CIRCLES {
        let disc = DeviceDisc {
            cx: origin_ticks_x + units_to_ticks(circle.cx, mark.size),
            cy: origin_ticks_y + units_to_ticks(circle.cy, mark.size),
            r: units_to_ticks(circle.r, mark.size),
        };
        let color = match circle.fill {
            brand::BrandFill::Foreground => MARK_FOREGROUND,
            brand::BrandFill::Cutout => BACKGROUND,
        };
        paint_layer(buffer, width, height, color, &[Prim::Disc(disc)]);
    }
    paint_capsule_row(buffer, width, height, &mark, progress.quantized());
    Some(())
}

/// Splash composition entry point returning freshly allocated storage.
///
/// Returns `None` for degenerate dimensions or oversized frames; see
/// [`redraw_boot_frame`] for the buffer-reusing variant.
#[must_use]
pub fn render_boot_frame(
    width: usize,
    height: usize,
    progress: SplashProgress,
) -> Option<Vec<BootPixel>> {
    let pixel_count = admitted_pixels(width, height)?;
    let mut buffer = vec![BACKGROUND; pixel_count];
    redraw_boot_frame(&mut buffer, width, height, progress)?;
    Some(buffer)
}
