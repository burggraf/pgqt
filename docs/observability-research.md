# PGQT Observability Research: OpenTelemetry + Prometheus/Grafana Integration

## Executive Summary

For PGQT (PostgreSQL-to-SQLite proxy), we recommend a **hybrid, lightweight observability stack** that balances feature richness with minimal performance impact and binary size:

| Approach | Binary Impact | CPU Overhead | Best For |
|----------|--------------|--------------|----------|
| **Recommended: `prometheus-client` + `tiny_http`** | ~1.5-2 MB | <0.1% hot path | Production metrics |
| **Optional: `tracing` + stdout exporter** | ~500 KB | 3-5% (with sampling) | Distributed tracing |
| **Avoid: Full OTel with OTLP/gRPC** | +5-10 MB | 5-10% | Not recommended for proxy |

**Key Insight:** The full OpenTelemetry stack with gRPC/OTLP exporters adds 5-10 MB to the binary due to `tonic`, `prost`, and HTTP/2 dependencies. For a lightweight proxy, this is disproportionate.

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        PGQT Proxy                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Query     │  │  Prometheus │  │   Tracing (Optional)    │  │
│  │   Handler   │──│   Metrics   │  │   (tracing + stdout)    │  │
│  │             │  │   Registry  │  │                         │  │
│  └─────────────┘  └──────┬──────┘  └─────────────────────────┘  │
│                          │                                      │
│                   ┌──────▼──────┐                               │
│                   │  /metrics   │                               │
│                   │   Endpoint  │                               │
│                   │ (tiny_http) │                               │
│                   └──────┬──────┘                               │
└──────────────────────────┼──────────────────────────────────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            ▼            ▼
        ┌─────────┐  ┌─────────┐  ┌─────────┐
        │Prometheus│  │ Grafana │  │  OTel   │
        │ Server   │  │ (viz)   │  │ Collector│
        └─────────┘  └─────────┘  └─────────┘
```

---

## 2. Recommended Implementation

### 2.1 Core Metrics with `prometheus-client`

**Why `prometheus-client` over `metrics` crate:**
- **40% better encoding performance** (visitor pattern, zero-allocation)
- **Smaller binary:** ~1.5-2 MB vs 2.5-4.5 MB with `metrics` + exporter stack
- **Type-safe labels** enforced at compile time (prevents high-cardinality bugs)
- **No forced dependencies** on `hyper`/`axum` - can use lightweight HTTP server

**Cargo.toml:**
```toml
[dependencies]
prometheus-client = "0.22"
tiny_http = "0.12"

# Optional: tracing support (feature-gated)
tracing = { version = "0.1", default-features = false, features = ["std"], optional = true }
tracing-subscriber = { version = "0.3", default-features = false, features = ["registry", "fmt"], optional = true }

[features]
default = ["plpgsql", "tls"]
metrics = ["prometheus-client", "tiny_http"]
tracing = ["dep:tracing", "dep:tracing-subscriber"]
```

### 2.2 Metrics Module Structure

**File: `src/metrics/mod.rs`**
```rust
//! Lightweight Prometheus metrics for PGQT
//! 
//! Uses prometheus-client for minimal overhead and binary size impact.

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::Registry;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

/// Core metrics for the PGQT proxy
#[derive(Clone)]
pub struct ProxyMetrics {
    // RED Method: Rate, Errors, Duration
    pub requests_total: Counter<AtomicU64>,
    pub requests_failed: Counter<AtomicU64>,
    pub query_duration: Histogram,
    
    // Connection metrics
    pub connections_active: Gauge<AtomicU64>,
    pub connections_total: Counter<AtomicU64>,
    
    // Query type breakdown
    pub queries_by_type: QueryTypeMetrics,
    
    // SQLite/Transpiler metrics
    pub transpile_cache_hits: Counter<AtomicU64>,
    pub transpile_cache_misses: Counter<AtomicU64>,
    pub sqlite_queries_total: Counter<AtomicU64>,
    
    // Pool metrics (if pooling enabled)
    pub pool_wait_time: Histogram,
    pub pool_checkout_failures: Counter<AtomicU64>,
}

#[derive(Clone)]
pub struct QueryTypeMetrics {
    pub select: Counter<AtomicU64>,
    pub insert: Counter<AtomicU64>,
    pub update: Counter<AtomicU64>,
    pub delete: Counter<AtomicU64>,
    pub ddl: Counter<AtomicU64>,
    pub other: Counter<AtomicU64>,
}

