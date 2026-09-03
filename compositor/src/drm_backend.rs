//! DRM/KMS backend for real hardware presentation.
//!
//! This backend opens a DRM device, enumerates connected displays, sets up
//! kernel mode setting (KMS) to configure each output, and presents composed
//! frames via page flipping synchronized to vblank.

use crate::scp::{
    ScpState,
    compose::{BYTES_PER_PIXEL, Framebuffer},
    protocol::OutputId,
};
use drm::{
    Device,
    control::{
        Device as ControlDevice, Mode, ModeTypeFlags, connector, crtc, dumbbuffer::DumbBuffer,
        framebuffer,
    },
};
use std::{
    fs::{File, OpenOptions},
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// A missing page-flip event must not hang the compositor forever. A quarter
/// second tolerates several missed 60 Hz vblanks while still allowing the main
/// loop to observe shutdown and report a failed connector promptly.
const VBLANK_TIMEOUT: Duration = Duration::from_millis(250);

/// A configured DRM output ready for scanout.
#[allow(dead_code)]
struct DrmOutput {
    output_id: OutputId,
    connector: connector::Handle,
    crtc: crtc::Handle,
    mode: Mode,
    /// Front and back framebuffer handles for double-buffering.
    front_fb: framebuffer::Handle,
    back_fb: framebuffer::Handle,
    /// Corresponding dumb buffer handles.
    front_buf: DumbBuffer,
    back_buf: DumbBuffer,
    /// Memory mappings for writing pixels.
    front_ptr: *mut u8,
    back_ptr: *mut u8,
    size: usize,
    width: u32,
    height: u32,
    stride: u32,
    /// Which buffer is currently being scanned out (true = front).
    using_front: bool,
}

// SAFETY: The pointers are to kernel-managed memory and DRM operations are
// thread-safe at the syscall level.
unsafe impl Send for DrmOutput {}

/// Wrapper around a DRM device file descriptor.
struct DrmDevice {
    file: File,
}

impl AsRawFd for DrmDevice {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.file.as_raw_fd()
    }
}

impl AsFd for DrmDevice {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}

impl Device for DrmDevice {}
impl ControlDevice for DrmDevice {}

/// DRM/KMS backend state.
pub struct DrmBackend {
    device: DrmDevice,
    outputs: Vec<DrmOutput>,
}

impl DrmBackend {
    /// Open the DRM device and enumerate connected displays.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())
            .map_err(|e| format!("Failed to open DRM device: {e}"))?;

        let device = DrmDevice { file };

        // Become DRM master so we can perform mode setting.
        let result = unsafe { libc::ioctl(device.as_raw_fd(), 0x4001641e) }; // DRM_IOCTL_SET_MASTER
        if result != 0 {
            return Err(format!("Failed to become DRM master: errno {}", unsafe {
                *libc::__errno_location()
            }));
        }

