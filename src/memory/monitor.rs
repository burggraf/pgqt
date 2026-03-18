//! Memory Monitoring for PGQT
//!
//! This module provides memory usage monitoring and automatic cleanup
//! capabilities to help manage memory pressure in high-load scenarios.

use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::{Duration, Instant};
use std::thread;

/// Configuration for memory monitoring
#[derive(Debug, Clone, Copy)]
pub struct MemoryMonitorConfig {
    /// Enable memory monitoring (default: false)
    pub enabled: bool,
    /// Memory threshold in bytes for normal operation (default: 64MB)
    pub threshold: usize,
    /// High memory threshold for aggressive cleanup (default: 128MB)
    pub high_threshold: usize,
    /// Check interval in seconds (default: 10)
    pub check_interval: u64,
    /// Enable automatic cleanup when thresholds are exceeded (default: false)
    pub auto_cleanup: bool,
}

impl Default for MemoryMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 64 * 1024 * 1024,      // 64MB
            high_threshold: 128 * 1024 * 1024, // 128MB
            check_interval: 10,
            auto_cleanup: false,
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    /// Current resident memory in bytes
    pub resident_bytes: usize,
    /// Virtual memory size in bytes
    pub virtual_bytes: usize,
    /// Peak resident memory in bytes
    pub peak_resident_bytes: usize,
    /// Timestamp of the measurement
    pub timestamp: Instant,
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self {
            resident_bytes: 0,
            virtual_bytes: 0,
            peak_resident_bytes: 0,
            timestamp: Instant::now(),
        }
    }
}

/// Callback type for memory pressure handlers
pub type MemoryPressureHandler = Arc<dyn Fn(MemoryPressureLevel) + Send + Sync>;

/// Level of memory pressure detected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressureLevel {
    /// Normal memory usage
    Normal,
    /// Threshold exceeded - moderate cleanup recommended
    ThresholdExceeded,
    /// High threshold exceeded - aggressive cleanup required
    HighThresholdExceeded,
}

impl MemoryPressureLevel {
    /// Check if cleanup should be performed
    pub fn should_cleanup(&self) -> bool {
        matches!(self, Self::ThresholdExceeded | Self::HighThresholdExceeded)
    }

    /// Check if aggressive cleanup is needed
    pub fn is_critical(&self) -> bool {
        matches!(self, Self::HighThresholdExceeded)
    }
}

/// Memory monitor that tracks usage and triggers cleanup when needed
pub struct MemoryMonitor {
    config: MemoryMonitorConfig,
    stats: Mutex<MemoryStats>,
    running: AtomicBool,
    check_count: AtomicU64,
    threshold_exceeded_count: AtomicU64,
    high_threshold_exceeded_count: AtomicU64,
    last_cleanup: Mutex<Option<Instant>>,
    handlers: Mutex<Vec<MemoryPressureHandler>>,
}

impl std::fmt::Debug for MemoryMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryMonitor")
            .field("config", &self.config)
            .field("stats", &self.stats)
            .field("running", &self.running.load(std::sync::atomic::Ordering::SeqCst))
            .field("check_count", &self.check_count)
            .field("threshold_exceeded_count", &self.threshold_exceeded_count)
            .field("high_threshold_exceeded_count", &self.high_threshold_exceeded_count)
            .field("last_cleanup", &self.last_cleanup)
            .field("handlers_count", &self.handlers.lock().unwrap().len())
            .finish()
    }
}

impl MemoryMonitor {
    /// Create a new memory monitor with the given configuration
    ///
    /// # Example
    /// ```
    /// use pgqt::memory::{MemoryMonitor, MemoryMonitorConfig};
    ///
    /// let config = MemoryMonitorConfig {
    ///     enabled: true,
    ///     threshold: 64 * 1024 * 1024,  // 64MB
    ///     high_threshold: 128 * 1024 * 1024,  // 128MB
    ///     check_interval: 10,
    ///     auto_cleanup: true,
    /// };
    /// let monitor = MemoryMonitor::new(config);
    /// ```
    pub fn new(config: MemoryMonitorConfig) -> Arc<Self> {
        let monitor = Arc::new(Self {
            config,
            stats: Mutex::new(MemoryStats::default()),
            running: AtomicBool::new(false),
            check_count: AtomicU64::new(0),
            threshold_exceeded_count: AtomicU64::new(0),
            high_threshold_exceeded_count: AtomicU64::new(0),
            last_cleanup: Mutex::new(None),
            handlers: Mutex::new(Vec::new()),
        });

        if config.enabled {
            monitor.start_monitoring();
        }

        monitor
    }

