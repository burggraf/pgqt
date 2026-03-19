# PGQT Observability Implementation Plan

**Status:** Draft
**Created:** 2025-03-19
**Based on:** `docs/observability-research.md`

---

## Overview

This plan outlines the implementation of a lightweight, hybrid observability stack for PGQT using `prometheus-client` and `tiny_http`. The approach prioritizes minimal binary size impact (~2-2.5 MB) and low runtime overhead (<0.1% on hot path).

### Goals

1. **Prometheus-compatible metrics endpoint** for production monitoring
2. **Type-safe, zero-allocation metrics** using `prometheus-client`
3. **Optional system metrics** (CPU, memory, disk) via `sysinfo`
4. **Embedded web configuration UI** reusing the same HTTP server
5. **Feature-gated compilation** for minimal impact when disabled

### Non-Goals

- Full OpenTelemetry OTLP/gRPC export (too heavy for proxy)
- Distributed tracing by default (optional feature)
- Complex web framework (keep it minimal with `tiny_http`)

---

## Phase 1: Core Metrics Infrastructure

**Duration:** 1-2 days
**Binary Impact:** +1.5-2 MB
**Dependencies:** `prometheus-client`, `tiny_http`

### 1.1 Add Dependencies

**File:** `Cargo.toml`

```toml
[dependencies]
prometheus-client = { version = "0.22", optional = true }
tiny_http = { version = "0.12", optional = true }

[features]
default = ["plpgsql", "tls"]
metrics = ["dep:prometheus-client", "dep:tiny_http"]
```

**Tasks:**
- [ ] Add `prometheus-client` dependency (optional)
- [ ] Add `tiny_http` dependency (optional)
- [ ] Add `metrics` feature flag
- [ ] Run `cargo check --features metrics` to verify

### 1.2 Create Metrics Module

**File:** `src/metrics/mod.rs`

```rust
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
```

**Tasks:**
- [ ] Create `src/metrics/mod.rs` with feature-gated exports
- [ ] Create `src/metrics/proxy.rs` with full implementation
- [ ] Add `QueryType` enum for query classification
- [ ] Implement no-op stubs for `#[cfg(not(feature = "metrics"))]`

### 1.3 Implement ProxyMetrics

**File:** `src/metrics/proxy.rs`

Implement the following metrics:

| Metric Name | Type | Description |
|-------------|------|-------------|
| `pgqt_requests_total` | Counter | Total requests processed |
| `pgqt_requests_failed_total` | Counter | Total failed requests |
| `pgqt_query_duration_seconds` | Histogram | Query execution latency |
| `pgqt_connections_active` | Gauge | Currently active connections |
| `pgqt_connections_total` | Counter | Total connections accepted |
| `pgqt_queries_select_total` | Counter | SELECT queries |
| `pgqt_queries_insert_total` | Counter | INSERT queries |
| `pgqt_queries_update_total` | Counter | UPDATE queries |
| `pgqt_queries_delete_total` | Counter | DELETE queries |
| `pgqt_queries_ddl_total` | Counter | DDL queries |
| `pgqt_queries_other_total` | Counter | Other queries |

**Tasks:**
- [ ] Define `ProxyMetrics` struct with all counters/gauges/histograms
- [ ] Implement `ProxyMetrics::new()` with registry registration
- [ ] Implement `record_query()` helper method
- [ ] Implement `inc_connections()` / `dec_connections()` helpers
- [ ] Write unit tests for metrics increment/observe

### 1.4 Implement MetricsServer

**File:** `src/metrics/server.rs`

```rust
use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use std::sync::{Arc, Mutex};
use tiny_http::Server;

pub struct MetricsServer {
    registry: Arc<Mutex<Registry>>,
    metrics: ProxyMetrics,
}

impl MetricsServer {
    pub fn new() -> Self { /* ... */ }
    pub fn metrics(&self) -> &ProxyMetrics { /* ... */ }
    pub fn start(self, port: u16) -> std::thread::JoinHandle<()> { /* ... */ }
}
```

**Tasks:**
- [ ] Create `MetricsServer` struct with registry and metrics
- [ ] Implement HTTP server with `tiny_http`
- [ ] Add `/metrics` endpoint (Prometheus text format)
- [ ] Add `/health` endpoint (JSON health check)
- [ ] Handle graceful shutdown on server drop
- [ ] Write integration test for `/metrics` endpoint

