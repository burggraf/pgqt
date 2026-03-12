# Phase 5: PostgreSQL-Compatible Transaction Support - Detailed Implementation Plan

## Executive Summary

This document provides a comprehensive plan for implementing real PostgreSQL-compatible transaction support in PGQT. The current implementation is a NO-OP that changes state without executing actual SQLite transactions, causing silent data integrity violations.

## Current State Analysis

### Problem Statement

**Current Architecture Issues:**
1. **Shared Connection**: All sessions share a single `Arc<Mutex<Connection>>` 
2. **NO-OP Transactions**: `BEGIN`/`COMMIT`/`ROLLBACK` only change `TransactionStatus` state without executing SQLite commands
3. **No Session Isolation**: Concurrent clients interfere with each other's transactions
4. **No Error Propagation**: Failed statements don't abort transactions per PostgreSQL semantics
5. **Missing 25P02 Error**: Transaction-aborted state not enforced

**Code Locations:**
- `src/handler/transaction.rs`: Placeholder logic that only changes state
- `src/handler/mod.rs`: Shared connection architecture
- `src/main.rs`: Session initialization with hardcoded ID (0)

### Research Findings

#### PostgreSQL Transaction Semantics (from official docs)

**Transaction States:**
- `Idle` ('I'): Not in a transaction - ReadyForQuery indicator
- `InTransaction` ('T'): Inside a valid transaction block
- `InError` ('E'): Transaction aborted due to error (25P02)

**Key Behaviors:**
1. **Error Handling**: Any error in a transaction block sets state to `InError`
2. **25P02 Enforcement**: In error state, all commands rejected until `ROLLBACK` or `ROLLBACK TO SAVEPOINT`
3. **COMMIT on Aborted**: `COMMIT` in error state issues `ROLLBACK` and returns warning
4. **Savepoints**: Allow partial rollback within transaction; rolling back to savepoint clears error state

**Transaction Isolation Levels:**
| PostgreSQL Level | SQLite Equivalent | Notes |
|------------------|-------------------|-------|
| READ UNCOMMITTED | READ UNCOMMITTED | Upgraded to READ COMMITTED by PG |
| READ COMMITTED | READ COMMITTED | Default for both |
| REPEATABLE READ | SERIALIZABLE | PG uses MVCC, SQLite uses snapshot |
| SERIALIZABLE | SERIALIZABLE | SQLite's default |

**Wire Protocol:**
- `ReadyForQuery` message includes transaction status byte: 'I', 'T', or 'E'
- No dedicated transaction message type - uses standard Query protocol
- Isolation levels set via SQL: `BEGIN ISOLATION LEVEL SERIALIZABLE`

#### SQLite Transaction Compatibility

**SQLite Modes:**
- `DEFERRED`: Default, acquires locks on first access
- `IMMEDIATE`: Acquires write lock immediately
- `EXCLUSIVE`: Exclusive lock, no other readers/writers

**Mapping to PostgreSQL:**
```
PostgreSQL BEGIN              → SQLite BEGIN DEFERRED
PostgreSQL BEGIN READ WRITE   → SQLite BEGIN IMMEDIATE  
PostgreSQL BEGIN READ ONLY    → SQLite BEGIN DEFERRED
PostgreSQL SET TRANSACTION    → PRAGMA query (limited support)
```

**Critical Limitation**: SQLite only supports one active writer transaction. Concurrent write transactions return `SQLITE_BUSY`. PGQT must handle this.

## Architecture Design

### Design Decision: Per-Session Connections with Connection Pool

**Rationale:**
- SQLite transactions are connection-scoped
- Each PostgreSQL session requires its own transaction state
- Connection pooling allows resource management

**Architecture:**