    /// Create a disabled memory monitor (no-op)
    pub fn disabled() -> Arc<Self> {
        Arc::new(Self {
            config: MemoryMonitorConfig::default(),
            stats: Mutex::new(MemoryStats::default()),
            running: AtomicBool::new(false),
            check_count: AtomicU64::new(0),
            threshold_exceeded_count: AtomicU64::new(0),
            high_threshold_exceeded_count: AtomicU64::new(0),
            last_cleanup: Mutex::new(None),
            handlers: Mutex::new(Vec::new()),
        })
    }

    /// Start the background monitoring thread
    fn start_monitoring(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return; // Already running
        }

        let monitor = Arc::clone(self);
        let interval = Duration::from_secs(self.config.check_interval);

        thread::spawn(move || {
            while monitor.running.load(Ordering::SeqCst) {
                monitor.check_memory();
                thread::sleep(interval);
            }
        });
    }

    /// Stop the background monitoring thread
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check current memory usage and trigger handlers if needed
    pub fn check_memory(&self) -> MemoryPressureLevel {
        self.check_count.fetch_add(1, Ordering::SeqCst);

        let stats = self.get_memory_stats();
        
        // Update stored stats
        {
            let mut stored = self.stats.lock().unwrap();
            *stored = stats;
        }

        // Determine pressure level
        let level = if stats.resident_bytes >= self.config.high_threshold {
            self.high_threshold_exceeded_count.fetch_add(1, Ordering::SeqCst);
            MemoryPressureLevel::HighThresholdExceeded
        } else if stats.resident_bytes >= self.config.threshold {
            self.threshold_exceeded_count.fetch_add(1, Ordering::SeqCst);
            MemoryPressureLevel::ThresholdExceeded
        } else {
            MemoryPressureLevel::Normal
        };

        // Trigger handlers if threshold exceeded
        if level.should_cleanup() {
            if self.config.auto_cleanup {
                self.perform_cleanup();
            }
            self.notify_handlers(level);
        }

        level
    }

    /// Get current memory statistics
    ///
    /// On Linux, this reads from /proc/self/status
    /// On macOS, this uses the mach crate (if available) or returns estimates
    /// On other platforms, returns zeroed stats
    pub fn get_memory_stats(&self) -> MemoryStats {
        #[cfg(target_os = "linux")]
        {
            self.get_linux_memory_stats()
        }

        #[cfg(target_os = "macos")]
        {
            self.get_macos_memory_stats()
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            MemoryStats {
                resident_bytes: 0,
                virtual_bytes: 0,
                peak_resident_bytes: 0,
                timestamp: Instant::now(),
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn get_linux_memory_stats(&self) -> MemoryStats {
        use std::fs;
        
        let mut stats = MemoryStats {
            timestamp: Instant::now(),
            ..Default::default()
        };

        if let Ok(contents) = fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if line.starts_with("VmRSS:") {
                    // Parse: "VmRSS:    1234 kB"
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            stats.resident_bytes = kb * 1024;
                        }
                    }
                } else if line.starts_with("VmSize:") {
                    // Parse: "VmSize:    1234 kB"
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            stats.virtual_bytes = kb * 1024;
                        }
                    }
                } else if line.starts_with("VmHWM:") {
                    // Parse: "VmHWM:    1234 kB" (peak resident)
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            stats.peak_resident_bytes = kb * 1024;
                        }
                    }
                }
            }
        }

        stats
    }

    #[cfg(target_os = "macos")]
    fn get_macos_memory_stats(&self) -> MemoryStats {
        // On macOS, we can use the rusage system call
        unsafe {
            let mut rusage: libc::rusage = std::mem::zeroed();
            if libc::getrusage(libc::RUSAGE_SELF, &mut rusage) == 0 {
                MemoryStats {
                    resident_bytes: (rusage.ru_maxrss as usize),
                    virtual_bytes: 0, // Not directly available from rusage
                    peak_resident_bytes: (rusage.ru_maxrss as usize),
                    timestamp: Instant::now(),
                }
            } else {
                MemoryStats {
                    timestamp: Instant::now(),
                    ..Default::default()
                }
            }
        }
    }

    /// Perform automatic cleanup
    fn perform_cleanup(&self) {
        let mut last_cleanup = self.last_cleanup.lock().unwrap();
        let now = Instant::now();
        
        // Don't cleanup more than once per check_interval
        if let Some(last) = *last_cleanup {
            if now.duration_since(last) < Duration::from_secs(self.config.check_interval) {
                return;
            }
        }
        
        *last_cleanup = Some(now);
        drop(last_cleanup);

        // Note: jemalloc-specific tuning could be added here if jemalloc feature is enabled
    }

    /// Notify all registered handlers
    fn notify_handlers(&self, level: MemoryPressureLevel) {
        let handlers = self.handlers.lock().unwrap();
        for handler in handlers.iter() {
            handler(level);
        }
    }

    /// Register a handler to be called when memory pressure is detected
    ///
    /// # Example
    /// ```
    /// use pgqt::memory::{MemoryMonitor, MemoryPressureLevel};
    /// use std::sync::Arc;
    ///
    /// let monitor = MemoryMonitor::disabled();
    /// monitor.on_memory_pressure(Arc::new(|level| {
    ///     match level {
    ///         MemoryPressureLevel::Normal => {},
    ///         MemoryPressureLevel::ThresholdExceeded => {
    ///             println!("Memory threshold exceeded!");
    ///         }
    ///         MemoryPressureLevel::HighThresholdExceeded => {
    ///             println!("CRITICAL: High memory threshold exceeded!");
    ///         }
    ///     }
    /// }));
    /// ```
    pub fn on_memory_pressure(&self, handler: MemoryPressureHandler) {
        let mut handlers = self.handlers.lock().unwrap();
        handlers.push(handler);
    }

    /// Get the last recorded memory statistics
    pub fn get_stats(&self) -> MemoryStats {
        *self.stats.lock().unwrap()
    }

    /// Get the number of memory checks performed
    pub fn check_count(&self) -> u64 {
        self.check_count.load(Ordering::SeqCst)
    }

    /// Get the number of times threshold was exceeded
    pub fn threshold_exceeded_count(&self) -> u64 {
        self.threshold_exceeded_count.load(Ordering::SeqCst)
    }

    /// Get the number of times high threshold was exceeded
    pub fn high_threshold_exceeded_count(&self) -> u64 {
        self.high_threshold_exceeded_count.load(Ordering::SeqCst)
    }

    /// Check if monitoring is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the configuration
    pub fn config(&self) -> MemoryMonitorConfig {
        self.config
    }

    /// Format bytes as human-readable string
    pub fn format_bytes(bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;
        
        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }
        
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

