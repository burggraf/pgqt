//! Tests for MetricsServer

#[cfg(feature = "metrics")]
mod tests {
    #[test]
    fn test_metrics_server_creation() {
        use pgqt::metrics::MetricsServer;

        let server = MetricsServer::new();
        // Should be able to access metrics
        let _metrics = server.metrics();
    }

    #[test]
    fn test_metrics_server_default() {
        use pgqt::metrics::MetricsServer;

        // Test Default trait
        let server: MetricsServer = Default::default();
        let _metrics = server.metrics();
    }
}

#[cfg(not(feature = "metrics"))]
mod stub_tests {
    // Stub tests for non-metrics builds
    #[test]
    fn stub_test() {
        // Nothing to test without metrics feature
    }
}
