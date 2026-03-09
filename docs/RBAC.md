# Role-Based Access Control (RBAC) in PGQT

## Overview

PGQT implements a PostgreSQL-compatible Role-Based Access Control (RBAC) system for SQLite. It allows you to manage users, groups (roles), and permissions using standard PostgreSQL syntax. These permissions are enforced by the PGQT proxy before queries are executed against the underlying SQLite database.

Since SQLite does not have a native concept of users or fine-grained permissions, PGQT emulates this by:
1. Maintaining an internal system catalog in SQLite (`__pg_authid__`, `__pg_auth_members__`, `__pg_acl__`, `__pg_relation_meta__`).
2. Tracking session state for each connected client.
3. Transpiling and executing RBAC-related DDL commands.
4. Intercepting every query to verify that the current user has the required privileges.

## Managing Roles

### Creating Roles
Roles can represent individual users or groups of users.

```sql
-- Create a login role (user)
CREATE ROLE alice WITH LOGIN PASSWORD 'secure_password';

-- Create a group role
CREATE ROLE developers;

-- Create a superuser
CREATE ROLE admin WITH SUPERUSER;
```

Supported attributes:
- `LOGIN`: Allows the role to be used as a connection identity.
- `SUPERUSER`: Bypasses all permission checks.
- `INHERIT`: Privileges of roles this role is a member of are inherited (default).
- `PASSWORD`: Stores an encrypted password for authentication.

### Dropping Roles
```sql
DROP ROLE alice;
DROP ROLE developers;
```

### Role Membership
Roles can be members of other roles, creating a hierarchy.

```sql
-- Make alice a member of the developers group
GRANT developers TO alice;

-- Remove alice from the developers group
REVOKE developers FROM alice;
```

## Managing Privileges

PGQT supports granting and revoking privileges on tables and views.

### Granting Privileges
```sql
-- Grant read access on a table to a user
GRANT SELECT ON customers TO alice;

-- Grant multiple privileges to a group
GRANT INSERT, UPDATE, DELETE ON orders TO developers;

-- Grant all privileges to PUBLIC (all roles)
GRANT ALL PRIVILEGES ON products TO PUBLIC;
```

Supported privileges:
- `SELECT`: Required to read from a table or view.
- `INSERT`: Required to add new rows.
- `UPDATE`: Required to modify existing rows.
- `DELETE`: Required to remove rows.
- `ALL PRIVILEGES`: Grants all of the above.

### Revoking Privileges
```sql
REVOKE DELETE ON orders FROM developers;
REVOKE ALL PRIVILEGES ON customers FROM alice;
```

## Ownership

The role that creates a table or view is automatically its **owner**. Owners have all privileges on their objects implicitly, and these privileges cannot be revoked.

Currently, ownership is tracked in the `__pg_relation_meta__` internal table.

### Changing Ownership
```sql
ALTER TABLE customers OWNER TO alice;
```

## Session Management

### Current User
When you connect to PGQT, your identity is established based on the connection parameters. This becomes the `authenticated_user`. By default, the `current_user` is the same as the `authenticated_user`.

### Switching Roles
You can switch your active role within a session if you have the necessary permissions.

```sql
-- Switch to a different role
SET ROLE developers;

-- Switch back to the original authenticated user
RESET ROLE;
SET ROLE NONE;
```

## Permission Enforcement

PGQT enforces permissions at the proxy level:
1. **Parser**: The proxy parses the incoming SQL to identify the operation type and all referenced tables.
2. **Resolution**: It resolves the `current_user`'s effective roles, including inherited roles.
3. **Validation**:
   - If the user is a `SUPERUSER`, the query is allowed.
   - For each table:
     - If the user is the owner, access is allowed.
     - If the user (or `PUBLIC`) has been granted the required privilege, access is allowed.
4. **Execution**: If all checks pass, the query is transpiled to SQLite SQL and executed. Otherwise, an error is returned to the client.

## System Catalog Views

You can inspect the RBAC state using standard PostgreSQL system views:

- `pg_roles`: Lists all roles and their attributes.
- `pg_authid`: Detailed role information (internal).
- `pg_auth_members`: Shows role inheritance/membership.
- `pg_class`: Includes the `relowner` column to show table owners.
- `pg_tables`: Shows tables and their owners.

## Examples

### Complete Workflow
```sql
-- As superuser (postgres)
CREATE ROLE app_user WITH LOGIN;
CREATE ROLE reporting_group;

CREATE TABLE data (id SERIAL, val TEXT);
GRANT SELECT ON data TO reporting_group;
GRANT reporting_group TO app_user;

-- Now connect as app_user
SELECT * FROM data; -- Works (via reporting_group membership)
INSERT INTO data VALUES (1, 'test'); -- Error: permission denied
```

## Error Messages

If a user lacks sufficient privileges, PGQT returns a standard PostgreSQL error:
- **SQLSTATE**: `42501`
- **Message**: `permission denied for table <table_name>`

## Limitations

1. **Schema Privileges**: Currently, permissions are primarily enforced at the table level. Granular schema-level permissions (`USAGE`, `CREATE`) are not yet fully implemented.
2. **Function Privileges**: `EXECUTE` privileges on functions are not yet enforced.
3. **Column-Level Privileges**: Permissions apply to the entire table; column-level `GRANT` is not supported.
4. **Row-Level Security (RLS)**: While PGQT supports RLS, it is managed separately from the base RBAC system (see `RLS.md`).
5. **Concurrent DDL**: Rapidly changing permissions while other queries are running may have slight latency in enforcement due to SQLite's locking model on internal tables.
