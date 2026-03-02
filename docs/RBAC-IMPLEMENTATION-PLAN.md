# Implementation Plan: Users & Permissions (RBAC) in PGQT

This plan outlines the steps to emulate PostgreSQL's Role-Based Access Control (RBAC) in SQLite via the `pgqt` proxy.

## Goals
- Support `CREATE ROLE`, `DROP ROLE`, `ALTER ROLE`.
- Support `GRANT` and `REVOKE` for both roles and object privileges.
- Enforce permissions on `SELECT`, `INSERT`, `UPDATE`, `DELETE`.
- Mimic PostgreSQL system catalogs (`pg_roles`, `pg_authid`, `pg_auth_members`, etc.).

## Phase 1: Storage & System Catalogs

### 1.1 Internal Metadata Tables
Create the following tables in `src/catalog.rs`:

- `__pg_authid__`: Stores roles and their attributes.
  - `oid`: INTEGER PRIMARY KEY (Manual OID management or autoincrement)
  - `rolname`: TEXT UNIQUE NOT NULL
  - `rolsuper`: BOOLEAN DEFAULT FALSE
  - `rolinherit`: BOOLEAN DEFAULT TRUE
  - `rolcreaterole`: BOOLEAN DEFAULT FALSE
  - `rolcreatedb`: BOOLEAN DEFAULT FALSE
  - `rolcanlogin`: BOOLEAN DEFAULT FALSE
  - `rolpassword`: TEXT (encrypted)

- `__pg_auth_members__`: Stores role membership (who belongs to which group).
  - `roleid`: INTEGER (The group/parent role OID)
  - `member`: INTEGER (The member role OID)
  - `grantor`: INTEGER (Who performed the grant)
  - `admin_option`: BOOLEAN DEFAULT FALSE
  - PRIMARY KEY (`roleid`, `member`)

- `__pg_acl__`: Stores Access Control Lists for objects.
  - `object_id`: INTEGER (OID of the table/view/schema/etc.)
  - `object_type`: TEXT (e.g., 'relation', 'database', 'schema')
  - `grantee_id`: INTEGER (Role OID, or 0 for PUBLIC)
  - `privilege`: TEXT (e.g., 'SELECT', 'INSERT', 'UPDATE', 'DELETE')
  - `grantable`: BOOLEAN DEFAULT FALSE
  - `grantor_id`: INTEGER
  - PRIMARY KEY (`object_id`, `object_type`, `grantee_id`, `privilege`)

### 1.2 Update System Views
Update `init_system_views` to point `pg_roles`, `pg_shadow`, `pg_authid`, `pg_auth_members` to the new internal tables.

### 1.3 Bootstrap
Initialize the catalog with a default `postgres` superuser (OID 10).

## Phase 2: Transpiler Enhancements

### 2.1 Object Extraction
Modify `TranspileResult` to include:
- `referenced_tables: Vec<String>`
- `operation_type: OperationType` (SELECT, INSERT, UPDATE, DELETE, DDL, etc.)

Update the AST walker to collect table names from `RangeVar` nodes.

### 2.2 RBAC DDL Support
Extend `reconstruct_sql_with_metadata` to handle:
- `CreateRoleStmt`: Map to `INSERT INTO __pg_authid__`.
- `DropRoleStmt`: Map to `DELETE FROM __pg_authid__`.
- `GrantRoleStmt`: Map to `INSERT INTO __pg_auth_members__`.
- `GrantStmt`: Map to `INSERT INTO __pg_acl__`.
- `SetRoleStmt`: Track in the proxy session (don't send to SQLite).

## Phase 3: Enforcement Engine

### 3.1 Session Context
In `SqliteHandler`, maintain a `SessionContext`:
- `authenticated_user`: The user from pgwire handshake.
- `current_user`: The user currently in effect (via `SET ROLE`).

### 3.2 Permission Checker
Implement a `check_permissions` function:
1. Resolve effective roles for `current_user` (recursive lookup in `__pg_auth_members__`).
2. If `rolsuper` is true for any effective role, allow.
3. For each referenced table:
   - Identify required privilege based on `operation_type`.
   - Check if any effective role (or PUBLIC) has the privilege in `__pg_acl__`.
   - Also check ownership (owners have all privileges).

### 3.3 Integration
Hook `check_permissions` into `SqliteHandler::execute_query` before any execution on the SQLite connection.

## Phase 4: Compatibility Functions

Implement PostgreSQL permission check functions as SQLite custom functions:
- `has_table_privilege(user, table, privilege)`
- `has_database_privilege(user, database, privilege)`
- `has_schema_privilege(user, schema, privilege)`
- `pg_has_role(user, role, privilege)`

## Phase 5: Testing
- Create a test suite `tests/rbac_tests.rs`.
- Test `CREATE ROLE` and `GRANT`.
- Test that a non-superuser cannot access a table without `GRANT`.
- Test role inheritance.
- Test `SET ROLE`.
