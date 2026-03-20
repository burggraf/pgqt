# Changelog

All notable changes to PGQT will be documented in this file.

## [Unreleased]

### Added
- **Observability Stack**: Full Prometheus-compatible metrics and monitoring
  - **Prometheus Metrics Endpoint** (`/metrics`): 13+ metrics for monitoring
    - Request counters: `pgqt_requests_total`, `pgqt_requests_failed_total`
    - Query latency histogram: `pgqt_query_duration_seconds` (1ms to 10s buckets)
    - Connection metrics: `pgqt_connections_active`, `pgqt_connections_total`
    - Query type breakdown: `pgqt_queries_select_total`, `pgqt_queries_insert_total`, etc.
    - Cache metrics: `pgqt_transpile_cache_hits_total`, `pgqt_transpile_cache_misses_total`
  - **System Metrics** (with `system-metrics` feature): CPU, memory, disk usage
    - `pgqt_system_cpu_usage_percent`: CPU usage across all cores
    - `pgqt_system_memory_used_bytes` / `pgqt_system_memory_total_bytes`
    - `pgqt_system_disk_used_bytes` / `pgqt_system_disk_total_bytes`
  - **Web Dashboard** (with `web-config` feature): Built-in HTML dashboard at `/`
  - **Health Check Endpoint** (`/health`): JSON health status
  - **Feature Flags**: `metrics`, `system-metrics`, `web-config`, `observability`
  - **CLI Flags**: `--metrics-enabled`, `--metrics-port` (env: `PGQT_METRICS_ENABLED`, `PGQT_METRICS_PORT`)
  - **Documentation**: `docs/metrics.md` with complete metrics reference and PromQL examples
  - **Examples**: `examples/prometheus.yml`, `examples/grafana-dashboard.json`
  - Binary impact: +2-2.5 MB with full observability

### Added
- **Input Validation Improvements (Phase 7.1)**: Aligned input validation with PostgreSQL semantics
  - **Interval Validation**: Stricter interval parsing with PostgreSQL-compatible error messages
    - `Interval::from_str()` now rejects empty strings and invalid inputs
    - Added `validate_interval()` function in `src/validation/types.rs`
    - Returns error code 22007 (invalid_datetime_format) for invalid intervals
    - Error messages: "invalid input syntax for type interval: \"{}\""
    - Updated tests in `src/interval.rs`
  - **JSON Validation**: Strict JSON parsing with proper PostgreSQL error messages
    - Added `validate_json_strict()` function in `src/jsonb.rs`
    - Empty strings rejected with proper error messages
    - Returns error code 22P02 (invalid_text_representation) for invalid JSON
    - Error messages: "invalid input syntax for type json: \"{}\""
    - Updated validation in `src/validation/types.rs`
  - **Numeric Range Validation**: Overflow detection for float values
    - Added `validate_numeric_with_overflow_check()` in `src/float_special.rs`
    - Detects overflow to infinity (e.g., "1e309")
    - Returns error code 22003 (numeric_value_out_of_range)
    - Error messages: "\"{}\" is out of range for type {}"
    - Explicit "infinity" values still accepted
  - **Error Code Mapping**: Added `NumericValueOutOfRange` to `PgErrorCode` enum
    - SQLSTATE code 22003 for numeric value out of range errors
    - Located in `src/handler/errors.rs`
  - **Validation Module Updates**: Extended `validate_value()` function
    - Added INTERVAL type validation
    - Added JSON/JSONB type validation
  - New tests: 15+ unit tests across `src/interval.rs`, `src/jsonb.rs`, `src/float_special.rs`

