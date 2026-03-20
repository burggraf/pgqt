# PGQT Metrics Reference

Complete reference for PGQT Prometheus metrics.

## Core Metrics

### pgqt_requests_total
- **Type:** Counter
- **Description:** Total number of requests processed
- **Labels:** None
- **Use case:** Track overall request volume

### pgqt_requests_failed_total
- **Type:** Counter
- **Description:** Total number of failed requests
- **Labels:** None
- **Use case:** Track error rate

### pgqt_query_duration_seconds
- **Type:** Histogram
- **Description:** Query execution latency distribution
- **Buckets:** 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s
- **Use case:** Monitor query performance, calculate percentiles

### pgqt_connections_active
- **Type:** Gauge
- **Description:** Number of currently active connections
- **Use case:** Monitor connection pool usage

### pgqt_connections_total
- **Type:** Counter
- **Description:** Total number of connections accepted
- **Use case:** Track connection rate

## Query Type Metrics

### pgqt_queries_select_total
- **Type:** Counter
- **Description:** Total SELECT queries

### pgqt_queries_insert_total
- **Type:** Counter
- **Description:** Total INSERT queries

### pgqt_queries_update_total
- **Type:** Counter
- **Description:** Total UPDATE queries

### pgqt_queries_delete_total
- **Type:** Counter
- **Description:** Total DELETE queries

### pgqt_queries_ddl_total
- **Type:** Counter
- **Description:** Total DDL queries (CREATE, ALTER, DROP, TRUNCATE)

### pgqt_queries_other_total
- **Type:** Counter
- **Description:** Total other queries

## Cache Metrics

### pgqt_transpile_cache_hits_total
- **Type:** Counter
- **Description:** Transpile cache hits

### pgqt_transpile_cache_misses_total
- **Type:** Counter
- **Description:** Transpile cache misses

## System Metrics (requires system-metrics feature)

### pgqt_system_cpu_usage_percent
- **Type:** Gauge
- **Description:** CPU usage percentage
- **Range:** 0-100

### pgqt_system_memory_used_bytes
- **Type:** Gauge
- **Description:** System memory currently used

### pgqt_system_memory_total_bytes
- **Type:** Gauge
- **Description:** Total system memory

### pgqt_system_disk_used_bytes
- **Type:** Gauge
- **Description:** Database file size

### pgqt_system_disk_total_bytes
- **Type:** Gauge
- **Description:** Total disk space

## Example PromQL Queries

```promql
# Request rate over 5 minutes
rate(pgqt_requests_total[5m])

# Error rate
rate(pgqt_requests_failed_total[5m]) / rate(pgqt_requests_total[5m])

# P99 latency
histogram_quantile(0.99, rate(pgqt_query_duration_seconds_bucket[5m]))

# Cache hit rate
pgqt_transpile_cache_hits_total / (pgqt_transpile_cache_hits_total + pgqt_transpile_cache_misses_total)

# Active connections
pgqt_connections_active
```
