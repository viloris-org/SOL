//! Software composition of committed SCP surfaces into one output image.
//!
//! [`crate::scp::stack::WindowStack`] answers *which* surface owns a point;
//! this module answers *what the output looks like*. It walks the same stack
//! bottom-up — the order a painter works in — maps each surface's committed
//! shared-memory buffer, and blends it into an output-sized image.
//!
//! The result is a plain RGBA8 image rather than a GPU texture on purpose. A
//! presentation backend (DRM/KMS dumb buffer, a test harness, a screenshot) can
//! consume it without any of them needing to agree on a graphics API first, and
//! the composition itself stays deterministic enough to assert on pixel values
//! in a headless test.
//!
//! ## Alpha
//!
//! SCP shared-memory content is **premultiplied**: a half-transparent red pixel
//! is `(0.5, 0, 0, 0.5)`, not `(1, 0, 0, 0.5)`. Compositing is therefore plain
//! source-over, `dst = src + dst * (1 - src_alpha)`, with no per-pixel divide.
//! `Xrgb8888` carries no alpha at all and is composited as fully opaque.
//!
//! The clear color is opaque, and source-over onto an opaque destination stays
//! opaque, so a composed [`Framebuffer`] is always fully opaque. That is what
//! lets [`Framebuffer::to_png`] emit the buffer verbatim: premultiplied and
//! straight alpha agree at `alpha = 255`.

use crate::scp::{
    protocol::{BufferFormat, Rect, SessionId, SurfaceId},
    stack::WindowStack,
    surface::{CapturePolicy, SurfaceBuffer, SurfaceBufferKind},
};
use std::os::fd::RawFd;

/// Bytes one composed pixel occupies.
pub const BYTES_PER_PIXEL: usize = 4;

/// Largest output a single composition pass will allocate.
///
/// The dimensions reaching [`compose`] come from output configuration rather
/// than from a client, but composition allocates `width * height * 4` bytes in
/// one go, so a misconfigured or hot-plugged mode should fail the frame instead
/// of the process. 16K by 16K is roughly 4 GiB and far past any real display.
pub const MAX_OUTPUT_DIMENSION: i32 = 16_384;

/// Opaque replacement painted over protected content in every capture frame.
///
/// The compositor owns this value; clients cannot choose a transparent color
/// or otherwise reveal the windows behind a protected surface.
pub const CAPTURE_REDACTION: Rgba8 = Rgba8(18, 18, 20, 255);

/// Why an output is being composed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPurpose {
    /// A local display scanout. Protected content remains visible.
    Display,
    /// Any pixel path that can leave the compositor: screenshots, recording,
    /// screen sharing, remote desktop, previews, or machine vision.
    Capture,
}

/// An 8-bit-per-channel color in the composition's own component order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba8(pub u8, pub u8, pub u8, pub u8);

impl Rgba8 {
    /// Fully opaque black, the default clear color of an output with no
    /// background surface mapped on it.
    pub const BLACK: Self = Self(0, 0, 0, 255);

    const fn as_bytes(self) -> [u8; BYTES_PER_PIXEL] {
        [self.0, self.1, self.2, self.3]
    }
}

