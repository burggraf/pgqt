# Implementation Plan 1.2: Anonymous Column Naming

## Problem Statement
PostgreSQL defaults unnamed select columns to `?column?` or specific derived names (e.g., from `CASE`, `CAST`, or function names). SQLite often uses the raw expression text or its own default naming convention. This causes failures in `case.sql` and `select.sql` due to column name mismatches in the result set.

## Proposed Solution
1. **Intercept Unnamed Targets**: In `src/transpiler/dml.rs` (specifically inside `reconstruct_select`), identify any `ResTarget` node where `name` is `None`.
2. **Apply Default Naming**:
   - For a general expression without an alias, assign the literal string `?column?` as the alias.
   - For `CASE` expressions, PostgreSQL sometimes uses `case` or the first branch's name; prioritize the `?column?` behavior unless an explicit alias exists.
   - For `CAST(expr AS type)`, PostgreSQL may use `type` as the default column name. Handle this in `src/transpiler/expr.rs` by adding an explicit `AS type` alias if one isn't already present.
3. **Handle Expression Rewrites**: Ensure that even when an expression is rewritten (e.g., PostgreSQL `||` → SQLite `||` or `concat()`), the original alias or the new `?column?` alias is preserved.

## Verification Steps
1. **Build & Lint**:
   - `cargo build --release`
   - Fix all compiler warnings.
2. **Core Tests**:
   - `./run_tests.sh`
3. **Compatibility Suite**:
   - `pytest postgres-compatibility-suite/runner.py -k "case.sql or select.sql"`
   - Verify that column name mismatches are resolved.
4. **Update Tracking**:
   - Mark Item 1.2 as "Complete" in `COMPATIBILITY_STATUS_PLAN.md`.

## Execution Commands
```bash
# Verify Column Aliasing manually
cargo run -- --transpile "SELECT 1, 'a' || 'b', CASE WHEN 1 < 2 THEN 3 END"
# Expected: SELECT 1 AS "?column?", 'a' || 'b' AS "?column?", CASE WHEN 1 < 2 THEN 3 END AS "?column?" (or specific Postgres name)
```
