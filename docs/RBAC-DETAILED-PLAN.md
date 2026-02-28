# Detailed Implementation Plan: Users & Permissions (RBAC)

This plan details the architecture and steps to implement a robust Role-Based Access Control (RBAC) system in PostgreSQLite.

## 1. Internal Storage Schema

We emulate PostgreSQL's system catalogs using internal SQLite tables.

### 1.1 Tables in `src/catalog.rs`
- `__pg_authid__`: Roles and capabilities.
  ```sql
  CREATE TABLE __pg_authid__ (
      oid INTEGER PRIMARY KEY,
      rolname TEXT UNIQUE NOT NULL,
      rolsuper BOOLEAN DEFAULT FALSE,
      rolinherit BOOLEAN DEFAULT TRUE,
      rolcreaterole BOOLEAN DEFAULT FALSE,
      rolcreatedb BOOLEAN DEFAULT FALSE,
      rolcanlogin BOOLEAN DEFAULT FALSE,
      rolpassword TEXT
  );
  ```
- `__pg_auth_members__`: Role membership tree.
  ```sql
  CREATE TABLE __pg_auth_members__ (
      roleid INTEGER NOT NULL,
      member INTEGER NOT NULL,
      grantor INTEGER NOT NULL,
      admin_option BOOLEAN DEFAULT FALSE,
      PRIMARY KEY (roleid, member)
  );
  ```
- `__pg_acl__`: Object-level privileges.
  ```sql
  CREATE TABLE __pg_acl__ (
      object_id INTEGER NOT NULL,
      object_type TEXT NOT NULL, -- 'relation', 'database', 'schema'
      grantee_id INTEGER NOT NULL, -- role OID or 0 for PUBLIC
      privilege TEXT NOT NULL, -- 'SELECT', 'INSERT', 'UPDATE', 'DELETE'
      grantable BOOLEAN DEFAULT FALSE,
      grantor_id INTEGER NOT NULL,
      PRIMARY KEY (object_id, object_type, grantee_id, privilege)
  );
  ```
- `__pg_relation_meta__`: Table ownership.
  ```sql
  CREATE TABLE __pg_relation_meta__ (
      relname TEXT PRIMARY KEY,
      relowner INTEGER NOT NULL
  );
  ```

## 2. Transpiler Enhancements

### 2.1 Metadata Extraction
The transpiler must return:
1. `referenced_tables: Vec<String>`: Extracted from `RangeVar` nodes.
2. `operation_type`: `SELECT`, `INSERT`, `UPDATE`, `DELETE`, or `DDL`.

### 2.2 RBAC DDL Support
Translate PostgreSQL RBAC commands to internal table operations:
- `CREATE ROLE name` -> `INSERT INTO __pg_authid__`
- `GRANT SELECT ON table TO role` -> `INSERT INTO __pg_acl__`
- `GRANT group_role TO user_role` -> `INSERT INTO __pg_auth_members__`

## 3. Proxy Session Management

Use `dashmap` to track sessions per client connection.

```rust
struct SessionContext {
    authenticated_user: String,
    current_user: String,
}

struct SqliteHandler {
    sessions: DashMap<u32, SessionContext>,
    // ...
}
```

## 4. Permission Enforcement Engine

Before executing any query, the proxy performs the following check:

### 4.1 Role Resolution (Recursive)
Retrieve all effective roles for the `current_user`:
```sql
WITH RECURSIVE effective_roles AS (
    SELECT oid FROM __pg_authid__ WHERE rolname = :current_user
    UNION
    SELECT m.roleid FROM __pg_auth_members__ m
    JOIN effective_roles er ON er.oid = m.member
)
SELECT oid FROM effective_roles;
```

### 4.2 Check Logic
1. If any effective role is a `superuser` (`rolsuper = 1`), **ALLOW**.
2. For each table in `referenced_tables`:
   - If the user is the owner (`__pg_relation_meta__.relowner`), **ALLOW**.
   - If a record exists in `__pg_acl__` for any effective role (or `grantee_id = 0` for PUBLIC) matching the `operation_type`, **ALLOW**.
3. Otherwise, **DENY** and return PostgreSQL error `42501`.

## 5. System Catalog Integration

Update `src/catalog.rs` views to point to internal tables:
- `pg_roles`, `pg_authid`, `pg_auth_members`.
- `pg_class` (join with `__pg_relation_meta__` to show correct `relowner`).

## 6. Implementation Steps

1. **Catalog Update**: Modify `init_catalog` to create the 4 tables and initialize `postgres` superuser.
2. **Transpiler Update**: Modify `reconstruct_sql_with_metadata` to collect table names and handle `CreateRoleStmt`, `GrantStmt`, etc.
3. **Session State**: Add `dashmap` to `SqliteHandler` and update it on connection/`SET ROLE`.
4. **Enforcer**: Implement the `check_permissions` function in `SqliteHandler`.
5. **Ownership Tracking**: Update `execute_query` for `CREATE TABLE` to store the creator as the owner in `__pg_relation_meta__`.
6. **Custom Functions**: Implement `has_table_privilege` etc. as real checks instead of returning `true`.
