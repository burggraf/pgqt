# Phase 1 Implementation Plan: Error Mapping & Diagnostic Foundation

This document outlines the detailed steps for Phase 1 of the PGQT architecture review implementation.

## 1. Create `src/handler/errors.rs`

The goal is to centralize error mapping from SQLite and internal errors to PostgreSQL `SQLSTATE` codes.

### Tasks
- [ ] Define `PgErrorCode` enum with common PostgreSQL error codes:
    - `SUCCESS` ("00000")
    - `INTEGRITY_CONSTRAINT_VIOLATION` ("23000")
    - `UNIQUE_VIOLATION` ("23505")
    - `FOREIGN_KEY_VIOLATION` ("23503")
    - `CHECK_VIOLATION` ("23514")
    - `NOT_NULL_VIOLATION` ("23502")
    - `SYNTAX_ERROR` ("42601")
    - `UNDEFINED_TABLE` ("42P01")
    - `UNDEFINED_COLUMN` ("42703")
    - `INSUFFICIENT_PRIVILEGE` ("42501")
    - `INTERNAL_ERROR` ("XX000")
- [ ] Define `PgError` struct:
    ```rust
    pub struct PgError {
        pub code: String,
        pub message: String,
        pub severity: String, // "ERROR", "FATAL", etc.
        pub detail: Option<String>,
        pub hint: Option<String>,
    }
    ```
- [ ] Implement mapping from `rusqlite::Error`:
    - `rusqlite::Error::SqliteFailure(err, msg)`:
        - `SQLITE_CONSTRAINT_UNIQUE` -> `23505`
        - `SQLITE_CONSTRAINT_FOREIGNKEY` -> `23503`
        - `SQLITE_CONSTRAINT_CHECK` -> `23514`
        - `SQLITE_CONSTRAINT_NOTNULL` -> `23502`
        - `SQLITE_ERROR` (often syntax) -> `42601` or check message for "no such table" -> `42P01`
- [ ] Implement `to_pg_error(anyhow::Error) -> PgError`:
    - Attempt to downcast to `rusqlite::Error` or `pg_query::Error`.
    - Fallback to `XX000`.
- [ ] Implement `Into<pgwire::error::ErrorInfo> for PgError`.

## 2. Integrate Error Mapping

Update the entry points where errors are returned to the client.

### Tasks
- [ ] **`src/main.rs`**: Update `impl SimpleQueryHandler for SqliteHandler`.
    - Use the new mapper to convert `anyhow::Error` to `ErrorInfo`.
    ```rust
    // Before:
    Ok(vec![Response::Error(Box::new(ErrorInfo::new(
        "ERROR".to_owned(),
        "XX000".to_owned(),
        e.to_string(),
    )))])

    // After:
    let pg_err = crate::handler::errors::PgError::from_anyhow(e);
    Ok(vec![Response::Error(Box::new(pg_err.into_error_info()))])
    ```
- [ ] **`src/handler/query.rs`**: (Optional) Consider returning `PgError` from `execute_query` directly to keep error context closer to the source.

## 3. Enhance Compatibility Runner

Improve the diagnostics in the compatibility suite.

### Tasks
- [ ] **`postgres-compatibility-suite/runner.py`**:
    - Update `execute_and_compare` to extract the `pgcode` (SQLSTATE) from `psycopg2.Error`.
    - Log the SQLSTATE in the failure message.
    ```python
    except psycopg2.Error as e:
        err_test = f"{e.pgcode}: {e.pgerror}" if e.pgcode else str(e)
    ```

## 4. Verification & Regression

- [ ] **Unit Tests**: Add tests in `src/handler/errors.rs` to verify that specific `rusqlite` errors map to the correct `SQLSTATE`.
- [ ] **Integration Test**: Create a new test in `tests/error_tests.rs` (or add to `integration_test.rs`) that:
    1. Connects via `psycopg2`.
    2. Triggers a unique constraint violation.
    3. Asserts that `e.pgcode == '23505'`.
- [ ] **Regression**: Run `./run_tests.sh` and ensure no existing tests fail due to the change in error reporting.
- [ ] **Compatibility**: Run `postgres-compatibility-suite/run_suite.sh` and observe if any "Connection Error" or "XX000" errors are replaced by specific SQLSTATEs.
