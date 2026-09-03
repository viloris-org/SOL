//! Linux DMA-BUF objects imported through SCP.
//!
//! DMA-BUF descriptors are not memfds. Requiring memfd seals would reject real
//! GPU allocations, so this module owns validation and lifetime separately from
//! the SHM buffer manager. It retains all plane and modifier metadata for a
//! native GBM/EGL/Vulkan importer.

use crate::scp::{
    protocol::{
        BufferFormat, BufferId, DRM_FORMAT_MOD_INVALID, DRM_FORMAT_MOD_LINEAR, DmabufFormat,
        DmabufPlane, SessionId,
    },
    surface::{SurfaceBuffer, SurfaceBufferKind},
    unix_socket,
};
use std::collections::{HashMap, HashSet};
use std::{
    io,
    os::fd::{AsFd, BorrowedFd},
};

pub const MAX_DMABUF_PLANES: usize = 4;
pub const MAX_DMABUFS_PER_SESSION: usize = 1024;
pub const MAX_DMABUF_DIMENSION: i32 = 16_384;

/// One imported client DMA-BUF. The manager owns every descriptor in fds.
#[derive(Debug)]
pub struct Dmabuf {
    pub id: BufferId,
    pub session_id: SessionId,
    pub width: i32,
    pub height: i32,
    pub format: DmabufFormat,
    pub modifier: u64,
    pub planes: Vec<DmabufPlane>,
    fds: Vec<i32>,
    in_use: bool,
}

impl Drop for Dmabuf {
    fn drop(&mut self) {
        close_all(&mut self.fds);
    }
}

impl Dmabuf {
    pub fn plane_fd(&self, plane: usize) -> Option<i32> {
        let index = usize::try_from(self.planes.get(plane)?.fd_index).ok()?;
        self.fds.get(index).copied()
    }

    pub const fn in_use(&self) -> bool {
        self.in_use
    }

    /// Import the complete image into GBM for native GPU rendering or scanout.
    ///
    /// Unlike the CPU fallback, this supports multi-plane images and explicit
    /// vendor modifiers. The returned GBM object has an independent lifetime,
    /// while this SCP object continues to own the received descriptors.
    pub fn import_to_gbm<T: AsFd, U: 'static>(
        &self,
        device: &gbm::Device<T>,
        usage: gbm::BufferObjectFlags,
    ) -> io::Result<gbm::BufferObject<U>> {
        let mut buffers = [None; MAX_DMABUF_PLANES];
        let mut strides = [0_i32; MAX_DMABUF_PLANES];
        let mut offsets = [0_i32; MAX_DMABUF_PLANES];
        for (index, plane) in self.planes.iter().enumerate() {
            let fd = self
                .plane_fd(index)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing DMA-BUF fd"))?;
            buffers[index] = Some(unsafe { BorrowedFd::borrow_raw(fd) });
            strides[index] = i32::try_from(plane.stride).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "DMA-BUF stride exceeds i32")
            })?;
            offsets[index] = i32::try_from(plane.offset).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "DMA-BUF offset exceeds i32")
            })?;
        }
        device.import_buffer_object_from_dma_buf_with_modifiers(
            u32::try_from(self.planes.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "too many DMA-BUF planes")
            })?,
            buffers,
            u32::try_from(self.width)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid width"))?,
            u32::try_from(self.height)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid height"))?,
            gbm_format(self.format),
            usage,
            strides,
            offsets,
            gbm::Modifier::from(self.modifier),
        )
    }
}

/// Session-scoped DMA-BUF object storage.
#[derive(Debug, Default)]
pub struct DmabufManager {
    buffers: HashMap<(SessionId, BufferId), Dmabuf>,
}

