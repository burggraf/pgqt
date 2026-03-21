# PGQT Feature Roadmap

This document tracks planned features, work-in-progress, and future enhancements for PGQT.

## Completed Features

### Core Functionality
- [x] PostgreSQL wire protocol (v3) support
- [x] SQL transpilation (PostgreSQL → SQLite)
- [x] Type preservation via shadow catalog
- [x] Schema/namespace support (ATTACH DATABASE)
- [x] Multi-port configuration

### Data Types
- [x] All standard PostgreSQL types (serial, integer, float, text, boolean, date/time, JSON, etc.)
- [x] Array types with operators (`&&`, `@>`, `<@`)
- [x] Range types (int4range, daterange, etc.)
- [x] Geometric types (point, box, circle, etc.)
- [x] Vector types (pgvector-compatible)
- [x] Enum types (CREATE TYPE ... AS ENUM)
- [x] Full-text search (tsvector, tsquery)

### SQL Features
- [x] Window functions (row_number, rank, lag, lead, etc.)
- [x] DISTINCT ON
- [x] LATERAL joins (table-valued functions)
- [x] Common Table Expressions (CTEs)
- [x] RETURNING clause
- [x] ON CONFLICT (upsert)
- [x] COPY FROM/TO

### Security & Access Control
- [x] Role-Based Access Control (RBAC)
- [x] Row-Level Security (RLS)
- [x] GRANT/REVOKE
- [x] Password and trust authentication modes

### Programmability
- [x] PL/pgSQL stored procedures (via Lua runtime)
- [x] Triggers (BEFORE/AFTER, FOR EACH ROW)
- [x] User-defined functions (SQL language)
- [x] Built-in functions (math, string, date/time, regex, etc.)

### Session & Configuration
- [x] SET/SHOW commands
- [x] set_config() and current_setting()
- [x] Session-level parameter persistence

### Infrastructure
- [x] Connection pooling
- [x] TLS/SSL support
- [x] Unix socket support
- [x] Prometheus metrics
- [x] Query result caching
- [x] Transpile caching

---

## Planned Features

### Enum Enhancements
- [ ] `ALTER TYPE ... ADD VALUE` - Add values to existing enums
- [ ] `enum_range()`, `enum_first()`, `enum_last()` functions
- [ ] Schema-qualified enum types

### Session Management
- [ ] `ALTER ROLE ... SET` - Role-specific defaults
- [ ] Persistent settings across sessions
- [ ] `ALTER SYSTEM` command support

### Advanced SQL
- [ ] `MERGE` statement (SQL:2003 upsert)
- [ ] `TABLESAMPLE` clause
- [ ] Recursive CTEs with cycle detection
- [ ] `GROUPING SETS`, `CUBE`, `ROLLUP`

### Performance
- [ ] Query plan analysis/EXPLAIN
- [ ] Index-only scans
- [ ] Parallel query execution

### Replication & Backup
- [ ] Logical replication protocol
- [ ] Point-in-time recovery
- [ ] Online backup support

### Extensions
- [ ] Extension loading mechanism
- [ ] `CREATE EXTENSION` support
- [ ] Extension API for custom types/functions

---

## Known Limitations

These are architectural limitations due to SQLite's design:

| Feature | Limitation | Workaround |
|---------|------------|------------|
| Concurrency | Single-writer model | Use connection pooling, WAL mode |
| MVCC | No multi-version concurrency control | Short transactions |
| Extensions | No PostGIS, pgvector native | Use SpatiaLite, sqlite-vec |
| Indexes | No GIN, GiST indexes | Use FTS5, R-Tree |
| Replication | No built-in replication | Application-level sync |

---

## Feature Request Process

To request a new feature:

1. Check if it's already in Planned Features above
2. Check if it's a Known Limitation
3. Open an issue with:
   - Use case description
   - PostgreSQL compatibility requirements
   - Expected behavior vs current behavior

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines on implementing new features.