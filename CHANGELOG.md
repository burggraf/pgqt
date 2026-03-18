# Changelog

All notable changes to PGQT will be documented in this file.

## [Unreleased]

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
