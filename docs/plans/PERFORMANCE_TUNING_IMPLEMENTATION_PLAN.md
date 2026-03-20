# PGQT Performance Tuning Implementation Plan

## Overview

This plan implements comprehensive performance tuning and configuration features to match pgsqlite's configurability. The implementation is divided into 4 phases, each with explicit completion criteria.

**Completion Criteria for Each Phase:**
1. Run `cargo check` to ensure code compiles
2. Fix any build errors
3. Run `./run_tests.sh` and ensure all tests pass
4. Create/update relevant documentation
5. A phase is NOT complete until all 4 steps are finished

---

## Phase 1: Essential Performance Tuning (SQLite PRAGMA & Cache Configuration)

**Goal:** Add CLI configuration for SQLite PRAGMA settings and expand cache configurability.

### 1.1 Add SQLite PRAGMA Configuration Options

**Files to modify:**
- `src/main.rs` - Add CLI arguments
- `src/config.rs` - Add configuration struct fields
- `src/connection_pool.rs` - Apply PRAGMA settings on connection creation

**Implementation:**

Add the following CLI arguments to `src/main.rs`:

```rust
/// SQLite journal mode (DELETE, TRUNCATE, PERSIST, MEMORY, WAL, OFF)
#[arg(long, env = "PGQT_JOURNAL_MODE", default_value = "WAL")]
journal_mode: String,

/// SQLite synchronous mode (OFF, NORMAL, FULL, EXTRA)
#[arg(long, env = "PGQT_SYNCHRONOUS", default_value = "NORMAL")]
synchronous: String,

/// SQLite page cache size in KB (negative) or pages (positive)
#[arg(long, env = "PGQT_CACHE_SIZE", default_value = "-64000")]
cache_size: i32,

/// SQLite memory-mapped I/O size in bytes
#[arg(long, env = "PGQT_MMAP_SIZE", default_value = "268435456")]
mmap_size: u64,

/// SQLite temp store mode (DEFAULT, FILE, MEMORY)
#[arg(long, env = "PGQT_TEMP_STORE", default_value = "MEMORY")]
temp_store: String,
```

**In `src/connection_pool.rs`**, modify `create_connection()` to apply these settings:

```rust
fn create_connection(&self) -> Result<Connection> {
    let conn = Connection::open(&self.db_path)?;
    
    // Apply PRAGMA settings from config
    conn.execute_batch(&format!(
        "PRAGMA journal_mode = {};
         PRAGMA synchronous = {};
         PRAGMA cache_size = {};
         PRAGMA mmap_size = {};
         PRAGMA temp_store = {};",
        self.journal_mode,
        self.synchronous,
        self.cache_size,
        self.mmap_size,
        self.temp_store
    ))?;
    
    // ... rest of initialization
}
```

### 1.2 Expand Transpile Cache Configuration

**Files to modify:**
- `src/main.rs` - Add CLI arguments
- `src/cache/mod.rs` - Make cache size configurable
- `src/handler/mod.rs` - Pass config to handler

**Implementation:**

Add CLI arguments:

```rust
/// Transpile cache size (number of cached queries)
#[arg(long, env = "PGQT_TRANSPILE_CACHE_SIZE", default_value = "256")]
transpile_cache_size: usize,

/// Transpile cache TTL in seconds (0 = no expiration)
#[arg(long, env = "PGQT_TRANSPILE_CACHE_TTL", default_value = "0")]
transpile_cache_ttl: u64,
```

Modify `TranspileCache` to support TTL:

```rust
pub struct TranspileCache {
    cache: Mutex<LruCache<String, CacheEntry>>,
    ttl: Duration,
}

struct CacheEntry {
    result: TranspileResult,
    created_at: Instant,
}
```

### 1.3 Add Query Result Caching

**Files to create/modify:**
- `src/cache/query_result.rs` - New file for query result cache
- `src/cache/mod.rs` - Export new module
- `src/main.rs` - Add CLI arguments

**Implementation:**

Create `src/cache/query_result.rs`:

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct QueryResultCache {
    cache: Mutex<HashMap<String, CachedResult>>,
    max_size: usize,
    ttl: Duration,
}

struct CachedResult {
    rows: Vec<Vec<serde_json::Value>>,
    columns: Vec<String>,
    created_at: Instant,
}
```

Add CLI arguments:

```rust
/// Enable query result caching
#[arg(long, env = "PGQT_ENABLE_RESULT_CACHE")]
enable_result_cache: bool,

