//! SCP buffer management (SHM pools and buffers).

use crate::scp::protocol::{BufferId, PoolId, ShmFormat};
use std::collections::HashMap;

/// Shared memory pool.
#[derive(Debug)]
pub struct ShmPool {
    pub id: PoolId,
    pub fd: i32,
    pub size: usize,
}

impl Drop for ShmPool {
    fn drop(&mut self) {
        if self.fd >= 0 {
            let _ = nix::unistd::close(self.fd);
        }
    }
}

/// Buffer created from a pool.
#[derive(Debug)]
pub struct Buffer {
    pub id: BufferId,
    pub pool_id: PoolId,
    pub offset: usize,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: ShmFormat,
    pub in_use: bool,
}

/// Buffer manager — tracks all SHM pools and buffers.
#[derive(Debug, Default)]
pub struct BufferManager {
    pools: HashMap<PoolId, ShmPool>,
    buffers: HashMap<BufferId, Buffer>,
}

impl BufferManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_pool(&mut self, id: PoolId, fd: i32, size: usize) -> Result<(), String> {
        if self.pools.contains_key(&id) {
            return Err("Pool ID already exists".to_string());
        }
        self.pools.insert(id, ShmPool { id, fd, size });
        Ok(())
    }

    pub fn destroy_pool(&mut self, id: PoolId) -> Result<(), String> {
        if self.buffers.values().any(|b| b.pool_id == id) {
            return Err("Cannot destroy pool with active buffers".to_string());
        }
        self.pools.remove(&id).ok_or("Pool not found")?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_buffer(
        &mut self,
        id: BufferId,
        pool_id: PoolId,
        offset: usize,
        width: i32,
        height: i32,
        stride: i32,
        format: ShmFormat,
    ) -> Result<(), String> {
        if !self.pools.contains_key(&pool_id) {
            return Err("Pool not found".to_string());
        }
        if self.buffers.contains_key(&id) {
            return Err("Buffer ID already exists".to_string());
        }

        let pool = self.pools.get(&pool_id).unwrap();
        let buffer_size = (height * stride) as usize;
        if offset + buffer_size > pool.size {
            return Err("Buffer exceeds pool size".to_string());
        }

        self.buffers.insert(
            id,
            Buffer {
                id,
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

    pub fn destroy_buffer(&mut self, id: BufferId) -> Result<(), String> {
        let buffer = self.buffers.get(&id).ok_or("Buffer not found")?;
        if buffer.in_use {
            return Err("Cannot destroy buffer while in use".to_string());
        }
        self.buffers.remove(&id);
        Ok(())
    }

    pub fn mark_buffer_in_use(&mut self, id: BufferId) -> Result<(), String> {
        let buffer = self.buffers.get_mut(&id).ok_or("Buffer not found")?;
        buffer.in_use = true;
        Ok(())
    }

    pub fn mark_buffer_released(&mut self, id: BufferId) -> Result<(), String> {
        let buffer = self.buffers.get_mut(&id).ok_or("Buffer not found")?;
        buffer.in_use = false;
        Ok(())
    }

    pub fn get_buffer(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&id)
    }

    pub fn get_pool(&self, id: PoolId) -> Option<&ShmPool> {
        self.pools.get(&id)
    }
}