### 1.5 Global Metrics Instance

**File:** `src/metrics/global.rs`

```rust
use std::sync::OnceLock;
use crate::metrics::ProxyMetrics;

static GLOBAL_METRICS: OnceLock<ProxyMetrics> = OnceLock::new();

pub fn init_global_metrics(metrics: ProxyMetrics) -> Result<(), ProxyMetrics> {
    GLOBAL_METRICS.set(metrics)
}

pub fn get_metrics() -> Option<&'static ProxyMetrics> {
    GLOBAL_METRICS.get()
}
```

**Tasks:**
- [ ] Implement `OnceLock`-based global metrics storage
- [ ] Add `init_global_metrics()` function
- [ ] Add `get_metrics()` function
- [ ] Document thread-safety guarantees

### 1.6 CLI Integration

**File:** `src/main.rs`

```rust
#[derive(Parser)]
struct Cli {
    // ... existing args ...
    
    /// Enable Prometheus metrics endpoint
    #[arg(long, env = "PGQT_METRICS_ENABLED")]
    metrics_enabled: bool,
    
    /// Port for metrics HTTP server
    #[arg(long, env = "PGQT_METRICS_PORT", default_value = "9090")]
    metrics_port: u16,
}
```

**Tasks:**
- [ ] Add `--metrics-enabled` CLI flag
- [ ] Add `--metrics-port` CLI flag (default: 9090)
- [ ] Add `PGQT_METRICS_ENABLED` and `PGQT_METRICS_PORT` env vars
- [ ] Initialize `MetricsServer` in main when `--metrics-enabled`
- [ ] Store global metrics reference for handler access

---

## Phase 2: Handler Integration

**Duration:** 1 day
**Prerequisite:** Phase 1 complete

### 2.1 Instrument Query Execution

**File:** `src/handler/mod.rs` (or `src/handler/query.rs` if extracted)

```rust
use crate::metrics::{get_metrics, QueryType};
use std::time::Instant;

impl SqliteHandler {
    pub fn execute_query_with_metrics(&self, client_id: u32, query: &str) -> Result<Vec<Response>> {
        let start = Instant::now();
        let query_type = classify_query(query);
        
        if let Some(m) = get_metrics() {
            m.inc_connections();
        }
        
        let result = self.execute_query_internal(client_id, query);
        
        let duration = start.elapsed().as_secs_f64();
        let success = result.is_ok();
        
        if let Some(m) = get_metrics() {
            m.record_query(query_type, duration, success);
            m.dec_connections();
        }
        
        result
    }
}

fn classify_query(query: &str) -> QueryType {
    let upper = query.trim().to_uppercase();
    if upper.starts_with("SELECT") { QueryType::Select }
    else if upper.starts_with("INSERT") { QueryType::Insert }
    else if upper.starts_with("UPDATE") { QueryType::Update }
    else if upper.starts_with("DELETE") { QueryType::Delete }
    else if upper.starts_with("CREATE") || upper.starts_with("ALTER") || upper.starts_with("DROP") { QueryType::Ddl }
    else { QueryType::Other }
}
```

**Tasks:**
- [ ] Add `std::time::Instant` timing around query execution
- [ ] Implement `classify_query()` helper
- [ ] Wrap existing query execution with metrics calls
- [ ] Handle errors gracefully (still record metrics on failure)
- [ ] Add connection active gauge tracking

### 2.2 Instrument Connection Handling

**File:** `src/handler/mod.rs`

```rust
impl SqliteHandler {
    pub async fn handle_connection(&mut self) -> Result<()> {
        if let Some(m) = get_metrics() {
            m.connections_total.inc();
        }
        // ... existing connection handling ...
    }
}
```

**Tasks:**
- [ ] Increment `connections_total` on new connection
- [ ] Track `connections_active` gauge throughout connection lifecycle
- [ ] Ensure metrics are recorded even on connection errors

### 2.3 Instrument Transpile Cache

**File:** `src/transpiler/mod.rs`

