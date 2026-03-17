# Changelog

All notable changes to PGQT will be documented in this file.

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