impl DmabufManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ownership of every fd transfers on entry, including all error paths.
    #[allow(clippy::too_many_arguments)]
    pub fn create_buffer(
        &mut self,
        session_id: SessionId,
        id: BufferId,
        width: i32,
        height: i32,
        format: DmabufFormat,
        modifier: u64,
        planes: Vec<DmabufPlane>,
        fds: Vec<i32>,
    ) -> Result<(), String> {
        let candidate = Dmabuf {
            id,
            session_id,
            width,
            height,
            format,
            modifier,
            planes,
            fds,
            in_use: false,
        };
        self.validate_candidate(&candidate)?;
        if self.buffers.contains_key(&(session_id, id)) {
            return Err("DMA-BUF ID already exists".to_string());
        }
        if self.session_buffers(session_id) >= MAX_DMABUFS_PER_SESSION {
            return Err(format!(
                "A session may hold at most {MAX_DMABUFS_PER_SESSION} DMA-BUF objects"
            ));
        }
        self.buffers.insert((session_id, id), candidate);
        Ok(())
    }

    fn validate_candidate(&self, buffer: &Dmabuf) -> Result<(), String> {
        if buffer.width <= 0
            || buffer.height <= 0
            || buffer.width > MAX_DMABUF_DIMENSION
            || buffer.height > MAX_DMABUF_DIMENSION
        {
            return Err(format!(
                "DMA-BUF dimensions must be within 1..={MAX_DMABUF_DIMENSION}"
            ));
        }
        if buffer.modifier == DRM_FORMAT_MOD_INVALID {
            return Err("DRM_FORMAT_MOD_INVALID is not valid for a concrete DMA-BUF".to_string());
        }
        let expected_planes = buffer.format.plane_count();
        if buffer.planes.len() != expected_planes {
            return Err(format!(
                "{:?} requires {expected_planes} plane(s), received {}",
                buffer.format,
                buffer.planes.len()
            ));
        }
        if buffer.planes.is_empty() || buffer.planes.len() > MAX_DMABUF_PLANES {
            return Err(format!(
                "DMA-BUF plane count must be within 1..={MAX_DMABUF_PLANES}"
            ));
        }
        if buffer.fds.is_empty() || buffer.fds.len() > MAX_DMABUF_PLANES {
            return Err(format!(
                "DMA-BUF descriptor count must be within 1..={MAX_DMABUF_PLANES}"
            ));
        }

        let mut referenced = HashSet::new();
        let mut validated_fds = HashSet::new();
        for (plane_index, plane) in buffer.planes.iter().enumerate() {
            let fd_index = usize::try_from(plane.fd_index)
                .map_err(|_| "DMA-BUF fd index does not fit in memory".to_string())?;
            let fd = *buffer.fds.get(fd_index).ok_or_else(|| {
                format!("DMA-BUF plane {plane_index} names missing descriptor {fd_index}")
            })?;
            referenced.insert(fd_index);
            if fd < 0 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
                return Err(format!("DMA-BUF descriptor {fd_index} is invalid"));
            }
            if validated_fds.insert(fd_index) {
                validate_dmabuf_fd(fd).map_err(|error| {
                    format!("Descriptor {fd_index} is not a usable DMA-BUF: {error}")
                })?;
            }
            let extent = plane_extent(buffer, plane_index)?;
            let actual = unix_socket::fd_size(fd)
                .map_err(|error| format!("Cannot size DMA-BUF descriptor {fd_index}: {error}"))?;
            if actual > 0 && extent > actual {
                return Err(format!(
                    "DMA-BUF plane {plane_index} needs {extent} bytes but descriptor {fd_index} has {actual}"
                ));
            }
        }
        if referenced.len() != buffer.fds.len() {
            return Err("DMA-BUF request contains an unreferenced descriptor".to_string());
        }
        Ok(())
    }

    pub fn get_buffer(&self, session_id: SessionId, id: BufferId) -> Option<&Dmabuf> {
        self.buffers.get(&(session_id, id))
    }

    pub fn contains(&self, session_id: SessionId, id: BufferId) -> bool {
        self.buffers.contains_key(&(session_id, id))
    }

    /// Acquire a surface view. GPU-only layouts use fd = -1 so the CPU
    /// compositor skips them without preventing native GBM import.
    pub fn acquire_surface_buffer(
        &mut self,
        session_id: SessionId,
        id: BufferId,
    ) -> Result<SurfaceBuffer, String> {
        let buffer = self
            .buffers
            .get(&(session_id, id))
            .ok_or("DMA-BUF not found")?;
        if buffer.in_use {
            return Err("DMA-BUF is still in use by the compositor".to_string());
        }
        let plane = buffer.planes[0];
        let cpu_format = packed_buffer_format(buffer.format);
        let cpu_mappable = buffer.modifier == DRM_FORMAT_MOD_LINEAR
            && buffer.planes.len() == 1
            && cpu_format.is_some();
        let stride = i32::try_from(plane.stride)
            .map_err(|_| "DMA-BUF stride exceeds the renderer limit".to_string())?;
        let fd = if cpu_mappable {
            let source_fd = buffer
                .plane_fd(0)
                .ok_or("DMA-BUF plane descriptor disappeared")?;
            let duplicate = unsafe { libc::fcntl(source_fd, libc::F_DUPFD_CLOEXEC, 0) };
            if duplicate < 0 {
                return Err(format!(
                    "Could not duplicate DMA-BUF descriptor: {}",
                    std::io::Error::last_os_error()
                ));
            }
            duplicate
        } else {
            -1
        };
        let (width, height) = (buffer.width, buffer.height);
        let Some(buffer) = self.buffers.get_mut(&(session_id, id)) else {
            if fd >= 0 {
                unix_socket::close_fd(fd);
            }
            return Err("DMA-BUF disappeared while being acquired".to_string());
        };
        buffer.in_use = true;
        Ok(SurfaceBuffer {
            buffer_id: id,
            offset: plane.offset as usize,
            managed: true,
            kind: SurfaceBufferKind::Dmabuf,
            fd,
            width,
            height,
            stride,
            format: cpu_format.unwrap_or(BufferFormat::Xrgb8888),
        })
    }

    pub fn mark_buffer_released(
        &mut self,
        session_id: SessionId,
        id: BufferId,
    ) -> Result<(), String> {
        let buffer = self
            .buffers
            .get_mut(&(session_id, id))
            .ok_or("DMA-BUF not found")?;
        buffer.in_use = false;
        Ok(())
    }

    pub fn destroy_buffer(&mut self, session_id: SessionId, id: BufferId) -> Result<(), String> {
        let buffer = self
            .buffers
            .get(&(session_id, id))
            .ok_or("DMA-BUF not found")?;
        if buffer.in_use {
            return Err("Cannot destroy DMA-BUF while in use".to_string());
        }
        self.buffers.remove(&(session_id, id));
        Ok(())
    }

    pub fn session_buffers(&self, session_id: SessionId) -> usize {
        self.buffers
            .keys()
            .filter(|(owner, _)| *owner == session_id)
            .count()
    }

    pub fn destroy_session(&mut self, session_id: SessionId) {
        self.buffers.retain(|(owner, _), _| *owner != session_id);
    }
}