### Added
- **Float Input Validation (Phase 6.2)**: Implemented PostgreSQL-compatible float input validation
  - `validate_float_input(s: &str) -> Result<f64, String>` function for validating float inputs
  - `validate_float()` SQLite function for runtime validation
  - Rejects invalid inputs matching PostgreSQL behavior:
    - `'xyz'::float4` - invalid text
    - `'5.0.0'::float4` - multiple decimal points
    - `'5 . 0'::float4` - spaces in number
    - `'     - 3.0'::float4` - spaces in negative number
    - `''::float4` - empty string
    - `'       '::float4` - whitespace only
  - Returns PostgreSQL-compatible error messages: "invalid input syntax for type double precision: \"{}\""
  - Preserves support for special values: NaN, infinity, -infinity
  - 8 new unit tests in `src/float_special.rs`
  - 9 new integration tests in `tests/float_tests.rs`

### Added
- **Special Float Value Handling (Phase 6.1)**: Implemented support for PostgreSQL's special float values
  - `nan()` function - returns IEEE 754 NaN (Not a Number)
  - `infinity()` function - returns positive infinity
  - `neg_infinity()` function - returns negative infinity
  - `float8_nan()` and `float8_infinity()` aliases for PostgreSQL compatibility
  - Transpiler support for `'NaN'::float8`, `'infinity'::float8`, `'-infinity'::float8` casts
  - Full arithmetic support: Infinity + 100 = Infinity, Infinity / Infinity = NaN, etc.
  - 11 new integration tests in `tests/float_tests.rs`
  - 5 new unit tests in `src/float_special.rs`

### Added
- **ON CONFLICT Enhancements (Phase 4.2)**: Fixed remaining ON CONFLICT (upsert) issues
  - Multiple conflict targets (e.g., `ON CONFLICT (col1, col2)`)
  - Complex WHERE clauses in DO UPDATE (e.g., `DO UPDATE SET ... WHERE ...`)
  - Subqueries in DO UPDATE SET (e.g., `SET col = (SELECT ...)`)
  - ON CONFLICT with RETURNING (SQLite 3.35.0+)
  - EXCLUDED table reference support (case-insensitive)
  - ON CONSTRAINT target support for named constraints
  - 9 new integration tests in `tests/insert_tests_upsert.rs`
  - 8 new unit tests in `src/transpiler/mod.rs`

### Added
- **RETURNING Clause Enhancements (Phase 4.1)**: Fixed remaining RETURNING clause issues for INSERT/UPDATE/DELETE statements
  - Complex expressions in RETURNING (e.g., `RETURNING id * 2`, `RETURNING UPPER(name)`)
  - Column aliases in RETURNING (e.g., `RETURNING id AS new_id`)
  - Subqueries in RETURNING (e.g., `RETURNING (SELECT COUNT(*) FROM other)`)
  - Aggregate functions in RETURNING supported via existing transpilation pipeline
  - Verified compatibility with triggers that modify NEW rows
  - SQLite 3.35.0+ native RETURNING support passes through complex expressions
  - 13 new integration tests in `tests/insert_tests.rs`
  - 4 new unit tests in `src/transpiler/mod.rs`

### Added
- **Statistical Aggregate Functions (Phase 3.3)**: Implemented internal statistical accumulator functions
  - `float8_accum(real[], real)` - Accumulates values for statistical computation [n, sum, sum_sqr]
  - `float8_combine(real[], real[])` - Combines two accumulators element-wise for parallel aggregation
  - `float8_regr_accum(real[], real, real)` - Accumulates for regression analysis [n, sum_x, sum_x2, sum_y, sum_y2, sum_xy]
  - `float8_regr_combine(real[], real[])` - Combines two regression accumulators element-wise
  - Accumulators stored as JSON strings in SQLite for compatibility
  - Supports parallel aggregation pattern via combine functions
  - Full unit tests in `src/stats_accum.rs` (15 new tests)
  - Integration tests in `tests/stats_accum_tests.rs` (12 new tests)
  - Documentation in `docs/AGGREGATES.md`

