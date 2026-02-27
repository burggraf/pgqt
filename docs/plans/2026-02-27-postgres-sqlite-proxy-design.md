# Design Document: PostgreSQL-to-SQLite Proxy (PGlite Proxy)

**Date:** 2026-02-27
**Status:** Approved
**Implementation Language:** Rust

## 1. Vision & Goals
Build a PostgreSQL-compatible shell for SQLite that allows users to treat a local SQLite file as a full-featured Postgres database.

- **Parity:** Support standard Postgres SQL, functions, and PL/pgSQL.
- **Strictness:** Enforce Postgres-style data types (e.g., `VARCHAR(10)`) at the proxy level.
- **Reversibility:** Store original metadata so the database can be migrated back to a real Postgres instance with 100% type fidelity.
- **Portability:** A single Rust binary that manages one or more SQLite files.

---

## 2. Core Architecture
The proxy acts as a **Stateful Middleware Server** between the client and SQLite.

### A. Network Layer (Postgres Wire Protocol)
- **Library:** `pgwire` (Rust) for handling the v3 protocol.
- **Multithreading:** Tokio-based async tasks per connection.
- **Auth:** Internal role-based authentication (mapped to Postgres users/roles).
- **SSL:** Optional TLS termination via Rustls.

### B. Session & Schema Management
- **One-to-Many Mapping:** Postgres "Schemas" (e.g., `public`, `auth`) are mapped to separate SQLite files via `ATTACH DATABASE`.
- **Search Path:** Proxy-level state tracks `search_path` and automatically prefixes table names in the AST.
- **Authorizer API:** Uses the SQLite `set_authorizer` C-API to intercept every internal SQL operation and verify permissions against the proxy's RBAC table.

---

## 3. SQL Engine & Transpilation

### A. AST-Based Rewriting
- **Library:** `libpg_query` (official Postgres parser) for high-fidelity AST generation.
- **Transpiler:** A custom Rust `Translator` walks the AST and converts Postgres-specific nodes into SQLite-compatible commands.
- **Polyfills:** 
  - `SERIAL` → `INTEGER PRIMARY KEY AUTOINCREMENT`.
  - `DISTINCT ON` → Rewritten using window functions (`ROW_NUMBER()`).
  - `now()` → `datetime('now')`.

### B. The Shadow Catalog (`__pg_meta__`)
- **Metadata Storage:** A hidden SQLite database (`__pg_meta__.db`) stores the original Postgres DDL and exact type definitions.
- **Type Enforcement:** The proxy validates every `INSERT/UPDATE` against the registry in `__pg_meta__` before execution.
- **Constraints:** Proxy-level enforcement of `VARCHAR(n)` length and complex Postgres `CHECK` constraints that SQLite cannot natively handle.

---

## 4. Procedural Engine (PL/pgSQL)

### A. Embedded Lua Runtime
- **Library:** `mlua` for embedding Lua 5.4 in the Rust proxy.
- **Transpilation:** Postgres `CREATE FUNCTION ... LANGUAGE plpgsql` bodies are parsed into ASTs and rewritten into equivalent Lua logic.
- **SPI Bridge:** A custom Lua `pl.execute()` function allows the script to call back into the proxy to run SQL against the SQLite backend.

### B. Trigger Emulation
- **Mechanism:** Standard SQLite `TRIGGER`s are created that call a custom "proxy-hook" UDF.
- **Row Variables:** The hook passes `OLD` and `NEW` row data into the corresponding Lua function for execution.

---

## 5. Concurrency & Performance
- **Isolation:** SQLite **Write-Ahead Log (WAL)** mode enabled by default.
- **Locking:** Proxy-side "Busy" retry handler with configurable backoff to simulate Postgres-like concurrency for multiple writers.
- **Connection Pooling:** Built-in proxy-level pooling to reuse SQLite file handles across sessions.

---

## 6. Testing & Verification
- **Compatibility Suite:** Run the standard `psql` regression tests and select subsets that should pass.
- **ORM Tests:** Verify compatibility with `Prisma`, `TypeORM`, and `Drizzle`.
- **Migration Test:** Round-trip test: Postgres → PGlite Proxy → Postgres (ensure schema and data are identical).