/// Query result cache size (number of cached results)
#[arg(long, env = "PGQT_RESULT_CACHE_SIZE", default_value = "100")]
result_cache_size: usize,

/// Query result cache TTL in seconds
#[arg(long, env = "PGQT_RESULT_CACHE_TTL", default_value = "60")]
result_cache_ttl: u64,
```

### Phase 1 Completion Checklist

- [ ] `cargo check` passes with no errors
- [ ] All build warnings addressed
- [ ] `./run_tests.sh` passes (all unit, integration, and E2E tests)
- [ ] Documentation created in `docs/performance-tuning.md`

---

## Phase 2: Connection Pooling Configuration

**Goal:** Make connection pooling configurable with read/write separation support.

### 2.1 Add Connection Pool Configuration Options

**Files to modify:**
- `src/main.rs` - Add CLI arguments
- `src/config.rs` - Add pool configuration struct
- `src/connection_pool.rs` - Expand pool implementation

**Implementation:**

Add CLI arguments:

```rust
/// Maximum number of concurrent connections
#[arg(long, env = "PGQT_MAX_CONNECTIONS", default_value = "100")]
max_connections: usize,

/// Connection pool size (number of connections to maintain)
#[arg(long, env = "PGQT_POOL_SIZE", default_value = "8")]
pool_size: usize,

/// Connection timeout in seconds
#[arg(long, env = "PGQT_CONNECTION_TIMEOUT", default_value = "30")]
connection_timeout: u64,

/// Idle connection timeout in seconds
#[arg(long, env = "PGQT_IDLE_TIMEOUT", default_value = "300")]
idle_timeout: u64,

/// Health check interval in seconds
#[arg(long, env = "PGQT_HEALTH_CHECK_INTERVAL", default_value = "60")]
health_check_interval: u64,

/// Maximum connection retries
#[arg(long, env = "PGQT_MAX_RETRIES", default_value = "3")]
max_retries: usize,

/// Enable read/write connection separation
#[arg(long, env = "PGQT_USE_POOLING")]
use_pooling: bool,
```

### 2.2 Implement Pool Timeout and Retry Logic

**In `src/connection_pool.rs`:**

```rust
pub struct ConnectionPool {
    // ... existing fields
    connection_timeout: Duration,
    idle_timeout: Duration,
    health_check_interval: Duration,
    max_retries: usize,
}

impl ConnectionPool {
    pub fn checkout_with_timeout(&self, client_id: u32) -> Result<(Arc<Mutex<Connection>>, ConnectionHandle)> {
        let start = Instant::now();
        let mut retries = 0;
        
        while start.elapsed() < self.connection_timeout {
            match self.try_checkout(client_id) {
                Ok(conn) => return Ok(conn),
                Err(e) if retries < self.max_retries => {
                    retries += 1;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(e),
            }
        }
        
        Err(anyhow!("Connection timeout"))
    }
}
```

### 2.3 Add Connection Health Checks

```rust
impl ConnectionPool {
    pub fn start_health_checker(&self) {
        let pool = self.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(pool.health_check_interval);
                pool.check_connections_health();
            }
        });
    }
    
    fn check_connections_health(&self) {
        // Validate and replace unhealthy connections
    }
}
```

### Phase 2 Completion Checklist

- [ ] `cargo check` passes with no errors
- [ ] All build warnings addressed
- [ ] `./run_tests.sh` passes (all tests)
- [ ] Documentation updated in `docs/performance-tuning.md`
- [ ] Connection pool tests added to `src/connection_pool.rs`

---

## Phase 3: Memory Management & Buffer Pool

**Goal:** Implement buffer pooling and memory management features.

### 3.1 Create Buffer Pool Module

**Files to create:**
- `src/buffer/pool.rs` - Buffer pool implementation
- `src/buffer/mod.rs` - Module exports

**Implementation:**

```rust
// src/buffer/pool.rs
use std::sync::Mutex;
use bytes::BytesMut;

pub struct BufferPool {
    pool: Mutex<Vec<BytesMut>>,
    initial_capacity: usize,
    max_capacity: usize,
    max_size: usize,
}

impl BufferPool {
    pub fn new(size: usize, initial_capacity: usize, max_capacity: usize) -> Self {
        let mut pool = Vec::with_capacity(size);
        for _ in 0..size {
            pool.push(BytesMut::with_capacity(initial_capacity));
        }
        
        Self {
            pool: Mutex::new(pool),
            initial_capacity,
            max_capacity,
            max_size: size,
        }
    }
    