impl ProxyMetrics {
    pub fn new(registry: &mut Registry) -> Self {
        // RED metrics
        let requests_total = Counter::default();
        registry.register("pgqt_requests_total", "Total requests processed", requests_total.clone());
        
        let requests_failed = Counter::default();
        registry.register("pgqt_requests_failed_total", "Total failed requests", requests_failed.clone());
        
        let query_duration = Histogram::new([0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0].into_iter());
        registry.register("pgqt_query_duration_seconds", "Query execution latency", query_duration.clone());
        
        // Connection metrics
        let connections_active = Gauge::default();
        registry.register("pgqt_connections_active", "Currently active connections", connections_active.clone());
        
        let connections_total = Counter::default();
        registry.register("pgqt_connections_total", "Total connections accepted", connections_total.clone());
        
        // Query type metrics
        let queries_by_type = QueryTypeMetrics {
            select: create_counter(registry, "pgqt_queries_select_total"),
            insert: create_counter(registry, "pgqt_queries_insert_total"),
            update: create_counter(registry, "pgqt_queries_update_total"),
            delete: create_counter(registry, "pgqt_queries_delete_total"),
            ddl: create_counter(registry, "pgqt_queries_ddl_total"),
            other: create_counter(registry, "pgqt_queries_other_total"),
        };
        
        // Cache metrics
        let transpile_cache_hits = Counter::default();
        registry.register("pgqt_transpile_cache_hits_total", "Transpile cache hits", transpile_cache_hits.clone());
        
        let transpile_cache_misses = Counter::default();
        registry.register("pgqt_transpile_cache_misses_total", "Transpile cache misses", transpile_cache_misses.clone());
        
        let sqlite_queries_total = Counter::default();
        registry.register("pgqt_sqlite_queries_total", "Total SQLite queries executed", sqlite_queries_total.clone());
        
        // Pool metrics
        let pool_wait_time = Histogram::new([0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0].into_iter());
        registry.register("pgqt_pool_wait_duration_seconds", "Time waiting for connection from pool", pool_wait_time.clone());
        
        let pool_checkout_failures = Counter::default();
        registry.register("pgqt_pool_checkout_failures_total", "Failed connection checkouts", pool_checkout_failures.clone());
        
        Self {
            requests_total,
            requests_failed,
            query_duration,
            connections_active,
            connections_total,
            queries_by_type,
            transpile_cache_hits,
            transpile_cache_misses,
            sqlite_queries_total,
            pool_wait_time,
            pool_checkout_failures,
        }
    }
    
    /// Record a query execution with timing
    pub fn record_query(&self, query_type: QueryType, duration_secs: f64, success: bool) {
        self.query_duration.observe(duration_secs);
        self.requests_total.inc();
        
        if !success {
            self.requests_failed.inc();
        }
        
        match query_type {
            QueryType::Select => self.queries_by_type.select.inc(),
            QueryType::Insert => self.queries_by_type.insert.inc(),
            QueryType::Update => self.queries_by_type.update.inc(),
            QueryType::Delete => self.queries_by_type.delete.inc(),
            QueryType::Ddl => self.queries_by_type.ddl.inc(),
            QueryType::Other => self.queries_by_type.other.inc(),
        }
    }
}

