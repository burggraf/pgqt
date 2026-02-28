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
| **Window Functions** | ⚠️ | Medium | SQLite support is more limited than PG. Basic `OVER`, `PARTITION BY`, `RANK` work, but advanced frames might need polyfills. |
| **Triggers** | ✅ | Low | Supported by SQLite, though syntax varies slightly and might need transpilation. |

## Data Types

| Type Category | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **Primitive Types** | ✅ | - | Mapped: INT -> INTEGER, TEXT -> TEXT, BYTEA -> BLOB, etc. |
| **JSON / JSONB** | ✅ | Low | SQLite has excellent JSON support (JSON1 extension). JSONB (binary) is now standard in newer SQLite versions. |
| **UUID** | ✅ | Low | Stored as TEXT or BLOB(16). |
| **Arrays** | ⚠️ | Medium | Emulated via JSON strings in SQLite. Needs transpilation for array operators (`&&`, `@>`). |
| **Enums** | ✅ | Low | Emulated via TEXT with CHECK constraints in SQLite. |
| **Ranges** | ❌ | Medium | Could be emulated via two columns (start, end) or JSON. |
| **Geometric Types** | ❌ | Medium | Possible via SpatiaLite extension or custom BLOB formats. |
| **Full-Text Search (TSVECTOR)** | ✅ | Medium | Types `TSVECTOR` and `TSQUERY` are mapped to TEXT with full FTS function emulation: to_tsvector, to_tsquery, plainto_tsquery, phraseto_tsquery, websearch_to_tsquery, ts_rank, ts_headline, setweight, strip. Operators: @@, &, |, !, <->, ||. See [docs/FTS.md](./FTS.md) for details. |

## PostgreSQL Specific Syntax

| Syntax / Clause | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **`INSERT ... ON CONFLICT` (Upsert)** | ✅ | Low | SQLite supports `ON CONFLICT` syntax. |
| **`RETURNING` Clause** | ✅ | Medium | SQLite 3.35.0+ supports `RETURNING`. Proxy handles older versions via `last_insert_rowid()`. |
| **`DISTINCT ON (...)`** | ⚠️ | Medium | Phase 3 Roadmap: Polyfill using window functions `ROW_NUMBER()`. |
| **`LATERAL` Joins** | ❌ | High | SQLite does not support lateral joins. Very difficult to polyfill without complex query restructuring. |
| **Postgres Casting (`::type`)** | ✅ | Low | Transpiler converts `x::int` to `CAST(x AS INTEGER)`. |
| **Operator Shorthands (`~~`, `!~`)** | ✅ | Low | Transpiler converts `~~` to `LIKE` and `!~` to `NOT REGEXP`. |

## Connectivity & Protocol

| Feature | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **Wire Protocol v3.0** | ✅ | - | Handled by `pgwire` crate. |
| **Simple Query Protocol** | ✅ | - | Fully supported. |
| **Extended Query Protocol** | ✅ | Medium | Support for `Parse`, `Bind`, `Describe`, `Execute` (Prepared Statements). |
| **SSL/TLS Connections** | ✅ | Low | Supported via `rustls` or `native-tls`. |
| **Copy Command** | ⚠️ | Medium | Basic `COPY FROM STDIN` works; advanced options (binary, encoding) need work. |

## Advanced & Administrative

| Feature | Support | Difficulty | Comments / Strategy |
| :--- | :---: | :---: | :--- |
| **System Catalogs (`pg_catalog`)** | ⚠️ | Medium | Essential tables like `pg_class`, `pg_type`, `pg_attribute` are partially emulated for ORM support. |
| **Shadow Catalog** | ✅ | - | Unique feature: `__pg_meta__` table preserves original PG types for reversibility. |
| **Row-Level Security (RLS)** | ✅ | Medium | Implemented via AST injection. Supports CREATE POLICY, ALTER TABLE ENABLE/DISABLE RLS, PERMISSIVE (OR) and RESTRICTIVE (AND) policies, USING and WITH CHECK clauses. See [docs/RLS.md](./RLS.md). |
| **Stored Procedures (PL/pgSQL)** | ❌ | High | Phase 3 Roadmap: Considering a Lua-based runtime to emulate procedural blocks. |
| **Logical Replication** | ❌ | High | Not applicable to single-file SQLite databases. |
| **Users & Permissions (RBAC)** | ✅ | Medium | Implemented via custom auth tables (`__pg_users__`, `__pg_roles__`, `__pg_permissions__`) with AST-based permission checks. Supports CREATE/ALTER/DROP USER, GRANT/REVOKE, and role-based access control for SELECT/INSERT/UPDATE/DELETE operations. |
| **Vector Search** | ✅ | Medium | pgvector-compatible vector search implemented in Rust. Supports VECTOR type, distance functions (L2, cosine, inner product, L1), operators (<->, <=>, <#>, <+>), and utility functions. See [docs/VECTOR.md](./VECTOR.md). |

## Key for Difficulty
- **Low**: Standard SQL or direct SQLite equivalent exists.
- **Medium**: Requires AST manipulation or lightweight polyfills.
- **High**: Fundamental architectural differences; requires significant engineering or runtime emulation.
