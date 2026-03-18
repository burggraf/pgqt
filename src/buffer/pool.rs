//! Buffer Pool for efficient memory reuse
//!
//! This module provides a buffer pool using BytesMut for efficient
//! memory management and reuse, reducing allocation overhead for
//! frequently created temporary buffers.

use bytes::BytesMut;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// Configuration for the buffer pool
#[derive(Debug, Clone, Copy)]
pub struct BufferPoolConfig {
    /// Maximum number of buffers to keep in the pool (default: 50)
    pub max_size: usize,
    /// Initial capacity for new buffers (default: 4096)
    pub initial_capacity: usize,
    /// Maximum capacity for buffers (default: 65536)
    pub max_capacity: usize,
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            max_size: 50,
            initial_capacity: 4096,
            max_capacity: 65536,
        }
    }
}

/// A buffer pool for efficient BytesMut reuse
///
/// The pool maintains a collection of pre-allocated buffers that can be
/// acquired and released, reducing allocation overhead for temporary buffers.
#[derive(Debug)]
pub struct BufferPool {
    config: BufferPoolConfig,
    buffers: Mutex<VecDeque<BytesMut>>,
}

impl BufferPool {
    /// Create a new buffer pool with the given configuration
    ///
    /// # Example
    /// ```
    /// use pgqt::buffer::{BufferPool, BufferPoolConfig};
    ///
    /// let config = BufferPoolConfig {
    ///     max_size: 100,
    ///     initial_capacity: 8192,
    ///     max_capacity: 131072,
    /// };
    /// let pool = BufferPool::new(config);
    /// ```
    pub fn new(config: BufferPoolConfig) -> Arc<Self> {
        let buffers = Mutex::new(VecDeque::with_capacity(config.max_size));
        Arc::new(Self { config, buffers })
    }

    /// Create a new buffer pool with default configuration
    pub fn default_pool() -> Arc<Self> {
        Self::new(BufferPoolConfig::default())
    }

    /// Acquire a buffer from the pool
    ///
    /// If a buffer is available in the pool, it will be returned with its
    /// contents cleared. If no buffer is available, a new one is allocated.
    ///
    /// # Returns
    /// A BytesMut with at least `initial_capacity` bytes available
    pub fn acquire(&self) -> BytesMut {
        let mut buffers = self.buffers.lock().unwrap();
        
        // Try to get a buffer from the pool
        while let Some(mut buffer) = buffers.pop_front() {
            // Clear the buffer for reuse
            buffer.clear();
            
            // Check if buffer capacity is within acceptable limits
            if buffer.capacity() <= self.config.max_capacity {
                return buffer;
            }
            // If buffer is too large, drop it and continue
        }
        
        // No suitable buffer available, allocate a new one
        drop(buffers); // Release lock before allocation
        BytesMut::with_capacity(self.config.initial_capacity)
    }

    /// Release a buffer back to the pool
    ///
    /// The buffer will be cleared and stored for reuse if the pool
    /// has not reached its maximum size. Buffers that exceed
    /// `max_capacity` are dropped.
    ///
    /// # Arguments
    /// * `buffer` - The BytesMut to return to the pool
    pub fn release(&self, mut buffer: BytesMut) {
        // Clear the buffer for reuse
        buffer.clear();
        
        // Only keep buffers within size limits
        if buffer.capacity() > self.config.max_capacity {
            return; // Drop the buffer
        }
        
        let mut buffers = self.buffers.lock().unwrap();
        
        // Only add to pool if we haven't reached max size
        if buffers.len() < self.config.max_size {
            buffers.push_back(buffer);
        }
        // Otherwise, drop the buffer (it will be deallocated)
    }

    /// Get the current number of buffers in the pool
    pub fn size(&self) -> usize {
        let buffers = self.buffers.lock().unwrap();
        buffers.len()
    }

    /// Get the maximum number of buffers the pool can hold
    pub fn max_size(&self) -> usize {
        self.config.max_size
    }

    /// Get the initial capacity for new buffers
    pub fn initial_capacity(&self) -> usize {
        self.config.initial_capacity
    }

    /// Get the maximum capacity for buffers
    pub fn max_capacity(&self) -> usize {
        self.config.max_capacity
    }

    /// Clear all buffers from the pool
    ///
    /// This can be used to free memory when memory pressure is detected.
    pub fn clear(&self) {
        let mut buffers = self.buffers.lock().unwrap();
        buffers.clear();
    }