impl Drop for MemoryMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_monitor_config_default() {
        let config = MemoryMonitorConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.threshold, 64 * 1024 * 1024);
        assert_eq!(config.high_threshold, 128 * 1024 * 1024);
        assert_eq!(config.check_interval, 10);
        assert!(!config.auto_cleanup);
    }

    #[test]
    fn test_memory_monitor_disabled() {
        let monitor = MemoryMonitor::disabled();
        assert!(!monitor.is_running());
        assert_eq!(monitor.check_count(), 0);
    }

    #[test]
    fn test_memory_pressure_level() {
        assert!(!MemoryPressureLevel::Normal.should_cleanup());
        assert!(!MemoryPressureLevel::Normal.is_critical());
        
        assert!(MemoryPressureLevel::ThresholdExceeded.should_cleanup());
        assert!(!MemoryPressureLevel::ThresholdExceeded.is_critical());
        
        assert!(MemoryPressureLevel::HighThresholdExceeded.should_cleanup());
        assert!(MemoryPressureLevel::HighThresholdExceeded.is_critical());
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(MemoryMonitor::format_bytes(0), "0.00 B");
        assert_eq!(MemoryMonitor::format_bytes(512), "512.00 B");
        assert_eq!(MemoryMonitor::format_bytes(1024), "1.00 KB");
        assert_eq!(MemoryMonitor::format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(MemoryMonitor::format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_memory_monitor_check() {
        let monitor = MemoryMonitor::disabled();
        
        // Even disabled monitor should be able to check memory
        let level = monitor.check_memory();
        
        // Should be normal since we're not using much memory in tests
        assert_eq!(level, MemoryPressureLevel::Normal);
        assert_eq!(monitor.check_count(), 1);
    }

    #[test]
    fn test_memory_monitor_handlers() {
        let monitor = MemoryMonitor::disabled();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        
        monitor.on_memory_pressure(Arc::new(move |_level| {
            called_clone.store(true, Ordering::SeqCst);
        }));
        
        // Manually trigger handler notification by checking memory
        // (won't actually trigger since memory is normal)
        monitor.check_memory();
        
        // Handler shouldn't be called for normal memory pressure
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_memory_stats_default() {
        let stats = MemoryStats::default();
        assert_eq!(stats.resident_bytes, 0);
        assert_eq!(stats.virtual_bytes, 0);
        assert_eq!(stats.peak_resident_bytes, 0);
    }
}