use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;

use crate::metrics::QueryType;

/// Metrics for the PostgreSQL proxy
///
/// This struct is cheap to clone since all metric types are internally
/// reference-counted (Arc-based).
#[derive(Clone)]
pub struct ProxyMetrics {
    pub requests_total: Counter,
    pub requests_failed_total: Counter,
    pub query_duration_seconds: Histogram,
    pub connections_active: Gauge,
    pub connections_total: Counter,
    pub queries_select_total: Counter,
    pub queries_insert_total: Counter,
    pub queries_update_total: Counter,
    pub queries_delete_total: Counter,
    pub queries_ddl_total: Counter,
    pub queries_other_total: Counter,
}

impl ProxyMetrics {
    pub fn new(registry: &mut Registry) -> Self {
        let requests_total = Counter::default();
        registry.register(
            "pgqt_requests_total",
            "Total requests processed",
            requests_total.clone(),
        );

        let requests_failed_total = Counter::default();
        registry.register(
            "pgqt_requests_failed_total",
            "Total failed requests",
            requests_failed_total.clone(),
        );

        // Use exponential buckets: 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s
        let query_duration_seconds = Histogram::new(exponential_buckets(0.001, 5.0, 12));
        registry.register(
            "pgqt_query_duration_seconds",
            "Query execution latency in seconds",
            query_duration_seconds.clone(),
        );

        let connections_active = Gauge::default();
        registry.register(
            "pgqt_connections_active",
            "Currently active connections",
            connections_active.clone(),
        );

        let connections_total = Counter::default();
        registry.register(
            "pgqt_connections_total",
            "Total connections accepted",
            connections_total.clone(),
        );

        let queries_select_total = Counter::default();
        registry.register(
            "pgqt_queries_select_total",
            "Total SELECT queries",
            queries_select_total.clone(),
        );

        let queries_insert_total = Counter::default();
        registry.register(
            "pgqt_queries_insert_total",
            "Total INSERT queries",
            queries_insert_total.clone(),
        );

        let queries_update_total = Counter::default();
        registry.register(
            "pgqt_queries_update_total",
            "Total UPDATE queries",
            queries_update_total.clone(),
        );

        let queries_delete_total = Counter::default();
        registry.register(
            "pgqt_queries_delete_total",
            "Total DELETE queries",
            queries_delete_total.clone(),
        );

        let queries_ddl_total = Counter::default();
        registry.register(
            "pgqt_queries_ddl_total",
            "Total DDL queries",
            queries_ddl_total.clone(),
        );

        let queries_other_total = Counter::default();
        registry.register(
            "pgqt_queries_other_total",
            "Total other queries",
            queries_other_total.clone(),
        );

        Self {
            requests_total,
            requests_failed_total,
            query_duration_seconds,
            connections_active,
            connections_total,
            queries_select_total,
            queries_insert_total,
            queries_update_total,
            queries_delete_total,
            queries_ddl_total,
            queries_other_total,
        }
    }

    /// Record a query execution with timing and success status
    pub fn record_query(&self, query_type: QueryType, duration_secs: f64, success: bool) {
        self.requests_total.inc();
        self.query_duration_seconds.observe(duration_secs);

        if !success {
            self.requests_failed_total.inc();
        }

        match query_type {
            QueryType::Select => self.queries_select_total.inc(),
            QueryType::Insert => self.queries_insert_total.inc(),
            QueryType::Update => self.queries_update_total.inc(),
            QueryType::Delete => self.queries_delete_total.inc(),
            QueryType::Ddl => self.queries_ddl_total.inc(),
            QueryType::Other => self.queries_other_total.inc(),
        };
        // Note: inc() returns the previous value which we ignore here
    }

    /// Increment active connections gauge
    pub fn inc_connections(&self) {
        self.connections_active.inc();
        self.connections_total.inc();
    }

    /// Decrement active connections gauge
    pub fn dec_connections(&self) {
        self.connections_active.dec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_metrics_new() {
        let mut registry = Registry::default();
        let _metrics = ProxyMetrics::new(&mut registry);

        // Verify all fields are initialized by checking it compiles and runs
    }

    #[test]
    fn test_record_query_success() {
        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);

        metrics.record_query(QueryType::Select, 0.05, true);

        assert_eq!(metrics.requests_total.get(), 1);
        assert_eq!(metrics.queries_select_total.get(), 1);
        assert_eq!(metrics.requests_failed_total.get(), 0);
    }

    #[test]
    fn test_record_query_failure() {
        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);

        metrics.record_query(QueryType::Insert, 0.1, false);

        assert_eq!(metrics.requests_total.get(), 1);
        assert_eq!(metrics.queries_insert_total.get(), 1);
        assert_eq!(metrics.requests_failed_total.get(), 1);
    }

    #[test]
    fn test_record_query_all_types() {
        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);

        metrics.record_query(QueryType::Select, 0.01, true);
        metrics.record_query(QueryType::Insert, 0.02, true);
        metrics.record_query(QueryType::Update, 0.03, true);
        metrics.record_query(QueryType::Delete, 0.04, true);
        metrics.record_query(QueryType::Ddl, 0.05, true);
        metrics.record_query(QueryType::Other, 0.06, true);

        assert_eq!(metrics.requests_total.get(), 6);
        assert_eq!(metrics.queries_select_total.get(), 1);
        assert_eq!(metrics.queries_insert_total.get(), 1);
        assert_eq!(metrics.queries_update_total.get(), 1);
        assert_eq!(metrics.queries_delete_total.get(), 1);
        assert_eq!(metrics.queries_ddl_total.get(), 1);
        assert_eq!(metrics.queries_other_total.get(), 1);
    }

    #[test]
    fn test_connections_management() {
        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);

        metrics.inc_connections();
        assert_eq!(metrics.connections_active.get(), 1);
        assert_eq!(metrics.connections_total.get(), 1);

        metrics.inc_connections();
        assert_eq!(metrics.connections_active.get(), 2);
        assert_eq!(metrics.connections_total.get(), 2);

        metrics.dec_connections();
        assert_eq!(metrics.connections_active.get(), 1);
        // connections_total should not decrease
        assert_eq!(metrics.connections_total.get(), 2);

        metrics.dec_connections();
        assert_eq!(metrics.connections_active.get(), 0);
        assert_eq!(metrics.connections_total.get(), 2);
    }
}