```rust
pub fn transpile_with_cache(sql: &str, cache: &mut Cache) -> Result<String> {
    if let Some(cached) = cache.get(sql) {
        if let Some(m) = get_metrics() {
            m.transpile_cache_hits.inc();
        }
        return Ok(cached);
    }
    
    if let Some(m) = get_metrics() {
        m.transpile_cache_misses.inc();
    }
    
    let result = transpile(sql);
    cache.insert(sql, result.clone());
    result
}
```

**Tasks:**
- [ ] Add cache hit/miss counters to `ProxyMetrics`
- [ ] Wrap cache lookup with metrics recording
- [ ] Test cache metrics increment correctly

---

## Phase 3: System Metrics (Optional)

**Duration:** 0.5 days
**Binary Impact:** +300-500 KB
**Prerequisite:** Phase 1 complete
**Feature Flag:** `system-metrics`

### 3.1 Add sysinfo Dependency

**File:** `Cargo.toml`

```toml
[dependencies]
sysinfo = { version = "0.30", default-features = false, optional = true }

[features]
system-metrics = ["metrics", "dep:sysinfo"]
```

**Tasks:**
- [ ] Add `sysinfo` with `default-features = false` (removes rayon)
- [ ] Add `system-metrics` feature (implies `metrics`)
- [ ] Verify binary size impact with `cargo bloat`

### 3.2 Implement SystemMetrics

**File:** `src/metrics/system.rs`

```rust
#[cfg(feature = "system-metrics")]
use sysinfo::{RefreshKind, System, CpuRefreshKind};

pub struct SystemMetrics {
    pub cpu_usage_percent: Gauge<AtomicU64>,
    pub memory_used_bytes: Gauge<AtomicU64>,
    pub memory_total_bytes: Gauge<AtomicU64>,
    
    #[cfg(feature = "system-metrics")]
    sys: System,
    #[cfg(feature = "system-metrics")]
    last_cpu_refresh: Instant,
}

impl SystemMetrics {
    pub fn new(registry: &mut Registry) -> Self { /* ... */ }
    pub fn refresh(&mut self, db_path: &str) { /* ... */ }
}
```

**Tasks:**
- [ ] Create `SystemMetrics` struct with CPU/memory gauges
- [ ] Implement `refresh()` method with optimized `RefreshKind`
- [ ] Add periodic refresh task (every 10-30 seconds)
- [ ] Add disk usage metrics for database file
- [ ] Feature-gate all sysinfo code

### 3.3 Periodic Refresh Task

**File:** `src/main.rs`

```rust
#[cfg(feature = "system-metrics")]
fn spawn_system_metrics_refresh(metrics: Arc<Mutex<SystemMetrics>>, db_path: String) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(15));
            if let Ok(mut m) = metrics.lock() {
                m.refresh(&db_path);
            }
        }
    });
}
```

**Tasks:**
- [ ] Spawn background thread for system metrics collection
- [ ] Use 15-second refresh interval
- [ ] Handle graceful shutdown

---

## Phase 4: Embedded Web Configuration UI

**Duration:** 1 day
**Binary Impact:** ~0 KB additional (reuses tiny_http)
**Prerequisite:** Phase 1 complete
**Feature Flag:** `web-config`

### 4.1 Feature Flag Setup

**File:** `Cargo.toml`

```toml
[features]
web-config = ["metrics"]  # Reuses tiny_http from metrics
```

**Tasks:**
- [ ] Add `web-config` feature (implies `metrics`)
- [ ] No new dependencies needed

### 4.2 Create Web Module

**File:** `src/web/mod.rs`

```rust
#[cfg(feature = "web-config")]
pub struct WebInterface {
    config_path: PathBuf,
}

#[cfg(feature = "web-config")]
impl WebInterface {
    pub fn new(config_path: PathBuf) -> Self { /* ... */ }
    fn serve_dashboard(&self) -> Response { /* ... */ }
    fn get_config_json(&self) -> Response { /* ... */ }
    fn update_config(&self, body: &str) -> Response { /* ... */ }
    fn get_stats_json(&self) -> Response { /* ... */ }
    fn serve_static(&self, path: &str) -> Response { /* ... */ }
}
```

**Tasks:**
- [ ] Create `src/web/mod.rs` with feature gate
- [ ] Implement routing for `/`, `/config`, `/api/*`
- [ ] Create embedded HTML dashboard
- [ ] Create embedded CSS/JS assets

### 4.3 Create Dashboard Assets

