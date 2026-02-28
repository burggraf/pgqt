# PGlite Proxy Implementation Plan: Phase 1 (Core Proxy)

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Build a basic PostgreSQL-to-SQLite proxy that accepts standard `psql` connections and executes simple queries against a local SQLite file.

**Architecture:** A Tokio-based async TCP server using `pgwire` to handle the Postgres v3 protocol and `rusqlite` to interface with SQLite.

**Tech Stack:** Rust, Tokio, pgwire, rusqlite, libpg_query (parsing).

---

### Task 1: Project Setup & Dependencies

**Files:**
- Modify: `Cargo.toml`
- Test: `cargo check`

**Step 1: Update Cargo.toml with core dependencies**

```toml
[package]
name = "pglite-proxy"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.0", features = ["full"] }
pgwire = "0.20"
rusqlite = { version = "0.31", features = ["bundled"] }
libpg_query = "16.0"
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
futures = "0.3"
```

**Step 2: Run cargo check to verify dependencies**

Run: `cargo check`
Expected: PASS (downloads and compiles metadata)

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: initial project dependencies"
```

---

### Task 2: Basic TCP Server & Handshake

**Files:**
- Modify: `src/main.rs`
- Test: `psql -h localhost -p 5432 -U postgres`

**Step 1: Implement the basic Tokio server loop**

```rust
use std::sync::Arc;
use tokio::net::TcpListener;
use pgwire::api::auth::noop::NoopStartupHandler;
use pgwire::api::query::SimpleQueryHandler;
use pgwire::api::results::{FieldInfo, Response, QueryResponse};
use pgwire::api::{ClientInfo, Type};
use pgwire::tokio::process_socket;
use anyhow::Result;

struct SqliteHandler;

#[async_trait::async_trait]
impl SimpleQueryHandler for SqliteHandler {
    async fn do_query<'a, C>(&self, _client: &C, _query: &'a str) -> Vec<Response>
    where
        C: ClientInfo,
    {
        // Placeholder for now
        vec![Response::EmptyQuery]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:5432").await?;
    let startup_handler = Arc::new(NoopStartupHandler);
    let query_handler = Arc::new(SqliteHandler);

    loop {
        let (incoming_socket, _) = listener.accept().await?;
        let startup_handler = startup_handler.clone();
        let query_handler = query_handler.clone();

        tokio::spawn(async move {
            process_socket(incoming_socket, None, startup_handler, query_handler, query_handler).await;
        });
    }
}
```

**Step 2: Run server and connect via psql**

Run: `cargo run` (in one terminal)
Run: `psql -h 127.0.0.1 -p 5432 -U postgres` (in another)
Expected: `psql` connects successfully (even if it does nothing yet).

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: basic tcp server with pgwire handshake"
```

---

### Task 3: SQLite Backend Integration

**Files:**
- Modify: `src/main.rs`
- Test: `SELECT 1` in psql

**Step 1: Integrate rusqlite into the QueryHandler**

```rust
use rusqlite::Connection;
use std::sync::Mutex;

struct SqliteHandler {
    conn: Mutex<Connection>,
}

impl SqliteHandler {
    fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

#[async_trait::async_trait]
impl SimpleQueryHandler for SqliteHandler {
    async fn do_query<'a, C>(&self, _client: &C, query: &'a str) -> Vec<Response>
    where
        C: ClientInfo,
    {
        let conn = self.conn.lock().unwrap();
        match conn.prepare(query) {
            Ok(mut stmt) => {
                let col_count = stmt.column_count();
                let field_infos: Vec<FieldInfo> = (0..col_count)
                    .map(|i| FieldInfo::new(stmt.column_name(i).unwrap().to_string(), None, None, Type::VARCHAR))
                    .collect();

                let mut rows = Vec::new();
                let mut results = stmt.query([]).unwrap();
                while let Some(row) = results.next().unwrap() {
                    let mut data = pgwire::api::results::DataRow::new();
                    for i in 0..col_count {
                        let val: String = row.get::<_, String>(i).unwrap_or_else(|_| "null".into());
                        data.push_field(Some(val.into_bytes()));
                    }
                    rows.push(data);
                }
                vec![Response::Query(QueryResponse::new(field_infos, rows))]
            }
            Err(e) => vec![Response::Error(Box::new(e))],
        }
    }
}
```

**Step 2: Run server and execute query**

Run: `cargo run`
Run: `psql -h 127.0.0.1 -p 5432 -U postgres -c "SELECT 1 as id, 'hello' as name;"`
Expected:
```
 id | name
----+-------
 1  | hello
(1 row)
```

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: execute simple queries against sqlite"
```

---

### Task 4: Basic Parsing & Transpilation (Phase 1)

**Files:**
- Create: `src/transpiler.rs`
- Modify: `src/main.rs`
- Test: `SELECT now()` in psql

**Step 1: Implement basic now() -> datetime('now') rewrite**

```rust
// src/transpiler.rs
pub fn transpile(sql: &str) -> String {
    sql.to_lowercase().replace("now()", "datetime('now')")
}
```

**Step 2: Update handler to use transpiler**

```rust
// src/main.rs
let rewritten_sql = transpiler::transpile(query);
// ... pass rewritten_sql to conn.prepare
```

**Step 3: Verify rewrite works**

Run: `psql -c "SELECT now();"`
Expected: SQLite-style timestamp string.

**Step 4: Commit**

```bash
git add src/transpiler.rs src/main.rs
git commit -m "feat: simple sql string transpilation for now()"
```
