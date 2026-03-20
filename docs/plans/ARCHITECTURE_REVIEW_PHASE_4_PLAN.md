# Phase 4 Implementation Plan: Data-Driven Registry

This document outlines the detailed steps for Phase 4 of the PGQT architecture review implementation.

## 1. Define Registry Core (`src/transpiler/registry.rs`)

The goal is to move from hardcoded `match` statements in `utils.rs` and `func.rs` to a registry-based approach.

### Tasks
- [ ] **Define `TypeRegistry`**:
    - A structure that holds a map from PostgreSQL type names (e.g., `VARCHAR`, `TIMESTAMP`) to SQLite type names (e.g., `text`, `numeric`).
    - Support for regex or prefix matching for types like `VARCHAR(N)`.
- [ ] **Define `FunctionRegistry`**:
    - A structure that maps PostgreSQL function names to their SQLite counterparts.
    - Support different types of mappings:
        - `Simple`: Rename (e.g., `length` -> `length`).
        - `Rewrite`: Replace with a different name (e.g., `now()` -> `datetime('now')`).
        - `Complex`: A closure or trait implementation that takes arguments and returns transpiled SQL (e.g., `substring`, `extract`).
        - `NoOp`: Automatically return `NULL` for known but unsupported functions.
- [ ] **Global/Default Registries**:
    - Create a `DefaultRegistry` that is initialized with all current mappings.

## 2. Migrate Type Logic

Replace the hardcoded logic in `src/transpiler/utils.rs`.

### Tasks
- [ ] Update `rewrite_type_for_sqlite` to use `TypeRegistry`.
- [ ] Populate the registry with all current mappings from the `match` statement in `utils.rs`.

## 3. Migrate Function Logic

Replace the hardcoded logic in `src/transpiler/func.rs`.

### Tasks
- [ ] Update `reconstruct_func_call` to check the `FunctionRegistry` first.
- [ ] Move built-in function overrides (like `now`, `current_timestamp`, `jsonb_extract_path`) into the registry.
- [ ] Implement "Dynamic Stubbing": If a function is not in the registry and not in the UDF catalog, check if a "stubbing mode" is enabled to return a `No-Op` result instead of an error.

## 4. Integrate with `TranspileContext`

Make the registry available during the entire transpilation process.

### Tasks
- [ ] Add `registry: Arc<Registry>` to `TranspileContext`.
- [ ] Update `TranspileContext::new()` to use the `DefaultRegistry`.

## 5. Verification & Regression

- [ ] **Unit Tests**:
    - Test adding a custom function to the registry and verifying it transpiles correctly.
    - Test "No-Op" stubbing for an unknown function.
- [ ] **Integration Tests**:
    - Run all existing tests in `tests/transpiler_tests.rs` to ensure no regressions in standard function transpilation.
- [ ] **Compatibility**:
    - Run `postgres-compatibility-suite/run_suite.sh`.
    - *Success Metric*: Same or better passing rate; cleaner code in `func.rs` and `utils.rs`.
    - Verify that adding a new supported function is now just a one-line addition to the registry.
