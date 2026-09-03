//! Token-driven software raster canvas for SOL Shell surfaces.
//!
//! The Shell's surfaces are composited by the compositor from shared memory, so
//! something has to turn design tokens into bytes. This is that something: a
//! deliberately small painter with rectangles, rounded rectangles, and text.
//!
//! It is **not** a renderer. SolKit's real drawing stack (`sol-graphics` over
//! `sol-ui`) is where Shell chrome ends up; until that stack can target an SCP
//! surface, a desktop that only exists as a model is a desktop nobody can look
//! at. This module is the minimum that makes the wallpaper, top bar, Dock, and
//! Launcher real pixels on a real output, and it accepts only `sol-design`
//! tokens so the visual contract does not fork while that is true.
//!
//! ## Premultiplied output
//!
//! SCP shared-memory content is premultiplied; `sol-design` tokens are straight
//! alpha. [`Canvas`] premultiplies on the way in, so callers keep working in
//! token colors and the compositor receives what its blend equation expects.

use sol_design::color::Rgba;

/// Bytes one canvas pixel occupies.
const BYTES_PER_PIXEL: usize = 4;

/// Largest canvas a Shell surface may allocate.
///
/// The Shell sizes its surfaces from compositor-reported output geometry, so
/// this is a guard against a malformed configure rather than against a hostile
/// caller — a Shell that tries to allocate an absurd surface should fail the
/// frame, not the session.
const MAX_DIMENSION: u32 = 16_384;

/// A rectangle in physical surface pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PixelRect {
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Shrink the rectangle by `inset` on every side.
    #[must_use]
    pub fn inset(self, inset: f32) -> Self {
        Self {
            x: self.x + inset,
            y: self.y + inset,
            width: (self.width - inset * 2.0).max(0.0),
            height: (self.height - inset * 2.0).max(0.0),
        }
    }

    /// Whether the rectangle would paint anything at all.
    #[must_use]
    pub fn is_empty(self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }

    #[must_use]
    pub fn center_x(self) -> f32 {
        self.x + self.width / 2.0
    }

    #[must_use]
    pub fn center_y(self) -> f32 {
        self.y + self.height / 2.0
    }
}