fn packed_buffer_format(format: DmabufFormat) -> Option<BufferFormat> {
    match format {
        DmabufFormat::Argb8888 => Some(BufferFormat::Argb8888),
        DmabufFormat::Xrgb8888 => Some(BufferFormat::Xrgb8888),
        DmabufFormat::Abgr8888 => Some(BufferFormat::Rgba8888),
        DmabufFormat::Rgb565 => Some(BufferFormat::Rgb565),
        DmabufFormat::Xbgr8888 | DmabufFormat::Nv12 => None,
    }
}

fn gbm_format(format: DmabufFormat) -> gbm::Format {
    match format {
        DmabufFormat::Argb8888 => gbm::Format::Argb8888,
        DmabufFormat::Xrgb8888 => gbm::Format::Xrgb8888,
        DmabufFormat::Abgr8888 => gbm::Format::Abgr8888,
        DmabufFormat::Xbgr8888 => gbm::Format::Xbgr8888,
        DmabufFormat::Rgb565 => gbm::Format::Rgb565,
        DmabufFormat::Nv12 => gbm::Format::Nv12,
    }
}

fn plane_extent(buffer: &Dmabuf, plane_index: usize) -> Result<u64, String> {
    let plane = &buffer.planes[plane_index];
    let width = u64::try_from(buffer.width).map_err(|_| "negative DMA-BUF width")?;
    let height = u64::try_from(buffer.height).map_err(|_| "negative DMA-BUF height")?;
    let (row_bytes, rows) = match (buffer.format, plane_index) {
        (DmabufFormat::Argb8888 | DmabufFormat::Xrgb8888, 0)
        | (DmabufFormat::Abgr8888 | DmabufFormat::Xbgr8888, 0) => (
            width.checked_mul(4).ok_or("DMA-BUF row size overflow")?,
            height,
        ),
        (DmabufFormat::Rgb565, 0) => (
            width.checked_mul(2).ok_or("DMA-BUF row size overflow")?,
            height,
        ),
        (DmabufFormat::Nv12, 0) => (width, height),
        (DmabufFormat::Nv12, 1) => (width.div_ceil(2) * 2, height.div_ceil(2)),
        _ => {
            return Err(format!(
                "invalid plane {plane_index} for {:?}",
                buffer.format
            ));
        }
    };
    let stride = u64::from(plane.stride);
    if stride < row_bytes {
        return Err(format!(
            "DMA-BUF plane {plane_index} stride {stride} is smaller than its {row_bytes}-byte row"
        ));
    }
    let preceding_rows = rows.saturating_sub(1);
    u64::from(plane.offset)
        .checked_add(
            preceding_rows
                .checked_mul(stride)
                .ok_or("DMA-BUF plane extent overflow")?,
        )
        .and_then(|extent| extent.checked_add(row_bytes))
        .ok_or_else(|| "DMA-BUF plane extent overflow".to_string())
}

fn close_all(fds: &mut Vec<i32>) {
    for fd in fds.drain(..) {
        if fd >= 0 {
            unix_socket::close_fd(fd);
        }
    }
}

