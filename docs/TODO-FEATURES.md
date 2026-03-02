# PostgreSQL Feature Compatibility Matrix (PGlite Proxy)

This document tracks PostgreSQL features and their current support status in PGlite Proxy. It serves as a roadmap for future development and a guide for users transitioning from PostgreSQL to SQLite via this proxy.

## Core Database Features

| Feature | Support | Difficulty to Implement | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **ACID Compliance** | ✅ | - | SQLite provides atomic, consistent, isolated, and durable transactions natively. |
| **Multi-Version Concurrency Control (MVCC)** | ❌ | High | SQLite uses a single-writer model. Implementing full MVCC would require a custom storage engine or complex snapshotting. |
| **SQL Transpilation** | ✅ | Medium | Currently using `pg_query` (PostgreSQL 17 parser) to rewrite queries for SQLite compatibility. |
| **Schemas (Namespaces)** | ✅ | Medium | Implemented using SQLite ATTACH DATABASE. Each non-public schema maps to a separate SQLite file. Supports CREATE SCHEMA, DROP SCHEMA, search_path, and pg_namespace catalog. |
| **Savepoints / Nested Transactions** | ✅ | Low | SQLite supports `SAVEPOINT`, `RELEASE`, and `ROLLBACK TO`. |
| **Foreign Keys** | ✅ | Low | Supported by SQLite; needs to ensure `PRAGMA foreign_keys = ON` is set. |
| **Check Constraints** | ✅ | Low | Natively supported by SQLite. |
| **Views** | ✅ | Low | Natively supported by SQLite. |
| **Common Table Expressions (CTE)** | ✅ | Low | Natively supported by SQLite (including Recursive CTEs). |
| **Window Functions** | ✅ | Medium | Full support for all window functions: row_number(), rank(), dense_rank(), percent_rank(), cume_dist(), ntile(), lag(), lead(), first_value(), last_value(), nth_value(). Aggregate functions as window functions (sum, avg, count, min, max). Frame specifications: ROWS, RANGE, GROUPS with BETWEEN, UNBOUNDED PRECEDING/FOLLOWING, CURRENT ROW, offset PRECEDING/FOLLOWING. See [docs/WINDOW.md](./WINDOW.md) for details. |
| **Triggers** | ✅ | Low | Supported by SQLite, though syntax varies slightly and might need transpilation. |

## Data Types

| Type Category | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **Primitive Types** | ✅ | - | Mapped: INT -> INTEGER, TEXT -> TEXT, BYTEA -> BLOB, etc. |
| **JSON / JSONB** | ✅ | Low | SQLite has excellent JSON support (JSON1 extension). JSONB (binary) is now standard in newer SQLite versions. |
| **UUID** | ✅ | Low | Stored as TEXT or BLOB(16). |
| **Arrays** | ✅ | Medium | Full array support via JSON strings in SQLite. Operators: && (overlap), @> (contains), <@ (contained by). Functions: array_append, array_prepend, array_cat, array_remove, array_replace, array_length, array_dims, array_ndims, cardinality, array_position, array_positions, array_to_string, string_to_array, array_fill, trim_array. See [docs/ARRAYS.md](./ARRAYS.md) for details. |
| **Enums** | ✅ | Low | Emulated via TEXT with CHECK constraints in SQLite. |
| **Ranges** | ✅ | Medium | Emulated via TEXT with PostgreSQL canonicalization and operator support. Supported types: int4range, int8range, numrange, tsrange, tstzrange, daterange. See [docs/RANGE.md](./RANGE.md). |
| **Geometric Types** | ✅ | Medium | Implemented using TEXT storage with PostgreSQL canonical string formats. Supported types: point, box, circle, line, lseg, path, polygon. Operators: && (overlaps), @> (contains), <@ (contained in), << (left), >> (right), <<\| (below), \|>> (above), <-> (distance), ?\|, ?-, ?\|\|, ?-\|. See [docs/GEO.md](./GEO.md). |
| **Full-Text Search (TSVECTOR)** | ✅ | Medium | Types `TSVECTOR` and `TSQUERY` are mapped to TEXT with full FTS function emulation: to_tsvector, to_tsquery, plainto_tsquery, phraseto_tsquery, websearch_to_tsquery, ts_rank, ts_headline, setweight, strip. Operators: @@, &, |, !, <->, ||. See [docs/FTS.md](./FTS.md) for details. |

