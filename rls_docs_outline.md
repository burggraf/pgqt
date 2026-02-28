# RLS Documentation Outline

## File: `docs/RLS.md`

### 1. Introduction to Row-Level Security (RLS)
- What is RLS?
- Why use RLS in postgresqlite?
- Overview of how RLS works (predicate-based filtering)

### 2. Comparison with PostgreSQL RLS
| Feature | PostgreSQL | postgresqlite |
|---------|-----------|---------------|
| Policy Storage | pg_policy system catalog | __pg_rls_policies__ table |
| ENABLE/DISABLE | ALTER TABLE syntax | Supported |
| FORCE/NO FORCE | Supported | Supported |
| PERMISSIVE Policies | Combined with OR | Combined with OR |
| RESTRICTIVE Policies | Combined with AND | Combined with AND |
| current_user() | Built-in | Custom SQLite function |
| session_user | Built-in | Custom SQLite function |

### 3. Getting Started
- Enabling RLS on a table
- Creating your first policy
- Testing RLS behavior

### 4. Usage Examples
#### 4.1 Basic Setup
```sql
ALTER TABLE employees ENABLE ROW LEVEL SECURITY;
```

#### 4.2 Creating Policies
```sql
-- Simple row-level policy
CREATE POLICY user_policy ON employees
  FOR SELECT
  USING (user_id = current_user());

-- INSERT policy with WITH CHECK
CREATE POLICY insert_policy ON employees
  FOR INSERT
  WITH CHECK (user_id = current_user());
```

#### 4.3 Policy Modes
```sql
-- PERMISSIVE (OR logic)
CREATE POLICY policy1 ON table1 AS PERMISSIVE FOR SELECT USING (col1 = 'a');
CREATE POLICY policy2 ON table1 AS PERMISSIVE FOR SELECT USING (col2 = 'b');
-- Result: (col1 = 'a') OR (col2 = 'b')

-- RESTRICTIVE (AND logic)
CREATE POLICY policy3 ON table2 AS RESTRICTIVE FOR SELECT USING (status = 'active');
CREATE POLICY policy4 ON table2 AS RESTRICTIVE FOR SELECT USING (dept = 'sales');
-- Result: (status = 'active') AND (dept = 'sales')
```

#### 4.4 Role-Based Policies
```sql
-- Apply to specific roles
CREATE POLICY admin_policy ON sensitive_data
  TO admin, superuser
  USING (true);

-- Apply to PUBLIC
CREATE POLICY public_policy ON public_posts
  TO PUBLIC
  USING (is_public = true);
```

#### 4.5 Operations: SELECT, INSERT, UPDATE, DELETE
```sql
-- SELECT: Uses USING clause
CREATE POLICY select_policy ON documents
  FOR SELECT USING (owner = current_user());

-- INSERT: Uses WITH CHECK clause
CREATE POLICY insert_policy ON documents
  FOR INSERT WITH CHECK (owner = current_user());

-- UPDATE: Uses both USING and WITH CHECK
CREATE POLICY update_policy ON documents
  FOR UPDATE USING (owner = current_user())
  WITH CHECK (owner = current_user());

-- DELETE: Uses USING clause
CREATE POLICY delete_policy ON documents
  FOR DELETE USING (owner = current_user());
```

### 5. ALTER TABLE Commands
```sql
-- Enable RLS
ALTER TABLE employees ENABLE ROW LEVEL SECURITY;

-- Disable RLS
ALTER TABLE employees DISABLE ROW LEVEL SECURITY;

-- Force RLS for owners
ALTER TABLE employees FORCE ROW LEVEL SECURITY;

-- Disable force (default)
ALTER TABLE employees NO FORCE ROW LEVEL SECURITY;

-- Modify policies
ALTER POLICY policy_name ON table_name TO new_role;
DROP POLICY policy_name ON table_name;
```

### 6. Built-in Functions
- `current_user()` - Returns the current session user
- `session_user()` - Returns the session user
- Integration with RBAC system

### 7. Limitations
- Subquery handling in policy expressions
- JOIN behavior (policies apply to base tables only)
- Transactions and RLS (single-statement at a time)
- No support for OR conditions within a single RESTRICTIVE policy
- WITH CHECK on INSERT requires transpilation to SELECT with WHERE

### 8. Security Best Practices
- Always have at least one PERMISSIVE policy when enabling RLS (default-deny behavior)
- Use RESTRICTIVE policies for mandatory filters
- Test policies thoroughly with different roles
- Consider performance impact of complex policy expressions
- Never use RLS as the only security measure - combine with RBAC
- Be careful with FORCE ROW LEVEL SECURITY (affects owners)

### 9. Troubleshooting
- Common errors and solutions
- Debugging policy evaluation
- Checking which policies apply to a table
- Performance considerations

### 10. Examples: Complete Multi-User Scenario
```sql
-- Setup: Create roles
CREATE ROLE alice WITH LOGIN;
CREATE ROLE bob WITH LOGIN;
CREATE ROLE admin WITH SUPERUSER;

-- Setup: Create table
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    owner TEXT NOT NULL,
    title TEXT,
    content TEXT,
    is_public BOOLEAN DEFAULT false
);

-- Setup: Enable RLS
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;

-- Create policies
CREATE POLICY owner_select ON documents
  FOR SELECT TO alice, bob
  USING (owner = current_user() OR is_public = true);

CREATE POLICY owner_insert ON documents
  FOR INSERT TO alice, bob
  WITH CHECK (owner = current_user());

CREATE POLICY owner_update ON documents
  FOR UPDATE TO alice, bob
  USING (owner = current_user())
  WITH CHECK (owner = current_user());

CREATE POLICY admin_full ON documents
  TO admin
  USING (true)
  WITH CHECK (true);

-- Test as alice
SET ROLE alice;
INSERT INTO documents (owner, title) VALUES ('alice', 'My Document');
SELECT * FROM documents; -- Only sees own + public docs

-- Test as bob
SET ROLE bob;
SELECT * FROM documents; -- Sees bob's docs + public docs
```

### 11. Internal Architecture (for developers)
- Metadata storage in `__pg_rls_policies__`
- AST injection in transpiler.rs
- Policy combination logic (PERMISSIVE=OR, RESTRICTIVE=AND)
- RLS context and session management
- Custom SQLite function registration