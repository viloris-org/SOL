//! Rendering abstraction layer for SOL compositor.
//!
//! This module decouples the SCP protocol layer from the actual rendering
//! backend (Smithay GL renderer, wgpu, etc.). The goal is to eliminate direct
//! Wayland/Smithay protocol dependencies while retaining the ability to use
//! Smithay's renderer as a backend during Phase 1.

mod smithay_backend;
mod types;

pub use smithay_backend::SmithayRenderer;
pub use types::{Color, Point, Rectangle, RenderElement, Size, Transform};

use std::error::Error;

/// Core rendering interface that backends must implement.
pub trait Renderer {
    type Error: Error + Send + Sync + 'static;

    /// Begin a new frame. Returns a Frame token that must be committed.
    fn begin_frame(&mut self) -> Result<Frame<'_>, Self::Error>;

    /// Clear the current render target with the given color.
    fn clear(&mut self, color: Color) -> Result<(), Self::Error>;

    /// Render a buffer at the given location.
    fn render_buffer(
        &mut self,
        buffer: &BufferRef,
        location: Point,
        scale: f64,
    ) -> Result<(), Self::Error>;

    /// Render a collection of elements (surfaces, decorations, overlays).
    fn render_elements(
        &mut self,
        elements: &[RenderElement],
        scale: f64,
    ) -> Result<(), Self::Error>;

    /// Commit the current frame to the output.
    fn commit_frame(&mut self, frame: Frame<'_>) -> Result<(), Self::Error>;
}

/// Frame token ensuring begin/commit pairing.
pub struct Frame<'a> {
    renderer: &'a mut dyn std::any::Any,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Frame<'a> {
    pub(crate) fn new<T: 'static>(renderer: &'a mut T) -> Self {
        Self {
            renderer,
            _marker: std::marker::PhantomData,
        }
    }
}

/// Reference to a client buffer (shared memory or dmabuf).
pub enum BufferRef {
    /// Shared memory buffer (pixels in shmem).
    Shm {
        data: *const u8,
        width: i32,
        height: i32,
        stride: i32,
        format: ShmFormat,
    },
    /// DMA-BUF handle (GPU memory, zero-copy).
    DmaBuf {
        fd: i32,
        width: i32,
        height: i32,
        format: u32, // DRM fourcc
        modifier: u64,
    },
}

unsafe impl Send for BufferRef {}
unsafe impl Sync for BufferRef {}

/// Shared memory buffer format.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ShmFormat {
    Argb8888,
    Xrgb8888,
    Rgba8888,
    Rgbx8888,
}

impl ShmFormat {
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::Argb8888 | Self::Xrgb8888 | Self::Rgba8888 | Self::Rgbx8888 => 4,
        }
    }
}