/// A premultiplied RGBA8 surface the Shell paints into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    /// Allocate a fully transparent canvas.
    ///
    /// Returns `None` for a degenerate or implausible extent rather than
    /// attempting the allocation.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return None;
        }
        let length = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(BYTES_PER_PIXEL)?;
        Some(Self {
            width,
            height,
            pixels: vec![0; length],
        })
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Consume the canvas, yielding premultiplied RGBA8 bytes.
    #[must_use]
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    /// Read one pixel as premultiplied RGBA8.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; BYTES_PER_PIXEL]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y as usize * self.width as usize + x as usize) * BYTES_PER_PIXEL;
        self.pixels
            .get(index..index + BYTES_PER_PIXEL)?
            .try_into()
            .ok()
    }

    /// Replace every pixel with one opaque-or-not token color.
    pub fn clear(&mut self, color: Rgba) {
        let value = premultiply(color, 1.0);
        for chunk in self.pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            chunk.copy_from_slice(&value);
        }
    }

    /// Fill an axis-aligned rectangle.
    pub fn fill_rect(&mut self, rect: PixelRect, color: Rgba) {
        self.fill_rounded_rect(rect, 0.0, color);
    }

    /// Fill a rectangle with rounded corners.
    ///
    /// Corners are antialiased by coverage: a pixel one unit outside the corner
    /// arc contributes nothing, one unit inside contributes fully, and the band
    /// between them ramps. Without it the Dock and Launcher would show visibly
    /// stepped corners against the wallpaper at any scale.
    pub fn fill_rounded_rect(&mut self, rect: PixelRect, radius: f32, color: Rgba) {
        if rect.is_empty() {
            return;
        }
        let radius = radius.max(0.0).min(rect.width / 2.0).min(rect.height / 2.0);

        let left = rect.x.floor().max(0.0) as u32;
        let top = rect.y.floor().max(0.0) as u32;
        let right = (rect.x + rect.width).ceil().clamp(0.0, self.width as f32) as u32;
        let bottom = (rect.y + rect.height).ceil().clamp(0.0, self.height as f32) as u32;

        for y in top..bottom.min(self.height) {
            for x in left..right.min(self.width) {
                let coverage = coverage_at(rect, radius, x as f32 + 0.5, y as f32 + 0.5);
                if coverage > 0.0 {
                    self.blend_pixel(x, y, color, coverage);
                }
            }
        }
    }

    /// Draw a horizontal hairline separator across a rectangle's bottom edge.
    ///
    /// A one-pixel line is the one place where a bare pixel count is the honest
    /// unit: the separator must land on a device pixel at any scale, so it is
    /// not a spacing token.
    pub fn fill_hairline(&mut self, rect: PixelRect, color: Rgba) {
        self.fill_rect(
            PixelRect::new(rect.x, rect.y + rect.height - 1.0, rect.width, 1.0),
            color,
        );
    }

    /// Draw a string in the built-in bitmap face, returning its painted width.
    ///
    /// `origin` is the text's top-left corner. `scale` multiplies the 5x7 cell,
    /// so a caller sizes text from a typography token rather than by choosing a
    /// pixel height directly (see [`text_scale_for_height`]).
    pub fn draw_text(&mut self, origin: (f32, f32), scale: f32, color: Rgba, text: &str) -> f32 {
        let scale = scale.max(1.0).round();
        let advance = (font::GLYPH_WIDTH as f32 + 1.0) * scale;
        let mut pen_x = origin.0;

        for character in text.chars() {
            let glyph = font::glyph(character);
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..font::GLYPH_WIDTH {
                    // Bit 4 is the leftmost column, so the literals in the font
                    // table read left to right exactly as the glyph looks.
                    let lit = bits & (1 << (font::GLYPH_WIDTH - 1 - column)) != 0;
                    if !lit {
                        continue;
                    }
                    self.fill_rect(
                        PixelRect::new(
                            pen_x + column as f32 * scale,
                            origin.1 + row as f32 * scale,
                            scale,
                            scale,
                        ),
                        color,
                    );
                }
            }
            pen_x += advance;
        }

        (pen_x - origin.0 - scale).max(0.0)
    }

    /// Width [`Canvas::draw_text`] would paint, without painting it.
    #[must_use]
    pub fn text_width(scale: f32, text: &str) -> f32 {
        let scale = scale.max(1.0).round();
        let count = text.chars().count() as f32;
        if count == 0.0 {
            return 0.0;
        }
        count * (font::GLYPH_WIDTH as f32 + 1.0) * scale - scale
    }

    fn blend_pixel(&mut self, x: u32, y: u32, color: Rgba, coverage: f32) {
        let index = (y as usize * self.width as usize + x as usize) * BYTES_PER_PIXEL;
        let Some(destination) = self.pixels.get_mut(index..index + BYTES_PER_PIXEL) else {
            return;
        };
        let source = premultiply(color, coverage);
        let inverse = u32::from(u8::MAX - source[3]);
        for channel in 0..BYTES_PER_PIXEL {
            let scaled = (u32::from(destination[channel]) * inverse + 127) / 255;
            destination[channel] =
                u8::try_from(u32::from(source[channel]) + scaled).unwrap_or(u8::MAX);
        }
    }
}

/// Cell scale that renders text closest to a typography token's pixel size.
///
/// The built-in face has one fixed cell, so a token's requested size becomes an
/// integer multiple of it. Rounding to an integer keeps every glyph edge on a
/// device pixel, which matters far more at this size than matching the token
/// exactly would.
#[must_use]
pub fn text_scale_for_height(pixels: f32) -> f32 {
    (pixels / font::GLYPH_HEIGHT as f32).round().max(1.0)
}

/// Painted height of text drawn at a given cell scale.
#[must_use]
pub fn text_height(scale: f32) -> f32 {
    scale.max(1.0).round() * font::GLYPH_HEIGHT as f32
}

/// Convert a straight-alpha token color into premultiplied RGBA8.
fn premultiply(color: Rgba, coverage: f32) -> [u8; BYTES_PER_PIXEL] {
    let alpha = (color.3 * coverage).clamp(0.0, 1.0);
    let channel = |value: f32| -> u8 { (value.clamp(0.0, 1.0) * alpha * 255.0 + 0.5) as u8 };
    [
        channel(color.0),
        channel(color.1),
        channel(color.2),
        (alpha * 255.0 + 0.5) as u8,
    ]
}

/// How much of a pixel a rounded rectangle covers.
fn coverage_at(rect: PixelRect, radius: f32, x: f32, y: f32) -> f32 {
    if x < rect.x || y < rect.y || x > rect.x + rect.width || y > rect.y + rect.height {
        return 0.0;
    }
    if radius <= 0.0 {
        return 1.0;
    }

    // Distance from the nearest corner arc's center, measured only in the
    // corner quadrants; anywhere else the rectangle is fully covered.
    let corner_x = if x < rect.x + radius {
        rect.x + radius
    } else if x > rect.x + rect.width - radius {
        rect.x + rect.width - radius
    } else {
        return 1.0;
    };
    let corner_y = if y < rect.y + radius {
        rect.y + radius
    } else if y > rect.y + rect.height - radius {
        rect.y + rect.height - radius
    } else {
        return 1.0;
    };

    let distance = ((x - corner_x).powi(2) + (y - corner_y).powi(2)).sqrt();
    (radius - distance + 0.5).clamp(0.0, 1.0)
}

