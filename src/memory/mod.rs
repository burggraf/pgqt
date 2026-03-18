//! Memory Management Module
//!
//! Provides memory monitoring, usage tracking, and automatic cleanup
//! capabilities to help manage memory pressure in high-load scenarios.
//!
//! # Example
//!
//! ```
//! use pgqt::memory::{MemoryMonitor, MemoryMonitorConfig, MemoryPressureLevel};
//! use std::sync::Arc;
//!
//! // Create a memory monitor with custom configuration
//! let config = MemoryMonitorConfig {
//!     enabled: true,
//!     threshold: 64 * 1024 * 1024,  // 64MB
//!     high_threshold: 128 * 1024 * 1024,  // 128MB
//!     check_interval: 10,
//!     auto_cleanup: true,
//! };
//! let monitor = MemoryMonitor::new(config);
//!
//! // Register a handler for memory pressure events
//! monitor.on_memory_pressure(Arc::new(|level| {
//!     match level {
//!         MemoryPressureLevel::Normal => {}
//!         MemoryPressureLevel::ThresholdExceeded => {
//!             println!("Memory threshold exceeded - consider cleanup");
//!         }
//!         MemoryPressureLevel::HighThresholdExceeded => {
//!             println!("CRITICAL: High memory threshold exceeded!");
//!         }
//!     }
//! }));
//!
//! // Check memory usage manually
//! let level = monitor.check_memory();
//! let stats = monitor.get_stats();
//! println!("Current memory: {} bytes", stats.resident_bytes);
//! ```

pub mod monitor;

pub use monitor::{MemoryMonitor, MemoryMonitorConfig, MemoryStats, MemoryPressureLevel, MemoryPressureHandler};