    /// Resize the pool to a new maximum size
    ///
    /// If the new size is smaller than the current number of buffers,
    /// excess buffers will be dropped.
    pub fn resize(&self, new_max_size: usize) {
        let mut buffers = self.buffers.lock().unwrap();
        
        while buffers.len() > new_max_size {
            buffers.pop_front();
        }
        
        // Note: We can't modify config.max_size here since it's not mutable
        // This would require a different approach for dynamic resizing
        drop(buffers);
    }
}

/// A scoped buffer that automatically returns to the pool when dropped
///
/// This provides a convenient RAII wrapper around pooled buffers.
pub struct PooledBuffer {
    buffer: Option<BytesMut>,
    pool: Arc<BufferPool>,
}

impl PooledBuffer {
    /// Create a new pooled buffer
    pub fn new(pool: Arc<BufferPool>) -> Self {
        let buffer = pool.acquire();
        Self {
            buffer: Some(buffer),
            pool,
        }
    }

    /// Get a reference to the underlying BytesMut
    pub fn get(&self) -> &BytesMut {
        self.buffer.as_ref().unwrap()
    }

    /// Get a mutable reference to the underlying BytesMut
    pub fn get_mut(&mut self) -> &mut BytesMut {
        self.buffer.as_mut().unwrap()
    }

    /// Convert to BytesMut, consuming the pooled buffer
    ///
    /// Note: This prevents the buffer from being returned to the pool.
    pub fn into_inner(mut self) -> BytesMut {
        self.buffer.take().unwrap()
    }

    /// Return the buffer to the pool early
    ///
    /// This is equivalent to dropping the PooledBuffer.
    pub fn release(mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.release(buffer);
        }
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = BytesMut;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.release(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_acquire_release() {
        let pool = BufferPool::default_pool();
        
        // Acquire a buffer
        let buffer = pool.acquire();
        assert_eq!(buffer.capacity(), 4096); // default initial_capacity
        
        // Release it back to the pool
        pool.release(buffer);
        assert_eq!(pool.size(), 1);
        
        // Acquire again - should get the same buffer back
        let buffer2 = pool.acquire();
        assert_eq!(pool.size(), 0);
        assert_eq!(buffer2.capacity(), 4096);
    }

    #[test]
    fn test_buffer_pool_max_size() {
        let config = BufferPoolConfig {
            max_size: 2,
            initial_capacity: 1024,
            max_capacity: 2048,
        };
        let pool = BufferPool::new(config);
        
        // Acquire and release more buffers than max_size
        let b1 = pool.acquire();
        let b2 = pool.acquire();
        let b3 = pool.acquire();
        
        pool.release(b1);
        pool.release(b2);
        pool.release(b3); // This should be dropped since pool is full
        
        assert_eq!(pool.size(), 2); // max_size
    }

    #[test]
    fn test_buffer_pool_max_capacity() {
        let config = BufferPoolConfig {
            max_size: 10,
            initial_capacity: 1024,
            max_capacity: 2048,
        };
        let pool = BufferPool::new(config);
        
        // Create a buffer larger than max_capacity
        let mut large_buffer = BytesMut::with_capacity(4096);
        large_buffer.extend_from_slice(&[0u8; 3000]);
        
        // Release it - should be dropped due to size
        pool.release(large_buffer);
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn test_pooled_buffer_raii() {
        let pool = BufferPool::default_pool();
        
        {
            let mut buf = PooledBuffer::new(pool.clone());
            buf.extend_from_slice(b"hello");
            assert_eq!(buf.get().len(), 5);
            // buf is dropped here and returned to pool
        }
        
        assert_eq!(pool.size(), 1);
    }

    #[test]
    fn test_buffer_pool_clear() {
        let pool = BufferPool::default_pool();
        
        let b1 = pool.acquire();
        let b2 = pool.acquire();
        pool.release(b1);
        pool.release(b2);
        
        assert_eq!(pool.size(), 2);
        
        pool.clear();
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn test_buffer_pool_custom_config() {
        let config = BufferPoolConfig {
            max_size: 100,
            initial_capacity: 8192,
            max_capacity: 65536,
        };
        let pool = BufferPool::new(config);
        
        assert_eq!(pool.max_size(), 100);
        assert_eq!(pool.initial_capacity(), 8192);
        assert_eq!(pool.max_capacity(), 65536);
        
        let buffer = pool.acquire();
        assert_eq!(buffer.capacity(), 8192);
    }
}