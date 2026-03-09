# PGQT Deferred Features & Trade-offs (Schema Compatibility)

This document tracks intentional trade-offs made during the implementation of schema compatibility fixes. It has been updated to reflect features recently completed in the RBAC implementation phase.

## 1. Completed Features ✅

The following features were previously deferred but are now fully implemented:

### Permissions & Ownership (`GRANT`, `REVOKE`, `ALTER OWNER`)
- **Status**: Implemented.
- **Details**: Full support for `GRANT` and `REVOKE` on tables, schemas, and functions. `ALTER OWNER` is supported for tables and functions.
- **Enforcement**: Permissions are verified by the proxy before query execution.

### Schema Defaults (`ALTER DEFAULT PRIVILEGES`)
- **Status**: Implemented.
- **Details**: Default privileges are tracked in the `__pg_default_acl__` catalog and applied during object creation.

---

## 2. Currently Deferred Features ⏳

The following items remain simplified or "no-oped" and should be revisited for full implementation as needed.

### Session Configuration (`set_config`)
- **Current State**: While `SET search_path` and `SET ROLE` are explicitly handled in the handler, the `set_config(name, value, is_local)` SQLite UDF is still a no-op that simply returns the value.
- **Impact**: Dynamic session configuration via function calls (common in some ORMs) does not yet update proxy state.
- **Future Work**: Integrate `set_config` with the `SessionContext` in `src/handler/mod.rs`.

### Custom Types (`CREATE TYPE ... AS ENUM`)
- **Current State**: `CREATE TYPE` statements are commented out, and unknown types are automatically rewritten to `TEXT`.
- **Trade-off**: Loss of type safety and enum validation.
- **Future Work**:
    - Store enum values in the shadow catalog (`__pg_catalog__.pg_enum`).
    - Transpile enum columns to `TEXT` with a `CHECK` constraint.

### Object Documentation (`COMMENT ON`)
- **Current State**: The storage backend (`__pg_description__` table and `pg_description` view) is implemented, but the transpiler still returns `-- COMMENT IGNORED`.
- **Impact**: Metadata is not yet being populated into the catalog from SQL scripts.
- **Future Work**: Connect `CommentStmt` in `src/transpiler/mod.rs` to the catalog storage.