/// A compact 5x7 bitmap face.
///
/// Every glyph is seven rows of five columns, written as binary literals so the
/// table reads as the shape it draws. Lowercase folds to uppercase and unknown
/// characters render as a filled box, the conventional "this glyph is missing"
/// mark — the Shell must never silently drop a character it was asked to show.
///
/// This exists so the desktop is legible before SolKit's text stack can target
/// an SCP surface. It is not a typography implementation: no shaping, no
/// kerning, no bidirectional layout, no non-ASCII coverage. Text that needs any
/// of those belongs in SolKit, not here.
mod font {
    pub const GLYPH_WIDTH: usize = 5;
    pub const GLYPH_HEIGHT: usize = 7;

    /// The mark drawn for a character outside the built-in face.
    const TOFU: [u8; GLYPH_HEIGHT] = [
        0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111,
    ];

    const SPACE: [u8; GLYPH_HEIGHT] = [0, 0, 0, 0, 0, 0, 0];

    pub fn glyph(character: char) -> [u8; GLYPH_HEIGHT] {
        match character.to_ascii_uppercase() {
            ' ' => SPACE,
            '0' => [
                0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
            ],
            '1' => [
                0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
            ],
            '2' => [
                0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
            ],
            '3' => [
                0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
            ],
            '4' => [
                0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
            ],
            '5' => [
                0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
            ],
            '6' => [
                0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
            ],
            '7' => [
                0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
            ],
            '8' => [
                0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
            ],
            '9' => [
                0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
            ],
            'A' => [
                0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
            ],
            'B' => [
                0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
            ],
            'C' => [
                0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
            ],
            'D' => [
                0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
            ],
            'E' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
            ],
            'F' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
            ],
            'G' => [
                0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
            ],
            'H' => [
                0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
            ],
            'I' => [
                0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
            ],
            'J' => [
                0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
            ],
            'K' => [
                0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
            ],
            'L' => [
                0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
            ],
            'M' => [
                0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
            ],
            'N' => [
                0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
            ],
            'O' => [
                0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
            ],
            'P' => [
                0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
            ],
            'Q' => [
                0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
            ],
            'R' => [
                0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
            ],
            'S' => [
                0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
            ],
            'T' => [
                0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
            ],
            'U' => [
                0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
            ],
            'V' => [
                0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
            ],
            'W' => [
                0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
            ],
            'X' => [
                0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
            ],
            'Y' => [
                0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
            ],
            'Z' => [
                0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
            ],
            '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
            ',' => [0, 0, 0, 0, 0b00110, 0b00110, 0b00100],
            ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
            ';' => [0, 0b01100, 0b01100, 0, 0b01100, 0b00100, 0b01000],
            '-' => [0, 0, 0, 0b01110, 0, 0, 0],
            '_' => [0, 0, 0, 0, 0, 0, 0b11111],
            '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
            '=' => [0, 0, 0b11111, 0, 0b11111, 0, 0],
            '/' => [
                0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
            ],
            '%' => [
                0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011,
            ],
            '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
            '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
            '(' => [
                0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
            ],
            ')' => [
                0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
            ],
            '[' => [
                0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
            ],
            ']' => [
                0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
            ],
            '#' => [
                0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010,
            ],
            '@' => [
                0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
            ],
            '*' => [0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0],
            '<' => [
                0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
            ],
            '>' => [
                0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
            ],
            '\'' => [0b00100, 0b00100, 0, 0, 0, 0, 0],
            '"' => [0b01010, 0b01010, 0, 0, 0, 0, 0],
            '&' => [
                0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
            ],
            _ => TOFU,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(red: f32, green: f32, blue: f32) -> Rgba {
        Rgba(red, green, blue, 1.0)
    }

