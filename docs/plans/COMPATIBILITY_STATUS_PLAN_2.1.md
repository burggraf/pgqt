# Implementation Plan 2.1: String & Math Functions (repeat)

## Problem Statement
The PostgreSQL `repeat(text, int)` function is missing in the SQLite-backed proxy. This function is used in `delete.sql` for test data generation and is a common utility in PostgreSQL.

## Proposed Solution
1. **Implement UDF in Rust**:
   - Open `src/functions.rs`.
   - Add a new function `repeat_fn` that takes a `String` and an `i64` (count) and returns the concatenated string.
   - Handle edge cases: if count <= 0, return an empty string.
2. **Register UDF**:
   - In `src/handler/mod.rs` (or where the SQLite connection is initialized, likely `SqliteHandler::new`), register the `repeat` function with the SQLite connection.
3. **Verify via Transpiler**:
   - Ensure the transpiler doesn't mangle `repeat()` calls (it shouldn't as it defaults to preserving unknown functions).

## Verification Steps
1. **Build & Lint**:
   - `cargo build --release`
   - Fix all compiler warnings.
2. **Core Tests**:
   - `./run_tests.sh`
3. **Compatibility Suite**:
   - `pytest postgres-compatibility-suite/runner.py -k "delete.sql"`
   - Verify that data generation using `repeat()` now works.
4. **Update Tracking**:
   - Mark Item 2.1 as "Complete" in `COMPATIBILITY_STATUS_PLAN.md`.