### Added
- **Bitwise Aggregate Functions (Phase 3.2)**: Implemented PostgreSQL-compatible bitwise aggregate functions
  - `bit_and(integer)` - Bitwise AND of all non-null values, returns NULL for empty set
  - `bit_or(integer)` - Bitwise OR of all non-null values, returns NULL for empty set
  - `bit_xor(integer)` - Bitwise XOR of all non-null values, returns NULL for empty set
  - NULL values are skipped during aggregation
  - Full unit tests in `src/bool_aggregates.rs` (16 new tests)
  - Integration tests in `tests/bool_aggregate_tests.rs` (6 new tests)
  - Documentation in `docs/AGGREGATES.md`

### Added
- **Boolean Aggregate Functions (Phase 3.1)**: Implemented PostgreSQL-compatible boolean aggregate functions
  - `bool_and(boolean)` - AND of all non-null values, returns true for empty set
  - `bool_or(boolean)` - OR of all non-null values, returns false for empty set
  - `every(boolean)` - SQL standard alias for bool_and
  - `booland_statefunc(boolean, boolean)` - State transition function for bool_and
  - `boolor_statefunc(boolean, boolean)` - State transition function for bool_or
  - NULL values are skipped during aggregation
  - Full unit tests in `src/bool_aggregates.rs`
  - Integration tests in `tests/bool_aggregate_tests.rs`
  - Documentation in `docs/AGGREGATES.md`

### Added
- **Interval Input Parsing (Phase 2.1)**: Implemented PostgreSQL interval type support
  - `Interval` struct in `src/interval.rs` with `months`, `days`, `microseconds` fields
  - Support for standard format: `'1 day 2 hours'`, `'1 year 6 months'`
  - Support for ISO 8601 format: `'P1Y2M3DT4H5M6S'`
  - Support for at-style format: `'@ 1 minute'`
  - Support for special values: `'infinity'`, `'-infinity'`
  - Fractional unit support: `'1.5 weeks'` converts to days and microseconds
  - Negative interval support: `'-1 day'`, `'1 day ago'`
  - Storage format: `months|days|microseconds` (delimited string)
  - `parse_interval()` function for SQL casts: `'1 day'::interval`
  - `parse_interval_storage()` for parsing storage format
  - Interval arithmetic functions: `interval_add`, `interval_sub`, `interval_mul`, `interval_div`, `interval_neg`
  - Interval comparison functions: `interval_eq`, `interval_lt`, `interval_le`, `interval_gt`, `interval_ge`, `interval_ne`
  - `extract_from_interval()` for EXTRACT functionality
  - Transpiler support for `::interval` casts in `src/transpiler/expr/utils.rs`
  - Functions registered in `src/handler/mod.rs`
  - Comprehensive unit tests in `src/interval.rs`
  - Integration tests in `tests/interval_tests.rs`
  - Documentation in `docs/INTERVAL.md`

### Added
- **JSON Operators (Phase 1.4)**: Implemented PostgreSQL-compatible JSON operators
  - `->` - Get JSON object field or array element (maps to `json_extract`)
  - `->>` - Get JSON object field or array element as text (maps to `json_extract`)
  - `#>` - Get JSON object at specified path (converts `{a,b}` to `$.a.b`)
  - `#>>` - Get JSON object at specified path as text
  - `@>` - JSON contains (uses `jsonb_contains` function)
  - `<@` - JSON is contained by (uses `jsonb_contained` function)
  - `?` - Does key exist? (uses `jsonb_exists` function)
  - `?\|` - Does any key exist? (uses `jsonb_exists_any` function)
  - `?&` - Do all keys exist? (uses `jsonb_exists_all` function)
  - `\|\|` - Concatenate JSON (uses new `json_concat` function)
  - `-` - Delete key/array element (uses `json_remove`)
  - `#-` - Delete at path (uses `json_remove` with path conversion)
  - Path conversion: PostgreSQL `{a,b,c}` → SQLite `$.a.b.c`
  - New `json_concat`, `json_delete`, `json_delete_path` functions in `src/jsonb.rs`
  - Operator handling in `src/transpiler/expr/operators.rs`
  - Integration tests in `tests/json_operator_tests.rs`
  - Documentation updated in `docs/JSON.md`

