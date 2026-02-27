# Feasibility Report: PostgreSQL Wire-Compatible Proxy for SQLite

## 1. Executive Summary
Building a PostgreSQL-compatible shell for SQLite—effectively a **Postgres-to-SQLite Proxy**—is a highly ambitious but technically feasible project. Several existing tools solve portions of this puzzle (notably wire protocol handling and basic type mapping), but no single solution currently provides deep SQL parity, PL/pgSQL interpretation, and reversible type metadata.

*   **Wire Protocol Parity:** **High Feasibility** (Existing libraries for Go and Rust).
*   **SQL Dialect Mapping:** **Medium-High Feasibility** (Requires robust AST-based transpilation via `libpg_query`).
*   **Advanced Features (RLS, Schemas):** **Medium Feasibility** (Requires a stateful proxy layer to inject logic).
*   **PL/pgSQL Support:** **Low-Medium Feasibility** (Requires building a custom procedural interpreter).

---

## 2. Proposed Architecture: The Stateful Proxy Model
The system should operate as a **Stateful Middleware Server** positioned between the PostgreSQL client (e.g., `psql`, `pgAdmin`, or an ORM) and the SQLite database engine.

### Core Components:
1.  **Frontend (Postgres Wire Protocol):** A TCP server implementing the Postgres v3 protocol. It handles SSL, authentication, and the stateful exchange of query/response messages.
2.  **State Management:** Tracks per-connection state, including `search_path`, `current_user`, active transactions, and session variables.
3.  **Transpilation Layer:**
    *   **Parser:** Uses `libpg_query` (the official Postgres parser) to generate a high-fidelity Abstract Syntax Tree (AST).
    *   **Mapper:** Traverses the AST to convert Postgres-specific syntax into SQLite equivalents.
4.  **Shadow Catalog (`__pg_meta__`):** A hidden SQLite schema used to store the "original" Postgres DDL and data types for future migration/reversibility.
5.  **Backend (SQLite Engine):** Executes the rewritten queries against a local `.sqlite` file or an in-memory database.

---

## 3. Deep Component Analysis

### A. SQL Transpilation & Dialect Mapping
PostgreSQL and SQLite have significant syntax differences that require more than simple string replacement.
*   **Recommendation:** Use [**libpg_query**](https://github.com/pganalyze/libpg_query). It extracts the actual parser from the PostgreSQL source code, ensuring 100% compatibility with incoming Postgres queries.
*   **Mapping Examples:**
    *   `SERIAL` → `INTEGER PRIMARY KEY AUTOINCREMENT`.
    *   `RETURNING` → Supported in SQLite 3.35.0+, but requires careful version checking.
    *   `DISTINCT ON` → Must be rewritten using window functions or subqueries in SQLite.

### B. Data Type Persistence & Bi-directional Mapping
To support migrating back to a "real" Postgres database, the proxy must "remember" the original types.
*   **Implementation:** When a user runs `CREATE TABLE users (id SERIAL, bio TEXT)`, the proxy:
    1.  Records the original types in a metadata table.
    2.  Translates `VARCHAR(n)` to SQLite `TEXT` but injects a `CHECK(length(col) <= n)` constraint.
    3.  Stores the original Postgres DDL so a `pg_dump`-like tool can reconstruct the exact Postgres schema later.

### C. PL/pgSQL & Procedural Logic
SQLite lacks a procedural language, which is the most significant hurdle.
*   **Approach 1 (Triggers):** Simple logic (e.g., updating a `modified_at` timestamp) can be transpiled into SQLite `TRIGGER`s.
*   **Approach 2 (Interpreter):** For full functions and `DO` blocks, the proxy must implement a custom interpreter. This involves parsing the function body (via `libpg_query`) and executing it as a series of SQL commands against the SQLite backend, registered as User-Defined Functions (UDFs).

---

## 4. Advanced Feature Emulation

### Row-Level Security (RLS)
SQLite has no native RLS. Emulation requires shifting security logic to the proxy layer:
*   **Read Security:** The proxy renames base tables (e.g., `_users_data`) and creates a VIEW (`users`) with a dynamic filter: `WHERE owner_id = current_proxy_user()`.
*   **Write Security:** Use `INSTEAD OF` triggers on the VIEW to validate `INSERT/UPDATE` operations, throwing `RAISE(ABORT, 'Access Denied')` if the current user lacks permission.

### Schemas and Namespacing
PostgreSQL schemas (e.g., `public`, `auth`) map naturally to SQLite's **Attached Databases**.
*   The proxy must track the `search_path` and automatically prefix table names (e.g., rewriting `SELECT * FROM users` to `SELECT * FROM auth.users`).

### Full-Text Search (FTS)
Postgres uses `tsvector` and `@@`. SQLite uses **FTS5**.
*   The proxy must detect FTS queries and rewrite them to use SQLite's `MATCH` operator against virtual tables, while keeping the FTS index synchronized via hidden triggers.

### Roles and RBAC
The proxy should use the **SQLite Authorizer API** (`sqlite3_set_authorizer`). This allows the proxy to intercept every read/write attempt at the column level and enforce Postgres-style permissions based on the authenticated role.

---

## 5. Technology Stack Recommendation

### **Winner: Rust**
*   **Performance:** High-performance string manipulation and AST walking are essential for low-latency proxying.
*   **Ecosystem:** [**pgwire**](https://github.com/sunng87/pgwire) provides a production-ready Postgres wire protocol implementation.
*   **Safety:** Memory safety is critical for a network-facing proxy server.
*   **C-Bindings:** Excellent support for `libpg_query` and the SQLite C-API (required for the Authorizer).

### **Alternative: Go**
*   **Pros:** Fast development cycle, excellent concurrency model for handling many connections.
*   **Libraries:** [**pgproto3**](https://github.com/jackc/pgproto3) and `pg_query_go`.

---

## 6. Comparison of Existing Tools

| Tool | Focus | Relevance to this Project |
| :--- | :--- | :--- |
| **libSQL (sqld)** | Distributed SQLite | Handles the wire protocol; lacks deep Postgres syntax parity. |
| **pgsqlite** | Protocol Adapter | Good reference for mapping `pg_catalog` system views. |
| **PGlite** | WASM Postgres | It is the full Postgres engine; useful if you don't need "native" SQLite files. |
| **SQLGlot** | Transpilation | Excellent for understanding the semantic differences between dialects. |

---

## 7. Risks and Limitations
*   **Concurrency:** SQLite's single-writer model contrasts with Postgres's MVCC. High-concurrency Postgres apps may experience locking issues.
*   **Extensions:** Features like `PostGIS` or `pgvector` require finding equivalent SQLite extensions (like `SpatiaLite` or `sqlite-vec`).
*   **Performance Overhead:** Every query incurs the cost of parsing, AST walking, and rewriting.

---

## 8. Proposed Roadmap
1.  **Phase 1 (The Pipe):** Basic TCP proxy that accepts `psql` connections and passes raw SQL to SQLite.
2.  **Phase 2 (The Parser):** Integrate `libpg_query` and implement basic `SELECT/INSERT/UPDATE` transpilation.
3.  **Phase 3 (The Shadow Catalog):** Implement metadata tracking and `pg_catalog` views so GUI tools like `DBeaver` can connect.
4.  **Phase 4 (Advanced Security):** Implement the Proxy-based RLS and Authorizer-based RBAC.
5.  **Phase 5 (The Runtime):** Develop the PL/pgSQL interpreter for procedural logic.