    pub fn acquire(&self) -> BytesMut {
        let mut pool = self.pool.lock().unwrap();
        pool.pop().unwrap_or_else(|| {
            BytesMut::with_capacity(self.initial_capacity)
        })
    }
    
    pub fn release(&self, mut buffer: BytesMut) {
        if buffer.capacity() <= self.max_capacity {
            buffer.clear();
            let mut pool = self.pool.lock().unwrap();
            if pool.len() < self.max_size {
                pool.push(buffer);
            }
        }
    }
}
```

### 3.2 Add Memory Monitoring

**Files to create:**
- `src/memory/monitor.rs` - Memory monitoring
- `src/memory/mod.rs` - Module exports

**Implementation:**

```rust
pub struct MemoryMonitor {
    threshold: usize,
    high_threshold: usize,
    check_interval: Duration,
    auto_cleanup: bool,
}

impl MemoryMonitor {
    pub fn start_monitoring(&self) {
        let monitor = self.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(monitor.check_interval);
                monitor.check_memory_usage();
            }
        });
    }
    
    fn check_memory_usage(&self) {
        let usage = get_memory_usage();
        
        if usage > self.high_threshold {
            log::warn!("High memory usage: {} bytes", usage);
            if self.auto_cleanup {
                self.trigger_cleanup();
            }
        } else if usage > self.threshold {
            log::info!("Memory usage elevated: {} bytes", usage);
        }
    }
}
```

### 3.3 Add Memory-Mapped I/O Support

**Files to modify:**
- `src/handler/mod.rs` - Use mmap for large values

**Implementation:**

```rust
pub struct MmapConfig {
    enabled: bool,
    min_size: usize,
    max_memory: usize,
    temp_dir: PathBuf,
}

impl SqliteHandler {
    fn handle_large_value(&self, value: &[u8]) -> Result<Value> {
        if self.mmap_config.enabled && value.len() > self.mmap_config.min_size {
            // Use memory-mapped file
            self.store_in_mmap(value)
        } else {
            // Use regular in-memory storage
            Ok(Value::from(value))
        }
    }
}
```

### 3.4 Add CLI Arguments for Memory Management

```rust
/// Enable buffer pool
#[arg(long, env = "PGQT_ENABLE_BUFFER_POOL")]
enable_buffer_pool: bool,

/// Buffer pool size
#[arg(long, env = "PGQT_BUFFER_POOL_SIZE", default_value = "50")]
buffer_pool_size: usize,

/// Buffer initial capacity in bytes
#[arg(long, env = "PGQT_BUFFER_INITIAL_CAPACITY", default_value = "4096")]
buffer_initial_capacity: usize,

/// Buffer max capacity in bytes
#[arg(long, env = "PGQT_BUFFER_MAX_CAPACITY", default_value = "65536")]
buffer_max_capacity: usize,

/// Enable automatic memory cleanup
#[arg(long, env = "PGQT_AUTO_CLEANUP")]
auto_cleanup: bool,

/// Enable memory monitoring
#[arg(long, env = "PGQT_MEMORY_MONITORING")]
memory_monitoring: bool,

/// Memory threshold for cleanup in bytes
#[arg(long, env = "PGQT_MEMORY_THRESHOLD", default_value = "67108864")]
memory_threshold: usize,

/// High memory threshold in bytes
#[arg(long, env = "PGQT_HIGH_MEMORY_THRESHOLD", default_value = "134217728")]
high_memory_threshold: usize,

/// Memory check interval in seconds
#[arg(long, env = "PGQT_MEMORY_CHECK_INTERVAL", default_value = "10")]
memory_check_interval: u64,

/// Enable memory-mapped I/O for large values
#[arg(long, env = "PGQT_ENABLE_MMAP")]
enable_mmap: bool,

/// Minimum size for memory mapping in bytes
#[arg(long, env = "PGQT_MMAP_MIN_SIZE", default_value = "65536")]
mmap_min_size: usize,

/// Maximum memory before temp files in bytes
#[arg(long, env = "PGQT_MMAP_MAX_MEMORY", default_value = "1048576")]
mmap_max_memory: usize,