### Added
- **JSON Type Casting & Validation Functions (Phase 1.5)**: Implemented PostgreSQL-compatible JSON validation functions
  - `json_typeof(json)` - Returns type of JSON value (null, boolean, number, string, array, object)
  - `jsonb_typeof(jsonb)` - Returns type of JSONB value
  - `json_strip_nulls(json)` - Removes object fields with null values (recursively)
  - `jsonb_strip_nulls(jsonb)` - Removes object fields with null values from JSONB
  - `json_pretty(json)` - Pretty-prints JSON with indentation
  - `jsonb_pretty(jsonb)` - Pretty-prints JSONB with indentation
  - `jsonb_set(target, path, new_value)` - Updates value at path (creates if missing)
  - `jsonb_insert(target, path, new_value)` - Inserts value at path (error if exists)
  - Path format conversion: PostgreSQL `{a,b,c}` syntax supported
  - Support for nested path navigation in set/insert operations
  - Support for array index access in paths
  - New helper functions: `strip_nulls()`, `set_value_at_path()`, `parse_pg_path()`
  - Added `PathPart` enum for path component representation
  - Unit tests in `src/jsonb.rs` for all new functions
  - Integration tests in `tests/json_validation_tests.rs`
  - Documentation updated in `docs/JSON.md`

### Added
- **JSON Processing Functions (Phase 1.2)**: Implemented PostgreSQL-compatible JSON processing functions
  - `json_each(json)` - Expand JSON object/array to row set (key-value or index-value pairs)
  - `jsonb_each(jsonb)` - Expand JSONB object/array to row set
  - `json_each_text(json)` - Like json_each but returns text values
  - `jsonb_each_text(jsonb)` - Like jsonb_each but returns text values
  - `json_array_elements(json)` - Expand JSON array to row set (just elements)
  - `jsonb_array_elements(jsonb)` - Expand JSONB array to row set
  - `json_array_elements_text(json)` - Like json_array_elements but text
  - `jsonb_array_elements_text(jsonb)` - Like jsonb_array_elements but text
  - `json_object_keys(json)` - Return keys of JSON object as row set
  - `jsonb_object_keys(jsonb)` - Return keys of JSONB object as row set
  - Implemented as scalar functions returning JSON arrays, used with SQLite's json_each()
  - Transpiler support in `src/transpiler/func.rs` for automatic function transformation
  - Comprehensive unit tests in `src/jsonb.rs`
  - Integration tests in `tests/json_processing_tests.rs`
  - Documentation updated in `docs/JSON.md`

### Added
- **JSON Constructor Functions (Phase 1.1)**: Implemented PostgreSQL-compatible JSON constructor functions
  - `to_json(anyelement)` - Convert any value to JSON
  - `to_jsonb(anyelement)` - Convert any value to JSONB
  - `array_to_json(anyarray)` - Convert array to JSON array
  - `json_build_object(VARIADIC "any")` - Build JSON object from variadic args (0-10 args)
  - `jsonb_build_object(VARIADIC "any")` - Build JSONB object from variadic args (0-10 args)
  - `json_build_array(VARIADIC "any")` - Build JSON array from variadic args (0-10 args)
  - `jsonb_build_array(VARIADIC "any")` - Build JSONB array from variadic args (0-10 args)
  - Functions registered in `src/jsonb.rs` with SQLite custom function API
  - Transpiler mappings added in `src/transpiler/registry.rs`
  - Comprehensive unit tests in `src/jsonb.rs`
  - Integration tests in `tests/json_constructor_tests.rs`
  - Documentation in `docs/JSON.md`

## [0.7.3] - 2026-03-18

### Performance Improvements

