# Implementation Plan 1.1: Proxy Stability for Recursive CTEs

## Problem Statement
The PostgreSQL compatibility test `with.sql` currently causes the PGQT proxy to hang or crash. This is likely due to the transpiled SQLite recursive CTE logic entering an infinite or extremely deep recursion loop that exceeds stack/memory limits, or SQLite hitting its own internal limits which the proxy doesn't handle gracefully.

## Proposed Solution
1. **Identify the Failure Point**: Run the proxy in debug mode with `with.sql` to determine if the crash occurs during transpilation (Rust side) or execution (SQLite side).
2. **Implement Depth Guards**:
   - Add a configuration-defined `max_recursion_depth` (default: 100) to `TranspileContext`.
   - In `src/transpiler/window.rs` (or where CTEs are handled), inject a depth counter if the `SEARCH` or `CYCLE` clauses are missing, or if it's a plain recursive CTE.
   - Alternatively, use SQLite's `LIMIT` clause inside the recursive branch of the CTE to provide a "hard stop" if no other termination condition is met.
3. **Graceful Error Handling**: Ensure that if recursion is truncated or fails, a proper PostgreSQL error code (e.g., `54001: statement_too_complex`) is returned instead of a connection drop.

## Verification Steps
1. **Build & Lint**:
   - `cargo build --release`
   - Fix all compiler warnings.
2. **Core Tests**:
   - `./run_tests.sh` (Ensure no regressions in existing unit/integration tests).
3. **Compatibility Suite**:
   - `pytest postgres-compatibility-suite/runner.py -k "with.sql"`
   - Verify the proxy no longer crashes.
4. **Update Tracking**:
   - Mark Item 1.1 as "In Progress" or "Complete" in `COMPATIBILITY_STATUS_PLAN.md`.

## Execution Commands
```bash
# Debugging
cargo run -- --transpile "$(cat postgres-compatibility-suite/sql/pg_regress/with.sql)"

# Test Run
PG_DSN="..." PROXY_PORT=5435 pytest postgres-compatibility-suite/runner.py -k "with.sql"
```
