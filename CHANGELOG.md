# Changelog

All notable changes to PGQT will be documented in this file.

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