const DMA_BUF_SYNC_READ: u64 = 1;
const DMA_BUF_SYNC_START: u64 = 0;
const DMA_BUF_SYNC_END: u64 = 4;
const DMA_BUF_IOCTL_SYNC: libc::c_ulong = 0x4008_6200;

fn validate_dmabuf_fd(fd: i32) -> io::Result<()> {
    let start = DMA_BUF_SYNC_START | DMA_BUF_SYNC_READ;
    if unsafe { libc::ioctl(fd, DMA_BUF_IOCTL_SYNC, &start) } != 0 {
        let error = io::Error::last_os_error();
        // Unit tests use memfds because CI has no GPU allocator. Production
        // requires the kernel DMA-BUF sync contract and rejects arbitrary fds.
        #[cfg(test)]
        if error.raw_os_error() == Some(libc::ENOTTY) {
            return Ok(());
        }
        return Err(error);
    }
    let end = DMA_BUF_SYNC_END | DMA_BUF_SYNC_READ;
    if unsafe { libc::ioctl(fd, DMA_BUF_IOCTL_SYNC, &end) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::memfd;
    use std::os::fd::IntoRawFd;

    fn descriptor(bytes: usize) -> i32 {
        let file = memfd::create_file("dmabuf-fixture").unwrap();
        file.set_len(bytes as u64).unwrap();
        file.into_raw_fd()
    }

    fn packed_plane(stride: u32) -> Vec<DmabufPlane> {
        vec![DmabufPlane {
            fd_index: 0,
            offset: 0,
            stride,
        }]
    }

    #[test]
    fn imports_and_acquires_a_linear_packed_buffer() {
        let mut manager = DmabufManager::new();
        manager
            .create_buffer(
                7,
                9,
                4,
                3,
                DmabufFormat::Xrgb8888,
                DRM_FORMAT_MOD_LINEAR,
                packed_plane(16),
                vec![descriptor(48)],
            )
            .unwrap();
        let surface = manager.acquire_surface_buffer(7, 9).unwrap();
        assert_eq!(surface.stride, 16);
        assert!(manager.get_buffer(7, 9).unwrap().in_use());
        drop(surface);
        manager.mark_buffer_released(7, 9).unwrap();
        manager.destroy_buffer(7, 9).unwrap();
    }

    #[test]
    fn accepts_nv12_planes_that_share_one_descriptor() {
        let mut manager = DmabufManager::new();
        manager
            .create_buffer(
                1,
                2,
                4,
                4,
                DmabufFormat::Nv12,
                DRM_FORMAT_MOD_LINEAR,
                vec![
                    DmabufPlane {
                        fd_index: 0,
                        offset: 0,
                        stride: 4,
                    },
                    DmabufPlane {
                        fd_index: 0,
                        offset: 16,
                        stride: 4,
                    },
                ],
                vec![descriptor(24)],
            )
            .unwrap();
        assert_eq!(
            manager.get_buffer(1, 2).unwrap().plane_fd(1),
            manager.get_buffer(1, 2).unwrap().plane_fd(0)
        );
        let surface = manager.acquire_surface_buffer(1, 2).unwrap();
        assert_eq!(surface.fd, -1, "NV12 is reserved for the GPU importer");
        drop(surface);
        manager.mark_buffer_released(1, 2).unwrap();
    }

    #[test]
    fn rejects_short_planes() {
        let fd = descriptor(15);
        let mut manager = DmabufManager::new();
        assert!(
            manager
                .create_buffer(
                    1,
                    1,
                    4,
                    1,
                    DmabufFormat::Xrgb8888,
                    DRM_FORMAT_MOD_LINEAR,
                    packed_plane(16),
                    vec![fd],
                )
                .is_err()
        );
    }

    #[test]
    fn rejects_unreferenced_descriptors() {
        let mut manager = DmabufManager::new();
        let error = manager
            .create_buffer(
                1,
                1,
                1,
                1,
                DmabufFormat::Argb8888,
                DRM_FORMAT_MOD_LINEAR,
                packed_plane(4),
                vec![descriptor(4), descriptor(4)],
            )
            .unwrap_err();
        assert!(error.contains("unreferenced"));
    }

    #[test]
    fn rejects_invalid_modifier_and_wrong_plane_count() {
        let mut manager = DmabufManager::new();
        assert!(
            manager
                .create_buffer(
                    1,
                    1,
                    1,
                    1,
                    DmabufFormat::Nv12,
                    DRM_FORMAT_MOD_INVALID,
                    packed_plane(1),
                    vec![descriptor(4)],
                )
                .is_err()
        );
    }
}