fn create_counter(registry: &mut Registry, name: &str) -> Counter<AtomicU64> {
    let counter = Counter::default();
    registry.register(name, "Query counter", counter.clone());
    counter
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

/// Global metrics registry wrapper
pub struct MetricsServer {
    registry: Arc<Mutex<Registry>>,
    metrics: ProxyMetrics,
}

impl MetricsServer {
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let metrics = ProxyMetrics::new(&mut registry);
        
        Self {
            registry: Arc::new(Mutex::new(registry)),
            metrics,
        }
    }
    
    pub fn metrics(&self) -> &ProxyMetrics {
        &self.metrics
    }
    
    /// Start the metrics HTTP server on the given port
    pub fn start_server(&self, port: u16) -> std::thread::JoinHandle<()> {
        let registry = self.registry.clone();
        let addr = format!("127.0.0.1:{}", port);
        
        std::thread::spawn(move || {
            let server = tiny_http::Server::http(&addr).expect("Failed to bind metrics server");
            println!("Metrics server listening on http://{}/metrics", addr);
            
            for request in server.incoming_requests() {
                match request.url() {
                    "/metrics" => {
                        let mut buffer = String::new();
                        if let Ok(reg) = registry.lock() {
                            encode(&mut buffer, &reg).unwrap();
                        }
                        
                        let response = tiny_http::Response::from_string(buffer)
                            .with_header(tiny_http::Header::from_bytes(
                                &b"Content-Type"[..],
                                &b"application/openmetrics-text; version=1.0.0; charset=utf-8"[..]
                            ).unwrap());
                        let _ = request.respond(response);
                    }
                    "/health" => {
                        let response = tiny_http::Response::from_string(r#"{"status":"healthy"}"#)
                            .with_header(tiny_http::Header::from_bytes(
                                &b"Content-Type"[..],
                                &b"application/json"[..]
                            ).unwrap());
                        let _ = request.respond(response);
                    }
                    _ => {
                        let response = tiny_http::Response::from_string("Not Found")
                            .with_status_code(404);
                        let _ = request.respond(response);
                    }
                }
            }
        })
    }
}

/// Global metrics instance (lazy-initialized)
use std::sync::OnceLock;

static GLOBAL_METRICS: OnceLock<ProxyMetrics> = OnceLock::new();

pub fn init_global_metrics(metrics: ProxyMetrics) {
    let _ = GLOBAL_METRICS.set(metrics);
}

pub fn get_metrics() -> Option<&'static ProxyMetrics> {
    GLOBAL_METRICS.get()
}
```

### 2.3 Integration Points in Handler

**In `src/handler/query.rs` or `src/handler/mod.rs`:**

```rust
use crate::metrics::{ProxyMetrics, QueryType, get_metrics};
use std::time::Instant;

impl SqliteHandler {
    pub fn execute_query_with_metrics(&self, client_id: u32, query: &str) -> Result<Vec<Response>> {
        let start = Instant::now();
        let query_type = classify_query(query);
        
        // Increment active connections gauge
        if let Some(m) = get_metrics() {
            m.connections_active.inc();
        }
        
        let result = self.execute_query_internal(client_id, query);
        
        let duration = start.elapsed().as_secs_f64();
        let success = result.is_ok();
        
        // Record metrics
        if let Some(m) = get_metrics() {
            m.record_query(query_type, duration, success);
            m.connections_active.dec();
        }
        
        result
    }
}

fn classify_query(query: &str) -> QueryType {
    let upper = query.trim().to_uppercase();
    if upper.starts_with("SELECT") {
        QueryType::Select
    } else if upper.starts_with("INSERT") {
        QueryType::Insert
    } else if upper.starts_with("UPDATE") {
        QueryType::Update
    } else if upper.starts_with("DELETE") {
        QueryType::Delete
    } else if upper.starts_with("CREATE") 
           || upper.starts_with("ALTER") 
           || upper.starts_with("DROP") {
        QueryType::Ddl
    } else {
        QueryType::Other
    }
}
```

### 2.4 CLI Integration

**In `src/main.rs`:**

```rust
#[derive(Parser, Debug)]
#[command(name = "pgqt")]
struct Cli {
    // ... existing args ...
    
    /// Enable Prometheus metrics endpoint
    #[arg(long, env = "PGQT_METRICS_ENABLED")]
    metrics_enabled: bool,
    
    /// Port for metrics HTTP server (default: 9090)
    #[arg(long, env = "PGQT_METRICS_PORT", default_value = "9090")]
    metrics_port: u16,
}

// In main():
if cli.metrics_enabled {
    let metrics_server = MetricsServer::new();
    let metrics = metrics_server.metrics().clone();
    crate::metrics::init_global_metrics(metrics);
    
    // Start metrics server in background thread
    metrics_server.start_server(cli.metrics_port);
}
```

---

## 3. Optional: Distributed Tracing

For distributed tracing without the binary size penalty:

```toml
[dependencies]
# Minimal tracing setup - no gRPC, no tonic
tracing = { version = "0.1", default-features = false, features = ["std"] }
tracing-subscriber = { version = "0.3", default-features = false, features = ["registry", "fmt", "env-filter"] }
```

**Key optimization:** Use stdout exporter or file exporter instead of OTLP/gRPC. The OTel collector can read these files or stdout.

---

## 4. Grafana Dashboard

### 4.1 Prometheus Scrape Config

```yaml
scrape_configs:
  - job_name: 'pgqt'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: /metrics
    scrape_interval: 15s
