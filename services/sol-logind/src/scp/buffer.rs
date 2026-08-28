//! Shared-memory frame buffer handed to the compositor.
//!
//! The login screen draws with Slint's software renderer, which writes into a
//! plain pixel slice. That slice has to be memory the compositor can also map,
//! so it is backed by a memfd rather than a `Vec`: the descriptor travels over
//! SCP with `AttachBuffer`, and both processes then look at the same pages.

use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
};

use slint::platform::software_renderer::PremultipliedRgbaColor;
use sol_compositor::scp::{memfd, protocol::BufferFormat};

/// Bytes one pixel occupies. `PremultipliedRgbaColor` is `#[repr(C)]` over four
/// `u8`s, so its memory order is R, G, B, A — which is what SCP's
/// [`BufferFormat::Rgba8888`] names.
const BYTES_PER_PIXEL: usize = 4;

/// The SCP format this buffer's contents are in.
///
/// Slint's software renderer produces *premultiplied* alpha. SCP does not spell
/// that out per format, so it is recorded here for whoever writes the compositor
/// renderer.
pub const FORMAT: BufferFormat = BufferFormat::Rgba8888;

/// A memfd-backed pixel buffer sized to one output.
pub struct FrameBuffer {
    fd: OwnedFd,
    /// Start of the shared mapping. Always non-null and `len` bytes long.
    mapping: *mut u8,
    len: usize,
    width: i32,
    height: i32,
}

impl FrameBuffer {
    /// Allocate a buffer for a `width`×`height` surface.
    pub fn new(width: i32, height: i32) -> io::Result<Self> {
        let (len, pixels) = extent(width, height)?;

        let fd = memfd::create("sol-logind-frame", true)?;
        // SAFETY: `create` returned a fresh descriptor that nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        set_length(fd.as_raw_fd(), len)?;

        // Fixing the size before the compositor ever sees the descriptor is what
        // lets it trust the geometry it was told. Writes stay allowed: every
        // frame is drawn into these same pages.
        memfd::add_seals(fd.as_raw_fd(), memfd::F_SEAL_SHRINK | memfd::F_SEAL_GROW)?;

        let mapping = map(fd.as_raw_fd(), len)?;
        debug_assert_eq!(pixels * BYTES_PER_PIXEL, len);

        Ok(Self {
            fd,
            mapping,
            len,
            width,
            height,
        })
    }

    /// Resize to `width`×`height`, reallocating only when the size really changed.
    ///
    /// The seals make the existing descriptor un-resizable on purpose, so a
    /// changed output size means a new buffer and a fresh `AttachBuffer`.
    pub fn resize(&mut self, width: i32, height: i32) -> io::Result<bool> {
        if self.width == width && self.height == height {
            return Ok(false);
        }
        *self = Self::new(width, height)?;
        Ok(true)
    }

    /// The pixels, as the software renderer's target type.
    pub fn pixels(&mut self) -> &mut [PremultipliedRgbaColor] {
        // SAFETY: the mapping is `len` writable bytes, `PremultipliedRgbaColor`
        // is four `u8`s with alignment 1 (page-aligned mmap satisfies it), and
        // `len` is an exact multiple of that size. `&mut self` keeps this the
        // only live reference on our side.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.mapping.cast::<PremultipliedRgbaColor>(),
                self.len / BYTES_PER_PIXEL,
            )
        }
    }

    pub const fn width(&self) -> i32 {
        self.width
    }

    pub const fn height(&self) -> i32 {
        self.height
    }

    /// Row length in bytes, which is what SCP's `stride` means.
    pub const fn stride(&self) -> i32 {
        self.width * BYTES_PER_PIXEL as i32
    }

    /// Row length in pixels, which is what the software renderer means by stride.
    pub const fn pixel_stride(&self) -> usize {
        self.width as usize
    }

    /// The descriptor to hand over with `AttachBuffer`.
    pub fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Drop for FrameBuffer {
    fn drop(&mut self) {
        // SAFETY: `mapping`/`len` are exactly what `mmap` returned and have not
        // been unmapped before now.
        unsafe {
            libc::munmap(self.mapping.cast::<libc::c_void>(), self.len);
        }
    }
}

// SAFETY: the mapping is owned solely by this struct and only reachable through
// `&mut self`, so moving one between threads hands over exclusive access.
unsafe impl Send for FrameBuffer {}

/// Byte length and pixel count for a surface, rejecting anything unusable.
fn extent(width: i32, height: i32) -> io::Result<(usize, usize)> {
    if width <= 0 || height <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("frame buffer dimensions must be positive, got {width}x{height}"),
        ));
    }
    // Widening first makes the product exact: two `i32`s always fit in an `i64`.
    let pixels = i64::from(width) * i64::from(height);
    let len = pixels
        .checked_mul(BYTES_PER_PIXEL as i64)
        .and_then(|len| usize::try_from(len).ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("frame buffer for {width}x{height} does not fit in memory"),
            )
        })?;
    Ok((len, pixels as usize))
}

fn set_length(fd: RawFd, len: usize) -> io::Result<()> {
    let length = libc::off_t::try_from(len)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame buffer is too large"))?;
    // SAFETY: ftruncate on an owned memfd.
    if unsafe { libc::ftruncate(fd, length) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn map(fd: RawFd, len: usize) -> io::Result<*mut u8> {
    // SAFETY: mapping `len` bytes of an owned descriptor that was just sized to
    // exactly that length.
    let mapping = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if mapping == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    Ok(mapping.cast::<u8>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_a_mapping_of_the_requested_size() {
        let mut buffer = FrameBuffer::new(64, 32).expect("allocate frame buffer");
        assert_eq!(buffer.width(), 64);
        assert_eq!(buffer.height(), 32);
        assert_eq!(buffer.stride(), 64 * 4);
        assert_eq!(buffer.pixel_stride(), 64);
        assert_eq!(buffer.pixels().len(), 64 * 32);
    }

    #[test]
    fn starts_zeroed_and_keeps_what_is_written() {
        let mut buffer = FrameBuffer::new(8, 8).expect("allocate frame buffer");
        assert!(buffer.pixels().iter().all(|pixel| pixel.alpha == 0));

        buffer.pixels()[9] = PremultipliedRgbaColor {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 4,
        };
        assert_eq!(buffer.pixels()[9].blue, 3);
        assert_eq!(buffer.pixels()[8].blue, 0);
    }

    #[test]
    fn resize_reallocates_only_on_a_real_change() {
        let mut buffer = FrameBuffer::new(16, 16).expect("allocate frame buffer");
        let original = buffer.as_raw_fd();

        assert!(!buffer.resize(16, 16).expect("no-op resize"));
        assert_eq!(buffer.as_raw_fd(), original);

        assert!(buffer.resize(32, 8).expect("resize"));
        assert_eq!(buffer.width(), 32);
        assert_eq!(buffer.pixels().len(), 32 * 8);
    }

    #[test]
    fn rejects_degenerate_dimensions() {
        for (width, height) in [(0, 16), (16, 0), (-1, 16)] {
            assert!(
                FrameBuffer::new(width, height).is_err(),
                "{width}x{height} must be rejected"
            );
        }
    }
}
