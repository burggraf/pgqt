# Users & Permissions (RBAC) Implementation Status

## Overview
This document tracks the progress of implementing PostgreSQL-compatible Role-Based Access Control (RBAC) in PostgreSQLite.

## Completed Tasks

### 1. Internal Storage Schema ✅
- ✅ `__pg_authid__`: Roles and capabilities (created)
- ✅ `__pg_auth_members__`: Role membership tree (created)
- ✅ `__pg_acl__`: Object-level privileges (created)
- ✅ Bootstrapped `postgres` superuser (OID 10)

### 2. System Catalog Integration ✅
- ✅ Updated `pg_roles` view to use `__pg_authid__`
- ✅ Added `pg_authid` view
- ✅ Updated `pg_auth_members` view

### 3. Transpiler Enhancements ✅
- ✅ Added `referenced_tables: Vec<String>` to `TranspileResult`
- ✅ Added `operation_type: OperationType` to `TranspileResult`
- ✅ Implemented `CreateRoleStmt` transpilation → `INSERT INTO __pg_authid__`
- ✅ Implemented `DropRoleStmt` transpilation → `DELETE FROM __pg_authid__`
- ✅ Implemented `GrantStmt` transpilation → `INSERT/DELETE FROM __pg_acl__`
- ✅ Implemented `GrantRoleStmt` transpilation → `INSERT/DELETE FROM __pg_auth_members__`

### 4. Session Management ✅
- ✅ Added `SessionContext` struct with `current_user` and `authenticated_user`
- ✅ Integrated `dashmap` for thread-safe session storage
- ✅ `SqliteHandler` now maintains a `sessions: Arc<DashMap<u32, SessionContext>>`

### 5. Permission Enforcement Engine ✅
- ✅ Implemented `SqliteHandler::check_permissions()` function
- ✅ Supports role inheritance via `WITH RECURSIVE` CTE
- ✅ Checks for superuser (bypasses all checks)
- ✅ Maps SQL operations to privileges (`SELECT`, `INSERT`, `UPDATE`, `DELETE`)
- ✅ Returns PostgreSQL error code `42501` for permission denied

### 6. Integration with Executable Handler ✅
- ✅ Updated `do_query` to store user metadata from client
- ✅ Permission checks run before query execution
- ✅ `CREATE TABLE` still works and stores metadata

## Remaining Tasks

### 1. Ownership Tracking ⚠️
Tasks:
- Track table owner during `CREATE TABLE`
- Store owner in `__pg_relation_meta__` table
- Owner has implicit all privileges

### 2. System Catalog Functions ✅
The basic stubs are in place:
- `has_table_privilege`
- `has_database_privilege`
- `has_schema_privilege`
- `pg_has_role`

These can be enhanced to query the ACL tables directly.

### 3. `SET ROLE` Support ⚠️
The transpiler needs to handle `SET ROLE` statements to update the session context.

### 4. Catalog Updates for `pg_class` ⚠️
Update the `pg_class` view in `src/catalog.rs` to join with `__pg_relation_meta__` to show correct `relowner`.

## Testing

All unit tests pass (37 tests).

Sample RBAC commands now work:
```sql
-- Create roles
CREATE ROLE app_user WITH LOGIN;
CREATE ROLE admin WITH SUPERUSER;

-- Grant role membership
GRANT admin TO app_user;

-- Grant privileges
GRANT SELECT ON users TO app_user;
GRANT INSERT, UPDATE ON orders TO app_user;

-- Revoke privileges
REVOKE INSERT ON orders FROM app_user;

-- Table access is enforced
SELECT * FROM users;  -- Allowed if granted or superuser
INSERT INTO orders;   -- Denied if not granted
```

## Known Limitations

1. **Schema Support**: Only `public` and `pg_catalog` schemas are supported. Other schemas would require ATTACH statements.
2. **Function-Level Permissions**: Only table/view permissions are implemented.
3. **Policy Enforcement**: Some edge cases around privilege resolution may need refinement.
4. **Interactive Session**: The session mapping uses a fixed key `0` instead of client-specific IDs.

## Files Modified

- `src/catalog.rs`: Added RBAC tables and views
- `src/transpiler.rs`: Added attribute extraction and DDL support
- `src/main.rs`: Added session management and permission checking
- `Cargo.toml`: Added `dashmap` dependency
- `docs/RBAC-DETAILED-PLAN.md`: Implementation plan
- `docs/RBAC-IMPLEMENTATION-PLAN.md`: High-level plan
