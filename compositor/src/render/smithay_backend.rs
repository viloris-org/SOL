//! Smithay renderer backend adapter.
//!
//! During Phase 1 migration, we continue using Smithay's GL renderer as the
//! actual backend, but expose it through our protocol-agnostic Renderer trait.

use std::fmt;

#[derive(Debug)]
pub struct SmithayRenderError(String);

impl fmt::Display for SmithayRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Smithay render error: {}", self.0)
    }
}

impl std::error::Error for SmithayRenderError {}

use super::{BufferRef, Color, Frame, Point, RenderElement, Renderer};
use smithay::backend::renderer::gles::GlesRenderer;

pub struct SmithayRenderer {
    inner: GlesRenderer,
}

impl SmithayRenderer {
    pub fn new(inner: GlesRenderer) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &GlesRenderer {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut GlesRenderer {
        &mut self.inner
    }
}

impl Renderer for SmithayRenderer {
    type Error = SmithayRenderError;

    fn begin_frame(&mut self) -> Result<Frame<'_>, Self::Error> {
        Ok(Frame::new(&mut self.inner))
    }

    fn clear(&mut self, _color: Color) -> Result<(), Self::Error> {
        // Actual clear happens in the Smithay frame render
        Ok(())
    }

    fn render_buffer(
        &mut self,
        _buffer: &BufferRef,
        _location: Point,
        _scale: f64,
    ) -> Result<(), Self::Error> {
        // TODO: implement buffer upload and rendering
        // For now this is a placeholder for the migration
        Ok(())
    }

    fn render_elements(
        &mut self,
        _elements: &[RenderElement],
        _scale: f64,
    ) -> Result<(), Self::Error> {
        // TODO: convert RenderElement to Smithay render elements
        Ok(())
    }

    fn commit_frame(&mut self, _frame: Frame<'_>) -> Result<(), Self::Error> {
        // Frame commit happens through the backend (winit/udev)
        Ok(())
    }
}
