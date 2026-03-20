# Implementation Plan 2.3: Subquery Array Indexing (A_Indirection on Sublinks)

## Problem Statement
PostgreSQL supports indexing into an array returned by a subquery: `SELECT (SELECT ARRAY[1,2,3])[1]`. Currently, the transpiler might not correctly handle the `A_Indirection` (indexing) node when its target is a `SubLink` (subquery), leading to a syntax error.

## Proposed Solution
1. **Locate Indirection Logic**:
   - In `src/transpiler/expr/operators.rs` (or where array indexing is handled), find the logic for `A_Indirection`.
2. **Handle Sublink Targets**:
   - Check if the `arg` of the `A_Indirection` node is a `SubLink`.
   - If it's a `SubLink`, first reconstruct the subquery (e.g., `(SELECT ...)`), then apply the SQLite equivalent for array indexing (likely `json_extract()` or a similar polyfill already used for other array indexing).
3. **Handle Parentheses**:
   - Ensure the subquery is correctly parenthesized so the indexing applies to the result of the entire subquery.
   - Example transformation: `(SELECT ARRAY[1,2,3])[1]` → `json_extract((SELECT ARRAY[1,2,3]), '$[0]')`.

## Verification Steps
1. **Build & Lint**:
   - `cargo build --release`
   - Fix all compiler warnings.
2. **Core Tests**:
   - `./run_tests.sh`
3. **Compatibility Suite**:
   - `pytest postgres-compatibility-suite/runner.py -k "subselect.sql"`
   - Verify that all complex subquery array indexing cases now pass.
4. **Update Tracking**:
   - Mark Item 2.3 as "Complete" in `COMPATIBILITY_STATUS_PLAN.md`.