/// A composed output image: row-major, tightly packed, RGBA8, opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Framebuffer {
    /// Allocate an output-sized image filled with one color.
    ///
    /// Returns `None` for a degenerate or implausibly large output rather than
    /// attempting the allocation.
    pub fn filled(width: i32, height: i32, color: Rgba8) -> Option<Self> {
        if width <= 0
            || height <= 0
            || width > MAX_OUTPUT_DIMENSION
            || height > MAX_OUTPUT_DIMENSION
        {
            return None;
        }
        let width = u32::try_from(width).ok()?;
        let height = u32::try_from(height).ok()?;
        let length = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(BYTES_PER_PIXEL)?;

        let mut pixels = vec![0_u8; length];
        let value = color.as_bytes();
        for chunk in pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            chunk.copy_from_slice(&value);
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Row-major RGBA8 bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Copy a clipped rectangle into a tightly-packed framebuffer.
    pub fn cropped(&self, rect: Rect) -> Option<Self> {
        let left = rect.x.max(0).min(self.width as i32);
        let top = rect.y.max(0).min(self.height as i32);
        let right = rect
            .x
            .saturating_add(rect.width)
            .max(0)
            .min(self.width as i32);
        let bottom = rect
            .y
            .saturating_add(rect.height)
            .max(0)
            .min(self.height as i32);
        if left >= right || top >= bottom {
            return None;
        }
        let width = u32::try_from(right - left).ok()?;
        let height = u32::try_from(bottom - top).ok()?;
        let row_bytes = width as usize * BYTES_PER_PIXEL;
        let mut pixels = Vec::with_capacity(row_bytes * height as usize);
        for y in top..bottom {
            let start = (y as usize * self.width as usize + left as usize) * BYTES_PER_PIXEL;
            pixels.extend_from_slice(self.pixels.get(start..start + row_bytes)?);
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Read one pixel, or `None` when the coordinate is outside the image.
    ///
    /// Present for tests and screenshot tooling; composition itself works on
    /// row slices rather than going through this.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y as usize * self.width as usize + x as usize) * BYTES_PER_PIXEL;
        let pixel = self.pixels.get(index..index + BYTES_PER_PIXEL)?;
        Some(Rgba8(pixel[0], pixel[1], pixel[2], pixel[3]))
    }

    /// Encode the image as a PNG.
    ///
    /// Uses stored (uncompressed) deflate blocks so the compositor keeps its
    /// zero-dependency posture — this exists to make a composed desktop
    /// reviewable and to attach evidence to a test run, not to be a fast or
    /// space-efficient image codec.
    pub fn to_png(&self) -> Vec<u8> {
        png::encode(self.width, self.height, &self.pixels)
    }

    /// Blend one surface's pixels into the image at an absolute position.
    ///
    /// `source` is the surface's own row-major content; `rect` is where the
    /// stack placed it on the output layout. Everything outside the image is
    /// clipped, including negative origins, so a surface that hangs off an edge
    /// contributes only its visible part.
    fn blend(&mut self, rect: Rect, source: &SurfacePixels<'_>) {
        // Intersect the surface rectangle with both the framebuffer and the
        // extent the client actually supplied. A layer surface is configured to
        // a size, but the buffer it attached may be smaller: painting the
        // configured rectangle would read past the mapping.
        let painted_width = rect.width.min(source.width);
        let painted_height = rect.height.min(source.height);
        if painted_width <= 0 || painted_height <= 0 {
            return;
        }

        let left = rect.x.max(0);
        let top = rect.y.max(0);
        let right = rect.x.saturating_add(painted_width).min(self.width as i32);
        let bottom = rect
            .y
            .saturating_add(painted_height)
            .min(self.height as i32);
        if left >= right || top >= bottom {
            return;
        }

        for y in top..bottom {
            let source_y = y - rect.y;
            let Some(source_row) = source.row(source_y) else {
                continue;
            };
            for x in left..right {
                let source_x = x - rect.x;
                let Some(pixel) = source.pixel_in(source_row, source_x) else {
                    continue;
                };
                let index = (y as usize * self.width as usize + x as usize) * BYTES_PER_PIXEL;
                let Some(destination) = self.pixels.get_mut(index..index + BYTES_PER_PIXEL) else {
                    continue;
                };
                blend_over(destination, pixel);
            }
        }
    }

    /// Paint an opaque rectangle, clipped to this framebuffer.
    fn fill_rect(&mut self, rect: Rect, color: Rgba8) {
        let left = rect.x.max(0).min(self.width as i32);
        let top = rect.y.max(0).min(self.height as i32);
        let right = rect
            .x
            .saturating_add(rect.width)
            .max(0)
            .min(self.width as i32);
        let bottom = rect
            .y
            .saturating_add(rect.height)
            .max(0)
            .min(self.height as i32);
        if left >= right || top >= bottom {
            return;
        }

        let value = color.as_bytes();
        for y in top..bottom {
            for x in left..right {
                let index = (y as usize * self.width as usize + x as usize) * BYTES_PER_PIXEL;
                if let Some(destination) = self.pixels.get_mut(index..index + BYTES_PER_PIXEL) {
                    destination.copy_from_slice(&value);
                }
            }
        }
    }
}

/// Source-over blend of a premultiplied source pixel onto a destination pixel.
fn blend_over(destination: &mut [u8], source: Rgba8) {
    if source.3 == u8::MAX {
        destination.copy_from_slice(&source.as_bytes());
        return;
    }
    if source.3 == 0 {
        return;
    }
    let inverse = u32::from(u8::MAX - source.3);
    let mix = |source: u8, destination: u8| -> u8 {
        // `+ 127` rounds to nearest rather than truncating, which keeps a chain
        // of translucent overlays from drifting darker frame after frame.
        let scaled = (u32::from(destination) * inverse + 127) / 255;
        u8::try_from(u32::from(source) + scaled).unwrap_or(u8::MAX)
    };
    destination[0] = mix(source.0, destination[0]);
    destination[1] = mix(source.1, destination[1]);
    destination[2] = mix(source.2, destination[2]);
    destination[3] = mix(source.3, destination[3]);
}

/// Read access to the committed buffer of one surface.
///
/// [`compose`] takes this rather than [`crate::scp::ScpState`] directly so a
/// test can compose a synthetic stack, and so the composition pass cannot
/// mutate protocol state while it walks it.
pub trait BufferSource {
    /// The buffer a surface last committed, or `None` when it has never
    /// committed one — an unmapped surface, which contributes no pixels.
    fn committed_buffer(
        &self,
        session_id: SessionId,
        surface_id: SurfaceId,
    ) -> Option<&SurfaceBuffer>;

    /// Compositor-owned capture policy for a surface.
    ///
    /// There is deliberately no default: every new buffer source must make an
    /// explicit policy decision before it can be used for capture composition.
    fn capture_policy(&self, session_id: SessionId, surface_id: SurfaceId) -> CapturePolicy;
}

/// Compose every mapped surface in `stack` into an image of `output`.
///
/// Surfaces are painted bottom-up, so the background layer lands first and the
/// overlay layer last. A surface whose buffer cannot be mapped is skipped and
/// logged: one client's bad descriptor must not cost the whole desktop its
/// frame.
pub fn compose(
    output: Rect,
    clear: Rgba8,
    stack: &WindowStack,
    source: &impl BufferSource,
) -> Option<Framebuffer> {
    compose_for(RenderPurpose::Display, output, clear, stack, source)
}

/// Compose an output for either local display or a capture consumer.
///
/// Capture exclusion is applied before a protected buffer is mapped. This is
/// important for the future protected GPU-buffer path: capture must be able to
/// redact a surface it is technically incapable of reading, and no protected
/// pixel may enter an effect or intermediate capture buffer first.
pub fn compose_for(
    purpose: RenderPurpose,
    output: Rect,
    clear: Rgba8,
    stack: &WindowStack,
    source: &impl BufferSource,
) -> Option<Framebuffer> {
    let mut framebuffer = Framebuffer::filled(output.width, output.height, clear)?;

    for entry in stack.iter_bottom_up() {
        let local_rect = Rect {
            x: entry.rect.x - output.x,
            y: entry.rect.y - output.y,
            width: entry.rect.width,
            height: entry.rect.height,
        };

        if purpose == RenderPurpose::Capture
            && matches!(
                source.capture_policy(entry.session_id, entry.surface_id),
                CapturePolicy::Excluded(_)
            )
        {
            // Never skip an excluded surface: doing that would expose content
            // behind a window that is opaque on the physical display.
            framebuffer.fill_rect(local_rect, CAPTURE_REDACTION);
            continue;
        }

        let Some(buffer) = source.committed_buffer(entry.session_id, entry.surface_id) else {
            continue;
        };
        let mapping = match Mapping::read_only_buffer(buffer) {
            Ok(mapping) => mapping,
            Err(error) => {
                tracing::warn!(
                    session_id = entry.session_id,
                    surface_id = entry.surface_id,
                    %error,
                    "skipping a surface whose committed buffer could not be mapped"
                );
                continue;
            }
        };
        let Some(pixels) = SurfacePixels::new(mapping.as_slice(), buffer) else {
            continue;
        };

        // Translate the absolute stack rectangle into output-local space: an
        // output at x=1920 in a multi-output layout draws its own surfaces at
        // x=0 in its own framebuffer.
        framebuffer.blend(local_rect, &pixels);
    }

    Some(framebuffer)
}

/// A mapped buffer interpreted through its declared format and stride.
struct SurfacePixels<'a> {
    bytes: &'a [u8],
    width: i32,
    height: i32,
    stride: usize,
    format: BufferFormat,
}