#### COPY Handler Bulk Optimization - 85x Speedup
- **Transaction Wrapping**: COPY FROM operations now use explicit SQLite transactions
  - Eliminates auto-commit overhead for each row
  - Provides 3-5x performance improvement
- **Multi-Row INSERT Batching**: Data is batched into multi-row INSERT statements
  - Dynamic batch sizing based on column count (respects SQLite's 999 parameter limit)
  - 3 columns: 100 rows/batch, 10 columns: 99 rows/batch, 50 columns: 19 rows/batch
  - Provides additional 3-5x performance improvement
- **Overall Performance**: 10-20x target exceeded with measured 85x speedup
  - COPY: ~109,000 rows/sec
  - Individual INSERTs: ~1,276 rows/sec
  - Benchmark: `tests/copy_performance_test.py`

### Added
- `calculate_batch_size()`: Helper to compute optimal batch size within SQLite limits
- `build_multirow_insert_sql()`: Generate multi-row INSERT statements
- `execute_batch()`: Unified batch execution for text/CSV formats
- `execute_batch_binary()`: Batch execution for binary format
- Unit tests for batch size calculation and multi-row INSERT generation
- Performance benchmark test comparing COPY vs individual INSERTs

### Fixed
- `with_transaction()`: Now handles already-in-transaction case gracefully
- Empty columns handling: Falls back to single-row inserts when columns not specified
- All COPY e2e tests pass with new batching implementation

### Added
- **Optional TLS Support**: TLS is now an optional feature flag to reduce binary size
  - Default build includes TLS (~12MB)
  - Smaller build without TLS (~9.5MB, -2.5MB / -21%)
  - Use `cargo build --release --no-default-features --features plpgsql` for smaller build
  - New build scripts: `build-release.sh`, `build-release-small.sh`, `build-both.sh`
  - See `docs/build-options.md` for detailed build configuration

### Fixed
- **Compilation error in `src/copy.rs`**: Fixed type mismatch in `with_transaction` method
  - Added `R: Clone` bound to generic parameter
  - Fixed `Ok(r.clone())` to `Ok(r)` to avoid reference cloning issues

## [0.7.2] - 2026-03-18

### Added
- **Supabase Dump Compatibility**: Full support for `supabase db dump` files
  - `CREATE EXTENSION` statements now handled gracefully (no-op with warning)
  - Duplicate GRANT statements no longer cause UNIQUE constraint violations
  - Use `supabase db dump --schema public -x "auth.*,storage.*"` for clean dumps

### Fixed
- **CREATE EXTENSION**: Now returns a no-op comment instead of syntax error
  - Adds warning to transpilation result for logging/debugging
  - Example: `-- CREATE EXTENSION 'pg_cron' ignored - extensions not supported in SQLite`
- **GRANT Statement Duplicate Handling**: Changed INSERT to INSERT OR IGNORE
  - Fixed `UNIQUE constraint failed: __pg_acl__` errors on duplicate grants
  - Applied to table, schema, and function grants
  - Applied to role membership grants (`__pg_auth_members__`)

## [0.7.1] - 2026-03-18

### Fixed
- **Environment Variable Names**: Fixed mismatch between documented and actual environment variable names
  - Changed `PG_LITE_HOST` → `PGQT_HOST`
  - Changed `PG_LITE_PORT` → `PGQT_PORT`
  - Changed `PG_LITE_DB` → `PGQT_DB`
  - Changed `PG_LITE_OUTPUT` → `PGQT_OUTPUT`
  - Changed `PG_LITE_ERROR_OUTPUT` → `PGQT_ERROR_OUTPUT`
  - Changed `PG_LITE_DEBUG` → `PGQT_DEBUG`
  - Environment variables now work correctly (previously only CLI arguments worked)

## [0.7.0] - 2026-03-17

### Added
- **Connection Pooling**: Full connection pooling implementation with configurable pool size, timeouts, and health checks
- **Memory Management**: Buffer pool and memory monitoring with configurable thresholds
- **Memory-Mapped I/O**: Mmap support for large values with configurable size limits
- **Multi-Port Configuration**: JSON-based configuration for running multiple listeners on different ports
- **TLS/SSL Support**: Full TLS encryption with certificate and ephemeral (self-signed) certificate options
- **Unix Socket Support**: Unix domain socket listener alongside TCP
- **Performance Tuning**: Extensive SQLite PRAGMA configuration options (journal mode, synchronous, cache size, etc.)
- **Query Result Caching**: Optional result caching with TTL support
- **Transpile Caching**: SQL statement transpilation cache with configurable size and TTL
- **Output Redirection**: Configurable output destinations (stdout, stderr, file, null) per port
- **Debug Mode**: Per-port debug output configuration
- **Trust Mode**: Optional password-less authentication for development
- **Auto User Creation**: Automatic user creation on first connection (development feature)

### Infrastructure
- **Configuration System**: New `src/config.rs` module with JSON and CLI configuration support
- **TLS Module**: New `src/tls.rs` for certificate management and TLS handshake
- **Handler Refactoring**: Restructured handler module for multi-port support

## [0.6.3] - 2026-03-17

### Documentation
- **README.md**: Updated documentation to reflect current implementation status
  - Removed outdated "No stored procedures" limitation - PL/pgSQL is fully implemented
  - Removed outdated "Limited window functions" limitation - Full window function support exists
  - Marked PL/pgSQL as complete in Phase 3 roadmap
  - Marked connection pooling as complete in Phase 4 roadmap
  - Changed Phase 3 and Phase 4 status from "In Progress" to "Complete"

## [0.6.2] - 2026-03-17

### Fixed
- **Array operator detection**: Fixed PostgreSQL array literal detection for curly brace arrays like `{"a","b"}`
- **JSONB vs Array precedence**: Updated operator dispatch to correctly distinguish PostgreSQL arrays from JSONB objects
- **array_e2e_test.py**: All 21 E2E array tests now passing

## [0.6.1] - 2026-03-17

### PostgreSQL Compatibility Improvements

**Overall compatibility improved from 40.89% to 66.69% (+25.8%)**

#### JOIN Operations (Phase 1.1) - 33.0% → 70.8%
- **JOIN result aliasing**: `(t1 JOIN t2) AS x` now works by wrapping in subquery
- **USING clause support**: Proper handling of `JOIN ... USING (id)` with column deduplication
- **USING clause aliasing**: `JOIN ... USING (col) AS alias` support
- **NATURAL JOIN support**: `NATURAL JOIN`, `NATURAL LEFT JOIN`, `NATURAL RIGHT JOIN`
- **LATERAL polyfill**: Support for LATERAL with table-valued functions (`json_each`, `generate_series`, etc.)
- **NULLS FIRST/LAST**: Emulation in ORDER BY using `(col IS NULL)` expressions
- **Fixed panic**: `extract_table_and_operation` no longer panics on edge cases
- **count(table.*)**: Fixed aggregate function with qualified star

#### String Operations (Phase 1.2) - 27.1% → 73.1%
- `btrim(string [, characters])` - Trim from both ends
- `position(substring IN string)` - Find substring position
- `overlay(string PLACING replacement FROM start [FOR length])` - String overlay
- `trim_scale(numeric)` - Trim trailing zeros
- `string_to_array(string, delimiter [, null_string])` - Split string to array
- **Regular expression functions**:
  - `regexp_count(string, pattern [, start [, flags]])`
  - `regexp_like(string, pattern [, flags])`
  - `regexp_instr(string, pattern [, start [, occurrence [, return_option [, flags]]]])`
  - `regexp_replace` with 3-6 argument forms
  - `similar_to_escape(pattern, escape)`
- **Bytea encode/decode**: `encode()` and `decode()` for hex, escape, base64

#### DML Improvements (Phase 1.3) - 47% → 61.6% avg
- **RETURNING clause**: Full support for INSERT, UPDATE, DELETE
  - `INSERT INTO ... RETURNING id, name`
  - `INSERT INTO ... RETURNING *`
  - `UPDATE ... RETURNING ...`
  - `DELETE ... RETURNING ...`
- **ON CONFLICT (Upsert)**:
  - `INSERT ... ON CONFLICT DO NOTHING`
  - `INSERT ... ON CONFLICT (columns) DO UPDATE SET ...`
  - Support for `EXCLUDED` pseudo-table
- **INSERT DEFAULT VALUES**: `INSERT INTO table DEFAULT VALUES`
- **Multi-column assignment**: `(col1, col2) = (SELECT val1, val2)`
- **Row constructor support**: `ROW(v.*)` expansion in UPDATE

#### Window Functions (Phase 3.1) - 44.3% → 72.0%
- `nth_value(expression, nth)` with IGNORE NULLS support
- `first_value(expression)` and `last_value(expression)`
- `cume_dist()` - Cumulative distribution
- Hypothetical set functions: `rank()`, `dense_rank()`, `percent_rank()`, `cume_dist()`
- Window frame improvements: `ROWS BETWEEN`, `RANGE BETWEEN`

#### Array Operations (Phase 3.3)
- `array_sort(array [, descending [, nulls_first]])` - Sort arrays
- `array_sample(array, n)` - Random sample of n elements
- `array_reverse(array)` - Reverse array
- `array_shuffle(array)` - Shuffle array
- `array_to_json(array)` - Convert to JSON
- `array_lower(array, dim)` and `array_upper(array, dim)` - Bounds

#### Timestamptz (Phase 2.3) - 14.9% → 62.6%
- **AT TIME ZONE operator**: `timestamp AT TIME ZONE 'UTC'`
- **timezone() function**: `timezone('America/New_York', timestamp)`
- Timezone-aware scalar functions in SQLite

#### JSONB Framework (Phase 2.1)
- New JSONB operator detection framework
- Foundation for `@>`, `<@`, `?`, `?|`, `?&` operators
- Operator precedence fixes (array vs JSONB dispatch)

#### Quick Wins (Phase 4.1)
- VARCHAR type improvements
- `isfinite()` function for interval/range types
- Various syntax error fixes

### Fixed
- Prioritized array operations over JSONB in operator dispatch
- Various build warnings and unused imports

## [Unreleased]

### Added
- **DML Improvements (Phase 1.3)**: Enhanced INSERT/UPDATE/DELETE statement support
  - **RETURNING clause support**: Added for INSERT, UPDATE, and DELETE statements
    - `INSERT INTO ... RETURNING id` - Return specific columns
    - `INSERT INTO ... RETURNING *` - Return all columns
    - `UPDATE ... RETURNING ...` - Return updated rows
    - `DELETE ... RETURNING ...` - Return deleted rows
  - **ON CONFLICT (Upsert) support**: Full PostgreSQL-compatible upsert functionality
    - `INSERT ... ON CONFLICT DO NOTHING` - Skip on conflict
    - `INSERT ... ON CONFLICT (columns) DO NOTHING` - Skip on specific conflict
    - `INSERT ... ON CONFLICT (columns) DO UPDATE SET ...` - Update on conflict
    - Support for `EXCLUDED` pseudo-table in DO UPDATE clauses
  - **INSERT with multiple VALUES rows**: Enhanced multi-row insertion support
  - **UPDATE with FROM clause**: Support for UPDATE with JOINs via FROM clause

### Added
- **String Operations Enhancement**: Improved PostgreSQL string function compatibility
  - `btrim(string [, characters])` - Trim characters from both ends
  - `position(substring IN string)` - Find position of substring
  - `overlay(string PLACING replacement FROM start [FOR length])` - String overlay
  - `decode(string, 'hex'|'escape'|'base64')` - Decode encoded strings to bytea
  - `encode(blob, 'hex'|'escape'|'base64')` - Encode bytea to various formats
  - `trim_scale(numeric)` - Trim trailing zeros from numeric values
  - `string_to_array(string, delimiter [, null_string])` - Split string to array
  - `regexp_count(string, pattern [, start [, flags]])` - Count pattern occurrences
  - `regexp_like(string, pattern [, flags])` - Check if pattern matches
  - `regexp_instr(string, pattern [, start [, occurrence [, return_option [, flags]]]])` - Find pattern position
  - `similar_to_escape(pattern, escape)` - SIMILAR TO pattern conversion helper
  - Enhanced `regexp_replace` with support for 3-6 argument forms
  - PostgreSQL compatibility suite string operations pass rate: 66.0% → 73.1%

### Added
- **JOIN Improvements**: Enhanced JOIN operation support
  - JOIN result aliasing: `(t1 JOIN t2) AS x` now works by wrapping in subquery
  - USING clause aliasing: `JOIN ... USING (col) AS alias` support
  - NATURAL JOIN support: `NATURAL JOIN`, `NATURAL LEFT JOIN`, etc.
  - Added warning for column renaming in table aliases (not supported in SQLite)

## [0.2.0] - 2026-03-14

### Added
- **Enum Type Support**: `CREATE TYPE ... AS ENUM` transpilation to SQLite `TEXT` with `CHECK` constraints.
- **Session Configuration**: Support for `SET` and `set_config()` with per-session persistence.
- **Improved LATERAL Joins**: Explicit support for table-valued functions in `LATERAL` joins and graceful errors for unsupported subquery `LATERAL` joins.
- **COMMENT ON Storage**: Real persistence for `COMMENT ON` metadata in the `__pg_description__` shadow table.
- **System Catalog Polish**: 
  - `pg_enum` system view.
  - Formatted `relacl`, `attacl`, and `nspacl` in catalog views.
  - Better `pg_proc.proargtypes` population using OIDs.
  - Support for `obj_description` and `pg_get_function_arguments` stubs.

### Fixed
- Fixed build warnings across the codebase.
- Improved `SessionContext` management with thread-local client tracking.

### Documentation
- Created `docs/ENUMS.md` and `docs/SETTINGS.md`.
- Updated feature list in `README.md`.

### Added
- **Trigger Support**: Full support for `BEFORE`/`AFTER` triggers on `INSERT`, `UPDATE`, and `DELETE`.
- **PL/pgSQL Runtime**: Lua-based execution environment for trigger functions and user-defined functions.
- **Trigger Functions**: Added support for several PostgreSQL built-in functions in triggers:
  - `NOW()`, `CURRENT_TIMESTAMP`, `CURRENT_DATE`, `CURRENT_TIME`
  - `COALESCE()`, `NULLIF()`
  - `LOWER()`, `UPPER()`, `LENGTH()`, `REPLACE()`, `TRIM()`, `SUBSTRING()`
  - `ABS()`, `ROUND()`, `CEIL()`, `FLOOR()`, `GREATEST()`, `LEAST()`
  - `DATE_TRUNC()`, `EXTRACT()`, `AGE()`
- **Multi-Row Trigger Support**: True "FOR EACH ROW" semantics for multi-row `UPDATE` and `DELETE` statements. Triggers now fire for every affected row, and `BEFORE` triggers can modify individual rows in a multi-row operation.

### Fixed
- Fixed several build warnings related to unused variables and imports.
- Improved SQL deparsing for `WHERE` clauses in trigger contexts.
- Correctly apply trigger-modified `NEW` values to the database for `INSERT` operations.

### Documentation
- Created `docs/TRIGGERS.md` with comprehensive usage guides and examples.
- Updated `README.md` with trigger features and roadmap status.
