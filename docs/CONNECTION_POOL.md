# Connection Pool

## Overview

PGQT now includes a connection pool that enables proper transaction support by providing each PostgreSQL session with its own isolated SQLite connection. This is a foundational change for Phase 5 (Transaction Support) of the architecture review implementation.

## Architecture

### Problem Solved

Previously, all PostgreSQL sessions shared a single SQLite connection (`Arc<Mutex<Connection>>`), which caused:
- Transaction state collisions between concurrent clients
- No proper transaction isolation
- Silent data integrity violations

### Solution

The connection pool (`src/connection_pool.rs`) provides:
- **Per-session connections**: Each client gets their own SQLite connection
- **Automatic checkout/checkin**: Connections are borrowed from the pool and returned when dropped
- **Transaction rollback on drop**: Active transactions are automatically rolled back when connections are returned
- **WAL mode**: Configured for better concurrency
- **Busy timeout**: 5-second timeout to handle concurrent access

## Usage

### Basic Usage

```rust
use pgqt::connection_pool::ConnectionPool;
use std::path::Path;

// Create a pool with max 10 connections
let pool = ConnectionPool::new(Path::new("myapp.db"), 10)?;

// Checkout a connection for client ID 1
let conn = pool.checkout(1)?;

// Use the connection (wrapped in Arc<Mutex<>>)
let arc = conn.get_arc()?;
let guard = arc.lock().unwrap();
guard.execute("SELECT 1", [])?;

// Connection automatically returns to pool when dropped
```

### Integration with SqliteHandler

The `SqliteHandler` now includes:
- `conn_pool`: The connection pool for per-session connections
- `client_connections`: Map from client_id to their checked-out connection

```rust
// In handler/mod.rs
pub struct SqliteHandler {
    pub conn: Arc<Mutex<Connection>>,           // Legacy shared connection
    pub conn_pool: ConnectionPool,              // New connection pool
    pub client_connections: Arc<DashMap<u32, PooledConnection>>, // Active connections
    // ... other fields
}
```

## Configuration

### Pool Size

Default pool size is 10 connections. This can be made configurable in the future via:
- Command-line flag: `--connection-pool-size 20`
- Configuration file: `connection_pool_size: 20`
- Environment variable: `PGQT_CONNECTION_POOL_SIZE=20`

### SQLite Configuration

Each connection is configured with:
- `PRAGMA journal_mode=WAL` - Write-Ahead Logging for better concurrency
- `PRAGMA busy_timeout=5000` - 5-second busy timeout
- `PRAGMA foreign_keys=ON` - Foreign key constraints enabled

## Transaction Behavior

### Automatic Rollback

When a pooled connection is dropped (e.g., client disconnects), any active transaction is automatically rolled back:

```rust
impl Drop for PooledConnection {
    fn drop(&mut self) {
        // ...
        if let Ok(mut guard) = conn.into_inner() {
            // Rollback any active transaction before returning to pool
            let _ = guard.execute("ROLLBACK", []);
            // ...
        }
    }
}
```

This ensures that:
- Abandoned transactions don't leave locks
- Partial changes are never committed
- Connection is clean for the next client

## Testing

The connection pool includes comprehensive unit tests:

```bash
# Run connection pool tests
cargo test --lib connection_pool

# Run all tests
./run_tests.sh
```

### Test Coverage

- Pool creation and initialization
- Checkout and drop behavior
- Duplicate checkout prevention
- Pool exhaustion handling
- Connection isolation between clients
- Transaction rollback on drop

## Migration Guide

### For Developers

The connection pool is integrated into `SqliteHandler` and is backward compatible:
- Existing code using `handler.conn` continues to work
- New per-session code should use `handler.client_connections`

### Future Work

Phase 5.2 will integrate the connection pool into query execution:
- Route transaction commands to per-session connections
- Implement proper transaction state management
- Add 25P02 error handling for aborted transactions

## Performance Considerations

### Connection Limits

SQLite has limits on concurrent connections:
- **Readers**: Unlimited (with WAL mode)
- **Writers**: Only 1 at a time (SQLite's single-writer limitation)

The pool size should be set based on:
- Expected number of concurrent clients
- Read vs write workload ratio
- Available memory (each connection uses ~1-2MB)

### Recommended Pool Sizes

| Workload Type | Pool Size | Notes |
|--------------|-----------|-------|
| Light (< 10 clients) | 5-10 | Default |
| Medium (10-50 clients) | 10-20 | Mostly reads |
| Heavy (> 50 clients) | 20-50 | Mostly reads |

## Troubleshooting

### "Connection pool exhausted" Error

**Cause**: All connections are checked out.

**Solutions**:
1. Increase pool size
2. Ensure connections are being dropped (check for connection leaks)
3. Reduce concurrent client count

### "Client already has a checked-out connection" Error

**Cause**: Client tried to checkout twice without dropping.

**Solution**: Ensure each client only checks out once and drops before re-checking.

### SQLITE_BUSY Errors

**Cause**: Write conflict - another connection is writing.

**Solution**: SQLite handles this with busy timeout, but you may need to:
1. Retry the transaction
2. Reduce transaction duration
3. Use read-only transactions where possible

## API Reference

### `ConnectionPool`

```rust
impl ConnectionPool {
    /// Create a new pool
    pub fn new(db_path: &Path, max_connections: usize) -> Result<Self>;
    
    /// Checkout a connection for a client
    pub fn checkout(&self, client_id: u32) -> Result<PooledConnection>;
    
    /// Check if client has a connection
    pub fn has_connection(&self, client_id: u32) -> bool;
    
    /// Get available connection count
    pub fn available_count(&self) -> usize;
    
    /// Get in-use connection count
    pub fn in_use_count(&self) -> usize;
}
```

### `PooledConnection`

```rust
impl PooledConnection {
    /// Get the Arc<Mutex<Connection>> for use
    pub fn get_arc(&self) -> Result<Arc<Mutex<Connection>>>;
}

impl Drop for PooledConnection {
    /// Automatically returns connection to pool and rolls back transactions
}
```
