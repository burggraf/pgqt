# PGQT Deferred Features & Trade-offs (Schema Compatibility)

This document tracks intentional trade-offs made during the implementation of schema compatibility fixes. These items were simplified or "no-oped" to unblock the loading of complex PostgreSQL schemas (like `im-schema.sql`) and should be revisited for full implementation as needed.

## 1. Session Configuration (`set_config`)

**Current State**: Implemented as a no-op SQLite UDF that returns its second argument (`value`).
**Trade-off**: It does not actually change any session state.
**Impact**: Commands like `SET search_path` or `SELECT set_config('search_path', ...)` will succeed, but `pgqt` will continue using its default schema resolution logic.
**Future Work**:
- Integrate `set_config('search_path', ...)` with `src/schema.rs` to actually update the connection's `search_path`.
- Support other common PostgreSQL session variables (e.g., `timezone`, `extra_float_digits`).

## 2. Custom Types (`CREATE TYPE ... AS ENUM`)

**Current State**: `CREATE TYPE` statements are commented out, and unknown types are automatically rewritten to `TEXT`.
**Trade-off**: Loss of type safety and enum validation. SQLite will accept any string in what was originally an enum column.
**Impact**: Schema loads successfully, but the database doesn't enforce that values must be within the enum set.
**Future Work**:
- Store enum values in the shadow catalog (`__pg_catalog__.pg_enum`).
- Transpile enum columns to `TEXT` with a `CHECK (column IN ('val1', 'val2', ...))` constraint for data integrity.

## 3. Permissions & Ownership (`GRANT`, `REVOKE`, `ALTER OWNER`)

**Current State**: These statements are commented out during transpilation.
**Trade-off**: Complete bypass of PostgreSQL's security model.
**Impact**: All users are treated with the same permissions as the underlying SQLite file process. This is consistent with SQLite's security model but fails PostgreSQL's expectation of multi-user access control.
**Future Work**:
- If multi-user support is required, track these permissions in the catalog.
- Integrate with the RLS (Row-Level Security) module already in `pgqt`.

## 4. Object Documentation (`COMMENT ON`)

**Current State**: `COMMENT ON` statements are commented out.
**Trade-off**: Metadata is lost.
**Impact**: Tools that rely on comments (like PostgREST or database documentation generators) will not see any descriptions for tables or columns.
**Future Work**:
- Store comments in a dedicated shadow catalog table (e.g., `__pg_catalog__.pg_description`).
- Expose these comments via the `pg_catalog` system views.

## 5. Schema Defaults (`ALTER DEFAULT PRIVILEGES`)

**Current State**: Commented out.
**Trade-off**: Future objects created in a schema will not automatically receive the intended permissions.
**Impact**: Primarily affects automation and complex multi-role environments.
**Future Work**:
- Track default privileges in the catalog and apply them during `CREATE` operations.
