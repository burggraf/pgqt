//! Lightweight Prometheus metrics for PGQT
//!
//! Uses prometheus-client for minimal overhead and binary size impact.

#[cfg(feature = "metrics")]
mod proxy;

#[cfg(feature = "metrics")]
pub use proxy::*;

/// Stub for non-metrics builds
#[cfg(not(feature = "metrics"))]
pub struct ProxyMetrics;

#[cfg(not(feature = "metrics"))]
impl ProxyMetrics {
    pub fn record_query(&self, _query_type: QueryType, _duration_secs: f64, _success: bool) {}
    pub fn inc_connections(&self) {}
    pub fn dec_connections(&self) {}
}

#[derive(Debug, Clone, Copy)]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
    Ddl,
    Other,
}