impl<'a> SurfacePixels<'a> {
    fn new(bytes: &'a [u8], buffer: &SurfaceBuffer) -> Option<Self> {
        let stride = usize::try_from(buffer.stride).ok()?;
        if buffer.width <= 0 || buffer.height <= 0 {
            return None;
        }
        Some(Self {
            bytes,
            width: buffer.width,
            height: buffer.height,
            stride,
            format: buffer.format,
        })
    }

    fn row(&self, y: i32) -> Option<&'a [u8]> {
        if y < 0 || y >= self.height {
            return None;
        }
        let start = usize::try_from(y).ok()?.checked_mul(self.stride)?;
        self.bytes.get(start..start.checked_add(self.stride)?)
    }

    fn pixel_in(&self, row: &[u8], x: i32) -> Option<Rgba8> {
        if x < 0 || x >= self.width {
            return None;
        }
        let bytes_per_pixel = usize::try_from(self.format.bytes_per_pixel()).ok()?;
        let start = usize::try_from(x).ok()?.checked_mul(bytes_per_pixel)?;
        let pixel = row.get(start..start.checked_add(bytes_per_pixel)?)?;
        Some(decode(self.format, pixel))
    }
}

/// Reinterpret four buffer bytes as an RGBA8 pixel.
///
/// `Argb8888` and `Xrgb8888` are the little-endian 32-bit words the name
/// implies, so on the wire their bytes arrive as B, G, R, A.
fn decode(format: BufferFormat, pixel: &[u8]) -> Rgba8 {
    match format {
        BufferFormat::Argb8888 => Rgba8(pixel[2], pixel[1], pixel[0], pixel[3]),
        BufferFormat::Xrgb8888 => Rgba8(pixel[2], pixel[1], pixel[0], u8::MAX),
        BufferFormat::Rgba8888 => Rgba8(pixel[0], pixel[1], pixel[2], pixel[3]),
        BufferFormat::Rgb565 => {
            let packed = u16::from_le_bytes([pixel[0], pixel[1]]);
            let red = ((packed >> 11) & 0x1f) as u8;
            let green = ((packed >> 5) & 0x3f) as u8;
            let blue = (packed & 0x1f) as u8;
            Rgba8(
                (red << 3) | (red >> 2),
                (green << 2) | (green >> 4),
                (blue << 3) | (blue >> 2),
                u8::MAX,
            )
        }
    }
}

