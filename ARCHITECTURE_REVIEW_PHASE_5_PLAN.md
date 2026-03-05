# Phase 5 Implementation Plan: Transactional Integrity

This document outlines the detailed steps for Phase 5 of the PGQT architecture review implementation.

## 1. Architectural Foundation (Session Isolation)

To support transactional integrity in a multi-client environment, each PostgreSQL session must have its own isolated SQLite transaction state.

### Tasks
- [ ] **Define `TransactionStatus` enum**:
    ```rust
    pub enum TransactionStatus {
        Idle,           // Not in a transaction
        InTransaction,  // BEGIN called, no errors
        InError,        // Command failed inside transaction, must ROLLBACK
    }
    ```
- [ ] **Update `SessionContext`**:
    - Add `transaction_status: TransactionStatus`.
    - Add `savepoints: Vec<String>` to track active savepoint names.
- [ ] **Connection Management Refactor**:
    - Move from a single `Arc<Mutex<Connection>>` in `SqliteHandler` to a model where the session contains its own connection.
    - Alternatively, implement a **Connection Pool** in `SqliteHandler` where sessions can lease a connection and hold it until the transaction completes or the session ends.
    - *Recommendation*: Since SQLite is file-based, give each session its own `rusqlite::Connection` opened to the same file.

## 2. Map Transaction Commands

Replace the placeholder logic in `src/handler/transaction.rs`.

### Tasks
- [ ] **Implement command logic**:
    - `BEGIN` / `START TRANSACTION`: 
        1. Execute SQLite `BEGIN`.
        2. Set `session.transaction_status = InTransaction`.
        3. Return `Response::TransactionStart`.
    - `COMMIT` / `END`:
        1. Execute SQLite `COMMIT`.
        2. Set `session.transaction_status = Idle`.
        3. Return `Response::TransactionEnd`.
    - `ROLLBACK` / `ABORT`:
        1. Execute SQLite `ROLLBACK`.
        2. Set `session.transaction_status = Idle`.
        3. Return `Response::TransactionEnd`.
    - `SAVEPOINT name`: Execute SQLite `SAVEPOINT name`.
    - `ROLLBACK TO SAVEPOINT name`: Execute SQLite `ROLLBACK TO name`.
    - `RELEASE SAVEPOINT name`: Execute SQLite `RELEASE name`.

## 3. Transaction Error Propagation

Enforce PostgreSQL-style transaction state logic.

### Tasks
- [ ] **Update `execute_query` in `src/handler/query.rs`**:
    - If `session.transaction_status == InError`, reject any command that is NOT `ROLLBACK` or `ROLLBACK TO SAVEPOINT` with error `25P02: current transaction is aborted`.
    - If a command fails and `session.transaction_status == InTransaction`, automatically set state to `InError`.

## 4. Connection Cleanup & Safety

### Tasks
- [ ] **Implement Session Dropping**:
    - Ensure that when a session ends (TCP socket closed), any active transaction on its SQLite connection is rolled back.
    - This is naturally handled by `rusqlite` when the `Connection` object is dropped, but explicit logic in `SqliteHandler::drop_session` is safer.

## 5. Verification & Regression

- [ ] **Integration Tests (`tests/transaction_tests.rs`)**:
    - Verify that `BEGIN; INSERT; ROLLBACK;` results in no data being inserted.
    - Verify that `BEGIN; INVALID_SQL; INSERT;` fails the second `INSERT` due to `InError` state.
    - Verify that `SAVEPOINT` works as expected.
- [ ] **Concurrency Tests**:
    - Run two separate PostgreSQL clients simultaneously and verify their transactions do not leak into each other.
- [ ] **Compatibility**:
    - Run `postgres-compatibility-suite/run_suite.sh`.
    - *Success Metric*: Tests involving multi-statement transactions and error rollbacks now pass correctly.
