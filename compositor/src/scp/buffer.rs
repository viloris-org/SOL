//! SCP buffer management (SHM pools and buffers).
//!
//! Pools and buffers are keyed by their owning session, not by the id alone. The
//! ids are chosen by the client, so a global namespace would let any client name
//! another's pool — and a pool is a shared memory mapping, which makes that a
//! cross-application read rather than merely a naming collision.

use crate::scp::{
    memfd,
    protocol::{BufferId, PoolId, SessionId, ShmFormat},
    unix_socket,
};
use std::collections::HashMap;

/// Largest shared-memory pool a client may declare.
///
/// The renderer maps a pool on the client's word about its size, so the value
/// needs a ceiling. 256 MiB holds several double-buffered 4K surfaces.
pub const MAX_POOL_SIZE: usize = 256 * 1024 * 1024;

/// Pools one client may hold at once.
///
/// Each pool pins a descriptor, so an unbounded count exhausts the compositor's
/// file-descriptor limit long before it exhausts memory — and a compositor that
/// cannot open a descriptor cannot accept a connection either. Well beyond what
/// a client double-buffering a few surfaces needs.
pub const MAX_POOLS_PER_SESSION: usize = 64;

/// Buffers one client may hold at once, across all of its pools.
pub const MAX_BUFFERS_PER_SESSION: usize = 1024;

/// Shared memory pool.
#[derive(Debug)]
pub struct ShmPool {
    pub id: PoolId,
    pub session_id: SessionId,
    pub fd: i32,
    pub size: usize,
}

impl Drop for ShmPool {
    fn drop(&mut self) {
        if self.fd >= 0 {
            unix_socket::close_fd(self.fd);
        }
    }
}

/// Buffer created from a pool.
#[derive(Debug)]
pub struct Buffer {
    pub id: BufferId,
    pub session_id: SessionId,
    pub pool_id: PoolId,
    pub offset: usize,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: ShmFormat,
    pub in_use: bool,
}

/// Validate client-supplied buffer geometry and return its total byte length.
///
/// Every value here arrives from a client, so each step is checked rather than
/// assumed. A negative stride or a `height * stride` product that does not fit in
/// an `i32` would otherwise wrap: in a release build that slips straight past the
/// pool bounds check, and in a debug build it panics while the shared state lock
/// is held, which poisons the lock and takes every other client down with it.
pub fn validate_geometry(
    width: i32,
    height: i32,
    stride: i32,
    bytes_per_pixel: i32,
) -> Result<usize, String> {
    if width <= 0 || height <= 0 || stride <= 0 {
        return Err("Buffer dimensions and stride must be positive".to_string());
    }

    let minimum_stride = width
        .checked_mul(bytes_per_pixel)
        .ok_or("Buffer width overflows the stride calculation")?;
    if stride < minimum_stride {
        return Err(format!(
            "Buffer stride {stride} is too small for {width} pixels at {bytes_per_pixel} bytes each"
        ));
    }

    // Widening first makes the product exact: two `i32`s always fit in an `i64`.
    let size = i64::from(height) * i64::from(stride);
    let size =
        usize::try_from(size).map_err(|_| "Buffer size does not fit in memory".to_string())?;
    if size > MAX_POOL_SIZE {
        return Err(format!(
            "Buffer size {size} exceeds the {MAX_POOL_SIZE}-byte limit"
        ));
    }
    Ok(size)
}

/// Check that a descriptor really backs `declared` bytes, and keeps doing so.
///
/// Two separate promises are needed before the renderer can map client memory:
///
/// 1. The file is at least as large as the client says. `fstat` answers that,
///    rather than taking the declaration on trust.
/// 2. It cannot become smaller afterwards. Without `F_SEAL_SHRINK` the check
///    above is only true at the instant it runs — the client can `ftruncate`
///    the memfd a moment later, and the compositor takes a SIGBUS reading a
///    page that no longer exists. A client crashing its own process is its
///    business; crashing the compositor ends every session on the machine.
pub fn validate_descriptor(fd: i32, declared: usize) -> Result<(), String> {
    if fd < 0 {
        return Err("Invalid buffer file descriptor".to_string());
    }

    let actual = unix_socket::fd_size(fd)
        .map_err(|error| format!("Cannot size the buffer descriptor: {error}"))?;
    if u64::try_from(declared).unwrap_or(u64::MAX) > actual {
        return Err(format!(
            "Declared size {declared} exceeds the descriptor's {actual} bytes"
        ));
    }

    if !memfd::is_shrink_sealed(fd) {
        return Err(
            "Buffer descriptors must be memfds sealed with F_SEAL_SHRINK so the mapping \
             cannot be truncated out from under the compositor"
                .to_string(),
        );
    }

    Ok(())
}