```

### 4.2 Key Metrics to Visualize

| Panel | PromQL Query | Purpose |
|-------|--------------|---------|
| **Request Rate** | `rate(pgqt_requests_total[5m])` | Throughput |
| **Error Rate** | `rate(pgqt_requests_failed_total[5m])` | Error tracking |
| **P99 Latency** | `histogram_quantile(0.99, rate(pgqt_query_duration_seconds_bucket[5m]))` | Tail latency |
| **Active Connections** | `pgqt_connections_active` | Connection saturation |
| **Cache Hit Rate** | `pgqt_transpile_cache_hits_total / (pgqt_transpile_cache_hits_total + pgqt_transpile_cache_misses_total)` | Cache efficiency |
| **Query Mix** | `rate(pgqt_queries_select_total[5m])`, etc. | Workload characterization |

---

## 5. Performance Considerations

### 5.1 Hot Path Optimizations

1. **Atomic operations only:** All `prometheus-client` metrics use `AtomicU64` - no locks in hot path
2. **Zero-allocation encoding:** Text encoding uses visitor pattern, no intermediate allocations
3. **Separate thread for HTTP:** Metrics server runs in dedicated thread, doesn't block async runtime
4. **Sampling for tracing:** If tracing enabled, use head-based sampling (10%) to reduce overhead

### 5.2 Binary Size Impact

| Component | Size Impact |
|-----------|-------------|
| `prometheus-client` + `tiny_http` | ~1.5-2 MB |
| `tracing` (minimal) | ~500 KB |
| **Total recommended** | **~2-2.5 MB** |
| Full OTel with gRPC | +5-10 MB (avoid) |

### 5.3 Runtime Overhead

| Operation | Overhead |
|-----------|----------|
| Counter increment | ~5-10 ns |
| Histogram observe | ~20-50 ns |
| Gauge set | ~5-10 ns |
| HTTP metrics scrape | ~1-5 ms (every 15s) |

---

## 6. Implementation Phases

### Phase 1: Core Metrics (MVP)
- [ ] Create `src/metrics/mod.rs` with `prometheus-client`
- [ ] Add `ProxyMetrics` struct with RED metrics
- [ ] Integrate into query execution path
- [ ] Add CLI flags for `--metrics-enabled` and `--metrics-port`
- [ ] Basic Grafana dashboard

### Phase 2: Enhanced Observability
- [ ] Connection pool metrics
- [ ] Transpile cache metrics
- [ ] Query type breakdown
- [ ] Health check endpoint

### Phase 3: Optional Tracing
- [ ] Feature-gated `tracing` integration
- [ ] Stdout/file exporter (no gRPC)
- [ ] Span correlation for query execution

---

## 7. System Metrics (CPU, Memory, Disk)

For host-level observability, we can add system metrics with minimal overhead:

### 7.1 Recommended: `sysinfo` with Optimized Features

**Cargo.toml:**
```toml
[dependencies]
# System metrics - disable default features to remove rayon/multithread
sysinfo = { version = "0.30", default-features = false, optional = true }
```

**Binary impact:** ~300-500 KB (vs ~1 MB with default features)

### 7.2 Optimized System Metrics Collection

**File: `src/metrics/system.rs`**
```rust
//! System-level metrics (CPU, memory, disk) with minimal overhead

use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::registry::Registry;
use std::sync::atomic::AtomicU64;

#[cfg(feature = "system-metrics")]
use sysinfo::{RefreshKind, System, CpuRefreshKind, MemoryRefreshKind};

pub struct SystemMetrics {
    pub cpu_usage_percent: Gauge<AtomicU64>,     // 0-10000 (0.01% precision)
    pub memory_used_bytes: Gauge<AtomicU64>,
    pub memory_total_bytes: Gauge<AtomicU64>,
    pub disk_used_bytes: Gauge<AtomicU64>,
    pub disk_total_bytes: Gauge<AtomicU64>,
    
    #[cfg(feature = "system-metrics")]
    sys: System,
    #[cfg(feature = "system-metrics")]
    last_cpu_refresh: std::time::Instant,
}