```
┌─────────────────────────────────────────────────────────────┐
│                    SqliteHandler                             │
├─────────────────────────────────────────────────────────────┤
│  ConnectionPool                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  pool: Arc<Mutex<Vec<rusqlite::Connection>>>        │    │
│  │  max_connections: usize                             │    │
│  │  db_path: PathBuf                                   │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  SessionManager                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  sessions: DashMap<ClientId, Session>               │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘

Session Structure:
┌─────────────────────────────────────────────────────────────┐
│  Session                                                     │
├─────────────────────────────────────────────────────────────┤
│  client_id: u32                                              │
│  authenticated_user: String                                  │
│  current_user: String                                        │
│  search_path: SearchPath                                     │
│  transaction_status: TransactionStatus                       │
│  savepoints: Vec<String>                                     │
│  conn: Option<PooledConnection>  ← CHECKED OUT FROM POOL     │
└─────────────────────────────────────────────────────────────┘
```

### Alternative Designs Considered

**Option 1: Shared Connection with Savepoint Simulation**
- Use SQLite savepoints to simulate per-session transactions
- Complex savepoint name management
- Difficult to guarantee isolation
- **REJECTED**: Too complex, error-prone

**Option 2: Database-per-Session (ATTACH)**
- Each session gets its own temp database
- Periodically merge to main database
- **REJECTED**: Complex merge logic, consistency issues

**Option 3: WAL Mode with Busy Timeout**
- Single connection, rely on WAL mode concurrency
- **REJECTED**: Doesn't provide transaction isolation per session

## Implementation Plan

### Phase 5.1: Connection Pool Infrastructure ✅ COMPLETE

**Files:** `src/connection_pool.rs` (new), `src/handler/mod.rs`

**Tasks:**

1. **Create Connection Pool Module**
   ```rust
   // src/connection_pool.rs
   pub struct ConnectionPool {
       db_path: PathBuf,
       available: Arc<Mutex<Vec<rusqlite::Connection>>>,
       in_use: Arc<Mutex<HashSet<u32>>>, // client_id -> connection tracking
       max_connections: usize,
       function_registrar: Arc<dyn Fn(&Connection) -> Result<()>>,
   }

   pub struct PooledConnection {
       conn: Option<rusqlite::Connection>,
       pool: Weak<Mutex<Vec<rusqlite::Connection>>>,
       client_id: u32,
   }

   impl ConnectionPool {
       pub fn new(db_path: &Path, max_connections: usize) -> Result<Self>;
       pub fn checkout(&self, client_id: u32) -> Result<PooledConnection>;
       pub fn checkin(&self, client_id: u32, conn: rusqlite::Connection);
   }
   ```

2. **Update SessionContext**
   ```rust
   // src/handler/mod.rs
   pub struct SessionContext {
       pub authenticated_user: String,
       pub current_user: String,
       pub search_path: SearchPath,
       pub transaction_status: TransactionStatus,
       pub savepoints: Vec<String>,
       pub client_id: u32,
       // Note: connection is stored separately in handler to avoid Clone issues
   }
   ```

3. **Update SqliteHandler**
   ```rust
   pub struct SqliteHandler {
       conn_pool: ConnectionPool,  // Replaces Arc<Mutex<Connection>>
       sessions: Arc<DashMap<u32, SessionContext>>,
       client_connections: Arc<DashMap<u32, rusqlite::Connection>>, // Active connections
       schema_manager: SchemaManager,
       copy_handler: copy::CopyHandler,
       functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>,
   }
   ```

**Verification:**
- Unit tests: Connection checkout/checkin
- Integration: Multiple clients can check out connections
- Regression: Existing tests still pass

### Phase 5.2: Per-Session Connection Management ✅ COMPLETE

**Files:** `src/handler/mod.rs`, `src/main.rs`

**Tasks:**

1. **Client ID Management**
   - Use `client.metadata().get("client_id")` or generate unique ID
   - Store in SessionContext

2. **Session Lifecycle**
   ```rust
   // In do_query, on first access:
   fn get_or_create_session(&self, client: &C) -> Result<(u32, SessionContext)> {
       let client_id = client.metadata().get("id")
           .and_then(|s| s.parse().ok())
           .unwrap_or_else(|| rand::random());
       
       // Check out connection if not already held
       if !self.client_connections.contains_key(&client_id) {
           let conn = self.conn_pool.checkout(client_id)?;
           self.client_connections.insert(client_id, conn);
       }
       
       // Get or create session context
       if let Some(session) = self.sessions.get(&client_id) {
           Ok((client_id, session.clone()))
       } else {
           let session = SessionContext::new(client_id, user);
           self.sessions.insert(client_id, session.clone());
           Ok((client_id, session))
       }
   }
   ```