**File:** `assets/config.html`

```html
<!DOCTYPE html>
<html>
<head>
    <title>PGQT Configuration</title>
    <style>/* embedded styles */</style>
</head>
<body>
    <h1>PGQT Configuration</h1>
    <section id="stats">...</section>
    <section id="config">...</section>
    <script>/* embedded JS */</script>
</body>
</html>
```

**Tasks:**
- [ ] Create `assets/config.html` with embedded dashboard
- [ ] Use `include_str!` for zero runtime file I/O
- [ ] Add `/api/config` GET endpoint
- [ ] Add `/api/config` POST endpoint
- [ ] Add `/api/stats` GET endpoint

### 4.4 Combined HTTP Server

**File:** `src/http_server.rs` (new file)

```rust
pub struct HttpServer {
    #[cfg(feature = "metrics")]
    registry: Arc<Mutex<Registry>>,
    #[cfg(feature = "metrics")]
    metrics: ProxyMetrics,
    #[cfg(feature = "web-config")]
    web: WebInterface,
}

impl HttpServer {
    fn handle_request(&self, request: Request) -> Response {
        match request.url() {
            #[cfg(feature = "metrics")]
            "/metrics" => self.serve_metrics(),
            #[cfg(feature = "metrics")]
            "/health" => self.serve_health(),
            #[cfg(feature = "web-config")]
            "/" | "/config" => self.web.serve_dashboard(),
            #[cfg(feature = "web-config")]
            path if path.starts_with("/api/") => self.web.handle_api(request),
            _ => Response::from_string("Not Found").with_status_code(404)
        }
    }
}
```

**Tasks:**
- [ ] Create unified `HttpServer` combining metrics and web
- [ ] Feature-gate each endpoint
- [ ] Handle routing in single `tiny_http` server
- [ ] Add CLI flags: `--web-enabled`, `--web_port`

---

## Phase 5: Testing & Documentation

**Duration:** 1 day
**Prerequisite:** Phases 1-4 complete

### 5.1 Unit Tests

**Tasks:**
- [ ] Test `ProxyMetrics::new()` creates all metrics
- [ ] Test `record_query()` increments correct counters
- [ ] Test `classify_query()` for all query types
- [ ] Test histogram observation
- [ ] Test gauge increment/decrement

### 5.2 Integration Tests

**File:** `tests/metrics_tests.rs`

```rust
#[test]
#[cfg(feature = "metrics")]
fn test_metrics_endpoint() {
    let server = MetricsServer::new();
    server.start(19090);
    
    let response = reqwest::blocking::get("http://127.0.0.1:19090/metrics").unwrap();
    assert!(response.status().is_success());
    assert!(response.text().unwrap().contains("pgqt_requests_total"));
}
```

**Tasks:**
- [ ] Add test for `/metrics` endpoint availability
- [ ] Add test for `/health` endpoint
- [ ] Add test for metrics format (Prometheus text)
- [ ] Add test for concurrent metrics recording

### 5.3 E2E Tests

**File:** `tests/metrics_e2e_test.py`

```python
def test_query_metrics_recorded():
    """Verify query execution updates metrics."""
    start_proxy_with_metrics()
    conn = psycopg2.connect(...)
    cur = conn.cursor()
    
    # Execute queries
    cur.execute("SELECT 1")
    cur.execute("INSERT INTO test VALUES (1)")
    
    # Check metrics
    metrics = requests.get("http://127.0.0.1:9090/metrics").text
    assert "pgqt_queries_select_total 1" in metrics
    assert "pgqt_queries_insert_total 1" in metrics
```

**Tasks:**
- [ ] Create `tests/metrics_e2e_test.py`
- [ ] Test query type classification
- [ ] Test error counter increments
- [ ] Test latency histogram

### 5.4 Documentation

**Tasks:**
- [ ] Update `README.md` with metrics usage
- [ ] Document all metrics and their meanings
- [ ] Add Grafana dashboard JSON example
- [ ] Add Prometheus scrape config example
- [ ] Document feature flags and binary size impacts

---

## Phase 6: Grafana Dashboard

**Duration:** 0.5 days
**Prerequisite:** Phase 5 complete

### 6.1 Dashboard JSON

**File:** `examples/grafana-dashboard.json`

Create a Grafana dashboard with:

- **Request Rate** panel: `rate(pgqt_requests_total[5m])`
- **Error Rate** panel: `rate(pgqt_requests_failed_total[5m])`
- **P50/P95/P99 Latency** panels: `histogram_quantile(0.50, rate(pgqt_query_duration_seconds_bucket[5m]))`
- **Active Connections** panel: `pgqt_connections_active`
- **Query Mix** panel: pie chart of query types
- **Cache Hit Rate** panel: `pgqt_transpile_cache_hits_total / (pgqt_transpile_cache_hits_total + pgqt_transpile_cache_misses_total)`

**Tasks:**
- [ ] Create `examples/grafana-dashboard.json`
- [ ] Add screenshot to documentation
- [ ] Test dashboard with live PGQT instance

### 6.2 Prometheus Config

**File:** `examples/prometheus.yml`

```yaml
scrape_configs:
  - job_name: 'pgqt'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: /metrics
    scrape_interval: 15s
```

**Tasks:**
- [ ] Create `examples/prometheus.yml`
- [ ] Create `examples/docker-compose.yml` with PGQT + Prometheus + Grafana

---

## Implementation Order

### Recommended Sequence

```
Week 1:
├── Day 1-2: Phase 1 (Core Metrics Infrastructure)
│   ├── Cargo.toml dependencies
│   ├── src/metrics/mod.rs + proxy.rs
│   ├── src/metrics/server.rs
│   ├── src/metrics/global.rs
│   └── CLI integration
│
├── Day 3: Phase 2 (Handler Integration)
│   ├── Query timing
│   ├── Query classification
│   ├── Connection tracking
│   └── Cache metrics
│
└── Day 4: Phase 5.1-5.2 (Unit & Integration Tests)
    ├── Unit tests
    └── Integration tests

Week 2:
├── Day 5: Phase 3 (System Metrics)
│   ├── sysinfo integration
│   ├── Periodic refresh
│   └── Tests
│
├── Day 6-7: Phase 4 (Web Configuration UI)
│   ├── Web module
│   ├── Dashboard HTML/CSS/JS
│   ├── API endpoints
│   └── Tests
│
├── Day 8: Phase 5.3-5.4 (E2E Tests & Docs)
│   ├── Python E2E tests
│   └── Documentation
│
└── Day 9: Phase 6 (Grafana Dashboard)
    ├── Dashboard JSON
    ├── Prometheus config
    └── Docker Compose example
```

---

## File Structure

```
src/
├── main.rs                 # CLI flags, initialization
├── lib.rs                  # Exports
├── handler/
│   └── mod.rs              # Instrumented query execution
├── transpiler/
│   └── mod.rs              # Cache metrics
├── metrics/
│   ├── mod.rs              # Feature-gated exports
│   ├── proxy.rs            # ProxyMetrics implementation
│   ├── server.rs            # MetricsServer + tiny_http
│   ├── global.rs            # Global metrics instance
│   └── system.rs            # SystemMetrics (feature-gated)
└── web/
    └── mod.rs               # WebInterface (feature-gated)

assets/
├── config.html              # Embedded dashboard HTML
├── style.css                # Embedded CSS
└── app.js                   # Embedded JS

examples/
├── grafana-dashboard.json   # Ready-to-import dashboard
├── prometheus.yml           # Scrape config
└── docker-compose.yml       # Full stack example

tests/
├── metrics_tests.rs         # Rust integration tests
└── metrics_e2e_test.py      # Python E2E test
```

---

## Feature Flags Summary

| Flag | Dependencies | Binary Impact | Description |
|------|--------------|---------------|-------------|
| `metrics` | `prometheus-client`, `tiny_http` | +1.5-2 MB | Core Prometheus metrics |
| `system-metrics` | `sysinfo` (implies `metrics`) | +0.3-0.5 MB | CPU/memory/disk metrics |
| `web-config` | (implies `metrics`) | +0 MB | Embedded web UI |
| `tracing` | `tracing`, `tracing-subscriber` | +0.5 MB | Distributed tracing (stdout) |
| `observability` | All above | +2-2.5 MB | Combined feature |

---

## Success Criteria

### Functional

