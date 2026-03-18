//! Buffer Pool Module
//!
//! Provides efficient buffer management using BytesMut for reduced
//! allocation overhead in high-throughput scenarios.
//!
//! # Example
//!
//! ```
//! use pgqt::buffer::{BufferPool, BufferPoolConfig, PooledBuffer};
//!
//! // Create a buffer pool with custom configuration
//! let config = BufferPoolConfig {
//!     max_size: 50,
//!     initial_capacity: 4096,
//!     max_capacity: 65536,
//! };
//! let pool = BufferPool::new(config);
//!
//! // Acquire a buffer from the pool
//! let mut buffer = pool.acquire();
//! buffer.extend_from_slice(b"Hello, World!");
//!
//! // Release it back to the pool
//! pool.release(buffer);
//!
//! // Or use the RAII wrapper
//! let mut pooled = PooledBuffer::new(pool.clone());
//! pooled.extend_from_slice(b"Automatic return to pool on drop");
//! ```

pub mod pool;

pub use pool::{BufferPool, BufferPoolConfig, PooledBuffer};