3. **Connection Cleanup on Disconnect**
   - Implement `on_client_disconnect` or use Drop trait
   - Auto-rollback any active transaction
   - Return connection to pool

**Verification:**
- Test: Two clients can run transactions independently
- Test: Connection returned to pool on disconnect
- Test: Transaction rolled back on disconnect

### Phase 5.3: Real Transaction Command Implementation

**Files:** `src/handler/transaction.rs`, `src/handler/query.rs`

**Tasks:**

1. **Rewrite Transaction Handler**
   ```rust
   pub async fn handle_transaction<C>(
       sql: &str,
       client_id: u32,
       handler: &SqliteHandler,
   ) -> Option<Result<Vec<Response>>> {
       let upper = sql.trim().to_uppercase();
       let mut session = handler.sessions.get_mut(&client_id)?;
       let conn = handler.client_connections.get_mut(&client_id)?;
       
       if upper.starts_with("BEGIN") || upper.starts_with("START TRANSACTION") {
           // Parse isolation level from SQL
           let isolation = parse_isolation_level(&upper);
           let sqlite_mode = match isolation {
               IsolationLevel::Serializable => "BEGIN IMMEDIATE",
               _ => "BEGIN DEFERRED",
           };
           
           conn.execute(sqlite_mode, [])?;
           session.transaction_status = TransactionStatus::InTransaction;
           session.savepoints.clear();
           
           return Some(Ok(vec![Response::Execution(Tag::new("BEGIN"))]));
       }
       
       // ... COMMIT, ROLLBACK, SAVEPOINT similarly
   }
   ```

2. **Transaction Status Tracking**
   - Execute actual SQLite commands
   - Update SessionContext.transaction_status
   - Manage savepoints Vec

3. **ReadyForQuery Status Reporting**
   - Modify response handling to include transaction status
   - Return 'I', 'T', or 'E' in ReadyForQuery message

**PostgreSQL Command Support Matrix:**

| PostgreSQL Command | SQLite Equivalent | Implementation |
|-------------------|-------------------|----------------|
| `BEGIN` | `BEGIN DEFERRED` | ✅ Core |
| `BEGIN READ WRITE` | `BEGIN IMMEDIATE` | ✅ Core |
| `BEGIN READ ONLY` | `BEGIN DEFERRED` + pragma | ⚠️ Limited |
| `BEGIN ISOLATION LEVEL ...` | `BEGIN` (SQLite ignores) | ⚠️ Warn |
| `COMMIT` | `COMMIT` | ✅ Core |
| `END` | `COMMIT` | ✅ Core |
| `ROLLBACK` | `ROLLBACK` | ✅ Core |
| `ABORT` | `ROLLBACK` | ✅ Core |
| `SAVEPOINT name` | `SAVEPOINT name` | ✅ Core |
| `ROLLBACK TO SAVEPOINT` | `ROLLBACK TO name` | ✅ Core |
| `RELEASE SAVEPOINT` | `RELEASE name` | ✅ Core |
| `COMMIT AND CHAIN` | `COMMIT` + `BEGIN` | ⚠️ Extension |
| `ROLLBACK AND CHAIN` | `ROLLBACK` + `BEGIN` | ⚠️ Extension |

### Phase 5.4: Transaction Error State (25P02)

**Files:** `src/handler/query.rs`, `src/handler/errors.rs`

**Tasks:**

1. **Error State Detection**
   ```rust
   // In execute_query:
   match execute_sqlite_query(...) {
       Ok(result) => Ok(result),
       Err(e) => {
           if session.transaction_status == TransactionStatus::InTransaction {
               session.transaction_status = TransactionStatus::InError;
           }
           Err(e)
       }
   }
   ```

