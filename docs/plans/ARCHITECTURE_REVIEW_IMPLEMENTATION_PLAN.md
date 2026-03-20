# PGQT Architecture Implementation Plan

This plan outlines a phased approach to implementing the recommendations from `ARCHITECTURE_REVIEW.md`. Each phase includes specific regression testing steps to ensure that PostgreSQL compatibility is maintained and improved throughout the process.

---

## Phase 1: Error Mapping & Diagnostic Foundation
**Goal**: Improve client library stability and provide better feedback for failing compatibility tests.

### Tasks
1.  **Create `src/handler/errors.rs`**:
    *   Define a mapping from `rusqlite::Error` to PostgreSQL `SQLSTATE` codes.
    *   Implement a `to_pg_error` function that returns a `pgwire` compatible error response.
2.  **Integrate Error Mapping**:
    *   Update `src/handler/query.rs` to wrap all SQLite executions in the new error mapper.
3.  **Enhance Compatibility Runner**:
    *   Update `postgres-compatibility-suite/runner.py` to optionally log the specific `SQLSTATE` received.

### Verification & Regression
*   **Step 1**: `cargo check` to ensure type safety.
*   **Step 2**: `./run_tests.sh --unit-only` to ensure core logic remains sound.
*   **Step 3**: Run `postgres-compatibility-suite/run_suite.sh`.
    *   *Success Metric*: No new failures; some existing failures may change from "Connection Error" to specific "SQLSTATE" errors.

---

## Phase 2: Result Set & Metadata Formalization
**Goal**: Centralize the logic for column naming and type metadata to resolve "Column Metadata & Naming" issues.

### Tasks
1.  **Implement `ResultSetRewriter`**:
    *   Create `src/handler/rewriter.rs` to handle post-execution data processing.
2.  **Migrate Column Naming Logic**:
    *   Move anonymous column renaming (`?column?` -> PostgreSQL conventions) from the transpiler/handler into the rewriter.
3.  **Type Metadata Enforcement**:
    *   Use the Shadow Catalog to lookup original PostgreSQL types and override `pgwire` field types in the response.

### Verification & Regression
*   **Step 1**: `cargo test --test schema_tests` to verify catalog lookups.
*   **Step 2**: Run `python3 tests/run_all_e2e.py` to check wire protocol consistency.
*   **Step 3**: Run `postgres-compatibility-suite/run_suite.sh`.
    *   *Success Metric*: Passing rate for "Section 2 (Column Metadata & Naming)" increases.

---

## Phase 3: Transpiler Structural Refactoring (Modularization)
**Goal**: De-risk the transpiler by breaking down the 1,000+ line `expr.rs`.

### Tasks
1.  **Initialize Modular Structure**:
    *   Create `src/transpiler/expr/` directory and `mod.rs`.
2.  **Incremental Extraction**:
    *   **3.2.1**: Extract Array operators/functions to `expr/arrays.rs`.
    *   **3.2.2**: Extract Range types/operators to `expr/ranges.rs`.
    *   **3.2.3**: Extract Geometric types to `expr/geo.rs`.
3.  **Refactor `reconstruct_node`**:
    *   Update the main loop in `expr.rs` to delegate to these sub-modules.

### Verification & Regression
*   **Step 1**: `cargo check` after every sub-module extraction.
*   **Step 2**: `./run_tests.sh --integration-only` (specifically `array_tests.rs`, `range_tests.rs`, and `geo_e2e_test.py`).
*   **Step 3**: `cargo run -- --transpile` with complex queries to verify output identity before/after refactor.

---

## Phase 4: Data-Driven Registry
**Goal**: Eliminate hardcoded `match` statements and enable easier "stubbing" of PostgreSQL functions.

### Tasks
1.  **Define Registry Core**:
    *   Create `src/transpiler/registry.rs`.
    *   Implement `TypeRegistry` and `FunctionRegistry` using `lazy_static` or passing through `TranspileContext`.
2.  **Migrate Logic**:
    *   Move `utils::rewrite_type_for_sqlite` mappings into the registry.
    *   Move `func::reconstruct_func_call` built-in overrides into the registry.
3.  **Dynamic Stubbing**:
    *   Allow the registry to provide "No-Op" stubs for unsupported functions automatically.

### Verification & Regression
*   **Step 1**: `cargo test` to ensure all built-in function transpilation still works.
*   **Step 2**: `./run_tests.sh --unit-only`.
*   **Step 3**: Run `postgres-compatibility-suite/run_suite.sh`.
    *   *Success Metric*: Reduced boilerplate in `func.rs`; ability to pass more tests by adding simple registry entries.

---

## Phase 5: Transactional Integrity
**Goal**: Support real `BEGIN`, `COMMIT`, `ROLLBACK`, and `SAVEPOINT` logic.

### Tasks
1.  **State Management**:
    *   Add `transaction_state` to `SessionContext` in `src/handler/mod.rs`.
2.  **Map Transaction Commands**:
    *   Update `src/handler/transaction.rs` to actually execute SQLite transaction commands instead of returning "OK" immediately.
3.  **Connection Cleanup**:
    *   Ensure that dropped connections trigger a `ROLLBACK` in SQLite if a transaction was active.

### Verification & Regression
*   **Step 1**: Create new integration tests in `tests/transaction_tests.rs` verifying multi-statement rollbacks.
*   **Step 2**: `python3 tests/run_all_e2e.py`.
*   **Step 3**: Run `postgres-compatibility-suite/run_suite.sh`.
    *   *Success Metric*: Passing rate for transaction-heavy SQL files improves.

---

## Continuous Regression Strategy

For **every** task completed within a phase, the following check-list must be executed:

1.  **Static Analysis**: `cargo check` and `cargo clippy`.
2.  **Unit/Integration**: `./run_tests.sh --no-e2e`.
3.  **End-to-End**: `python3 tests/run_all_e2e.py`.
4.  **Compatibility Check**: Run the specific failing test file from `postgres-compatibility-suite` that the task was intended to address.
5.  **Full Suite**: Run `./run_tests.sh` (all tests) before moving to the next task or phase.
