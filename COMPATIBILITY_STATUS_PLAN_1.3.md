# Implementation Plan 1.3: Robust INSERT Padding

## Problem Statement
The current `INSERT` logic is intended to pad missing values with `DEFAULT` or `NULL` if the provided `VALUES` list is shorter than the table's column count. However, tests like `insert.sql` fail when `DEFAULT` is used as an explicit value (e.g., `INSERT INTO t VALUES (DEFAULT, 7)` where `t` has 3 columns), suggesting that the presence of `DEFAULT` may be confusing the padding logic or that the padding isn't triggering correctly in all cases.

## Proposed Solution
1. **Enhance Table Metadata Retrieval**: In `src/transpiler/dml.rs`, ensure that whenever an `INSERT INTO table (...) VALUES (...)` is encountered, the full schema (including column counts) for the target table is fetched from the catalog.
2. **Correct Column Counting**:
   - Count the number of elements in the `VALUES` list, treating `DEFAULT` as a regular value for counting purposes.
   - If `count(VALUES) < count(TABLE_COLUMNS)` and no explicit column list was provided in the `INSERT` statement, append the required number of `DEFAULT` tokens to the `VALUES` list.
3. **Refine DEFAULT Resolution**: Ensure that `DEFAULT` placeholders are correctly resolved to the SQLite column defaults (e.g., `now()` → `datetime('now')`) after padding.
4. **Handle Multiple Value Rows**: If an `INSERT` has multiple rows in the `VALUES` clause, ensure each row is padded to the same length.

## Verification Steps
1. **Build & Lint**:
   - `cargo build --release`
   - Fix all compiler warnings.
2. **Core Tests**:
   - `./run_tests.sh`
3. **Compatibility Suite**:
   - `pytest postgres-compatibility-suite/runner.py -k "insert.sql"`
   - Verify that all standard PostgreSQL `INSERT` behaviors are correctly transpiled.
4. **Update Tracking**:
   - Mark Item 1.3 as "Complete" in `COMPATIBILITY_STATUS_PLAN.md`.

## Execution Commands
```bash
# Verify padding manually
cargo run -- --transpile "CREATE TABLE t (a int, b int, c int default 5); INSERT INTO t VALUES (1);"
# Expected: INSERT INTO t (a, b, c) VALUES (1, NULL, 5); -- Or similar SQLite logic
```
