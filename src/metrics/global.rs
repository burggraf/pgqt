//! Global metrics instance for easy access across the codebase

use std::sync::OnceLock;

use crate::metrics::ProxyMetrics;

static GLOBAL_METRICS: OnceLock<ProxyMetrics> = OnceLock::new();

/// Initialize the global metrics instance
///
/// This should be called once during application startup, after creating
/// the MetricsServer. Returns Err if already initialized.
pub fn init_global_metrics(metrics: ProxyMetrics) -> Result<(), ProxyMetrics> {
    GLOBAL_METRICS.set(metrics)
}

/// Get a reference to the global metrics instance
///
/// Returns None if not yet initialized. Callers should handle this gracefully
/// by skipping metrics recording if metrics are not enabled.
pub fn get_metrics() -> Option<&'static ProxyMetrics> {
    GLOBAL_METRICS.get()
}

/// Check if global metrics have been initialized
#[allow(dead_code)]
pub fn is_initialized() -> bool {
    GLOBAL_METRICS.get().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus_client::registry::Registry;

    // Note: These tests must run serially since they use a global static
    // In practice, tests should use `cargo test -- --test-threads=1` or a test mutex

    #[test]
    fn test_global_metrics_initialization() {
        // This test assumes a clean state. In practice, tests using the global
        // metrics should be run with `--test-threads=1` to avoid interference.

        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);

        // Initialize global - should succeed (unless another test initialized it)
        // Since we can't reset OnceLock, we check the current state
        if !is_initialized() {
            assert!(init_global_metrics(metrics.clone()).is_ok());
            assert!(is_initialized());

            // Get global should return Some
            let global = get_metrics();
            assert!(global.is_some());
        } else {
            // Already initialized, verify we can get it
            assert!(is_initialized());
            assert!(get_metrics().is_some());

            // Second init should fail
            assert!(init_global_metrics(metrics.clone()).is_err());
        }
    }

    #[test]
    fn test_get_metrics_returns_some_when_initialized() {
        // Skip if not initialized (another test may have initialized it)
        if !is_initialized() {
            // Initialize for this test
            let mut registry = Registry::default();
            let metrics = ProxyMetrics::new(&mut registry);
            let _ = init_global_metrics(metrics); // May fail if another test initialized
        }

        // Should return Some if initialized
        if is_initialized() {
            assert!(get_metrics().is_some());
        }
    }

    #[test]
    fn test_double_initialization_fails() {
        // Create metrics for potential initialization
        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);

        if !is_initialized() {
            // First initialization
            assert!(init_global_metrics(metrics.clone()).is_ok());
            assert!(is_initialized());

            // Second initialization should fail
            let mut registry2 = Registry::default();
            let metrics2 = ProxyMetrics::new(&mut registry2);
            assert!(init_global_metrics(metrics2).is_err());
        } else {
            // Already initialized from another test - verify second init fails
            assert!(init_global_metrics(metrics).is_err());
        }
    }
}
