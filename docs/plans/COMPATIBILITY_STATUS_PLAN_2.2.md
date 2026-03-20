# Implementation Plan 2.2: Advanced Set Operators (Nesting & Precedence)

## Problem Statement
PostgreSQL allows set operations (like `UNION`, `INTERSECT`, `EXCEPT`) to be nested and ordered independently. For example, `(SELECT ...) UNION (SELECT ...) ORDER BY ...` might fail in the current transpiler because it doesn't wrap nested set operations in subqueries, causing SQLite to misinterpret the `ORDER BY` clause.

## Proposed Solution
1. **Analyze Set Operation Precedence**:
   - In `src/transpiler/dml.rs`, examine `reconstruct_set_operation_stmt`.
2. **Implement Wrapping Logic**:
   - If a `SelectStmt` has an `op > 1` (is a set operation) and is being used as a child of another set operation, or has an `ORDER BY` or `LIMIT` clause that should only apply to that specific branch, wrap it in `SELECT * FROM (...)`.
   - Ensure parentheses are handled correctly for SQLite's grammar, which is more restrictive about top-level parentheses in set operations.
3. **Handle Union vs Union All**:
   - Ensure that `UNION` (distinct) is correctly transpiled when nested, as SQLite requires the distinctness to be applied correctly across the branches.

## Verification Steps
1. **Build & Lint**:
   - `cargo build --release`
   - Fix all compiler warnings.
2. **Core Tests**:
   - `./run_tests.sh`
3. **Compatibility Suite**:
   - `pytest postgres-compatibility-suite/runner.py -k "union.sql or intersect.sql"`
   - Verify that complex set operation trees now transpile to valid SQLite.
4. **Update Tracking**:
   - Mark Item 2.2 as "Complete" in `COMPATIBILITY_STATUS_PLAN.md`.