/// Temporary directory for memory-mapped files
#[arg(long, env = "PGQT_TEMP_DIR")]
temp_dir: Option<String>,
```

### Phase 3 Completion Checklist

- [ ] `cargo check` passes with no errors
- [ ] All build warnings addressed
- [ ] `./run_tests.sh` passes (all tests)
- [ ] Documentation updated in `docs/performance-tuning.md`
- [ ] Memory monitoring tests added

---

## Phase 4: Network Options (Unix Sockets & SSL)

**Goal:** Add Unix socket support and SSL/TLS configuration.

### 4.1 Add Unix Socket Support

**Files to modify:**
- `src/main.rs` - Add CLI arguments and socket listener
- `Cargo.toml` - Add Unix socket dependencies if needed

**Implementation:**

Add CLI arguments:

```rust
/// Directory for Unix domain socket
#[arg(long, env = "PGQT_SOCKET_DIR")]
socket_dir: Option<String>,

/// Disable TCP listener, use only Unix socket
#[arg(long, env = "PGQT_NO_TCP")]
no_tcp: bool,
```

Add Unix socket listener:

```rust
async fn start_unix_socket_listener(socket_path: &Path, handler: Arc<SqliteHandler>) -> Result<()> {
    let listener = tokio::net::UnixListener::bind(socket_path)?;
    
    loop {
        let (socket, _) = listener.accept().await?;
        let handler = handler.clone();
        
        tokio::spawn(async move {
            process_unix_socket(socket, handler).await;
        });
    }
}
```

### 4.2 Add SSL/TLS Support

**Files to modify:**
- `src/main.rs` - Add CLI arguments
- `src/tls.rs` (new file) - TLS configuration
- `Cargo.toml` - Add TLS dependencies (rustls, tokio-rustls)

**Implementation:**

Add to `Cargo.toml`:

```toml
[dependencies]
rustls = "0.21"
tokio-rustls = "0.24"
```

Add CLI arguments:

```rust
/// Enable SSL/TLS support
#[arg(long, env = "PGQT_SSL")]
ssl: bool,

/// Path to SSL certificate file
#[arg(long, env = "PGQT_SSL_CERT")]
ssl_cert: Option<String>,

/// Path to SSL private key file
#[arg(long, env = "PGQT_SSL_KEY")]
ssl_key: Option<String>,

/// Path to CA certificate file
#[arg(long, env = "PGQT_SSL_CA")]
ssl_ca: Option<String>,

/// Generate ephemeral SSL certificates on startup
#[arg(long, env = "PGQT_SSL_EPHEMERAL")]
ssl_ephemeral: bool,
```

Create `src/tls.rs`:

```rust
use rustls::{Certificate, PrivateKey, ServerConfig};
use std::sync::Arc;

pub struct TlsConfig {
    config: Arc<ServerConfig>,
}

