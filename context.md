# Code Context

## Files Retrieved
1. `src/handler/mod.rs` - Main PostgreSQL wire protocol handler, defines `SqliteHandler` and its implementation of `ExtendedQueryHandler`.
2. `src/handler/transaction.rs` - Contains logic for identifying, parsing, and executing PostgreSQL transaction control commands (BEGIN, COMMIT, ROLLBACK, SAVEPOINT).
3. `src/handler/query.rs` (lines 390-440) - Handles the execution of individual queries, including the early detection and processing of transaction control statements.

## Key Code

**`src/handler/transaction.rs` - `execute_transaction_command` function:**
This function is responsible for executing `BEGIN` and `COMMIT` against the SQLite connection.

```rust
pub fn execute_transaction_command(
    cmd: TransactionCommand,
    session: &mut SessionContext,
    conn: &Connection,
) -> Result<Vec<Response>> {
    match cmd {
        TransactionCommand::Begin => {
            if session.transaction_status != TransactionStatus::Idle {
                // If already in a transaction, just acknowledge
                return Ok(vec![Response::TransactionStart(Tag::new("BEGIN"))]);
            }
            // Execute SQLite BEGIN
            conn.execute("BEGIN", [])?;
            session.transaction_status = TransactionStatus::InTransaction;
            Ok(vec![Response::TransactionStart(Tag::new("BEGIN"))])
        }

        TransactionCommand::Commit => {
            if session.transaction_status == TransactionStatus::Idle {
                // If no active transaction, just acknowledge
                return Ok(vec![Response::TransactionEnd(Tag::new("COMMIT"))]);
            }
            // Execute SQLite COMMIT
            conn.execute("COMMIT", [])?;
            session.transaction_status = TransactionStatus::Idle;
            session.savepoints.clear();
            Ok(vec![Response::TransactionEnd(Tag::new("COMMIT"))])
        }
        // ... (Rollback and Savepoint handling omitted)
    }
}
```

**`src/handler/query.rs` - `execute_single_query_params` (partial, around line 417):**
This snippet shows the early interception of transaction control statements before full transpilation.

```rust
        // ... (other special handling)

        if crate::handler::transaction::is_transaction_control(original_sql) {
            let mut session_clone = {
                let session_ref = self.sessions().get(&client_id).unwrap_or_else(|| {
                    self.sessions().insert(client_id, SessionContext::new("postgres".to_string()));
                    self.sessions().get(&client_id).unwrap()
                });
                session_ref.clone()
            };

            if let Some(cmd) = crate::handler::transaction::parse_transaction_command(original_sql) {
                let result = {
                    let conn_guard = self.conn().lock().unwrap();
                    crate::handler::transaction::execute_transaction_command(
                        cmd,
                        &mut session_clone,
                        &conn_guard,
                    )
                };

                self.sessions().insert(client_id, session_clone);
                return result; // Short-circuits further processing
            }
        }

        // ... (rest of query processing, including transpilation for non-transaction statements)
```

## Architecture

The `SqliteHandler` (in `src/handler/mod.rs`) acts as the main entry point for PostgreSQL client interactions, implementing the `pgwire::api::query::ExtendedQueryHandler` trait. When a query is received, it delegates to `execute_query_params` (in `src/handler/query.rs`).

The core transaction logic is centralized in `src/handler/transaction.rs`. Upon receiving a query, `execute_single_query_params` first checks if it's a transaction control statement (`BEGIN`, `COMMIT`, etc.) using `is_transaction_control`. If it is, the command is parsed, a clone of the `SessionContext` is created, and `execute_transaction_command` is called directly against the `rusqlite::Connection`. This allows `BEGIN`/`COMMIT` to bypass the heavy SQL transpilation step. The updated `SessionContext` is then stored, and the appropriate wire protocol response (`TransactionStart`/`TransactionEnd`) is returned.

For non-transaction control statements (`INSERT`, `UPDATE`), they proceed to the full transpilation pipeline (`crate::transpiler::transpile_with_context`) before execution against SQLite.

All interactions with the underlying `rusqlite::Connection` are protected by a `Mutex` (`conn.lock().unwrap()`), ensuring serialized access per session.

## Start Here

`src/handler/query.rs` specifically the `execute_single_query_params` function (around line 390) is the best place to start. This function orchestrates the initial processing of incoming SQL statements, including the critical early detection and handling of transaction control commands, and the subsequent dispatch to the transpilation pipeline for other queries.

# Project Context

(This section is automatically included from `AGENTS.md` and provides general project information for all agents.)