impl SystemMetrics {
    pub fn new(registry: &mut Registry) -> Self {
        let cpu_usage = Gauge::default();
        registry.register("pgqt_cpu_usage_percent", "CPU usage (0-100)", cpu_usage.clone());
        
        let mem_used = Gauge::default();
        registry.register("pgqt_memory_used_bytes", "Memory used by process", mem_used.clone());
        
        let mem_total = Gauge::default();
        registry.register("pgqt_memory_total_bytes", "Total system memory", mem_total.clone());
        
        let disk_used = Gauge::default();
        registry.register("pgqt_disk_used_bytes", "Disk used by database", disk_used.clone());
        
        let disk_total = Gauge::default();
        registry.register("pgqt_disk_total_bytes", "Total disk space", disk_total.clone());
        
        #[cfg(feature = "system-metrics")]
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                .with_memory(MemoryRefreshKind::nothing().with_ram())
        );
        
        Self {
            cpu_usage_percent: cpu_usage,
            memory_used_bytes: mem_used,
            memory_total_bytes: mem_total,
            disk_used_bytes: disk_used,
            disk_total_bytes: disk_total,
            #[cfg(feature = "system-metrics")]
            sys,
            #[cfg(feature = "system-metrics")]
            last_cpu_refresh: std::time::Instant::now(),
        }
    }
    
    /// Refresh metrics - call periodically (e.g., every 10-30 seconds)
    pub fn refresh(&mut self, db_path: &str) {
        #[cfg(feature = "system-metrics")]
        {
            // CPU requires ~200ms between refreshes for accurate reading
            let now = std::time::Instant::now();
            if now.duration_since(self.last_cpu_refresh).as_millis() >= 200 {
                self.sys.refresh_cpu_usage();
                let cpu = self.sys.global_cpu_usage();
                self.cpu_usage_percent.set((cpu * 100.0) as u64); // 0-10000 for 2 decimal precision
                self.last_cpu_refresh = now;
            }
            
            // Memory is cheap to refresh
            self.sys.refresh_memory();
            self.memory_used_bytes.set(self.sys.used_memory());
            self.memory_total_bytes.set(self.sys.total_memory());
            
            // Disk usage for database file
            if let Ok(metadata) = std::fs::metadata(db_path) {
                self.disk_used_bytes.set(metadata.len());
            }
            if let Ok(info) = fs2::available_space(db_path) {
                // fs2 gives available space, calculate total
                let used = self.disk_used_bytes.get();
                self.disk_total_bytes.set(used + info);
            }
        }
    }
}
```

### 7.3 Performance Considerations

| Metric | Collection Cost | Recommended Frequency |
|--------|-----------------|----------------------|
| CPU usage | ~1ms (requires 200ms delay between calls) | Every 10-30s |
| Memory | ~0.1ms | Every 10s |
| Disk (DB file only) | ~0.01ms | Every 30s |
| Full disk scan | ~1-10ms (can hang on stale NFS) | On startup only |

**Key optimizations:**
1. **Use `RefreshKind::nothing()`** as base, add only needed metrics
2. **Avoid `System::new_all()`** - it iterates all processes
3. **Skip `multithread` feature** - removes `rayon` dependency
4. **Cache `System` instance** - don't recreate it

---

## 8. Embedded Web Configuration Interface

**Yes, we can serve a configuration page using the same `tiny_http` dependency!**

### 8.1 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    PGQT Proxy                               │
│  ┌─────────────┐  ┌──────────────────────────────────────┐  │
│  │   Metrics   │  │         Web Interface                │  │
│  │  /metrics   │  │  ┌──────────┐  ┌──────────────────┐  │  │
│  │  (Prometheus)│  │  │  Static  │  │  Config API      │  │  │
│  └─────────────┘  │  │  Assets  │  │  (read/write)    │  │  │
│                   │  │  (HTML)  │  │                  │  │  │
│                   │  └────┬─────┘  └────────┬─────────┘  │  │
│                   │       └─────────────────┘            │  │
│                   │              │                       │  │
│                   │         tiny_http (shared)           │  │
│                   └──────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 8.2 Implementation

**File: `src/web/mod.rs`**
```rust
//! Embedded web interface for PGQT configuration and monitoring