2. **25P02 Enforcement**
   ```rust
   // At start of query execution:
   if session.transaction_status == TransactionStatus::InError {
       return Err(PgError::new(
           PgErrorCode::TransactionRollback,
           "25P02",
           "current transaction is aborted, commands ignored until end of transaction block",
       ));
   }
   ```

3. **Add Missing Error Code**
   ```rust
   // In src/handler/errors.rs
   TransactionRollback,  // 40000
   TransactionIntegrityConstraintViolation,  // 40002
   SerializationFailure,  // 40001
   StatementCompletionUnknown,  // 40003
   DeadlockDetected,  // 40P01
   ```

4. **Savepoint Recovery**
   - `ROLLBACK TO SAVEPOINT` clears error state
   - Only clear if savepoint existed before error

**Error State Transitions:**
```
┌─────────┐    BEGIN     ┌─────────────────┐
│  Idle   │ ────────────→│  InTransaction  │
│   ('I') │              │     ('T')       │
└─────────┘              └─────────────────┘
                                │
                                │ SQL Error
                                ▼
                         ┌─────────────────┐
        ROLLBACK/        │    InError      │◄────── Any command
    ROLLBACK TO SP      │     ('E')       │       except ROLLBACK
         │              └─────────────────┘       rejected with 25P02
         │                       │
         └───────────────────────┘
```

### Phase 5.5: Wire Protocol Integration

**Files:** `src/main.rs`, `src/handler/mod.rs`

**Tasks:**

1. **ReadyForQuery Status**
   - The pgwire library handles ReadyForQuery automatically
   - Need to check if it reads transaction status from somewhere
   - May need to implement custom `ClientPortalStore`

2. **Error Response Enhancement**
   - Ensure SQLSTATE codes are returned correctly
   - Add transaction status to error context

**Research Required:**
- Check pgwire documentation for transaction status reporting
- May need PR to pgwire crate or fork

### Phase 5.6: Concurrency and Busy Handling

**Files:** `src/connection_pool.rs`, `src/handler/query.rs`

**Tasks:**

1. **WAL Mode Configuration**
   ```rust
   // In ConnectionPool::new
   conn.execute("PRAGMA journal_mode=WAL", [])?;
   conn.execute("PRAGMA busy_timeout=5000", [])?; // 5 second timeout
   ```

2. **Busy Error Handling**
   ```rust
   match conn.execute(...) {
       Err(rusqlite::Error::SqliteFailure(ffi::Error { code: ErrorCode::DatabaseBusy, .. }, _)) => {
           Err(PgError::new(
               PgErrorCode::SerializationFailure,  // 40001
               "40001",
               "could not serialize access due to concurrent update",
           ))
       }
       Err(e) => Err(e.into()),
       Ok(result) => Ok(result),
   }
   ```

3. **Connection Pool Tuning**
   - Default: 10 connections
   - Configurable via CLI/environment
   - Queue-based checkout with timeout

### Phase 5.7: Testing and Verification

**Files:** `tests/transaction_tests.rs` (new)

**Test Cases:**

1. **Basic Transaction Tests**
   ```rust
   #[test]
   fn test_begin_commit() {
       // BEGIN; INSERT; COMMIT;
       // Verify data persisted
   }
   
   #[test]
   fn test_begin_rollback() {
       // BEGIN; INSERT; ROLLBACK;
       // Verify data NOT persisted
   }
   ```

2. **Error State Tests**
   ```rust
   #[test]
   fn test_error_state_blocks_commands() {
       // BEGIN; INVALID_SQL; INSERT; 
       // Verify second INSERT gets 25P02
   }
   
   #[test]
   fn test_rollback_clears_error_state() {
       // BEGIN; INVALID_SQL; ROLLBACK; INSERT;
       // Verify INSERT succeeds
   }
   ```

