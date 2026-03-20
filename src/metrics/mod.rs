#[cfg(feature = "metrics")]
mod proxy;

#[cfg(feature = "metrics")]
pub use proxy::*;

#[cfg(feature = "metrics")]
mod server;

#[cfg(feature = "metrics")]
pub use server::*;

#[cfg(feature = "web-config")]
pub use server::WebInterface;

#[cfg(feature = "metrics")]
mod global;

#[cfg(feature = "metrics")]
pub use global::*;

#[cfg(feature = "system-metrics")]
mod system;

#[cfg(feature = "system-metrics")]
pub use system::*;

/// Stub for non-metrics builds
#[cfg(not(feature = "metrics"))]
pub struct ProxyMetrics;

#[cfg(not(feature = "metrics"))]
impl ProxyMetrics {
    pub fn new(_registry: &mut ()) -> Self {
        Self
    }

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