/// A read-only mapping of a client buffer, unmapped on drop.
struct Mapping {
    address: *mut libc::c_void,
    length: usize,
    data_offset: usize,
    dma_sync_fd: Option<RawFd>,
}

impl Mapping {
    /// Map a committed surface buffer for reading.
    ///
    /// Revalidates the descriptor rather than trusting the check made when the
    /// buffer was attached. [`crate::scp::buffer::validate_descriptor`] enforces
    /// both halves of what makes client memory safe to map: the file really is
    /// as large as declared, and it carries `F_SEAL_SHRINK` so it cannot become
    /// smaller afterwards. Without that seal a client could `ftruncate` between
    /// attach and composition, and the compositor — not the client — would take
    /// the `SIGBUS`, ending every session on the machine.
    fn read_only_buffer(buffer: &SurfaceBuffer) -> Result<Self, String> {
        let required = crate::scp::buffer::validate_geometry(
            buffer.width,
            buffer.height,
            buffer.stride,
            buffer.format.bytes_per_pixel(),
        )?;
        let extent = buffer
            .offset
            .checked_add(required)
            .ok_or("buffer offset and byte length overflow")?;
        match buffer.kind {
            SurfaceBufferKind::Shm => {
                crate::scp::buffer::validate_descriptor(buffer.fd, extent)?;
            }
            SurfaceBufferKind::Dmabuf => {
                if buffer.fd < 0 {
                    return Err("DMA-BUF requires the native GPU importer".to_string());
                }
                let actual = crate::scp::unix_socket::fd_size(buffer.fd)
                    .map_err(|error| format!("Cannot size DMA-BUF descriptor: {error}"))?;
                if actual > 0 && u64::try_from(extent).unwrap_or(u64::MAX) > actual {
                    return Err(format!(
                        "DMA-BUF needs {extent} bytes but its descriptor has {actual}"
                    ));
                }
            }
        }
        Self::read_only(
            buffer.fd,
            extent,
            buffer.offset,
            (buffer.kind == SurfaceBufferKind::Dmabuf).then_some(buffer.fd),
        )
    }