/// Buffer manager — tracks every client's SHM pools and buffers.
#[derive(Debug, Default)]
pub struct BufferManager {
    pools: HashMap<(SessionId, PoolId), ShmPool>,
    buffers: HashMap<(SessionId, BufferId), Buffer>,
}

impl BufferManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a shared-memory pool for one client.
    ///
    /// Takes ownership of `fd`: it is closed on every failure path here, so
    /// callers do not have to unwind it themselves.
    pub fn create_pool(
        &mut self,
        session_id: SessionId,
        id: PoolId,
        fd: i32,
        size: usize,
    ) -> Result<(), String> {
        let result = self.insert_pool(session_id, id, fd, size);
        if result.is_err() && fd >= 0 {
            unix_socket::close_fd(fd);
        }
        result
    }

    fn insert_pool(
        &mut self,
        session_id: SessionId,
        id: PoolId,
        fd: i32,
        size: usize,
    ) -> Result<(), String> {
        if self.pools.contains_key(&(session_id, id)) {
            return Err("Pool ID already exists".to_string());
        }
        if self.session_pools(session_id) >= MAX_POOLS_PER_SESSION {
            return Err(format!(
                "A session may hold at most {MAX_POOLS_PER_SESSION} shared-memory pools"
            ));
        }
        if size == 0 {
            return Err("Pool size must be positive".to_string());
        }
        if size > MAX_POOL_SIZE {
            return Err(format!(
                "Pool size {size} exceeds the {MAX_POOL_SIZE}-byte limit"
            ));
        }

        // The descriptor is authoritative about how much memory exists, and its
        // seals about whether that stays true. A pool is a mapping the renderer
        // will read from, so neither may be taken on the client's word.
        validate_descriptor(fd, size)?;

        self.pools.insert(
            (session_id, id),
            ShmPool {
                id,
                session_id,
                fd,
                size,
            },
        );
        Ok(())
    }

    pub fn destroy_pool(&mut self, session_id: SessionId, id: PoolId) -> Result<(), String> {
        if self
            .buffers
            .values()
            .any(|buffer| buffer.session_id == session_id && buffer.pool_id == id)
        {
            return Err("Cannot destroy pool with active buffers".to_string());
        }
        self.pools
            .remove(&(session_id, id))
            .ok_or("Pool not found")?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_buffer(
        &mut self,
        session_id: SessionId,
        id: BufferId,
        pool_id: PoolId,
        offset: usize,
        width: i32,
        height: i32,
        stride: i32,
        format: ShmFormat,
    ) -> Result<(), String> {
        if self.buffers.contains_key(&(session_id, id)) {
            return Err("Buffer ID already exists".to_string());
        }
        if self.session_buffers(session_id) >= MAX_BUFFERS_PER_SESSION {
            return Err(format!(
                "A session may hold at most {MAX_BUFFERS_PER_SESSION} buffers"
            ));
        }
        let pool = self
            .pools
            .get(&(session_id, pool_id))
            .ok_or("Pool not found")?;

        let buffer_size = validate_geometry(width, height, stride, format.bytes_per_pixel())?;
        let end = offset
            .checked_add(buffer_size)
            .ok_or("Buffer offset and size overflow")?;
        if end > pool.size {
            return Err(format!(
                "Buffer needs {end} bytes but the pool holds {}",
                pool.size
            ));
        }

        self.buffers.insert(
            (session_id, id),
            Buffer {
                id,
                session_id,
                pool_id,
                offset,
                width,
                height,
                stride,
                format,
                in_use: false,
            },
        );
        Ok(())
    }

    pub fn destroy_buffer(&mut self, session_id: SessionId, id: BufferId) -> Result<(), String> {
        let buffer = self
            .buffers
            .get(&(session_id, id))
            .ok_or("Buffer not found")?;
        if buffer.in_use {
            return Err("Cannot destroy buffer while in use".to_string());
        }
        self.buffers.remove(&(session_id, id));
        Ok(())
    }

    pub fn mark_buffer_in_use(
        &mut self,
        session_id: SessionId,
        id: BufferId,
    ) -> Result<(), String> {
        let buffer = self
            .buffers
            .get_mut(&(session_id, id))
            .ok_or("Buffer not found")?;
        buffer.in_use = true;
        Ok(())
    }

    pub fn mark_buffer_released(
        &mut self,
        session_id: SessionId,
        id: BufferId,
    ) -> Result<(), String> {
        let buffer = self
            .buffers
            .get_mut(&(session_id, id))
            .ok_or("Buffer not found")?;
        buffer.in_use = false;
        Ok(())
    }

    pub fn get_buffer(&self, session_id: SessionId, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&(session_id, id))
    }

    pub fn get_pool(&self, session_id: SessionId, id: PoolId) -> Option<&ShmPool> {
        self.pools.get(&(session_id, id))
    }

    /// Pools currently held by one client.
    pub fn session_pools(&self, session_id: SessionId) -> usize {
        self.pools
            .keys()
            .filter(|(owner, _)| *owner == session_id)
            .count()
    }

    /// Buffers currently held by one client.
    pub fn session_buffers(&self, session_id: SessionId) -> usize {
        self.buffers
            .keys()
            .filter(|(owner, _)| *owner == session_id)
            .count()
    }

    /// Drop every pool and buffer belonging to a departing client.
    ///
    /// Pools own their descriptors, so this is also what keeps a disconnect from
    /// leaking one.
    pub fn destroy_session(&mut self, session_id: SessionId) {
        self.buffers.retain(|(owner, _), _| *owner != session_id);
        self.pools.retain(|(owner, _), _| *owner != session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POOL: usize = 4096;

    /// A shrink-sealed memfd of exactly `bytes` length, as a raw descriptor.
    ///
    /// This is the shape of descriptor a client has to send: sized to what it
    /// declares, and sealed so it stays that size.
    fn sealed_descriptor(bytes: usize) -> i32 {
        let fd = unsealed_descriptor(bytes);
        memfd::add_seals(fd, memfd::F_SEAL_SHRINK).expect("seal the memfd");
        fd
    }

    /// A memfd of the right size that the client can still shrink.
    fn unsealed_descriptor(bytes: usize) -> i32 {
        use std::io::Write;
        use std::os::unix::io::{FromRawFd, IntoRawFd};

        let fd = memfd::create("pool-fixture", true).expect("create memfd");
        // SAFETY: create returned a fresh owned descriptor nothing else holds.
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(&vec![0_u8; bytes]).expect("size the memfd");
        file.into_raw_fd()
    }

    fn manager_with_pool() -> BufferManager {
        let mut manager = BufferManager::new();
        manager
            .create_pool(1, 1, sealed_descriptor(POOL), POOL)
            .expect("create pool");
        manager
    }

    #[test]
    fn accepts_a_buffer_that_fits() {
        let mut manager = manager_with_pool();
        manager
            .create_buffer(1, 1, 1, 0, 16, 16, 64, ShmFormat::Argb8888)
            .expect("16x16 at stride 64 needs 1024 bytes");
        assert!(manager.get_buffer(1, 1).is_some());
    }

    #[test]
    fn rejects_a_negative_stride_instead_of_wrapping() {
        let mut manager = manager_with_pool();
        // `height * stride` used to be -1, which sign-extended to usize::MAX and
        // wrapped the bounds check back inside the pool.
        let error = manager
            .create_buffer(1, 1, 1, 5, 4096, 1, -1, ShmFormat::Argb8888)
            .expect_err("a negative stride must be refused");
        assert!(error.contains("must be positive"), "unexpected: {error}");
    }

    #[test]
    fn rejects_geometry_that_overflows_rather_than_panicking() {
        let mut manager = manager_with_pool();
        let error = manager
            .create_buffer(1, 1, 1, 0, 1, i32::MAX, i32::MAX, ShmFormat::Argb8888)
            .expect_err("an overflowing product must be refused");
        assert!(error.contains("exceeds"), "unexpected: {error}");
    }

    #[test]
    fn rejects_an_offset_that_overflows_the_pool_bounds() {
        let mut manager = manager_with_pool();
        let error = manager
            .create_buffer(1, 1, 1, usize::MAX - 8, 16, 16, 64, ShmFormat::Argb8888)
            .expect_err("an overflowing offset must be refused");
        assert!(error.contains("overflow"), "unexpected: {error}");
    }

    #[test]
    fn rejects_a_stride_too_narrow_for_the_format() {
        let mut manager = manager_with_pool();
        let error = manager
            .create_buffer(1, 1, 1, 0, 16, 16, 32, ShmFormat::Argb8888)
            .expect_err("16 pixels need 64 bytes at 4 bytes each");
        assert!(error.contains("too small"), "unexpected: {error}");

        // The same stride is fine for a 16-bit format.
        manager
            .create_buffer(1, 2, 1, 0, 16, 16, 32, ShmFormat::Rgb565)
            .expect("16 pixels need 32 bytes at 2 bytes each");
    }

    #[test]
    fn rejects_a_buffer_larger_than_its_pool() {
        let mut manager = manager_with_pool();
        let error = manager
            .create_buffer(1, 1, 1, 0, 64, 64, 256, ShmFormat::Argb8888)
            .expect_err("16 KiB does not fit in a 4 KiB pool");
        assert!(error.contains("pool holds"), "unexpected: {error}");
    }

    #[test]
    fn pools_are_private_to_their_session() {
        let mut manager = manager_with_pool();

        // Another client may reuse the same numeric id for its own pool.
        manager
            .create_pool(2, 1, sealed_descriptor(POOL), POOL)
            .expect("pool ids are per-session");

        // But it cannot build a buffer out of session 1's pool, because the
        // lookup is keyed by the caller's own session.
        manager
            .create_buffer(2, 1, 1, 0, 16, 16, 64, ShmFormat::Argb8888)
            .expect("session 2 uses its own pool 1");

        assert!(manager.get_buffer(1, 1).is_none(), "no buffer leaked to 1");
        assert!(manager.get_buffer(2, 1).is_some());
    }

    #[test]
    fn a_client_cannot_destroy_another_clients_buffer() {
        let mut manager = manager_with_pool();
        manager
            .create_buffer(1, 7, 1, 0, 16, 16, 64, ShmFormat::Argb8888)
            .expect("create buffer");

        let error = manager
            .destroy_buffer(2, 7)
            .expect_err("session 2 must not reach session 1's buffer");
        assert!(error.contains("not found"), "unexpected: {error}");
        assert!(manager.get_buffer(1, 7).is_some(), "buffer survives");
    }

    #[test]
    fn disconnecting_releases_pools_and_buffers() {
        let mut manager = manager_with_pool();
        manager
            .create_buffer(1, 1, 1, 0, 16, 16, 64, ShmFormat::Argb8888)
            .expect("create buffer");
        manager
            .create_pool(2, 1, sealed_descriptor(POOL), POOL)
            .expect("other client");

        manager.destroy_session(1);

        assert!(manager.get_pool(1, 1).is_none());
        assert!(manager.get_buffer(1, 1).is_none());
        assert!(manager.get_pool(2, 1).is_some(), "other client untouched");
    }

    #[test]
    fn rejects_an_absurd_pool_size() {
        let mut manager = BufferManager::new();
        let error = manager
            .create_pool(1, 1, sealed_descriptor(0), MAX_POOL_SIZE + 1)
            .expect_err("an oversized pool must be refused");
        assert!(error.contains("exceeds"), "unexpected: {error}");
    }

    #[test]
    fn refuses_a_descriptor_the_client_can_still_shrink() {
        let mut manager = BufferManager::new();
        let error = manager
            .create_pool(1, 1, unsealed_descriptor(POOL), POOL)
            .expect_err("an unsealed pool must be refused");
        assert!(error.contains("F_SEAL_SHRINK"), "unexpected: {error}");
    }

    #[test]
    fn refuses_a_descriptor_that_cannot_be_sealed_at_all() {
        // A socket is the shape of thing an attacker reaches for: it passes
        // through SCM_RIGHTS like a memfd but has no size and no seals.
        use std::os::unix::io::IntoRawFd;
        use std::os::unix::net::UnixStream;

        let (socket, _peer) = UnixStream::pair().expect("create socket pair");
        let mut manager = BufferManager::new();
        let error = manager
            .create_pool(1, 1, socket.into_raw_fd(), POOL)
            .expect_err("a socket is not shared memory");
        assert!(
            error.contains("exceeds the descriptor") || error.contains("F_SEAL_SHRINK"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn refuses_more_pools_than_a_session_may_hold() {
        let mut manager = BufferManager::new();
        for id in 0..MAX_POOLS_PER_SESSION {
            manager
                .create_pool(1, id as PoolId, sealed_descriptor(POOL), POOL)
                .expect("pools within the limit");
        }

        let error = manager
            .create_pool(
                1,
                MAX_POOLS_PER_SESSION as PoolId,
                sealed_descriptor(POOL),
                POOL,
            )
            .expect_err("the limit must hold");
        assert!(error.contains("at most"), "unexpected: {error}");

        // The cap is per session, not global.
        manager
            .create_pool(2, 0, sealed_descriptor(POOL), POOL)
            .expect("another client is unaffected");
    }

    #[test]
    fn checks_a_declared_size_against_the_real_descriptor() {
        let mut manager = BufferManager::new();
        let error = manager
            .create_pool(1, 1, sealed_descriptor(512), 4096)
            .expect_err("a declaration larger than the descriptor must be refused");
        assert!(
            error.contains("exceeds the descriptor"),
            "unexpected: {error}"
        );

        manager
            .create_pool(1, 1, sealed_descriptor(4096), 4096)
            .expect("a declaration the descriptor covers is accepted");
    }
}
