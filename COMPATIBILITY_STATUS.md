# PGQT Compatibility Status & Roadmap

This document tracks the current state of PostgreSQL compatibility and identified work items based on the `postgres-compatibility-suite`.

## Current Status (March 2026)
- **Files Passing**: 14
- **Files Failing**: 35
- **Recent Wins**:
    - Full CTE (WITH clause) support including recursive CTEs.
    - Fixed subquery column aliasing (e.g., `(VALUES (1)) v(x)`).
    - Basic `CREATE VIEW` transpilation.
    - Hang prevention for unsupported `SEARCH` and `CYCLE` clauses.
    - Multiple system function stubs (`pg_typeof`, `random`, `regr_count`).

---

## High-Priority Future Work

### 1. INSERT Logic Improvements ✅ COMPLETE
- **Automatic Padding**: ✅ Implemented. Detects when an `INSERT` has fewer values than the table has columns. Fetches the schema from the catalog and pads with `DEFAULT` or `NULL` to match PostgreSQL behavior.
- **DEFAULT Keyword**: ✅ Implemented. Supports the `DEFAULT` keyword in `VALUES` lists, resolving it to the correct SQLite default expressions (e.g., `now()` → `datetime('now')`).

### 2. Column Metadata & Naming
- **Anonymous Columns**: Map SQLite's default column names (often the expression text) to PostgreSQL's `?column?` convention for unnamed results.
- **Type Cast Aliasing**: We currently alias `CAST(x AS type)` as `type`. We should expand this to cover more complex expressions where Postgres expects specific auto-generated names.

### 3. Catalog & Introspection (`pg_catalog`)
- **System View Completeness**: Add missing columns to `pg_attribute`, `pg_class`, and `pg_type`.
- **OID/Regclass Support**: Implement better mapping between SQLite's `rowid` and PostgreSQL's `OID`, and support `::regclass` casts more robustly.
- **More Stubs**: Add `pg_get_indexdef`, `pg_get_constraintdef`, and `format_type`.

### 4. Polyfills for Statistical Aggregates
- **Real Math**: Implement the actual logic for `regr_sxx`, `regr_sxy`, `corr`, and `covar_pop` using the state-tracking pattern found in `src/stats.rs`.
- **Transition Functions**: Properly implement transition functions like `float8_accum` to support custom aggregate definitions.

### 5. Validation & Strictness
- **Length Constraints**: Implement proxy-side validation for `VARCHAR(N)` and `CHAR(N)` to throw errors on "value too long," as expected by `pg_regress`.
- **Domain/Type Checks**: Add basic checks for range values and interval syntax validity.

### 6. Complex SQL Features
- **SEARCH/CYCLE Clauses**: Investigate polyfilling `SEARCH` and `CYCLE` for recursive CTEs using standard SQL if possible, or provide better error messages.
- **LATERAL Joins**: The current transpiler lacks `LATERAL` support for subqueries; this is a frequent requirement for complex Postgres queries.