    #[test]
    fn a_cleared_canvas_holds_the_token_color_in_every_pixel() {
        let mut canvas = Canvas::new(4, 3).expect("allocate");
        canvas.clear(opaque(1.0, 0.0, 0.0));

        assert_eq!(canvas.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(canvas.pixel(3, 2), Some([255, 0, 0, 255]));
        assert_eq!(canvas.pixel(4, 0), None);
    }

    #[test]
    fn a_translucent_fill_is_premultiplied_for_the_compositor() {
        let mut canvas = Canvas::new(2, 2).expect("allocate");
        canvas.fill_rect(PixelRect::new(0.0, 0.0, 2.0, 2.0), Rgba(1.0, 1.0, 1.0, 0.5));

        // Premultiplied: the color channels are scaled by alpha, not left at
        // full white. A compositor blending straight-alpha bytes as if they were
        // premultiplied washes every translucent Shell surface out.
        let pixel = canvas.pixel(0, 0).expect("pixel");
        assert_eq!(pixel[3], 128);
        assert!(pixel[0] <= pixel[3], "channels must not exceed alpha");
    }

    #[test]
    fn a_rectangle_paints_only_inside_its_bounds() {
        let mut canvas = Canvas::new(8, 8).expect("allocate");
        canvas.fill_rect(PixelRect::new(2.0, 2.0, 4.0, 4.0), opaque(0.0, 1.0, 0.0));

        assert_eq!(canvas.pixel(3, 3), Some([0, 255, 0, 255]));
        assert_eq!(canvas.pixel(1, 1), Some([0, 0, 0, 0]));
        assert_eq!(canvas.pixel(6, 6), Some([0, 0, 0, 0]));
    }

    #[test]
    fn a_rectangle_reaching_past_the_canvas_is_clipped_rather_than_panicking() {
        let mut canvas = Canvas::new(4, 4).expect("allocate");
        canvas.fill_rect(
            PixelRect::new(-10.0, -10.0, 100.0, 100.0),
            opaque(0.0, 0.0, 1.0),
        );
        assert_eq!(canvas.pixel(3, 3), Some([0, 0, 255, 255]));
    }

    #[test]
    fn rounded_corners_are_softer_than_the_interior() {
        let mut canvas = Canvas::new(16, 16).expect("allocate");
        canvas.fill_rounded_rect(
            PixelRect::new(0.0, 0.0, 16.0, 16.0),
            6.0,
            opaque(1.0, 1.0, 1.0),
        );

        let corner = canvas.pixel(0, 0).expect("corner").into_iter().nth(3);
        let middle = canvas.pixel(8, 8).expect("middle").into_iter().nth(3);
        assert_eq!(middle, Some(255));
        assert!(corner < middle, "the corner must not be fully covered");
    }

    #[test]
    fn text_paints_ink_and_reports_a_matching_width() {
        let mut canvas = Canvas::new(64, 16).expect("allocate");
        let painted = canvas.draw_text((1.0, 1.0), 1.0, opaque(1.0, 1.0, 1.0), "09:41");

        assert_eq!(painted, Canvas::text_width(1.0, "09:41"));
        let ink = (0..16)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .filter(|(x, y)| canvas.pixel(*x, *y).is_some_and(|pixel| pixel[3] > 0))
            .count();
        assert!(ink > 0, "a clock string must leave visible ink");
    }

    #[test]
    fn an_unsupported_character_renders_a_visible_mark_rather_than_nothing() {
        let mut with_glyph = Canvas::new(16, 16).expect("allocate");
        let mut without_glyph = Canvas::new(16, 16).expect("allocate");
        with_glyph.draw_text((0.0, 0.0), 1.0, opaque(1.0, 1.0, 1.0), "A");
        without_glyph.draw_text((0.0, 0.0), 1.0, opaque(1.0, 1.0, 1.0), "字");

        let ink = |canvas: &Canvas| {
            (0..16)
                .flat_map(|y| (0..16).map(move |x| (x, y)))
                .filter(|(x, y)| canvas.pixel(*x, *y).is_some_and(|pixel| pixel[3] > 0))
                .count()
        };
        assert!(
            ink(&without_glyph) > 0,
            "missing glyphs must still be shown"
        );
        assert!(ink(&with_glyph) > 0);
    }

    #[test]
    fn an_empty_string_paints_nothing_and_measures_zero() {
        let mut canvas = Canvas::new(8, 8).expect("allocate");
        assert_eq!(
            canvas.draw_text((0.0, 0.0), 2.0, opaque(1.0, 1.0, 1.0), ""),
            0.0
        );
        assert_eq!(Canvas::text_width(2.0, ""), 0.0);
        assert_eq!(canvas.pixel(0, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn a_degenerate_canvas_is_refused_before_allocation() {
        assert!(Canvas::new(0, 8).is_none());
        assert!(Canvas::new(8, 0).is_none());
        assert!(Canvas::new(MAX_DIMENSION + 1, 8).is_none());
    }

    #[test]
    fn a_typography_token_resolves_to_a_whole_cell_scale() {
        assert_eq!(text_scale_for_height(7.0), 1.0);
        assert_eq!(text_scale_for_height(14.0), 2.0);
        // Never zero: a token smaller than one cell still has to be legible.
        assert_eq!(text_scale_for_height(1.0), 1.0);
        assert_eq!(text_height(2.0), 14.0);
    }
}