## PostgreSQL Specific Syntax

| Syntax / Clause | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **`INSERT ... ON CONFLICT` (Upsert)** | ✅ | Low | SQLite supports `ON CONFLICT` syntax. |
| **`RETURNING` Clause** | ✅ | Medium | SQLite 3.35.0+ supports `RETURNING`. Proxy handles older versions via `last_insert_rowid()`. |
| **`DISTINCT ON (...)`** | ✅ | Medium | Polyfilled using ROW_NUMBER() window function. See [docs/DISTINCT_ON.md](./DISTINCT_ON.md) for details. |
| **`LATERAL` Joins** | ⚠️ | High | Supported for table-valued functions (like `jsonb_each`). Not supported for subqueries. See [docs/LATERAL.md](./LATERAL.md). |
| **Postgres Casting (`::type`)** | ✅ | Low | Transpiler converts `x::int` to `CAST(x AS INTEGER)`. |
| **Operator Shorthands (`~~`, `!~`)** | ✅ | Low | Transpiler converts `~~` to `LIKE` and `!~` to `NOT REGEXP`. |

## Connectivity & Protocol

| Feature | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **Wire Protocol v3.0** | ✅ | - | Handled by `pgwire` crate. |
| **Simple Query Protocol** | ✅ | - | Fully supported. |
| **Extended Query Protocol** | ✅ | Medium | Support for `Parse`, `Bind`, `Describe`, `Execute` (Prepared Statements). |
| **SSL/TLS Connections** | ✅ | Low | Supported via `rustls` or `native-tls`. |
| **Copy Command** | ✅ | Medium | Full support for `COPY FROM STDIN` and `COPY TO STDOUT` in TEXT, CSV, and BINARY formats. See [docs/COPY.md](./COPY.md). |

## Advanced & Administrative

| Feature | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **System Catalogs (`pg_catalog`)** | ✅ | Medium | Full implementation of pg_class, pg_type, pg_attribute, pg_index, pg_constraint, pg_tables, pg_views, pg_indexes, and more for complete ORM compatibility. |
| **Shadow Catalog** | ✅ | - | Unique feature: `__pg_meta__` table preserves original PG types for reversibility. |
| **Row-Level Security (RLS)** | ✅ | Medium | Implemented via AST injection. Supports CREATE POLICY, ALTER TABLE ENABLE/DISABLE RLS, PERMISSIVE (OR) and RESTRICTIVE (AND) policies, USING and WITH CHECK clauses. See [docs/RLS.md](./RLS.md). |
| **Stored Procedures (PL/pgSQL)** | ❌ | High | Phase 3 Roadmap: Considering a Lua-based runtime to emulate procedural blocks. |
| **User-Defined Functions (SQL)** | ✅ | Medium | Phase 1 Complete: Full support for CREATE FUNCTION with SQL language. Supports IN/OUT/INOUT parameters, RETURNS SCALAR/SETOF/TABLE/VOID, function attributes (STRICT, IMMUTABLE, STABLE, VOLATILE, SECURITY DEFINER, PARALLEL). See [docs/FUNCTIONS.md](./FUNCTIONS.md). |
| **User-Defined Functions (PL/pgSQL)** | ⏳ | High | Phase 2 Roadmap: PL/pgSQL procedural language via Lua runtime. |
| **Logical Replication** | ❌ | High | Not applicable to single-file SQLite databases. |
| **Users & Permissions (RBAC)** | ✅ | Medium | Implemented via custom auth tables (`__pg_users__`, `__pg_roles__`, `__pg_permissions__`) with AST-based permission checks. Supports CREATE/ALTER/DROP USER, GRANT/REVOKE, and role-based access control for SELECT/INSERT/UPDATE/DELETE operations. |
| **Vector Search** | ✅ | Medium | pgvector-compatible vector search implemented in Rust. Supports VECTOR type, distance functions (L2, cosine, inner product, L1), operators (<->, <=>, <#>, <+>), and utility functions. See [docs/VECTOR.md](./VECTOR.md). |

## Key for Difficulty
- **Low**: Standard SQL or direct SQLite equivalent exists.
- **Medium**: Requires AST manipulation or lightweight polyfills.
- **High**: Fundamental architectural differences; requires significant engineering or runtime emulation.