use tiny_http::{Server, Response, Request, Method, Header};
use std::sync::Arc;
use std::path::PathBuf;

pub struct WebInterface {
    config_path: PathBuf,
}

impl WebInterface {
    pub fn new(config_path: PathBuf) -> Self {
        Self { config_path }
    }
    
    pub fn start(self, port: u16) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let addr = format!("127.0.0.1:{}", port);
            let server = Server::http(&addr).expect("Failed to bind web server");
            
            println!("Web interface available at http://{}", addr);
            
            for request in server.incoming_requests() {
                let response = self.handle_request(request);
                let _ = request.respond(response);
            }
        })
    }
    
    fn handle_request(&self, mut request: Request) -> Response {
        match (request.method(), request.url()) {
            // Dashboard / Config UI
            (&Method::Get, "/") | (&Method::Get, "/config") => {
                self.serve_dashboard()
            }
            
            // Current configuration (JSON API)
            (&Method::Get, "/api/config") => {
                self.get_config_json()
            }
            
            // Update configuration
            (&Method::Post, "/api/config") => {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_ok() {
                    self.update_config(&body)
                } else {
                    Response::from_string("Bad Request").with_status_code(400)
                }
            }
            
            // Runtime stats (JSON)
            (&Method::Get, "/api/stats") => {
                self.get_stats_json()
            }
            
            // Static assets (CSS, JS)
            (&Method::Get, path) if path.starts_with("/static/") => {
                self.serve_static(path)
            }
            
            _ => Response::from_string("Not Found").with_status_code(404)
        }
    }
    
    fn serve_dashboard(&self) -> Response {
        // Embed HTML directly - no file I/O at runtime
        let html = include_str!("../../assets/config.html");
        Response::from_string(html)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap())
    }
    
    fn get_config_json(&self) -> Response {
        match std::fs::read_to_string(&self.config_path) {
            Ok(content) => Response::from_string(content)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
            Err(_) => Response::from_string("{}").with_status_code(404)
        }
    }
    
    fn update_config(&self, body: &str) -> Response {
        // Validate JSON before saving
        if serde_json::from_str::<serde_json::Value>(body).is_ok() {
            match std::fs::write(&self.config_path, body) {
                Ok(_) => Response::from_string(r#"{"status":"saved"}"#)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                Err(e) => Response::from_string(format!(r#"{{"error":"{}"}}"#, e)).with_status_code(500)
            }
        } else {
            Response::from_string(r#"{"error":"Invalid JSON"}"#).with_status_code(400)
        }
    }
    
    fn get_stats_json(&self) -> Response {
        // Return runtime statistics
        let stats = serde_json::json!({
            "uptime_seconds": 0, // TODO: track startup time
            "connections_active": 0,
            "connections_total": 0,
            "queries_total": 0,
            "version": env!("CARGO_PKG_VERSION"),
        });
        Response::from_string(stats.to_string())
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
    }
    
    fn serve_static(&self, path: &str) -> Response {
        // Map file extensions to content types
        let content_type = match path.rsplit('.').next() {
            Some("css") => "text/css",
            Some("js") => "application/javascript",
            Some("png") => "image/png",
            Some("svg") => "image/svg+xml",
            _ => "application/octet-stream",
        };
        
        // For security, only serve known embedded assets
        let content = match path {
            "/static/style.css" => Some(include_str!("../../assets/style.css")),
            "/static/app.js" => Some(include_str!("../../assets/app.js")),
            _ => None,
        };
        
        match content {
            Some(data) => Response::from_string(data)
                .with_header(Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap()),
            None => Response::from_string("Not Found").with_status_code(404)
        }
    }
}
```

### 8.3 HTML Dashboard (assets/config.html)

```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>PGQT Configuration</title>
    <link rel="stylesheet" href="/static/style.css">
</head>
<body>
    <div class="container">
        <h1>PGQT Configuration</h1>
        
        <section>
            <h2>Runtime Statistics</h2>
            <div id="stats" class="stats-grid">
                <div class="stat">
                    <label>Active Connections</label>
                    <span id="connections-active">-</span>
                </div>
                <div class="stat">
                    <label>Total Queries</label>
                    <span id="queries-total">-</span>
                </div>
                <div class="stat">
                    <label>Uptime</label>
                    <span id="uptime">-</span>
                </div>
            </div>
        </section>
        
        <section>
            <h2>Configuration</h2>
            <textarea id="config-editor" rows="20"></textarea>
            <div class="actions">
                <button onclick="loadConfig()">Reload</button>
                <button onclick="saveConfig()">Save</button>
            </div>
            <div id="status"></div>
        </section>
    </div>
    
    <script src="/static/app.js"></script>
</body>
</html>
```

### 8.4 HTTPS/TLS Support

**For secure configuration access, `tiny_http` supports TLS:**

```toml
[dependencies]
tiny_http = { version = "0.12", features = ["rustls"] }
```

**Binary size impact with TLS:**
- `tiny_http` (HTTP only): ~300-600 KB
- `tiny_http` + `rustls`: ~1.5-3 MB additional
- `tiny_http` + `native-tls`: ~1 MB additional (uses system TLS)

**Implementation:**
```rust
use tiny_http::{Server, SslConfig};

let server = Server::https("0.0.0.0:8443", SslConfig {
    certificate: include_bytes!("cert.pem").to_vec(),
    private_key: include_bytes!("key.pem").to_vec(),
}).expect("Failed to start HTTPS server");
```

### 8.5 Combined Metrics + Web Interface

**Single HTTP server serving both:**

```rust
pub struct HttpServer {
    registry: Arc<Mutex<Registry>>,
    metrics: ProxyMetrics,
    web: WebInterface,
}

impl HttpServer {
    fn handle_request(&self, mut request: Request) -> Response {
        match request.url() {
            "/metrics" => self.serve_metrics(),
            "/health" => self.serve_health(),
            "/" | "/config" => self.web.serve_dashboard(),
            path if path.starts_with("/api/") => self.web.handle_api(request),
            path if path.starts_with("/static/") => self.web.serve_static(path),
            _ => Response::from_string("Not Found").with_status_code(404)
        }
    }
}
```

---

## 9. Feature Flag Strategy

```toml
[features]
default = ["plpgsql", "tls"]

# Observability features (all optional)
metrics = ["prometheus-client", "tiny_http"]
system-metrics = ["sysinfo"]
tracing = ["dep:tracing", "dep:tracing-subscriber"]
web-config = ["metrics"]  # Reuses tiny_http from metrics

# Combined feature for full observability
observability = ["metrics", "system-metrics", "web-config"]
```

---

## 10. Summary: Binary Size Impact

| Feature Set | Additional Size | Total PGQT Estimate |
|-------------|-----------------|---------------------|
| Base (no observability) | - | ~8-10 MB |
| + `metrics` only | +1.5-2 MB | ~10-12 MB |
| + `metrics` + `system-metrics` | +2-2.5 MB | ~10-12.5 MB |
| + `metrics` + `system-metrics` + `web-config` | +2-2.5 MB | ~10-12.5 MB |
| + `metrics` + `system-metrics` + `web-config` + HTTPS | +3.5-5 MB | ~12-15 MB |
| + `tracing` | +0.5 MB | Add to any above |

**Key insight:** The web config interface adds **zero additional dependencies** when combined with metrics (reuses `tiny_http`).

---

## 11. Alternatives Considered

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **prometheus-client + tiny_http** | Fastest, smallest, type-safe | Slightly more boilerplate | ✅ **Recommended** |
| `metrics` crate + exporter | Ergonomic macros, flexible | Larger binary, more deps | ❌ Rejected (size) |
| OpenTelemetry OTLP | Full ecosystem, standardized | +5-10 MB binary, gRPC overhead | ❌ Rejected (size) |
| `tokio-metrics` | Tokio-native, detailed | Only runtime metrics, not app-level | ⚠️ Complement only |
| Separate web framework (axum/rocket) | Rich ecosystem | +5-15 MB, async runtime | ❌ Rejected (size) |

---

## 12. References

- [prometheus-client docs](https://docs.rs/prometheus-client)
- [tiny_http docs](https://docs.rs/tiny_http)
- [RED Method for Microservices](https://grafana.com/files/grafanacon_eu_2018/Tom_Wilkie_GrafanaCon_EU_2018.pdf)
- [Tokio Runtime Metrics](https://docs.rs/tokio-metrics)
- [Rust Observability at Scale](https://itnext.io/the-rust-renaissance-in-observability-lessons-from-building-at-scale-cf12cbb96ebf)
