# PGQT Compatibility Improvement Plan

This plan outlines the prioritized steps to improve PostgreSQL compatibility based on recent test suite results and identified gaps.

## Phase 1: Stability & Core Correctness (High Urgency)

### 1.1 Proxy Stability for Recursive CTEs
- **Problem**: `with.sql` causes proxy hangs/crashes, likely due to uncontrolled recursion or memory limits in the transpiled SQLite logic.
- **Action**: Implement a hard recursion depth limit in `src/transpiler/dml.rs` and ensure proper error handling for deep recursion.
- **Status**: Completed (Added `max_recursion_depth` to `TranspileContext` and injected `LIMIT` into recursive CTEs).
- **Metric**: `with.sql` should either pass or return a controlled error instead of crashing.

### 1.2 Anonymous Column Naming
- **Problem**: PostgreSQL defaults to `?column?` for unnamed results; SQLite often uses the expression text. Tests like `case.sql` fail on column name mismatches.
- **Action**: Update `reconstruct_select` in `src/transpiler/dml.rs` to automatically alias any unnamed `ResTarget` to `?column?` or the appropriate Postgres-compatible name.
- **Status**: Completed (Updated `reconstruct_select` to apply default aliases: `?column?`, function names, `case`, `coalesce`, and type names for casts).
- **Metric**: Passing `case.sql` and `select.sql`.

### 1.3 Robust INSERT Padding
- **Problem**: `INSERT INTO t VALUES (DEFAULT, 7)` fails when `t` has 3 columns. The current padding logic doesn't seem to account for `DEFAULT` placeholders correctly.
- **Action**: Refine the `INSERT` transpilation in `src/transpiler/dml.rs` to ensure padding triggers whenever the `VALUES` list count is less than the table's column count, even when `DEFAULT` is present.
- **Status**: Completed (Updated `reconstruct_values_as_union_all` to support padding and fixed `current_column_index` tracking to correctly resolve `DEFAULT` placeholders).
- **Metric**: Passing `insert.sql`.

## Phase 2: Feature Parity & Polyfills (Medium Impact)

### 2.1 String & Math Functions
- **Problem**: `repeat()` and other common string functions are missing.
- **Action**: Implement `repeat(text, int)` as a SQLite user-defined function (UDF) in `src/handler/mod.rs`.
- **Status**: Completed (Implemented `repeat(text, int)` as a Rust-based SQLite UDF).
- **Metric**: Passing `delete.sql` (which uses `repeat` for test data generation).

### 2.2 Advanced Set Operators
- **Problem**: `(SELECT ...) UNION (SELECT ...) ORDER BY ...` fails when combined with `INTERSECT` because of incorrect clause ordering in the transpiled SQL.
- **Action**: Fix the `SelectStmt` reconstruction logic to properly wrap set operations in subqueries when an `ORDER BY` is present on a specific branch.
- **Status**: Completed (Updated `reconstruct_set_operation_stmt` to wrap nested set operations, or branches with `ORDER BY`/`LIMIT`, in `SELECT * FROM (...)` subqueries for SQLite compatibility).
- **Metric**: Passing `union.sql`.

### 2.3 Subquery Array Indexing
- **Problem**: `SELECT (SELECT ARRAY[1,2,3])[1]` fails with syntax errors.
- **Action**: Improve the transpiler's handling of `A_Indirection` on subqueries.
- **Status**: Completed (Added support for `A_Indirection` nodes in `reconstruct_node`, mapping them to SQLite's `json_extract`. Also ensured correct subquery parenthesizing and default 'array' aliasing).
- **Limitation**: Direct array indexing on array literals (e.g., `ARRAY[1,2,3][1]`) is not supported because `pg_query` parser cannot parse this syntax. Only subquery array indexing (e.g., `(SELECT ARRAY[...])[1]`) is transpiled.
- **Metric**: Passing `subselect.sql` (for subquery cases).

## Phase 3: Long-Tail Compatibility (Edge Cases)

### 3.1 Statistics & Aggregates
- **Problem**: Missing `regr_*` and `corr` functions.
- **Action**: Complete the implementation of statistical aggregates in `src/stats.rs`.

### 3.2 System Catalog Completeness
- **Problem**: Introspection tools (like `psql`) fail due to missing columns in `pg_catalog` views.
- **Action**: Audit `src/catalog/system_views.rs` and add missing standard columns to `pg_attribute`, `pg_class`, etc.

## Monitoring & Verification
- **Nightly Suite**: Run `./postgres-compatibility-suite/run_suite.sh` after every major phase.
- **Regress Tracking**: Maintain `COMPATIBILITY_STATUS.md` with updated "Files Passing" counts.
