//! sol-graphics — Rendering abstraction for SolKit
//!
//! This crate provides a rendering abstraction layer that allows SolKit apps
//! to render content without knowing about Slint, OpenGL, or other rendering
//! backends.
//!
//! # Architecture
//!
//! ```text
//! SolKit App → sol-graphics abstraction → Renderer (Slint, Vulkan, etc.)
//! ```
//!
//! The renderer is selected at compile time via Cargo features:
//! - `slint`: Uses Slint as the rendering backend (ADR-0004)
//!
//! Render targets:
//! - `Renderbuffer`: GPU-backed offscreen buffer
//! - `Surface`: Display surface that can be presented

use sol_design::{color::Color, radius::Radius, spacing::Spacing};

/// A render buffer for offscreen rendering.
#[derive(Debug)]
pub struct Renderbuffer {
    /// The buffer width in pixels.
    pub width: u32,
    /// The buffer height in pixels.
    pub height: u32,
    /// The scale factor for HiDPI displays.
    pub scale: f32,
}

impl Renderbuffer {
    /// Create a new renderbuffer.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            scale: 1.0,
        }
    }

    /// Create a renderbuffer with HiDPI scale.
    pub fn with_scale(width: u32, height: u32, scale: f32) -> Self {
        Self { width, height, scale }
    }

    /// Get the pixel dimensions (scaled).
    pub fn pixel_size(&self) -> (u32, u32) {
        ((self.width as f32 * self.scale) as u32, 
         (self.height as f32 * self.scale) as u32)
    }
}

impl Default for Renderbuffer {
    fn default() -> Self {
        Self::new(800, 600)
    }
}

/// A display surface that can be presented to the user.
#[derive(Debug)]
pub struct Surface {
    /// The surface's logical size.
    pub size: (f32, f32),
    /// The scale factor for HiDPI displays.
    pub scale: f32,
    /// Whether the surface is opaque.
    pub opaque: bool,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            size: (800.0, 600.0),
            scale: 1.0,
            opaque: true,
        }
    }
}

impl Surface {
    /// Create a new surface.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            size: (width, height),
            ..Default::default()
        }
    }

    /// Create a high-DPI surface.
    pub fn high_dpi(width: f32, height: f32, scale: f32) -> Self {
        Self {
            size: (width, height),
            scale,
            ..Default::default()
        }
    }

    /// Create an opaque surface.
    pub fn opaque(mut self) -> Self {
        self.opaque = true;
        self
    }

    /// Create a transparent surface.
    pub fn transparent(mut self) -> Self {
        self.opaque = false;
        self
    }
}

/// A 2D drawing context.
#[derive(Debug)]
pub struct GraphicsContext {
    /// The current render target.
    pub target: Surface,
    /// Whether the context is ready for drawing.
    pub ready: bool,
}

impl GraphicsContext {
    /// Create a new graphics context.
    pub fn new(target: Surface) -> Self {
        Self {
            target,
            ready: false,
        }
    }

    /// Prepare the context for drawing.
    pub fn prepare(&mut self) {
        self.ready = true;
    }

    /// Clear the surface with a color.
    pub fn clear(&self, color: Color) {
        // Clear operation - implemented by the backend
        let _ = color;
    }

    /// Draw a rectangle with the given color.
    pub fn draw_rect(&self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        let _ = (x, y, width, height);
        let _ = color;
        // Rendered by backend
    }

    /// Draw rounded rectangle.
    pub fn draw_rounded_rect(&self, x: f32, y: f32, width: f32, height: f32, radius: Radius, color: Color) {
        let _ = (x, y, width, height);
        let _ = (radius, color);
        // Rendered by backend
    }

    /// Draw padded content area.
    pub fn with_padding(&self, padding: Spacing) -> (f32, f32, f32, f32) {
        (
            padding.px(),
            padding.px(),
            self.target.size.0 - padding.px() * 2.0,
            self.target.size.1 - padding.px() * 2.0,
        )
    }

    /// Present the rendered content.
    pub fn present(&mut self) {
        self.ready = false;
        // Backend presents to display
    }
}

/// Render state for persistent state across frames.
#[derive(Debug, Default)]
pub struct RenderState {
    /// The current frame counter.
    pub frame_count: u64,
    /// The current timestamp in seconds.
    pub timestamp: f64,
}

impl RenderState {
    /// Create a new render state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the timestamp.
    pub fn tick(&mut self, now: f64) {
        self.timestamp = now;
        self.frame_count += 1;
    }

    /// Get the delta time since the last frame.
    pub fn delta_time(&self) -> f64 {
        1.0 / 60.0 // Default to 60fps if no previous frame
    }
}

/// A brush for filling shapes.
#[derive(Debug, Clone)]
pub enum Brush {
    /// Solid color.
    Color(Color),
    /// Linear gradient.
    LinearGradient {
        from: (f32, f32),
        to: (f32, f32),
        stops: Vec<(f32, Color)>,
    },
}

impl Default for Brush {
    fn default() -> Self {
        Self::Color(Color::Surface)
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Color(color)
    }
}

/// A paint operation.
#[derive(Debug, Clone)]
pub struct Paint {
    /// The brush to use.
    pub brush: Brush,
    /// Whether the paint is opaque.
    pub opaque: bool,
    /// Opacity of the paint (0.0..=1.0).
    pub opacity: f32,
}

impl Default for Paint {
    fn default() -> Self {
        Self {
            brush: Brush::default(),
            opaque: true,
            opacity: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderbuffer_default() {
        let rb = Renderbuffer::default();
        assert_eq!(rb.pixel_size(), (800, 600));
        assert_eq!(rb.scale, 1.0);
    }

    #[test]
    fn renderbuffer_scaled() {
        let rb = Renderbuffer::with_scale(800, 600, 2.0);
        assert_eq!(rb.pixel_size(), (1600, 1200));
    }

    #[test]
    fn surface_default() {
        let surf = Surface::default();
        assert_eq!(surf.size, (800.0, 600.0));
        assert!(surf.opaque);
    }

    #[test]
    fn surface_high_dpi() {
        let surf = Surface::high_dpi(800.0, 600.0, 1.5);
        assert_eq!(surf.scale, 1.5);
    }

    #[test]
    fn graphics_context_clear() {
        let ctx = GraphicsContext::new(Surface::default());
        ctx.clear(Color::Surface);
    }

    #[test]
    fn graphics_context_draw_rect() {
        let ctx = GraphicsContext::new(Surface::default());
        ctx.clear(Color::Surface);
        ctx.draw_rect(10.0, 10.0, 100.0, 50.0, Color::Accent);
    }

    #[test]
    fn graphics_context_padding() {
        let ctx = GraphicsContext::new(Surface::new(400.0, 300.0));
        let (x, y, w, h) = ctx.with_padding(Spacing::Md);
        assert_eq!(x, 12.0);
        assert_eq!(y, 12.0);
        assert_eq!(w, 400.0 - 24.0);
        assert_eq!(h, 300.0 - 24.0);
    }

    #[test]
    fn render_state_tick() {
        let mut state = RenderState::new();
        state.tick(1.0);
        assert_eq!(state.frame_count, 1);
        assert_eq!(state.timestamp, 1.0);
    }

    #[test]
    fn brush_from_color() {
        let brush = Brush::from(Color::Accent);
        assert!(matches!(brush, Brush::Color(Color::Accent)));
    }

    #[test]
    fn paint_default() {
        let paint = Paint::default();
        assert!(matches!(paint.brush, Brush::Color(Color::Surface)));
        assert!(paint.opaque);
        assert_eq!(paint.opacity, 1.0);
    }
}
