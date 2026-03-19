//! HTTP server for Prometheus metrics scraping

use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use std::sync::{Arc, Mutex};
use tiny_http::{Response, Server};

use crate::metrics::ProxyMetrics;

/// HTTP server for serving Prometheus metrics
pub struct MetricsServer {
    registry: Arc<Mutex<Registry>>,
    metrics: ProxyMetrics,
}

impl MetricsServer {
    /// Create a new metrics server with an empty registry
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);

        Self {
            registry: Arc::new(Mutex::new(registry)),
            metrics,
        }
    }

    /// Get a reference to the proxy metrics for recording
    pub fn metrics(&self) -> &ProxyMetrics {
        &self.metrics
    }

    /// Clone the metrics handle for global registration
    ///
    /// This allows registering the metrics with the global metrics instance
    /// while keeping the server operational.
    pub fn clone_metrics(&self) -> ProxyMetrics {
        self.metrics.clone()
    }

    /// Start the HTTP server in a background thread
    /// Returns a JoinHandle that can be used to wait for server completion
    pub fn start(self, port: u16) -> std::thread::JoinHandle<()> {
        let addr = format!("0.0.0.0:{}", port);
        let server = Server::http(&addr).expect("Failed to bind metrics server");
        let registry = self.registry.clone();

        std::thread::spawn(move || {
            println!("Metrics server listening on http://{}/metrics", addr);

            for request in server.incoming_requests() {
                let response = Self::handle_request(&request, &registry);
                let _ = request.respond(response);
            }
        })
    }

    /// Handle an incoming HTTP request
    fn handle_request(
        request: &tiny_http::Request,
        registry: &Arc<Mutex<Registry>>,
    ) -> Response<std::io::Cursor<Vec<u8>>> {
        match request.url() {
            "/metrics" => {
                let mut buffer = String::new();
                if let Ok(reg) = registry.lock() {
                    if encode(&mut buffer, &reg).is_ok() {
                        return Response::from_string(buffer)
                            .with_header(tiny_http::Header::from_bytes(
                                &b"Content-Type"[..],
                                &b"text/plain; charset=utf-8"[..],
                            ).expect("valid header"));
                    }
                }
                Response::from_string("Failed to encode metrics").with_status_code(500)
            }
            "/health" => {
                let health = r#"{"status":"healthy"}"#;
                Response::from_string(health)
                    .with_header(tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/json"[..],
                    ).expect("valid header"))
            }
            _ => Response::from_string("Not Found").with_status_code(404),
        }
    }
}

impl Default for MetricsServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_metrics_server_new() {
        let server = MetricsServer::new();
        // Should be able to access metrics
        let _metrics = server.metrics();
    }

    #[test]
    fn test_metrics_server_default() {
        let server: MetricsServer = Default::default();
        let _metrics = server.metrics();
    }

    #[test]
    fn test_registry_is_populated() {
        let server = MetricsServer::new();
        
        // Verify the registry contains our metrics by encoding it
        let mut buffer = String::new();
        if let Ok(reg) = server.registry.lock() {
            encode(&mut buffer, &reg).expect("should encode");
        }
        
        // The registry should contain pgqt_ prefixed metrics
        assert!(buffer.contains("pgqt_"), "Registry should contain pgqt_ prefixed metrics");
    }

    #[test]
    fn test_server_start() {
        let server = MetricsServer::new();
        let _handle = server.start(19999);

        // Give server time to start
        thread::sleep(Duration::from_millis(200));

        // The server is running in a background thread
        // We verified it starts without panicking
    }
}