    fn read_only(
        fd: RawFd,
        length: usize,
        data_offset: usize,
        dma_sync_fd: Option<RawFd>,
    ) -> Result<Self, String> {
        if length == 0 {
            return Err("refusing to map an empty buffer".to_string());
        }
        // SAFETY: `length` is the validated byte extent of `fd`, checked against
        // the descriptor's real size immediately above. The mapping is private
        // and read-only, and is unmapped in `Drop` with the same length.
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                fd,
                0,
            )
        };
        if address == libc::MAP_FAILED {
            return Err(format!(
                "mmap of a client buffer failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if let Some(fd) = dma_sync_fd
            && let Err(error) = dma_buf_sync(fd, DMA_BUF_SYNC_START | DMA_BUF_SYNC_READ)
        {
            unsafe {
                libc::munmap(address, length);
            }
            return Err(error);
        }
        Ok(Self {
            address,
            length,
            data_offset,
            dma_sync_fd,
        })
    }

    fn as_slice(&self) -> &[u8] {
        // SAFETY: `address` is a live mapping of exactly `length` readable
        // bytes, and the returned slice cannot outlive `self`.
        let mapping = unsafe { std::slice::from_raw_parts(self.address.cast::<u8>(), self.length) };
        &mapping[self.data_offset..]
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if let Some(fd) = self.dma_sync_fd {
            let _ = dma_buf_sync(fd, DMA_BUF_SYNC_END | DMA_BUF_SYNC_READ);
        }
        // SAFETY: unmapping the same address and length that `mmap` returned.
        unsafe {
            libc::munmap(self.address, self.length);
        }
    }
}

const DMA_BUF_SYNC_READ: u64 = 1;
const DMA_BUF_SYNC_START: u64 = 0;
const DMA_BUF_SYNC_END: u64 = 4;
const DMA_BUF_IOCTL_SYNC: libc::c_ulong = 0x4008_6200;

fn dma_buf_sync(fd: RawFd, flags: u64) -> Result<(), String> {
    let result = unsafe { libc::ioctl(fd, DMA_BUF_IOCTL_SYNC, &flags) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    // A fake mmap-able fd is useful for deterministic tests. Real DMA-BUFs
    // support the sync ioctl; other errors still reject the frame.
    if error.raw_os_error() == Some(libc::ENOTTY) {
        return Ok(());
    }
    Err(format!("DMA_BUF_IOCTL_SYNC failed: {error}"))
}

/// Minimal PNG writer.
///
/// Deliberately not a general image library: it emits exactly the one form the
/// compositor needs — 8-bit RGBA, no interlacing, no filtering — with stored
/// deflate blocks so that no compression dependency enters the crate.
mod png {
    /// Largest payload a single stored deflate block can carry.
    const MAX_STORED_BLOCK: usize = u16::MAX as usize;

    pub fn encode(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

        let mut header = Vec::with_capacity(13);
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, no interlace
        chunk(&mut output, b"IHDR", &header);

        chunk(
            &mut output,
            b"IDAT",
            &zlib(&scanlines(width, height, pixels)),
        );
        chunk(&mut output, b"IEND", &[]);
        output
    }

    /// Prefix every row with PNG filter type 0 ("None").
    fn scanlines(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let row_length = width as usize * super::BYTES_PER_PIXEL;
        let mut raw = Vec::with_capacity(height as usize * (row_length + 1));
        for y in 0..height as usize {
            raw.push(0);
            let start = y * row_length;
            match pixels.get(start..start + row_length) {
                Some(row) => raw.extend_from_slice(row),
                None => raw.resize(raw.len() + row_length, 0),
            }
        }
        raw
    }

    /// Wrap data in a zlib stream made only of stored deflate blocks.
    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut stream = vec![0x78, 0x01]; // deflate, 32K window, fastest
        if data.is_empty() {
            stream.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]);
        }
        for (index, block) in data.chunks(MAX_STORED_BLOCK).enumerate() {
            let final_block = (index + 1) * MAX_STORED_BLOCK >= data.len();
            stream.push(u8::from(final_block));
            let length = block.len() as u16;
            stream.extend_from_slice(&length.to_le_bytes());
            stream.extend_from_slice(&(!length).to_le_bytes());
            stream.extend_from_slice(block);
        }
        stream.extend_from_slice(&adler32(data).to_be_bytes());
        stream
    }

    fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
        output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        output.extend_from_slice(kind);
        output.extend_from_slice(payload);

        let mut digest = crc32(u32::MAX, kind);
        digest = crc32(digest, payload);
        output.extend_from_slice(&(digest ^ u32::MAX).to_be_bytes());
    }

    fn crc32(mut digest: u32, data: &[u8]) -> u32 {
        for byte in data {
            digest ^= u32::from(*byte);
            for _ in 0..8 {
                let carry = digest & 1;
                digest >>= 1;
                if carry != 0 {
                    digest ^= 0xedb8_8320;
                }
            }
        }
        digest
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut low: u32 = 1;
        let mut high: u32 = 0;
        for byte in data {
            low = (low + u32::from(*byte)) % 65_521;
            high = (high + low) % 65_521;
        }
        (high << 16) | low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::{memfd, stack::StackEntry, stack::StackKind};
    use std::io::Write;

    struct Fixture {
        buffers: Vec<((SessionId, SurfaceId), SurfaceBuffer)>,
        excluded: Vec<(SessionId, SurfaceId)>,
    }

    impl BufferSource for Fixture {
        fn committed_buffer(
            &self,
            session_id: SessionId,
            surface_id: SurfaceId,
        ) -> Option<&SurfaceBuffer> {
            self.buffers
                .iter()
                .find(|(key, _)| *key == (session_id, surface_id))
                .map(|(_, buffer)| buffer)
        }

        fn capture_policy(&self, session_id: SessionId, surface_id: SurfaceId) -> CapturePolicy {
            if self.excluded.contains(&(session_id, surface_id)) {
                CapturePolicy::Excluded(crate::scp::surface::ProtectionReason::Drm)
            } else {
                CapturePolicy::Allowed
            }
        }
    }

    /// Build a real sealed memfd-backed buffer filled with one RGBA color.
    fn buffer(width: i32, height: i32, color: [u8; 4]) -> SurfaceBuffer {
        let mut file = memfd::create_file("scp-compose-test").expect("create memfd");
        let pixels: Vec<u8> = std::iter::repeat_n(color, (width * height) as usize)
            .flatten()
            .collect();
        file.write_all(&pixels).expect("write pixels");
        let fd = memfd::into_raw_fd(file);
        memfd::seal_readonly(fd).expect("seal the buffer as a client must");
        SurfaceBuffer {
            buffer_id: 1,
            offset: 0,
            managed: false,
            kind: SurfaceBufferKind::Shm,
            fd,
            width,
            height,
            stride: width * 4,
            format: BufferFormat::Rgba8888,
        }
    }

    fn entry(surface_id: SurfaceId, rect: Rect) -> StackEntry {
        StackEntry {
            session_id: 1,
            surface_id,
            kind: StackKind::Toplevel(surface_id),
            rect,
            accepts_keyboard: false,
        }
    }

    fn output() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        }
    }

    #[test]
    fn an_output_with_no_mapped_surface_composes_to_the_clear_color() {
        let fixture = Fixture {
            buffers: Vec::new(),
            excluded: Vec::new(),
        };
        let frame = compose(output(), Rgba8::BLACK, &WindowStack::new(), &fixture)
            .expect("compose an empty output");

        assert_eq!((frame.width(), frame.height()), (8, 8));
        assert_eq!(frame.pixel(0, 0), Some(Rgba8::BLACK));
        assert_eq!(frame.pixel(7, 7), Some(Rgba8::BLACK));
        assert_eq!(frame.pixel(8, 0), None);
    }

    #[test]
    fn surfaces_paint_bottom_up_so_the_topmost_wins() {
        let fixture = Fixture {
            buffers: vec![
                ((1, 1), buffer(8, 8, [255, 0, 0, 255])),
                ((1, 2), buffer(4, 4, [0, 255, 0, 255])),
            ],
            excluded: Vec::new(),
        };
        // `WindowStack` is ordered topmost-first: surface 2 is above surface 1.
        let mut stack = WindowStack::new();
        stack.push(entry(
            2,
            Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            },
        ));
        stack.push(entry(1, output()));

        let frame = compose(output(), Rgba8::BLACK, &stack, &fixture).expect("compose");
        assert_eq!(frame.pixel(0, 0), Some(Rgba8(0, 255, 0, 255)));
        assert_eq!(frame.pixel(5, 5), Some(Rgba8(255, 0, 0, 255)));
    }

    #[test]
    fn a_surface_is_clipped_to_the_output_instead_of_overflowing_it() {
        let fixture = Fixture {
            buffers: vec![((1, 1), buffer(8, 8, [0, 0, 255, 255]))],
            excluded: Vec::new(),
        };
        let mut stack = WindowStack::new();
        stack.push(entry(
            1,
            Rect {
                x: 6,
                y: -2,
                width: 8,
                height: 8,
            },
        ));

        let frame = compose(output(), Rgba8::BLACK, &stack, &fixture).expect("compose");
        assert_eq!(frame.pixel(7, 0), Some(Rgba8(0, 0, 255, 255)));
        assert_eq!(frame.pixel(5, 0), Some(Rgba8::BLACK));
        assert_eq!(frame.pixel(7, 6), Some(Rgba8::BLACK));
    }

    #[test]
    fn a_translucent_surface_blends_with_what_is_beneath_it() {
        // Premultiplied 50% white over opaque black.
        let fixture = Fixture {
            buffers: vec![((1, 1), buffer(8, 8, [128, 128, 128, 128]))],
            excluded: Vec::new(),
        };
        let mut stack = WindowStack::new();
        stack.push(entry(1, output()));

        let frame = compose(output(), Rgba8::BLACK, &stack, &fixture).expect("compose");
        let pixel = frame.pixel(0, 0).expect("pixel inside the output");
        assert_eq!(pixel, Rgba8(128, 128, 128, 255));
    }

    #[test]
    fn a_surface_smaller_than_its_configured_rectangle_paints_only_what_it_supplied() {
        let fixture = Fixture {
            buffers: vec![((1, 1), buffer(2, 2, [255, 255, 0, 255]))],
            excluded: Vec::new(),
        };
        let mut stack = WindowStack::new();
        stack.push(entry(1, output()));

        let frame = compose(output(), Rgba8::BLACK, &stack, &fixture).expect("compose");
        assert_eq!(frame.pixel(1, 1), Some(Rgba8(255, 255, 0, 255)));
        assert_eq!(frame.pixel(2, 2), Some(Rgba8::BLACK));
    }

    #[test]
    fn a_composed_frame_encodes_as_a_well_formed_png() {
        let frame = Framebuffer::filled(4, 3, Rgba8(10, 20, 30, 255)).expect("allocate");
        let png = frame.to_png();

        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &4_u32.to_be_bytes());
        assert_eq!(&png[20..24], &3_u32.to_be_bytes());
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn capture_replaces_a_protected_surface_without_exposing_what_is_behind_it() {
        let fixture = Fixture {
            buffers: vec![
                ((1, 1), buffer(8, 8, [0, 0, 255, 255])),
                ((1, 2), buffer(4, 4, [255, 0, 0, 255])),
            ],
            excluded: vec![(1, 2)],
        };
        let mut stack = WindowStack::new();
        stack.push(entry(
            2,
            Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            },
        ));
        stack.push(entry(1, output()));

        let display = compose_for(
            RenderPurpose::Display,
            output(),
            Rgba8::BLACK,
            &stack,
            &fixture,
        )
        .expect("compose display");
        let capture = compose_for(
            RenderPurpose::Capture,
            output(),
            Rgba8::BLACK,
            &stack,
            &fixture,
        )
        .expect("compose capture");

        assert_eq!(display.pixel(0, 0), Some(Rgba8(255, 0, 0, 255)));
        assert_eq!(capture.pixel(0, 0), Some(CAPTURE_REDACTION));
        assert_eq!(capture.pixel(5, 5), Some(Rgba8(0, 0, 255, 255)));
    }

    #[test]
    fn capture_redacts_a_protected_surface_even_without_a_mappable_buffer() {
        let fixture = Fixture {
            buffers: Vec::new(),
            excluded: vec![(1, 1)],
        };
        let mut stack = WindowStack::new();
        stack.push(entry(1, output()));

        let capture = compose_for(
            RenderPurpose::Capture,
            output(),
            Rgba8::BLACK,
            &stack,
            &fixture,
        )
        .expect("compose capture");

        assert_eq!(capture.pixel(0, 0), Some(CAPTURE_REDACTION));
        assert_eq!(capture.pixel(7, 7), Some(CAPTURE_REDACTION));
    }

    #[test]
    fn pooled_rgb565_content_is_read_from_its_declared_offset() {
        let mut file = memfd::create_file("scp-offset-compose-test").unwrap();
        // Prefix bytes model another buffer in the same pool. 0xf800 is pure
        // red in little-endian RGB565.
        file.write_all(&[9, 8, 7, 0x00, 0xf8]).unwrap();
        let fd = memfd::into_raw_fd(file);
        memfd::seal_readonly(fd).unwrap();
        let fixture = Fixture {
            buffers: vec![(
                (1, 1),
                SurfaceBuffer {
                    buffer_id: 4,
                    offset: 3,
                    managed: true,
                    kind: SurfaceBufferKind::Shm,
                    fd,
                    width: 1,
                    height: 1,
                    stride: 2,
                    format: BufferFormat::Rgb565,
                },
            )],
            excluded: Vec::new(),
        };
        let mut stack = WindowStack::new();
        stack.push(entry(
            1,
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        ));

        let frame = compose(
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            Rgba8::BLACK,
            &stack,
            &fixture,
        )
        .unwrap();
        assert_eq!(frame.pixel(0, 0), Some(Rgba8(255, 0, 0, 255)));
    }

    #[test]
    fn an_implausible_output_size_fails_the_frame_rather_than_the_allocator() {
        assert!(Framebuffer::filled(0, 100, Rgba8::BLACK).is_none());
        assert!(Framebuffer::filled(100, -1, Rgba8::BLACK).is_none());
        assert!(Framebuffer::filled(MAX_OUTPUT_DIMENSION + 1, 4, Rgba8::BLACK).is_none());
    }
}
