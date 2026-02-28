# Row-Level Security (RLS) in PGlite Proxy

Row-Level Security (RLS) is a powerful security feature that enables fine-grained access control by filtering rows based on the current user or session context. This document covers RLS implementation in PGlite Proxy, which provides PostgreSQL-compatible RLS semantics over SQLite.

## Table of Contents

1. [Introduction](#introduction)
2. [Comparison with PostgreSQL](#comparison-with-postgresql)
3. [Getting Started](#getting-started)
4. [Usage Examples](#usage-examples)
5. [ALTER TABLE Commands](#alter-table-commands)
6. [Built-in Functions](#built-in-functions)
7. [Limitations](#limitations)
8. [Security Best Practices](#security-best-practices)
9. [Troubleshooting](#troubleshooting)
10. [Complete Multi-User Scenario](#complete-multi-user-scenario)
11. [Internal Architecture](#internal-architecture)

---

## Introduction

### What is Row-Level Security?

Row-Level Security is a database security mechanism that restricts which rows users can access in a table. Unlike traditional table-level permissions (GRANT/REVOKE), RLS allows you to define policies that dynamically filter rows based on:

- The current user's identity
- The user's role memberships
- Other session context variables

### Why Use RLS in PGlite Proxy?

PGlite Proxy implements PostgreSQL-compatible RLS, allowing you to:

- **Build multi-tenant applications** where each tenant sees only their own data
- **Implement data isolation** between users or groups
- **Enforce mandatory access controls** using RESTRICTIVE policies
- **Leverage existing PostgreSQL knowledge** and tooling
- **Transition from PostgreSQL to SQLite** without rewriting security logic

### How RLS Works

RLS works by transparently injecting a WHERE clause into every DML query (SELECT, INSERT, UPDATE, DELETE) that references a protected table. This filtering happens automatically at the SQL transpilation level:

```
User Query:          SELECT * FROM documents
                          │
                          ▼
RLS Policy Check:    WHERE owner = current_user()
                          │
                          ▼
Final Query:         SELECT * FROM documents WHERE (owner = current_user())
```

---

## Comparison with PostgreSQL

| Feature | PostgreSQL | PGlite Proxy |
|---------|-----------|--------------|
| **Policy Storage** | `pg_policy` system catalog | `__pg_rls_policies__` table |
| **ENABLE/DISABLE** | `ALTER TABLE` syntax | Supported |
| **FORCE/NO FORCE** | Supported | Supported |
| **PERMISSIVE Policies** | Combined with OR | Combined with OR |
| **RESTRICTIVE Policies** | Combined with AND | Combined with AND |
| **current_user()** | Built-in | Custom SQLite function |
| **session_user** | Built-in | Custom SQLite function |

### Policy Combination Logic

PGlite Proxy implements PostgreSQL's exact policy combination semantics:

```
Final Expression = (permissive_1 OR permissive_2 OR ...) AND (restrictive_1 AND restrictive_2 AND ...)
```

- **PERMISSIVE** policies are combined with OR logic (any permissive policy grants access)
- **RESTRICTIVE** policies are combined with AND logic (all restrictive policies must pass)
- If no PERMISSIVE policies apply, access is denied by default

---

## Getting Started

### Step 1: Enable RLS on a Table

Before creating any policies, you must enable RLS on the target table:

```sql
ALTER TABLE employees ENABLE ROW LEVEL SECURITY;
```

### Step 2: Create a Policy

Define who can access what data using `CREATE POLICY`:

```sql
-- Simple row-level policy
CREATE POLICY user_policy ON employees
  FOR SELECT
  USING (user_id = current_user());
```

### Step 3: Test RLS Behavior

Query the table to see RLS in action:

```sql
-- As a regular user
SET ROLE employee;
SELECT * FROM employees;  -- Only shows rows where user_id matches current_user()
```

---

## Usage Examples

### Basic Setup

Create a table with RLS enabled:

```sql
CREATE TABLE employees (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    department TEXT,
    user_id TEXT NOT NULL
);

ALTER TABLE employees ENABLE ROW LEVEL SECURITY;
```

### Creating Policies

#### SELECT Policy

```sql
CREATE POLICY select_own_records ON employees
  FOR SELECT
  USING (user_id = current_user());
```

#### INSERT Policy with WITH CHECK

```sql
CREATE POLICY insert_own_records ON employees
  FOR INSERT
  WITH CHECK (user_id = current_user());
```

#### UPDATE Policy

```sql
CREATE POLICY update_own_records ON employees
  FOR UPDATE
  USING (user_id = current_user())
  WITH CHECK (user_id = current_user());
```

#### DELETE Policy

```sql
CREATE POLICY delete_own_records ON employees
  FOR DELETE
  USING (user_id = current_user());
```

### Policy Modes

#### PERMISSIVE (OR Logic)

Multiple PERMISSIVE policies are combined with OR:

```sql
-- User can see rows where col1 = 'a' OR col2 = 'b'
CREATE POLICY policy1 ON table1 AS PERMISSIVE FOR SELECT USING (col1 = 'a');
CREATE POLICY policy2 ON table1 AS PERMISSIVE FOR SELECT USING (col2 = 'b');
-- Result: (col1 = 'a') OR (col2 = 'b')
```

#### RESTRICTIVE (AND Logic)

Multiple RESTRICTIVE policies are combined with AND:

```sql
-- User must satisfy BOTH conditions
CREATE POLICY policy3 ON table2 AS RESTRICTIVE FOR SELECT USING (status = 'active');
CREATE POLICY policy4 ON table2 AS RESTRICTIVE FOR SELECT USING (dept = 'sales');
-- Result: (status = 'active') AND (dept = 'sales')
```

### Role-Based Policies

#### Apply to Specific Roles

```sql
-- Only admin and superuser roles can see all data
CREATE POLICY admin_policy ON sensitive_data
  TO admin, superuser
  USING (true);
```

#### Apply to PUBLIC

```sql
-- All users can see public posts
CREATE POLICY public_policy ON public_posts
  TO PUBLIC
  USING (is_public = true);
```

### Operations: SELECT, INSERT, UPDATE, DELETE

Each DML operation uses different policy clauses:

```sql
-- SELECT: Uses USING clause
CREATE POLICY select_policy ON documents
  FOR SELECT USING (owner = current_user());

-- INSERT: Uses WITH CHECK clause
CREATE POLICY insert_policy ON documents
  FOR INSERT WITH CHECK (owner = current_user());

-- UPDATE: Uses both USING (filter rows) and WITH CHECK (validate new values)
CREATE POLICY update_policy ON documents
  FOR UPDATE USING (owner = current_user())
  WITH CHECK (owner = current_user());

-- DELETE: Uses USING clause (filter rows to delete)
CREATE POLICY delete_policy ON documents
  FOR DELETE USING (owner = current_user());
```

### Combining All Operations in One Policy

```sql
-- Apply the same rules to all operations
CREATE POLICY all_access ON documents
  FOR ALL
  USING (owner = current_user())
  WITH CHECK (owner = current_user());
```

---

## ALTER TABLE Commands

### Enable RLS

```sql
ALTER TABLE employees ENABLE ROW LEVEL SECURITY;
```

### Disable RLS

```sql
ALTER TABLE employees DISABLE ROW LEVEL SECURITY;
```

Note: Disabling RLS does not delete policies—they remain in the metadata and can be re-enabled later.

### Force RLS for Owners

By default, table owners bypass RLS. Use FORCE to apply RLS to owners as well:

```sql
ALTER TABLE employees FORCE ROW LEVEL SECURITY;
```

### Disable Force (Default)

```sql
ALTER TABLE employees NO FORCE ROW LEVEL SECURITY;
```

### Modify Policies

```sql
-- Drop a policy
DROP POLICY policy_name ON table_name;

-- Modify which roles a policy applies to
ALTER POLICY policy_name ON table_name TO new_role;
```

---

## Built-in Functions

### current_user()

Returns the current session user. This is the primary function used in RLS policies:

```sql
CREATE POLICY user_policy ON documents
  FOR SELECT
  USING (owner = current_user());
```

### session_user()

Returns the session user (equivalent to `current_user()` in most cases):

```sql
CREATE POLICY session_policy ON logs
  FOR SELECT
  USING (session_user() = 'admin' OR level = 'public');
```

### Integration with RBAC System

PGlite Proxy's RLS integrates with the RBAC system to automatically resolve user roles:

```sql
-- Create roles
CREATE ROLE alice WITH LOGIN;
CREATE ROLE bob WITH LOGIN;
CREATE ROLE manager;

-- Grant role membership
GRANT manager TO alice;

-- Policies can target specific roles
CREATE POLICY manager_view ON projects
  TO manager
  USING (true);
```

---

## Limitations

### Subquery Handling

RLS policies applied to tables referenced in subqueries may not work as expected. The transpiler currently applies RLS to base tables only.

```sql
-- This may not filter correctly in subqueries
SELECT * FROM orders WHERE user_id IN (
    SELECT id FROM users WHERE email = current_user()
);
```

### JOIN Behavior

When querying across multiple tables, each table's RLS policies apply independently. There's no automatic RLS propagation through JOINs.

```sql
-- Both tables apply their own RLS policies
SELECT o.* FROM orders o
JOIN users u ON o.user_id = u.id;
```

### Transactions

RLS is applied on a per-statement basis. Multi-statement transactions do not have additional RLS protections beyond what individual statements provide.

### WITH CHECK on INSERT

SQLite doesn't support CHECK constraints on INSERT VALUES. PGlite Proxy handles this by transpiling INSERT statements with WITH CHECK into an equivalent form that validates the constraint.

### RESTRICTIVE Policy Limitations

A single RESTRICTIVE policy cannot use OR conditions within its expression. Use multiple RESTRICTIVE policies combined with AND logic instead.

---

## Security Best Practices

### Always Have at Least One PERMISSIVE Policy

When RLS is enabled but no policies exist (or none apply), access is denied by default. However, when creating policies:

1. Always ensure at least one PERMISSIVE policy covers your intended users
2. Test with different roles to verify access patterns
3. Use RESTRICTIVE policies for mandatory filters (like tenant isolation)

```sql
-- Good: Explicit permissive policy for users
CREATE POLICY user_select ON documents
  FOR SELECT
  TO PUBLIC
  USING (owner = current_user());
```

### Use RESTRICTIVE Policies for Mandatory Filters

RESTRICTIVE policies are combined with AND, making them ideal for mandatory access controls:

```sql
-- All users must be active AND belong to the organization
CREATE POLICY org_policy AS RESTRICTIVE FOR SELECT
  USING (org_id IN (SELECT org_id FROM user_orgs WHERE user_id = current_user()));
```

### Test Policies Thoroughly

Test with different roles and edge cases:

```sql
-- Test as different users
SET ROLE alice;
SELECT * FROM documents;  -- Should see only alice's docs

SET ROLE bob;
SELECT * FROM documents;  -- Should see only bob's docs

SET ROLE admin;
SELECT * FROM documents;  -- Should see all (if admin policy exists)
```

### Consider Performance Impact

Complex policy expressions can impact query performance:

- Avoid subqueries in policy USING clauses when possible
- Index columns used in policy expressions
- Consider materialized views for complex multi-table policies

### Combine RLS with RBAC

RLS is not a replacement for proper RBAC. Use both:

```sql
-- Layer 1: Table-level permissions (RBAC)
GRANT SELECT ON documents TO app_user;

-- Layer 2: Row-level filtering (RLS)
CREATE POLICY row_filter ON documents
  FOR SELECT
  USING (owner = current_user());
```

### Be Careful with FORCE ROW LEVEL SECURITY

Using `FORCE ROW LEVEL SECURITY` applies RLS to table owners, which can break applications that rely on owner access:

```sql
-- Only use when owner access should also be restricted
ALTER TABLE sensitive_data FORCE ROW LEVEL SECURITY;
```

---

## Troubleshooting

### Common Errors

#### "RLS policy prevented the operation"

This means no policy allowed the operation. Check:
- Are policies defined for the operation type (SELECT, INSERT, etc.)?
- Does the current user belong to a role targeted by the policy?
- Is the policy expression evaluating correctly?

#### "Cannot INSERT due to WITH CHECK policy"

The data being inserted doesn't satisfy the WITH CHECK expression:
```sql
-- This will fail if current_user is not 'alice'
INSERT INTO documents (owner, title) VALUES ('bob', 'Test');
```

### Debugging Policy Evaluation

Check which policies exist on a table:

```sql
SELECT * FROM __pg_rls_policies__ WHERE polrelid = 'documents'::regclass;
```

Check if RLS is enabled on a table:

```sql
SELECT * FROM __pg_relation_meta__ WHERE relname = 'documents';
```

### Performance Considerations

If RLS causes performance issues:

1. **Profile the queries**: Use EXPLAIN to see the injected WHERE clause
2. **Index policy columns**: Add indexes on columns used in USING expressions
3. **Simplify policy expressions**: Avoid complex subqueries in policies

---

## Complete Multi-User Scenario

This example demonstrates a complete multi-user document management system:

### Setup: Create Roles

```sql
CREATE ROLE alice WITH LOGIN;
CREATE ROLE bob WITH LOGIN;
CREATE ROLE admin WITH SUPERUSER;
```

### Setup: Create Table

```sql
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    owner TEXT NOT NULL,
    title TEXT,
    content TEXT,
    is_public BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT now()
);
```

### Setup: Enable RLS

```sql
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;
```

### Create Policies

```sql
-- Users can see their own documents and public ones
CREATE POLICY owner_select ON documents
  FOR SELECT TO alice, bob
  USING (owner = current_user() OR is_public = true);

-- Users can only insert their own documents
CREATE POLICY owner_insert ON documents
  FOR INSERT TO alice, bob
  WITH CHECK (owner = current_user());

-- Users can only update their own documents
CREATE POLICY owner_update ON documents
  FOR UPDATE TO alice, bob
  USING (owner = current_user())
  WITH CHECK (owner = current_user());

-- Users can only delete their own documents
CREATE POLICY owner_delete ON documents
  FOR DELETE TO alice, bob
  USING (owner = current_user());

-- Admin can see and modify everything
CREATE POLICY admin_full ON documents
  TO admin
  USING (true)
  WITH CHECK (true);
```

### Test as Alice

```sql
SET ROLE alice;

-- Alice inserts her document
INSERT INTO documents (owner, title, content) 
VALUES ('alice', 'Alice Private', 'This is private');

-- Alice inserts a public document
INSERT INTO documents (owner, title, content, is_public) 
VALUES ('alice', 'Alice Public', 'This is public', true);

-- Alice sees only her documents
SELECT * FROM documents;
-- Returns: Alice Private, Alice Public

-- Alice tries to see Bob's private document (will be filtered)
SET ROLE bob;
INSERT INTO documents (owner, title, content) 
VALUES ('bob', 'Bob Private', 'This is private');

SET ROLE alice;
SELECT * FROM documents;
-- Returns: Alice Private, Alice Public, Alice Public (not Bob's)
```

### Test as Bob

```sql
SET ROLE bob;

SELECT * FROM documents;
-- Returns: Bob Private, Alice Public (public docs from all users)
```

### Test as Admin

```sql
SET ROLE admin;

SELECT * FROM documents;
-- Returns: All documents (admin bypasses via superuser)
```

---

## Internal Architecture

This section provides details for developers working on RLS implementation.

### Metadata Storage

RLS policies are stored in the `__pg_rls_policies__` table:

| Column | Type | Description |
|--------|------|-------------|
| `polname` | TEXT | Policy name |
| `polrelid` | TEXT | Table name |
| `polcmd` | TEXT | Command: ALL, SELECT, INSERT, UPDATE, DELETE |
| `polpermissive` | BOOLEAN | true = PERMISSIVE, false = RESTRICTIVE |
| `polroles` | TEXT | Comma-separated role names or "PUBLIC" |
| `polqual` | TEXT | USING expression |
| `polwithcheck` | TEXT | WITH CHECK expression |

### Table Metadata

RLS status per table is stored in `__pg_relation_meta__`:

| Column | Type | Description |
|--------|------|-------------|
| `relname` | TEXT | Table name |
| `rls_enabled` | BOOLEAN | Whether RLS is enabled |
| `rls_forced` | BOOLEAN | Whether FORCE RLS is enabled |

### AST Injection

The transpiler in `src/transpiler.rs` uses `pg_query` to:

1. Parse the incoming PostgreSQL SQL into an AST
2. Identify the target table(s)
3. Look up applicable RLS policies
4. Build the RLS expression combining PERMISSIVE (OR) and RESTRICTIVE (AND) policies
5. Inject the expression into the WHERE clause at the AST level
6. Reconstruct SQLite-compatible SQL

### Policy Combination

The policy combination logic in `src/rls.rs`:

```rust
// Pseudocode for policy combination
let permissive_exprs: Vec<String> = ...;  // Policies with polpermissive = true
let restrictive_exprs: Vec<String> = ...; // Policies with polpermissive = false

let final_expr = match (permissive_exprs.len(), restrictive_exprs.len()) {
    (0, 0) => None,  // No policies - deny all
    (p > 0, 0) => Some(permissive_exprs.join(" OR ")),
    (0, r > 0) => Some(restrictive_exprs.join(" AND ")),
    (p > 0, r > 0) => Some(
        format!("({}) AND ({})", 
            permissive_exprs.join(" OR "),
            restrictive_exprs.join(" AND ")
        )
    ),
};
```

### Session Context

The RLS system uses the session context from `main.rs`:

```rust
struct SessionContext {
    authenticated_user: String,
    current_user: String,
    user_roles: Vec<String>,
    bypass_rls: bool,
}
```

### Bypass Logic

Users can bypass RLS if:
1. They have the `BYPASSRLS` attribute (superuser or explicit)
2. They are the table owner AND RLS is not FORCE-enabled

---

## Migration Guide: From View-Based to AST-Based RLS

If you previously implemented RLS using a view-based approach, this guide helps you migrate to the new AST-based RLS system.

### Overview of Changes

| Aspect | View-Based RLS | AST-Based RLS |
|--------|---------------|---------------|
| Implementation | Manual view creation + triggers | Automatic policy injection |
| Query modification | String concatenation | AST-level modification |
| Policy storage | Custom tables | `__pg_rls_policies__` |
| PostgreSQL compatibility | Partial | Full |

### Migration Steps

#### Step 1: Export Existing Policies

If you have existing view-based RLS policies, document them:

```sql
-- Document your current view-based policies
-- Example: user_data_view that filters by user_id
CREATE VIEW user_data_view AS
SELECT * FROM documents WHERE owner = current_user();
```

#### Step 2: Enable RLS on Tables

```sql
-- Enable RLS on tables that previously had view-based filtering
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;
```

#### Step 3: Create Equivalent Policies

Convert each view-based filter to a policy:

```sql
-- View-based: WHERE owner = current_user()
-- Policy-based:
CREATE POLICY owner_select ON documents
  FOR SELECT
  USING (owner = current_user());

CREATE POLICY owner_insert ON documents
  FOR INSERT
  WITH CHECK (owner = current_user());

CREATE POLICY owner_update ON documents
  FOR UPDATE
  USING (owner = current_user())
  WITH CHECK (owner = current_user());

CREATE POLICY owner_delete ON documents
  FOR DELETE
  USING (owner = current_user());
```

#### Step 4: Drop Old Views (Optional)

Once RLS policies are working, you can drop the old views:

```sql
DROP VIEW IF EXISTS user_data_view;
```

#### Step 5: Test Thoroughly

```sql
-- Test as each user role
SET ROLE alice;
SELECT * FROM documents;  -- Should filter to alice's docs

SET ROLE bob;
SELECT * FROM documents;  -- Should filter to bob's docs

-- Test INSERT
SET ROLE alice;
INSERT INTO documents (owner, title) VALUES ('alice', 'Test');

-- Test UPDATE
UPDATE documents SET title = 'Updated' WHERE id = 1;

-- Test DELETE
DELETE FROM documents WHERE id = 1;
```

### Common Migration Patterns

#### Multi-Tenant Isolation

**Before (view-based):**
```sql
CREATE VIEW tenant_data AS
SELECT * FROM data WHERE tenant_id = current_tenant();
```

**After (RLS):**
```sql
ALTER TABLE data ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON data
  FOR ALL
  USING (tenant_id = current_tenant())
  WITH CHECK (tenant_id = current_tenant());
```

#### Role-Based Access

**Before (view-based):**
```sql
CREATE VIEW admin_data AS
SELECT * FROM sensitive_data WHERE is_admin = true;
```

**After (RLS):**
```sql
ALTER TABLE sensitive_data ENABLE ROW LEVEL SECURITY;

CREATE POLICY admin_full_access ON sensitive_data
  TO admin, superuser
  USING (true)
  WITH CHECK (true);

CREATE POLICY user_read_only ON sensitive_data
  FOR SELECT
  TO public_user
  USING (true);
```

### Verifying Migration

Check that RLS is working correctly:

```sql
-- Verify RLS is enabled
SELECT relname, rls_enabled, rls_forced
FROM __pg_relation_meta__
WHERE relname = 'documents';

-- List all policies
SELECT polname, polcmd, polpermissive, polroles, polqual
FROM __pg_rls_policies__
WHERE polrelid = 'documents';
```

---

## See Also

- [PostgreSQL RLS Documentation](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [RBAC Documentation](./RBAC.md)
- [Catalog Schema](./CATALOG.md)
- [Transpiler Architecture](../src/transpiler.rs)