- [ ] `/metrics` endpoint returns Prometheus-compatible metrics
- [ ] All query types are correctly classified and counted
- [ ] Query latency histogram shows P50/P95/P99 correctly
- [ ] Connection active gauge reflects actual connections
- [ ] System metrics refresh without blocking the proxy
- [ ] Web configuration UI can read/write config
- [ ] Health endpoint returns valid JSON

### Performance

- [ ] Metrics overhead < 0.1% of request latency
- [ ] Counter increment < 10ns
- [ ] Histogram observe < 50ns
- [ ] HTTP metrics scrape < 5ms
- [ ] System metrics refresh < 10ms

### Binary Size

- [ ] Base build (no features) unchanged
- [ ] `metrics` feature adds < 2 MB
- [ ] `system-metrics` adds < 0.5 MB
- [ ] `web-config` adds < 50 KB
- [ ] Full `observability` feature adds < 2.5 MB

### Testing

- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] E2E test passes with real PostgreSQL client
- [ ] Grafana dashboard imports and displays correctly
- [ ] Prometheus scrapes metrics successfully

---

## Completion Checklist

**This section defines the mandatory verification steps before the implementation can be considered complete.**

### Build Verification

- [ ] **Clean release build passes:**
  ```bash
  cargo build --release
  ```
  - Must compile without errors
  - Must complete successfully for default features (`plpgsql`, `tls`)

- [ ] **Feature-gated builds pass:**
  ```bash
  # Minimal build (no observability)
  cargo build --release --no-default-features --features plpgsql

  # With metrics only
  cargo build --release --features plpgsql,tls,metrics

  # Full observability
  cargo build --release --features plpgsql,tls,observability
  ```
  - All feature combinations must compile cleanly

- [ ] **No build warnings:**
  ```bash
  cargo build --release 2>&1 | grep -i warning
  ```
  - Must return empty output
  - If warnings exist, fix them before proceeding

- [ ] **Clippy passes:**
  ```bash
  cargo clippy --all-features -- -D warnings
  ```
  - Must exit with code 0
  - All warnings treated as errors

### Test Verification

- [ ] **Full test suite passes:**
  ```bash
  ./run_tests.sh
  ```
  - All unit tests pass
  - All integration tests pass
  - All E2E tests pass
  - Must show "All tests passed" (or equivalent) at the end

- [ ] **Metrics-specific tests pass:**
  ```bash
  # With metrics feature enabled
  cargo test --features metrics

  # E2E metrics test
  python3 tests/metrics_e2e_test.py
  ```
  - All metrics unit/integration tests pass
  - E2E test verifies query counting and latency recording

- [ ] **No regressions in existing tests:**
  ```bash
  cargo test
  ./run_tests.sh --no-e2e  # Quick sanity check
  ```
  - All pre-existing tests still pass
  - No new test failures introduced

### Documentation Requirements

- [ ] **README.md updated:**
  - Add "Observability" section with:
    - Overview of metrics capabilities
    - CLI flags (`--metrics-enabled`, `--metrics-port`, etc.)
    - Environment variables (`PGQT_METRICS_ENABLED`, `PGQT_METRICS_PORT`)
    - Quick start example
  - Add link to detailed documentation

- [ ] **Feature flags documented:**
  - Add section in README.md or separate `docs/features.md`:
    ```
    ## Feature Flags

    | Flag | Description | Binary Impact |
    |------|-------------|---------------|
    | `metrics` | Prometheus metrics endpoint | +1.5-2 MB |
    | `system-metrics` | CPU/memory/disk metrics | +0.3-0.5 MB |
    | `web-config` | Embedded web UI | ~0 KB |
    | `observability` | All above combined | +2-2.5 MB |

    ### Building with Observability

    ```bash
    # Minimal (no observability)
    cargo build --release --no-default-features --features plpgsql

    # With Prometheus metrics
    cargo build --release --features plpgsql,tls,metrics

    # Full observability stack
    cargo build --release --features plpgsql,tls,observability
    ```
    ```

- [ ] **Metrics reference documented:**
  - Create `docs/metrics.md` with:
    - Complete list of all metrics and their meanings
    - Metric types (counter, gauge, histogram)
    - Labels (if any)
    - Example PromQL queries

- [ ] **Grafana dashboard provided:**
  - `examples/grafana-dashboard.json` - ready to import
  - `examples/prometheus.yml` - scrape configuration
  - `examples/docker-compose.yml` - full stack example