        Ok(Self {
            device,
            outputs: Vec::new(),
        })
    }

    /// Scan for connected displays and register them with the compositor.
    pub fn enumerate_outputs(
        &mut self,
        state: &Arc<Mutex<ScpState>>,
    ) -> Result<Vec<OutputId>, String> {
        let resources = self
            .device
            .resource_handles()
            .map_err(|e| format!("Failed to get DRM resources: {e}"))?;

        let mut output_ids = Vec::new();

        for &connector_handle in resources.connectors() {
            let connector_info = self
                .device
                .get_connector(connector_handle, false)
                .map_err(|e| format!("Failed to get connector info: {e}"))?;

            // Only configure connected displays.
            if connector_info.state() != connector::State::Connected {
                continue;
            }

            let modes = connector_info.modes();
            if modes.is_empty() {
                tracing::warn!(?connector_handle, "connector has no modes");
                continue;
            }

            // Pick the preferred mode (first mode with the Preferred flag).
            let mode = modes
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
                .or_else(|| modes.first())
                .copied()
                .ok_or("No usable mode found")?;

            // Find a CRTC for this connector.
            let encoder = connector_info
                .current_encoder()
                .and_then(|enc| self.device.get_encoder(enc).ok())
                .or_else(|| {
                    connector_info
                        .encoders()
                        .iter()
                        .filter_map(|&e| self.device.get_encoder(e).ok())
                        .next()
                })
                .ok_or("No encoder found for connector")?;

            let crtc = encoder
                .crtc()
                .or_else(|| {
                    resources
                        .crtcs()
                        .iter()
                        .copied()
                        .find(|&c| !self.outputs.iter().any(|o| o.crtc == c))
                })
                .ok_or("No available CRTC")?;

            let width = mode.size().0 as i32;
            let height = mode.size().1 as i32;
            let refresh_rate = calculate_refresh_rate(&mode);

            // Register with the compositor.
            let output_id = {
                let mut guard = state
                    .lock()
                    .map_err(|_| "compositor state lock was poisoned")?;
                let connector_name = format!(
                    "{:?}-{}",
                    connector_info.interface(),
                    connector_info.interface_id()
                );
                guard.add_output(
                    connector_name.clone(),
                    format!("DRM output {connector_name}"),
                    width,
                    height,
                    refresh_rate,
                )?
            };

            // Create dumb buffers for this output.
            let (front_fb, front_buf, front_ptr, front_size, front_stride) =
                self.create_dumb_buffer(width as u32, height as u32)?;
            let (back_fb, back_buf, back_ptr, _back_size, _back_stride) =
                self.create_dumb_buffer(width as u32, height as u32)?;

            // Set the initial mode.
            self.device
                .set_crtc(
                    crtc,
                    Some(front_fb),
                    (0, 0),
                    &[connector_handle],
                    Some(mode),
                )
                .map_err(|e| format!("Failed to set CRTC: {e}"))?;

            tracing::info!(
                ?connector_handle,
                ?crtc,
                width,
                height,
                refresh = refresh_rate,
                "DRM output configured"
            );

            self.outputs.push(DrmOutput {
                output_id,
                connector: connector_handle,
                crtc,
                mode,
                front_fb,
                back_fb,
                front_buf,
                back_buf,
                front_ptr,
                back_ptr,
                size: front_size,
                width: width as u32,
                height: height as u32,
                stride: front_stride,
                using_front: true,
            });

            output_ids.push(output_id);
        }

        if output_ids.is_empty() {
            return Err("No connected displays found".to_string());
        }

        Ok(output_ids)
    }

    /// Create a dumb buffer (CPU-accessible scanout buffer).
    fn create_dumb_buffer(
        &self,
        width: u32,
        height: u32,
    ) -> Result<(framebuffer::Handle, DumbBuffer, *mut u8, usize, u32), String> {
        let mut handle = self
            .device
            .create_dumb_buffer(
                (width, height),
                drm::buffer::DrmFourcc::Xrgb8888,
                32, // bits per pixel
            )
            .map_err(|e| format!("Failed to create dumb buffer: {e}"))?;

        let fb = self
            .device
            .add_framebuffer(&handle, 24, 32)
            .map_err(|e| format!("Failed to add framebuffer: {e}"))?;

        // Memory-map the buffer so we can write pixels.
        let mut map = self
            .device
            .map_dumb_buffer(&mut handle)
            .map_err(|e| format!("Failed to map dumb buffer: {e}"))?;

        // The mapping owns the buffer content now.
        let ptr = map.as_mut_ptr();
        let size = map.len();
        let stride = width * 4; // 4 bytes per pixel for XRGB8888

        // Leak the mapping so it stays alive - we'll clean up manually in Drop
        std::mem::forget(map);

        Ok((fb, handle, ptr, size, stride))
    }

    /// Present a composed frame to a specific output.
    pub fn present_frame(
        &mut self,
        output_id: OutputId,
        framebuffer: &Framebuffer,
    ) -> Result<(), String> {
        // Find the output and extract values we need.
        let output_index = self
            .outputs
            .iter()
            .position(|o| o.output_id == output_id)
            .ok_or("Output not found")?;

        let output = &self.outputs[output_index];
        let (back_ptr, back_fb, stride, crtc) = if output.using_front {
            (output.back_ptr, output.back_fb, output.stride, output.crtc)
        } else {
            (
                output.front_ptr,
                output.front_fb,
                output.stride,
                output.crtc,
            )
        };

        // Blit pixels to the back buffer.
        self.blit_to_buffer(back_ptr, stride, framebuffer)?;

        // Page flip: swap front and back buffers.
        self.device
            .page_flip(crtc, back_fb, drm::control::PageFlipFlags::EVENT, None)
            .map_err(|e| format!("Page flip failed: {e}"))?;

        // Update state
        self.outputs[output_index].using_front = !self.outputs[output_index].using_front;

        Ok(())
    }

    /// Copy a composed framebuffer into a dumb buffer for scanout.
    fn blit_to_buffer(
        &self,
        dst_ptr: *mut u8,
        dst_stride: u32,
        framebuffer: &Framebuffer,
    ) -> Result<(), String> {
        let width = framebuffer.width() as usize;
        let height = framebuffer.height() as usize;
        let src_stride = width * BYTES_PER_PIXEL;
        let dst_stride = dst_stride as usize;
        let src_pixels = framebuffer.pixels();

        // SAFETY: The mapping is valid for its declared size, and we never
        // write past `height * dst_stride`.
        unsafe {
            for y in 0..height {
                let src_offset = y * src_stride;
                let dst_offset = y * dst_stride;
                std::ptr::copy_nonoverlapping(
                    src_pixels[src_offset..].as_ptr(),
                    dst_ptr.add(dst_offset),
                    src_stride.min(dst_stride),
                );
            }
        }

        Ok(())
    }

    /// Wait for the next vblank event on any output.
    ///
    /// Returns when a page flip completes, synchronizing presentation to the
    /// display's refresh rate.
    pub fn wait_for_vblank(&self) -> Result<(), String> {
        use drm::control::Event;

        let deadline = Instant::now() + VBLANK_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Timed out waiting for a DRM page-flip event".to_string());
            }
            let timeout_ms = i32::try_from(remaining.as_millis().max(1)).unwrap_or(i32::MAX);
            let mut descriptor = libc::pollfd {
                fd: self.device.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
            if ready == 0 {
                return Err("Timed out waiting for a DRM page-flip event".to_string());
            }
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(format!("Could not poll DRM events: {error}"));
            }

            match self.device.receive_events() {
                Ok(events) => {
                    for event in events {
                        if matches!(event, Event::PageFlip(_)) {
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    // A readiness edge can be consumed by another DRM event;
                    // poll again within the same bounded deadline.
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        continue;
                    }
                    return Err(format!("Failed to receive DRM events: {e}"));
                }
            }
        }
    }

    /// Return all registered output IDs.
    pub fn output_ids(&self) -> Vec<OutputId> {
        self.outputs.iter().map(|o| o.output_id).collect()
    }

    /// Pixel extent used to normalize absolute input coordinates and confine
    /// relative pointer motion. Outputs currently share the compositor origin;
    /// once layout policy assigns positions this can become the layout union.
    pub fn desktop_extent(&self) -> Option<(u32, u32)> {
        Some((
            self.outputs.iter().map(|output| output.width).max()?,
            self.outputs.iter().map(|output| output.height).max()?,
        ))
    }
}

impl Drop for DrmBackend {
    fn drop(&mut self) {
        for output in &mut self.outputs {
            // Unmap the buffers
            unsafe {
                libc::munmap(output.front_ptr as *mut libc::c_void, output.size);
                libc::munmap(output.back_ptr as *mut libc::c_void, output.size);
            }
            // Destroy framebuffers
            let _ = self.device.destroy_framebuffer(output.front_fb);
            let _ = self.device.destroy_framebuffer(output.back_fb);
        }
        // Drop DRM master
        let _ = unsafe { libc::ioctl(self.device.as_raw_fd(), 0x4001641f) }; // DRM_IOCTL_DROP_MASTER
    }
}

/// Calculate the refresh rate in millihertz from a DRM mode.
fn calculate_refresh_rate(mode: &Mode) -> i32 {
    let htotal = mode.hsync().2 as u32;
    let vtotal = mode.vsync().2 as u32;
    if htotal == 0 || vtotal == 0 {
        return 60_000; // fallback
    }
    let clock_khz = mode.clock() as u64;
    let refresh_mhz = (clock_khz * 1_000_000) / (htotal as u64 * vtotal as u64);
    refresh_mhz.min(i32::MAX as u64) as i32
}