3. **Savepoint Tests**
   ```rust
   #[test]
   fn test_savepoint_rollback_partial() {
       // BEGIN; INSERT 1; SAVEPOINT sp1; INSERT 2; ROLLBACK TO sp1; COMMIT;
       // Verify only INSERT 1 persisted
   }
   
   #[test]
   fn test_savepoint_release() {
       // BEGIN; SAVEPOINT sp1; RELEASE sp1; ROLLBACK;
       // Verify ROLLBACK undoes everything
   }
   ```

4. **Concurrency Tests**
   ```rust
   #[test]
   fn test_concurrent_transactions_isolated() {
       // Client 1: BEGIN; INSERT; (hold)
       // Client 2: BEGIN; SELECT; (should not see Client 1's insert)
   }
   
   #[test]
   fn test_concurrent_write_conflict() {
       // Both clients try to update same row
       // Verify one gets 40001 (serialization_failure)
   }
   ```

5. **Wire Protocol Tests**
   ```rust
   #[test]
   fn test_ready_for_query_status() {
       // Verify 'I', 'T', 'E' returned correctly
   }
   ```

## Error Codes Reference

| SQLSTATE | Error Code | Description | Usage |
|----------|------------|-------------|-------|
| 25P02 | TransactionRollback | Transaction aborted | Commands ignored in error state |
| 40001 | SerializationFailure | Serialization failure | Concurrent access conflict |
| 40002 | TransactionIntegrityConstraintViolation | Integrity constraint | Deferred constraint violation |
| 40P01 | DeadlockDetected | Deadlock detected | Resource deadlock |

## Migration Path

### Backward Compatibility

1. **Graceful Degradation**: Single-connection mode if pool fails
2. **Configuration Flag**: `--legacy-transactions` to disable (for debugging)
3. **Feature Detection**: Clients can detect transaction support via version()

### Deployment Steps

1. Implement Phase 5.1-5.3 (core functionality)
2. Run full test suite
3. Deploy to staging with monitoring
4. Gradual rollout with feature flag
5. Full deployment after validation

## Performance Considerations

### Connection Pool Sizing

| Workload | Pool Size | Rationale |
|----------|-----------|-----------|
| Light (< 10 concurrent) | 5-10 | SQLite handles reads well |
| Medium (10-50 concurrent) | 10-20 | WAL mode allows concurrent reads |
| Heavy (> 50 concurrent) | 20-50 | Mostly read-heavy workloads |

**Note**: SQLite only allows ONE concurrent writer. Heavy write concurrency will see contention.

### Optimization Strategies

1. **Connection Warmup**: Pre-initialize connections on startup
2. **Statement Caching**: Use `prepare_cached` for repeated statements
3. **Batch Operations**: Encourage batch inserts over many small transactions

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Connection pool exhaustion | High | Implement queue with timeout; proper sizing |
| SQLite busy errors | Medium | WAL mode + busy_timeout; retry logic |
| Memory usage (many connections) | Medium | Limit pool size; connection timeout |
| Schema changes while transactions active | High | Use `DROP TABLE` etc. carefully; document |
| Breaking existing clients | High | Feature flag; gradual rollout |

## Documentation Updates

1. **README.md**: Update transaction support section
2. **docs/TRANSACTIONS.md**: New document with examples
3. **Architecture.md**: Update architecture diagrams
4. **CHANGELOG.md**: Document breaking changes

## Success Criteria

1. ✅ `postgres-compatibility-suite` transaction tests pass
2. ✅ Concurrent clients have isolated transactions
3. ✅ 25P02 error returned for commands in aborted transaction
4. ✅ Savepoints work correctly
5. ✅ All existing E2E tests pass
6. ✅ Performance acceptable (< 10% regression)

## Timeline Estimate

| Phase | Effort | Duration |
|-------|--------|----------|
| 5.1 Connection Pool | 2 days | Week 1 |
| 5.2 Session Management | 2 days | Week 1 |
| 5.3 Transaction Commands | 2 days | Week 2 |
| 5.4 Error State | 1 day | Week 2 |
| 5.5 Wire Protocol | 1 day | Week 2 |
| 5.6 Concurrency | 2 days | Week 3 |
| 5.7 Testing | 3 days | Week 3 |
| **Total** | **13 days** | **3 weeks** |
