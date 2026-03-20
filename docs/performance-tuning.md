# PGQT Performance Tuning Guide

This document describes the performance tuning options available in PGQT for optimizing SQLite database performance and query caching.

## Table of Contents

- [Overview](#overview)
- [SQLite PRAGMA Configuration](#sqlite-pragma-configuration)
  - [Journal Mode](#journal-mode)
  - [Synchronous Mode](#synchronous-mode)
  - [Cache Size](#cache-size)
  - [Memory-Mapped I/O](#memory-mapped-io)
  - [Temp Store](#temp-store)
- [Configuration Methods](#configuration-methods)
  - [CLI Arguments](#cli-arguments)
  - [JSON Configuration File](#json-configuration-file)
- [Transpile Cache](#transpile-cache)
- [SQLite Prepared Statement Cache](#sqlite-prepared-statement-cache)
- [Query Result Cache](#query-result-cache)
- [Connection Pooling](#connection-pooling)
- [Memory Management](#memory-management)
  - [Buffer Pool](#buffer-pool)
  - [Memory Monitoring](#memory-monitoring)
  - [Memory-Mapped I/O for Large Values](#memory-mapped-io-for-large-values)
- [Network Options](#network-options)
  - [Unix Sockets](#unix-sockets)
  - [SSL/TLS Encryption](#ssltls-encryption)
- [Performance Recommendations](#performance-recommendations)

## Overview

PGQT provides several performance tuning options that allow you to optimize SQLite behavior for your specific workload:

1. **SQLite PRAGMA Settings**: Control low-level SQLite behavior for durability vs performance trade-offs
2. **Transpile Cache**: Cache transpiled SQL queries to avoid repeated parsing overhead
3. **SQLite Prepared Statement Cache**: Cache compiled SQLite statements to avoid repeated SQL parsing and query plan generation
4. **Query Result Cache**: Cache query results for frequently executed read-only queries
5. **Connection Pooling**: Configure connection pool size and behavior for concurrent workloads
6. **Memory Management**: Buffer pool for efficient memory reuse, memory monitoring, and memory-mapped I/O for large values

## SQLite PRAGMA Configuration

### Journal Mode

Controls how SQLite handles transaction durability:

| Mode | Description | Use Case |
|------|-------------|----------|
| `wal` (default) | Write-Ahead Logging - allows concurrent readers and writers | General purpose, recommended |
| `delete` | Traditional rollback journal | Compatibility with older SQLite versions |
| `truncate` | Truncate journal file on commit | Slightly faster than delete |
| `persist` | Keep journal file but zero it out | Reduces filesystem operations |
| `memory` | Store journal in memory | Fastest, but not durable |
| `off` | No journaling | Fastest, completely unsafe |

**CLI:** `--journal-mode wal`  
**Env:** `PGQT_JOURNAL_MODE=wal`

### Synchronous Mode

Controls how aggressively SQLite writes to disk:

| Mode | Value | Description | Use Case |
|------|-------|-------------|----------|
| `off` | 0 | No syncing | Maximum speed, minimal durability |
| `normal` (default) | 1 | Sync at critical moments | Good balance |
| `full` | 2 | Sync at every commit | Maximum durability |
| `extra` | 3 | Extra sync for WAL mode | Maximum durability with WAL |

**CLI:** `--synchronous normal`  
**Env:** `PGQT_SYNCHRONOUS=normal`

### Cache Size

Controls the amount of memory SQLite uses for caching database pages:

- **Positive values**: Number of pages to cache
- **Negative values**: Cache size in kilobytes (e.g., `-2000` = 2MB)
- **Default**: `-2000` (2MB cache)

**CLI:** `--cache-size -2000`  
**Env:** `PGQT_CACHE_SIZE=-2000`

**Recommendations:**
- Small databases (< 100MB): `-2000` to `-4000` (2-4MB)
- Medium databases (100MB - 1GB): `-8000` to `-16000` (8-16MB)
- Large databases (> 1GB): `-32000` or more (32MB+)

### Memory-Mapped I/O

Enables memory-mapped file I/O for potentially faster read access:

- **Default**: `0` (disabled)
- **Recommended**: Try `268435456` (256MB) for read-heavy workloads

**CLI:** `--mmap-size 268435456`  
**Env:** `PGQT_MMAP_SIZE=268435456`

**Note:** Memory-mapped I/O may not provide benefits on all systems and can use significant virtual memory.

### Temp Store

Controls where temporary tables and indices are stored:

| Mode | Value | Description | Use Case |
|------|-------|-------------|----------|
| `default` (default) | 0 | Use compile-time default | General purpose |
| `file` | 1 | Store temp data in files | Large temporary datasets |
| `memory` | 2 | Store temp data in memory | Fast temp operations |

**CLI:** `--temp-store default`  
**Env:** `PGQT_TEMP_STORE=default`

## Configuration Methods

### CLI Arguments

All PRAGMA settings can be configured via command-line arguments:

```bash
pgqt \
  --journal-mode wal \
  --synchronous normal \
  --cache-size -4000 \
  --mmap-size 268435456 \
  --temp-store memory \
  --database myapp.db
```

### JSON Configuration File

For multi-port configurations, use a JSON configuration file:

```json
{
  "ports": [
    {
      "port": 5432,
      "database": "/var/lib/pgqt/production.db",
      "pragma": {
        "journal_mode": "wal",
        "synchronous": "normal",
        "cache_size": -8000,
        "mmap_size": 268435456,
        "temp_store": "memory"
      },
      "cache": {
        "transpile_cache_size": 512,
        "transpile_cache_ttl": 300,
        "enable_result_cache": true,
        "result_cache_size": 128,
        "result_cache_ttl": 60
      }
    }
  ]
}
```

## Transpile Cache

The transpile cache stores parsed and transpiled SQL queries to avoid repeated parsing overhead.

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `transpile_cache_size` | 256 | Number of queries to cache |
| `transpile_cache_ttl` | 0 (no TTL) | Cache entry TTL in seconds |

**CLI:**
```bash
pgqt --transpile-cache-size 512 --transpile-cache-ttl 300
```

**Env:**
```bash
PGQT_TRANSPILE_CACHE_SIZE=512
PGQT_TRANSPILE_CACHE_TTL=300
```

### Recommendations

- **Small workloads**: 128-256 entries
- **Medium workloads**: 256-512 entries
- **Large workloads with many unique queries**: 1024+ entries
- **TTL**: Set to 300 (5 minutes) or more if your query patterns are stable

## SQLite Prepared Statement Cache

PGQT uses SQLite's prepared statement cache to avoid repeated SQL parsing and query plan generation. When a query is executed, SQLite prepares the statement (parses SQL, generates bytecode, optimizes the query plan). With prepared statement caching, subsequent executions of the same query structure reuse the prepared statement, significantly improving performance for repeated queries.

### How It Works

- **Cache Size**: Fixed at 64 prepared statements (configurable in future releases)
- **Cache Key**: Based on the SQL statement text
- **Automatic Invalidation**: The cache is automatically cleared when DDL operations (CREATE, ALTER, DROP) are executed to ensure schema changes are reflected

### Performance Benefits

- **20-30% improvement** for OLTP workloads with repeated queries
- Eliminates SQL parsing overhead for cached statements
- Reuses optimized query plans
- Reduces memory allocations for statement structures

### Cache Invalidation

The prepared statement cache is automatically cleared when:
- DDL operations are executed (CREATE TABLE, ALTER TABLE, DROP TABLE, etc.)
- Schema changes occur that could affect query plans

This ensures that schema changes are immediately reflected in subsequent queries.

### Monitoring

To verify prepared statement caching is working, you can check the SQLite cache statistics:

```sql
-- Note: SQLite does not expose direct cache statistics,
-- but you can observe performance improvements in query latency
```

### Recommendations

- **Default cache size (64)**: Suitable for most workloads
- **Read-heavy OLTP workloads**: Benefit most from prepared statement caching
- **Unique query-heavy workloads**: May see less benefit if every query is different

## Query Result Cache

The query result cache stores actual query results for frequently executed read-only queries.

**Note:** This is currently disabled by default and primarily useful for read-heavy workloads with stable data.

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `enable_result_cache` | false | Enable query result caching |
| `result_cache_size` | 64 | Number of results to cache |
| `result_cache_ttl` | 60 | Cache entry TTL in seconds |

**CLI:**
```bash
pgqt --enable-result-cache --result-cache-size 128 --result-cache-ttl 60
```

**Env:**
```bash
PGQT_ENABLE_RESULT_CACHE=true
PGQT_RESULT_CACHE_SIZE=128
PGQT_RESULT_CACHE_TTL=60
```

### When to Use

- Read-heavy workloads (> 90% reads)
- Stable data that doesn't change frequently
- Repeated identical queries
- Can tolerate slightly stale data

### When NOT to Use

- Write-heavy workloads
- Real-time data requirements
- Highly dynamic data
- Large result sets (memory pressure)

## Connection Pooling

PGQT uses a connection pool to manage SQLite connections for concurrent client sessions. The pool configuration controls how connections are created, reused, and managed.

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `max_connections` | 100 | Maximum number of connections allowed |
| `pool_size` | 8 | Initial number of connections to create |
| `connection_timeout` | 30 | Timeout in seconds when checking out a connection |
| `idle_timeout` | 300 | Seconds before closing unused connections |
| `health_check_interval` | 60 | Seconds between health checks |
| `max_retries` | 3 | Maximum retry attempts for failed checkouts |
| `use_pooling` | false | Enable connection pooling (default: false for backward compatibility) |

**CLI:**
```bash
pgqt \
  --max-connections 100 \
  --pool-size 8 \
  --connection-timeout 30 \
  --idle-timeout 300 \
  --health-check-interval 60 \
  --max-retries 3 \
  --use-pooling
```

**Env:**
```bash
PGQT_MAX_CONNECTIONS=100
PGQT_POOL_SIZE=8
PGQT_CONNECTION_TIMEOUT=30
PGQT_IDLE_TIMEOUT=300
PGQT_HEALTH_CHECK_INTERVAL=60
PGQT_MAX_RETRIES=3
PGQT_USE_POOLING=true
```

### JSON Configuration

```json
{
  "ports": [
    {
      "port": 5432,
      "database": "/var/lib/pgqt/production.db",
      "pool": {
        "max_connections": 100,
        "pool_size": 8,
        "connection_timeout": 30,
        "idle_timeout": 300,
        "health_check_interval": 60,
        "max_retries": 3,
        "use_pooling": true
      }
    }
  ]
}
```

### Recommendations

- **Small workloads**: `max_connections=20`, `pool_size=2`
- **Medium workloads**: `max_connections=50`, `pool_size=5`
- **Large workloads**: `max_connections=100`, `pool_size=8`
- **Connection timeout**: Increase to 60+ seconds for slow networks
- **Idle timeout**: Set to 0 to disable (keep all connections open)

### When to Enable Pooling

- Multiple concurrent clients
- Connection establishment overhead is noticeable
- Predictable connection patterns

### When NOT to Enable Pooling

- Single-client applications
- Memory-constrained environments
- Unpredictable connection spikes

## Memory Management

PGQT provides advanced memory management features to optimize memory usage and handle large values efficiently.

### Buffer Pool

The buffer pool provides efficient memory reuse using `BytesMut` buffers. This reduces allocation overhead for frequently created temporary buffers, improving performance in high-throughput scenarios.

#### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `enable_buffer_pool` | false | Enable buffer pool for memory reuse |
| `buffer_pool_size` | 50 | Maximum number of buffers to keep in the pool |
| `buffer_initial_capacity` | 4096 | Initial capacity for new buffers (bytes) |
| `buffer_max_capacity` | 65536 | Maximum capacity for buffers (bytes) |

**CLI:**
```bash
pgqt \
  --enable-buffer-pool \
  --buffer-pool-size 100 \
  --buffer-initial-capacity 8192 \
  --buffer-max-capacity 131072
```

**Env:**
```bash
PGQT_ENABLE_BUFFER_POOL=true
PGQT_BUFFER_POOL_SIZE=100
PGQT_BUFFER_INITIAL_CAPACITY=8192
PGQT_BUFFER_MAX_CAPACITY=131072
```

#### JSON Configuration

```json
{
  "ports": [
    {
      "port": 5432,
      "database": "/var/lib/pgqt/production.db",
      "memory": {
        "buffer_pool": {
          "enabled": true,
          "pool_size": 100,
          "initial_capacity": 8192,
          "max_capacity": 131072
        }
      }
    }
  ]
}
```

#### When to Use

- High-throughput workloads with frequent buffer allocations
- Applications processing many small to medium-sized queries
- Memory-constrained environments where allocation overhead matters

#### Recommendations

- **Small workloads**: `pool_size=25`, `initial_capacity=4096`
- **Medium workloads**: `pool_size=50`, `initial_capacity=8192`
- **Large workloads**: `pool_size=100`, `initial_capacity=16384`
- **Buffer max capacity**: Set based on your largest expected query/response size

### Memory Monitoring

Memory monitoring tracks memory usage and can trigger automatic cleanup when thresholds are exceeded. This helps prevent out-of-memory conditions in long-running deployments.

#### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `memory_monitoring` | false | Enable memory monitoring |
| `memory_threshold` | 67108864 (64MB) | Memory threshold for normal operation |
| `high_memory_threshold` | 134217728 (128MB) | High memory threshold for aggressive cleanup |
| `memory_check_interval` | 10 | Check interval in seconds |
| `auto_cleanup` | false | Enable automatic cleanup when thresholds exceeded |

**CLI:**
```bash
pgqt \
  --memory-monitoring \
  --memory-threshold 67108864 \
  --high-memory-threshold 134217728 \
  --memory-check-interval 10 \
  --auto-cleanup
```

**Env:**
```bash
PGQT_MEMORY_MONITORING=true
PGQT_MEMORY_THRESHOLD=67108864
PGQT_HIGH_MEMORY_THRESHOLD=134217728
PGQT_MEMORY_CHECK_INTERVAL=10
PGQT_AUTO_CLEANUP=true
```

#### JSON Configuration

```json
{
  "ports": [
    {
      "port": 5432,
      "database": "/var/lib/pgqt/production.db",
      "memory": {
        "monitoring": {
          "enabled": true,
          "threshold": 67108864,
          "high_threshold": 134217728,
          "check_interval": 10,
          "auto_cleanup": true
        }
      }
    }
  ]
}
```

#### When to Use

- Long-running production deployments
- Workloads with unpredictable memory patterns
- Multi-tenant environments where memory isolation is important
- Applications where memory leaks could be catastrophic

#### Recommendations

- **Threshold**: Set to 70-80% of available memory for the process
- **High threshold**: Set to 85-90% of available memory
- **Check interval**: 10 seconds for most workloads, 5 seconds for critical systems
- **Auto cleanup**: Enable for production, disable for debugging memory issues

### Memory-Mapped I/O for Large Values

PGQT can use memory-mapped I/O for handling large values (BLOBs, large text). This allows the system to handle values larger than available RAM by mapping them to temporary files.

#### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `enable_mmap` | false | Enable memory-mapped I/O for large values |
| `mmap_min_size` | 65536 (64KB) | Minimum size to use mmap (bytes) |
| `mmap_max_memory` | 1048576 (1MB) | Maximum total mmap memory (bytes) |
| `temp_dir` | system temp | Temporary directory for mmap files |

**CLI:**
```bash
pgqt \
  --enable-mmap \
  --mmap-min-size 65536 \
  --mmap-max-memory 10485760 \
  --temp-dir /var/tmp/pgqt
```

**Env:**
```bash
PGQT_ENABLE_MMAP=true
PGQT_MMAP_MIN_SIZE=65536
PGQT_MMAP_MAX_MEMORY=10485760
PGQT_TEMP_DIR=/var/tmp/pgqt
```

#### JSON Configuration

```json
{
  "ports": [
    {
      "port": 5432,
      "database": "/var/lib/pgqt/production.db",
      "memory": {
        "mmap": {
          "enabled": true,
          "min_size": 65536,
          "max_memory": 10485760,
          "temp_dir": "/var/tmp/pgqt"
        }
      }
    }
  ]
}
```

#### When to Use

- Applications storing large BLOBs or text fields
- Systems with limited RAM handling large datasets
- Workloads with occasional very large values

#### Recommendations

- **Minimum size**: 64KB is a good default; smaller values may not benefit from mmap
- **Max memory**: Set based on your temp directory filesystem capacity
- **Temp directory**: Use a fast local filesystem (SSD) for best performance

## Performance Recommendations

### Development/Testing

```bash
pgqt \
  --journal-mode wal \
  --synchronous off \
  --cache-size -2000 \
  --temp-store memory
```

### Production (Balanced)

```bash
pgqt \
  --journal-mode wal \
  --synchronous normal \
  --cache-size -8000 \
  --mmap-size 268435456 \
  --temp-store memory \
  --transpile-cache-size 512 \
  --transpile-cache-ttl 300
```

### Production (Maximum Durability)

```bash
pgqt \
  --journal-mode wal \
  --synchronous full \
  --cache-size -4000
```

### Production (Maximum Performance)

```bash
pgqt \
  --journal-mode wal \
  --synchronous normal \
  --cache-size -16000 \
  --mmap-size 536870912 \
  --temp-store memory \
  --transpile-cache-size 1024 \
  --transpile-cache-ttl 600 \
  --enable-result-cache \
  --result-cache-size 256 \
  --result-cache-ttl 30
```

## Monitoring

To verify your settings are applied correctly, you can query SQLite's PRAGMA values:

```sql
-- Check journal mode
PRAGMA journal_mode;

-- Check synchronous mode
PRAGMA synchronous;

-- Check cache size
PRAGMA cache_size;

-- Check mmap size
PRAGMA mmap_size;

-- Check temp store
PRAGMA temp_store;
```

## Environment Variables Summary

| Variable | Description | Default |
|----------|-------------|---------|
| `PGQT_JOURNAL_MODE` | SQLite journal mode | `wal` |
| `PGQT_SYNCHRONOUS` | SQLite synchronous mode | `normal` |
| `PGQT_CACHE_SIZE` | SQLite cache size in pages | `-2000` |
| `PGQT_MMAP_SIZE` | SQLite mmap size in bytes | `0` |
| `PGQT_TEMP_STORE` | SQLite temp store mode | `default` |
| `PGQT_TRANSPILE_CACHE_SIZE` | Transpile cache entries | `256` |
| `PGQT_TRANSPILE_CACHE_TTL` | Transpile cache TTL (seconds) | `0` |
| `PGQT_ENABLE_RESULT_CACHE` | Enable query result cache | (not set) |
| `PGQT_RESULT_CACHE_SIZE` | Result cache entries | `64` |
| `PGQT_RESULT_CACHE_TTL` | Result cache TTL (seconds) | `60` |
| `PGQT_MAX_CONNECTIONS` | Maximum pool connections | `100` |
| `PGQT_POOL_SIZE` | Initial pool connections | `8` |
| `PGQT_CONNECTION_TIMEOUT` | Connection checkout timeout (seconds) | `30` |
| `PGQT_IDLE_TIMEOUT` | Idle connection timeout (seconds) | `300` |
| `PGQT_HEALTH_CHECK_INTERVAL` | Health check interval (seconds) | `60` |
| `PGQT_MAX_RETRIES` | Max checkout retry attempts | `3` |
| `PGQT_USE_POOLING` | Enable connection pooling | (not set) |
| `PGQT_ENABLE_BUFFER_POOL` | Enable buffer pool for memory reuse | (not set) |
| `PGQT_BUFFER_POOL_SIZE` | Buffer pool size (number of buffers) | `50` |
| `PGQT_BUFFER_INITIAL_CAPACITY` | Buffer initial capacity (bytes) | `4096` |
| `PGQT_BUFFER_MAX_CAPACITY` | Buffer maximum capacity (bytes) | `65536` |
| `PGQT_MEMORY_MONITORING` | Enable memory monitoring | (not set) |
| `PGQT_MEMORY_THRESHOLD` | Memory threshold (bytes) | `67108864` (64MB) |
| `PGQT_HIGH_MEMORY_THRESHOLD` | High memory threshold (bytes) | `134217728` (128MB) |
| `PGQT_MEMORY_CHECK_INTERVAL` | Memory check interval (seconds) | `10` |
| `PGQT_AUTO_CLEANUP` | Enable automatic memory cleanup | (not set) |
| `PGQT_ENABLE_MMAP` | Enable mmap for large values | (not set) |
| `PGQT_MMAP_MIN_SIZE` | Minimum size for mmap (bytes) | `65536` |
| `PGQT_MMAP_MAX_MEMORY` | Maximum mmap memory (bytes) | `1048576` |
| `PGQT_TEMP_DIR` | Temporary directory for mmap files | (system temp) |
| `PGQT_SOCKET_DIR` | Directory for Unix socket files | (not set) |
| `PGQT_NO_TCP` | Disable TCP listener | (not set) |
| `PGQT_SSL` | Enable TLS/SSL encryption | (not set) |
| `PGQT_SSL_CERT` | Path to SSL certificate file | (not set) |
| `PGQT_SSL_KEY` | Path to SSL private key file | (not set) |
| `PGQT_SSL_CA` | Path to SSL CA certificate | (not set) |
| `PGQT_SSL_EPHEMERAL` | Use ephemeral self-signed certificate | (not set) |

## Network Options

PGQT supports advanced network options for improved security and performance:

### Unix Sockets

Unix domain sockets provide a more efficient and secure way for local connections compared to TCP. They bypass the network stack entirely and use filesystem permissions for access control.

#### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `socket_dir` | None | Directory to create Unix socket files |
| `no_tcp` | false | Disable TCP listener (Unix socket only) |

**CLI:**
```bash
# Enable Unix socket alongside TCP
pgqt --socket-dir /var/run/pgqt --database myapp.db

# Unix socket only (no TCP)
pgqt --socket-dir /var/run/pgqt --no-tcp --database myapp.db
```

**Env:**
```bash
PGQT_SOCKET_DIR=/var/run/pgqt PGQT_NO_TCP=true pgqt --database myapp.db
```

**JSON Configuration:**
```json
{
  "ports": [
    {
      "port": 5432,
      "database": "/var/lib/pgqt/production.db",
      "network": {
        "socket_dir": "/var/run/pgqt",
        "no_tcp": false
      }
    }
  ]
}
```

#### Socket File Naming

Socket files are named using the pattern: `pgqt.{port}.sock`

For example, with `--socket-dir /var/run/pgqt` and port 5432:
- Socket path: `/var/run/pgqt/pgqt.5432.sock`

#### Connecting via Unix Socket

**psql:**
```bash
psql -h /var/run/pgqt -p 5432 -d postgres
# or
psql "host=/var/run/pgqt port=5432 dbname=postgres"
```

**Python (psycopg2):**
```python
import psycopg2
conn = psycopg2.connect(
    host='/var/run/pgqt',
    port=5432,
    database='postgres'
)
```

#### When to Use Unix Sockets

- **Local-only access**: When all clients are on the same machine
- **Security**: Bypasses network stack, uses filesystem permissions
- **Performance**: Lower latency than TCP for local connections
- **Container environments**: Share socket via volume mounts

#### Platform Support

- **Linux**: Full support
- **macOS**: Full support
- **Windows**: Not supported (Unix sockets are Unix-only)

### SSL/TLS Encryption

PGQT supports TLS/SSL encryption for secure connections over the network. This protects data in transit from eavesdropping and tampering.

#### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `ssl` | false | Enable TLS/SSL encryption |
| `ssl_cert` | None | Path to SSL certificate file (PEM format) |
| `ssl_key` | None | Path to SSL private key file (PEM format) |
| `ssl_ca` | None | Path to CA certificate for client verification (optional) |
| `ssl_ephemeral` | false | Generate self-signed certificate (development only) |

#### Using Your Own Certificate

For production use, provide your own certificate from a trusted CA:

**CLI:**
```bash
pgqt \
  --ssl \
  --ssl-cert /etc/pgqt/server.crt \
  --ssl-key /etc/pgqt/server.key \
  --database myapp.db
```

**Env:**
```bash
PGQT_SSL=true \
PGQT_SSL_CERT=/etc/pgqt/server.crt \
PGQT_SSL_KEY=/etc/pgqt/server.key \
pgqt --database myapp.db
```

**JSON Configuration:**
```json
{
  "ports": [
    {
      "port": 5432,
      "database": "/var/lib/pgqt/production.db",
      "tls": {
        "ssl": true,
        "ssl_cert": "/etc/pgqt/server.crt",
        "ssl_key": "/etc/pgqt/server.key"
      }
    }
  ]
}
```

#### Ephemeral (Self-Signed) Certificates

For development and testing, you can use ephemeral self-signed certificates:

**CLI:**
```bash
pgqt --ssl --ssl-ephemeral --database myapp.db
```

**Warning:** Ephemeral certificates are regenerated on each restart and are not trusted by clients by default. They should **never** be used in production.

#### Client Certificate Verification (mTLS)

For mutual TLS (client certificate verification), provide a CA certificate:

**CLI:**
```bash
pgqt \
  --ssl \
  --ssl-cert /etc/pgqt/server.crt \
  --ssl-key /etc/pgqt/server.key \
  --ssl-ca /etc/pgqt/ca.crt \
  --database myapp.db
```

This requires clients to present a valid certificate signed by the specified CA.

#### Connecting with SSL

**psql:**
```bash
# Require SSL
psql "host=localhost port=5432 dbname=postgres sslmode=require"

# Verify server certificate
psql "host=localhost port=5432 dbname=postgres sslmode=verify-ca sslrootcert=/path/to/ca.crt"
```

**Python (psycopg2):**
```python
import psycopg2
conn = psycopg2.connect(
    host='localhost',
    port=5432,
    database='postgres',
    sslmode='require'
)
```

#### SSL Modes

PGQT supports the following PostgreSQL SSL modes:

| Mode | Description |
|------|-------------|
| `disable` | No SSL (default when `--ssl` not used) |
| `allow` | Try non-SSL first, fall back to SSL |
| `prefer` | Try SSL first, fall back to non-SSL |
| `require` | SSL required, no certificate verification |
| `verify-ca` | SSL required, verify server certificate |
| `verify-full` | SSL required, verify certificate and hostname |

#### Certificate Requirements

- **Format**: PEM-encoded X.509 certificates
- **Certificate file**: Must contain the server certificate chain
- **Key file**: Must contain the unencrypted private key (PKCS#8 or RSA)
- **Key permissions**: Should be readable only by the pgqt user (0600)

#### When to Use SSL

- **Production environments**: Always use SSL for production
- **Remote connections**: Required when clients connect over the network
- **Compliance**: Many security standards require encrypted connections
- **Multi-tenant environments**: Protect data between tenants

#### Performance Considerations

- SSL adds a small overhead (~5-10%) for connection establishment
- Data transfer overhead is minimal with modern CPUs (AES-NI)
- Consider using connection pooling to amortize SSL handshake cost

## Further Reading

- [SQLite PRAGMA documentation](https://www.sqlite.org/pragma.html)
- [SQLite WAL mode](https://www.sqlite.org/wal.html)
- [SQLite performance tuning](https://www.sqlite.org/eqp.html)