impl TlsConfig {
    pub fn from_files(cert_path: &str, key_path: &str) -> Result<Self> {
        let certs = load_certs(cert_path)?;
        let key = load_private_key(key_path)?;
        
        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
    
    pub fn generate_ephemeral() -> Result<Self> {
        // Generate self-signed certificate
        // ... implementation
    }
}
```

### Phase 4 Completion Checklist

- [ ] `cargo check` passes with no errors
- [ ] All build warnings addressed
- [ ] `./run_tests.sh` passes (all tests)
- [ ] Documentation updated in `docs/performance-tuning.md`
- [ ] SSL tests added (if feasible)

---

## Testing Strategy

### Unit Tests

Each new module should have corresponding unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_buffer_pool_acquire_release() {
        let pool = BufferPool::new(10, 1024, 4096);
        let buffer = pool.acquire();
        pool.release(buffer);
    }
    
    #[test]
    fn test_connection_pool_timeout() {
        let pool = ConnectionPool::new(/* ... */);
        // Test timeout behavior
    }
}
```

### Integration Tests

Add to `tests/performance_tests.rs`:

```rust
#[test]
fn test_pragma_configuration() {
    // Test that PRAGMA settings are applied correctly
}

#[test]
fn test_cache_configuration() {
    // Test cache size and TTL behavior
}

#[test]
fn test_connection_pool_limits() {
    // Test max connections enforcement
}
```

### E2E Tests

Create `tests/performance_e2e_test.py`:

```python
def test_pragma_settings():
    """Test that PRAGMA settings are configurable."""
    # Start proxy with custom PRAGMA settings
    # Verify settings are applied

def test_connection_pool():
    """Test connection pooling behavior."""
    # Create multiple concurrent connections
    # Verify pool handles them correctly
```

---

## Documentation

### Create `docs/performance-tuning.md`

```markdown
# Performance Tuning Guide

## SQLite PRAGMA Configuration

### Journal Mode
- `--journal-mode WAL` (default) - Best for concurrent reads
- `--journal-mode DELETE` - Simple, slower concurrent access
- `--journal-mode MEMORY` - Fastest, no durability

### Synchronous Mode
- `--synchronous NORMAL` (default) - Balanced performance/durability
- `--synchronous FULL` - Maximum durability, slower writes
- `--synchronous OFF` - Fastest, risk of corruption

### Cache Size
- `--cache-size -64000` (default) - 64MB page cache
- Positive values = number of pages
- Negative values = KB

## Connection Pooling

### Basic Configuration
```bash
pgqt --max-connections 100 --pool-size 8
```

### With Timeouts
```bash
pgqt --connection-timeout 30 --idle-timeout 300
```

## Memory Management

### Buffer Pool
```bash
pgqt --enable-buffer-pool --buffer-pool-size 100
```

### Memory Monitoring
```bash
pgqt --memory-monitoring --auto-cleanup
```

## Unix Sockets

```bash
# Unix socket only
pgqt --socket-dir /tmp --no-tcp

# Both TCP and Unix socket
pgqt --socket-dir /tmp
```

## SSL/TLS

```bash
# With custom certificates
pgqt --ssl --ssl-cert server.crt --ssl-key server.key

# With ephemeral certificates (development)
pgqt --ssl --ssl-ephemeral
```
```

---

## Summary of New Configuration Options

| Category | Option | Default | Description |
|----------|--------|---------|-------------|
| **SQLite PRAGMA** | `--journal-mode` | WAL | Journal mode |
| | `--synchronous` | NORMAL | Synchronous mode |
| | `--cache-size` | -64000 | Page cache size |
| | `--mmap-size` | 268435456 | Memory-mapped I/O |
| **Connection Pool** | `--max-connections` | 100 | Max concurrent |
| | `--pool-size` | 8 | Pool size |
| | `--connection-timeout` | 30 | Timeout (sec) |
| | `--idle-timeout` | 300 | Idle timeout |
| | `--health-check-interval` | 60 | Health check |
| | `--max-retries` | 3 | Max retries |
| | `--use-pooling` | false | Read/write separation |
| **Cache** | `--transpile-cache-size` | 256 | Transpile cache |
| | `--transpile-cache-ttl` | 0 | Cache TTL |
| | `--enable-result-cache` | false | Result caching |
| | `--result-cache-size` | 100 | Result cache size |
| | `--result-cache-ttl` | 60 | Result cache TTL |
| **Buffer Pool** | `--enable-buffer-pool` | false | Buffer pooling |
| | `--buffer-pool-size` | 50 | Buffer pool size |
| | `--buffer-initial-capacity` | 4096 | Initial capacity |
| | `--buffer-max-capacity` | 65536 | Max capacity |
| **Memory** | `--auto-cleanup` | false | Auto cleanup |
| | `--memory-monitoring` | false | Memory monitoring |
| | `--memory-threshold` | 64MB | Cleanup threshold |
| | `--high-memory-threshold` | 128MB | High threshold |
| | `--memory-check-interval` | 10 | Check interval |
| **MMap** | `--enable-mmap` | false | Memory mapping |
| | `--mmap-min-size` | 65536 | Min mmap size |
| | `--mmap-max-memory` | 1MB | Max memory |
| | `--temp-dir` | System temp | Temp directory |
| **Network** | `--socket-dir` | None | Unix socket dir |
| | `--no-tcp` | false | Disable TCP |
| **SSL** | `--ssl` | false | Enable SSL |
| | `--ssl-cert` | None | Certificate path |
| | `--ssl-key` | None | Private key path |
| | `--ssl-ca` | None | CA certificate |
| | `--ssl-ephemeral` | false | Ephemeral certs |

---

## Implementation Order

1. **Phase 1** - Start here, foundation for other phases
2. **Phase 2** - Builds on connection pool from Phase 1
3. **Phase 3** - Independent, can be done in parallel with Phase 2
4. **Phase 4** - Network layer, independent of other phases

## Notes

- Each phase must be completed before starting the next
- Run tests after every significant change
- Update documentation as features are added
- Consider adding metrics/logging for monitoring
