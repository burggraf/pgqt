# Implementation Plan - LATERAL Joins

## Problem Statement
PostgreSQL's `LATERAL` joins allow correlated subqueries in the `FROM` clause. SQLite does not natively support `LATERAL` for subqueries, but it *does* support them implicitly for table-valued functions (like `json_each`).

## Research Findings
1.  **Table-Valued Functions**: SQLite supports implicit lateral joins for functions like `json_each`, `json_tree`, `fts5`, etc.
2.  **Subqueries**: SQLite does *not* support lateral subqueries. Emulating them requires complex query restructuring (e.g., using window functions or CTEs), which is difficult to do generically in a transpiler.
3.  **Current Status**: The proxy already transpiles `LATERAL` for table-valued functions by simply omitting the `LATERAL` keyword (since it's implicit in SQLite). However, it silently transpiles lateral subqueries into standard subqueries, which then fail in SQLite with "no such column" errors.

## Proposed Strategy
1.  **Formalize Support for Table-Valued Functions**: Ensure `LATERAL` is correctly handled for all table-valued functions.
2.  **Explicitly Reject Unsupported Lateral Subqueries**: Modify the transpiler to detect `lateral: true` in `RangeSubselect` and throw a clear error or warning, rather than producing invalid SQL.
3.  **Improve Error Messaging**: Provide guidance to the user on how to rewrite their query for SQLite (e.g., using window functions).
4.  **Documentation**: Update `TODO-FEATURES.md` and `README.md` to reflect partial support (supported for functions, not for subqueries).

## Implementation Steps

### 1. Transpiler Updates (`src/transpiler.rs`)
- Modify `reconstruct_range_subselect` to check for `lateral: true`.
- If `lateral: true`, return an error or a special marker that can be handled.
- Since `transpile` currently returns a `String`, we might need to change it to return a `Result` or handle the error gracefully within the string (e.g., by adding a comment).
- Alternatively, we can use the `TranspileContext` to track if an unsupported feature was used.

### 2. Unit Tests
- Add tests for `LATERAL jsonb_each(...)`.
- Add tests for `LATERAL (SELECT ...)` and verify it fails or reports an error.

### 3. E2E Tests
- Create `tests/lateral_e2e_test.py`.
- Test `jsonb_each` with `LATERAL`.
- Test `jsonb_array_elements` with `LATERAL`.
- Verify that unsupported lateral subqueries fail gracefully.

### 4. Documentation
- Update `docs/TODO-FEATURES.md`.
- Update `README.md`.
- Create `docs/LATERAL.md` to explain the support and workarounds.

## Verification
- Run `cargo test`.
- Run `python3 tests/lateral_e2e_test.py`.
- Run `./run_tests.sh`.