- [ ] **CLI help text updated:**
  ```bash
  pgqt --help
  ```
  - Must show `--metrics-enabled` and `--metrics-port` flags
  - Must show `--web-enabled` and `--web-port` flags (if implemented)

### Release Process

Once all above criteria are met:

- [ ] **Bump version number:**
  ```bash
  # In Cargo.toml
  version = "0.X.Y" -> "0.X.(Y+1)"  # Patch release
  # or
  version = "0.X.Y" -> "0.(X+1).0"  # Minor release for new feature
  ```
  - Follow semantic versioning
  - Update version in any other relevant files (README, etc.)

- [ ] **Update CHANGELOG.md:**
  ```markdown
  ## [0.X.Y] - YYYY-MM-DD

  ### Added
  - Prometheus metrics endpoint (`/metrics`)
  - System metrics (CPU, memory, disk) via `--metrics-system` flag
  - Embedded web configuration UI
  - Feature flags: `metrics`, `system-metrics`, `web-config`, `observability`

  ### Changed
  - CLI now supports `--metrics-enabled` and `--metrics-port` flags
  - Environment variables `PGQT_METRICS_ENABLED`, `PGQT_METRICS_PORT`
  ```

- [ ] **Commit changes:**
  ```bash
  git add -A
  git commit -m "feat: add observability stack with Prometheus metrics

  - Add prometheus-client + tiny_http for metrics endpoint
  - Add /metrics and /health endpoints
  - Add system metrics (CPU, memory, disk) via sysinfo
  - Add embedded web configuration UI
  - Feature flags: metrics, system-metrics, web-config, observability
  - Binary impact: +2-2.5 MB with full observability

  Closes #XX"
  ```

- [ ] **Push to remote:**
  ```bash
  git push origin main
  # or
  git push origin feature/observability
  ```

- [ ] **Create release (if applicable):**
  - Tag the release: `git tag v0.X.Y`
  - Push tag: `git push origin v0.X.Y`
  - Create GitHub release with release notes

### Summary: Must Pass Before Completion

| Step | Command | Expected Result |
|------|---------|-----------------|
| Clean build | `cargo build --release` | ✅ No errors, no warnings |
| Clippy | `cargo clippy --all-features -- -D warnings` | ✅ Exit 0 |
| Tests | `./run_tests.sh` | ✅ All tests pass |
| Metrics tests | `cargo test --features metrics` | ✅ All pass |
| Documentation | Review README.md, docs/ | ✅ Complete and accurate |
| Version bump | Check Cargo.toml | ✅ Version incremented |
| Commit | `git status` | ✅ All changes committed |
| Push | `git push` | ✅ Pushed to remote |

**Only after all items in this checklist are complete can the implementation be considered done.**

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| `tiny_http` not async-compatible | Medium | Run in separate thread, don't block tokio runtime |
| Histogram bucket misconfiguration | Low | Use standard buckets: 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s |
| High-cardinality labels | High | Enforce compile-time labels, no runtime label values |
| sysinfo CPU refresh blocking | Medium | Cache CPU reading, refresh only every 200ms+ |
| Web UI security | Medium | Bind to localhost only, add optional auth in future |
| Build fails on feature combinations | High | Test all feature combinations in CI |
| Test flakiness in E2E | Medium | Use stable ports, proper cleanup, retry logic |
| Documentation out of sync | Medium | Update docs as part of each phase, not at end |
| Version bump forgotten | Low | Add to completion checklist |

---

## Open Questions

1. **Config file format for web UI?** JSON is simple, but should we support TOML/YAML?
2. **Auth for web UI?** Defer to future, or add basic auth now?
3. **Metrics retention in memory?** Prometheus scrapes, we don't need retention
4. **Custom metrics for users?** Allow user-defined metrics via config? (Out of scope)
5. **TLS for metrics endpoint?** Add `tiny_http` TLS feature? (Future consideration)

---

## References

- [prometheus-client documentation](https://docs.rs/prometheus-client)
- [tiny_http documentation](https://docs.rs/tiny_http)
- [RED Method](https://grafana.com/files/grafanacon_eu_2018/Tom_Wilkie_GrafanaCon_EU_2018.pdf)
- [sysinfo documentation](https://docs.rs/sysinfo)