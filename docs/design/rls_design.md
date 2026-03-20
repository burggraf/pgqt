# RLS Design Document: AST-Based Row-Level Security

## Executive Summary

This document outlines the architecture for implementing PostgreSQL-compatible Row-Level Security (RLS) in postgresqlite via AST injection. This replaces the current view-based approach with a more robust and transparent implementation.

## Current State Analysis

### Existing RLS Implementation (View-Based)
The current `src/rls.rs` implements RLS through:
- `RlsContext`: Stores current_user, user_roles, bypass_rls flag
- `build_rls_expression()`: Combines PERMISSIVE policies with OR, RESTRICTIVE with AND
- `get_rls_where_clause()`: Retrieves applicable policies and builds WHERE clause
- `apply_rls_to_sql()`: Appends RLS WHERE clause to SQL string (naive string concatenation)

**Problems with current approach:**
1. String-based SQL modification is fragile and error-prone
2. Doesn't handle INSERT/UPDATE WITH CHECK clauses properly
3. No AST-level understanding of query structure
4. Difficult to handle complex queries (subqueries, JOINs, etc.)

### Transpiler Architecture
The `src/transpiler.rs` uses `pg_query` to:
1. Parse PostgreSQL SQL into AST
2. Walk the AST and reconstruct SQLite-compatible SQL
3. Extract metadata (table names, column types) during transpilation

Key DML reconstruction functions:
- `reconstruct_select_stmt()`: Handles SELECT with WHERE clause
- `reconstruct_insert_stmt()`: Handles INSERT (no WHERE clause)
- `reconstruct_update_stmt()`: Handles UPDATE with WHERE clause
- `reconstruct_delete_stmt()`: Handles DELETE with WHERE clause

### Catalog Schema
The `src/catalog.rs` already has RLS metadata tables:
- `__pg_rls_policies__`: Stores policy definitions
  - polname, polrelid (table), polcmd (ALL/SELECT/INSERT/UPDATE/DELETE)
  - polpermissive (boolean), polroles (comma-separated)
  - polqual (USING expr), polwithcheck (WITH CHECK expr)
- `__pg_relation_meta__`: Tracks RLS enabled/forced per table

## Proposed Architecture

### Design Goals
1. **PostgreSQL Compatibility**: Match PG's RLS semantics exactly
2. **AST-Based Injection**: Modify queries at the AST level, not string level
3. **Transparent Operation**: RLS should be invisible to applications
4. **Performance**: Minimal overhead for policy evaluation
5. **Security**: No possibility of RLS bypass through clever queries

### Core Components

#### 1. RLS Policy Engine (`src/rls.rs` - Refactored)

**Policy Combination Logic (PostgreSQL Semantics):**
```
Final Expression = (permissive_1 OR permissive_2 OR ...) AND (restrictive_1 AND restrictive_2 AND ...)
```

**Policy Applicability:**
- Command matching: ALL matches everything, specific commands only match their operation
- Role matching: Empty roles = PUBLIC, check if user's roles intersect with policy roles

**Expression Types:**
- **USING**: For SELECT, UPDATE, DELETE on existing rows
- **WITH CHECK**: For INSERT, UPDATE on new rows (falls back to USING if not specified)

#### 2. AST Injection Layer (`src/transpiler.rs` - Extended)

New function signatures:
```rust
/// Inject RLS into SELECT statement AST
fn inject_rls_into_select(
    stmt: &mut SelectStmt,
    rls_expr: &str,
    table_name: &str,
) -> Result<()>

/// Inject RLS into UPDATE statement AST  
fn inject_rls_into_update(
    stmt: &mut UpdateStmt,
    using_expr: Option<&str>,
    with_check_expr: Option<&str>,
    table_name: &str,
) -> Result<()>

/// Inject RLS into DELETE statement AST
fn inject_rls_into_delete(
    stmt: &mut DeleteStmt,
    rls_expr: &str,
    table_name: &str,
) -> Result<()>

/// Inject RLS into INSERT statement (via trigger or pre-check)
fn inject_rls_into_insert(
    stmt: &mut InsertStmt,
    with_check_expr: &str,
    table_name: &str,
) -> Result<()>
```

**AST Injection Strategy:**

For **SELECT**:
- Parse RLS expression into AST nodes
- Inject into WHERE clause: `original_where AND (rls_expr)`
- If no original WHERE, create new WHERE with RLS expression

For **UPDATE**:
- **USING**: Inject into WHERE clause (same as SELECT)
- **WITH CHECK**: Requires post-update validation
  - Option A: Convert to `UPDATE ... WHERE (original) AND (using) RETURNING *` + validate new rows
  - Option B: Use SQLite trigger for WITH CHECK validation
  - Option C: Pre-check in application layer (less ideal)

For **DELETE**:
- Inject into WHERE clause (same as SELECT)

For **INSERT**:
- SQLite doesn't support CHECK constraints on INSERT VALUES
- Options:
  1. Use `INSERT ... SELECT ... WHERE (with_check)` pattern
  2. Use INSTEAD OF INSERT trigger
  3. Post-insert validation with rollback

#### 3. Session Context Integration

Current session context in `main.rs`:
```rust
struct SessionContext {
    authenticated_user: String,
    current_user: String,
}
```

Extended for RLS:
```rust
struct SessionContext {
    authenticated_user: String,
    current_user: String,
    user_roles: Vec<String>,  // Fetch from __pg_auth_members__
    bypass_rls: bool,         // Based on role attributes
}
```

**Role Resolution:**
1. Query `__pg_authid__` for current user's OID
2. Query `__pg_auth_members__` for all roles the user is a member of
3. Always include "PUBLIC" in role list

#### 4. Bypass RLS Logic

Users can bypass RLS if:
1. They have `BYPASSRLS` attribute (superuser or explicit)
2. They are the table owner AND RLS is not FORCE-enabled

Check in `can_bypass_rls()`:
```rust
if ctx.bypass_rls || is_superuser {
    return Ok(true);
}
if is_table_owner && !is_rls_forced {
    return Ok(true);
}
Ok(false)
```

## Implementation Plan

### Phase 1: Foundation (Week 1)
1. **Extend SessionContext** with role resolution
2. **Refactor RLS expression building** for clarity
3. **Add AST parsing** for RLS expressions using pg_query

### Phase 2: AST Injection (Week 2)
1. **Implement SELECT injection** - inject RLS into WHERE clause
2. **Implement UPDATE injection** - handle USING in WHERE
3. **Implement DELETE injection** - same as SELECT
4. **Design INSERT strategy** - evaluate options

### Phase 3: INSERT Handling (Week 3)
1. **Implement chosen INSERT strategy**
2. **Handle WITH CHECK for UPDATE**
3. **Integration testing** for all DML types

### Phase 4: Integration & Testing (Week 4)
1. **Wire into transpiler pipeline**
2. **Add bypass logic**
3. **Comprehensive test suite**
4. **Performance benchmarking**

## Technical Details

### AST Injection Implementation

**Parsing RLS Expressions:**
```rust
fn parse_rls_expression(expr: &str) -> Result<Node> {
    let sql = format!("SELECT * FROM dummy WHERE {}", expr);
    let parsed = pg_query::parse(&sql)?;
    // Extract WHERE clause nodes from parsed AST
}
```

**Injecting into SELECT:**
```rust
fn inject_into_where_clause(
    original_where: &Option<Node>,
    rls_nodes: Vec<Node>,
) -> Node {
    match original_where {
        None => create_bool_expr(AND, rls_nodes),
        Some(where_clause) => {
            let mut args = vec![where_clause.clone()];
            args.extend(rls_nodes);
            create_bool_expr(AND, args)
        }
    }
}
```

### INSERT WITH CHECK Strategy

**Recommended Approach: INSERT...SELECT Pattern**

Convert:
```sql
INSERT INTO documents (title, owner_id) VALUES ('Secret', 1);
```

To:
```sql
INSERT INTO documents (title, owner_id) 
SELECT 'Secret', 1 
WHERE (1 = current_user_id);  -- WITH CHECK expression
```

For VALUES with multiple rows:
```sql
INSERT INTO documents (title, owner_id) 
SELECT * FROM (VALUES ('Doc1', 1), ('Doc2', 2)) AS v(title, owner_id)
WHERE (owner_id = current_user_id);
```

### UPDATE WITH CHECK Strategy

**Approach: Two-phase validation**

1. First, apply USING expression to filter rows that can be updated
2. After UPDATE, verify WITH CHECK expression on modified rows
3. If any row fails WITH CHECK, rollback the entire UPDATE

Implementation:
```rust
// Phase 1: UPDATE with USING in WHERE
UPDATE documents SET title = 'New' WHERE id = 1 AND (owner_id = current_user);

// Phase 2: Verify WITH CHECK
SELECT COUNT(*) FROM documents WHERE id = 1 AND NOT (owner_id = current_user);
// If count > 0, rollback
```

## PostgreSQL Compatibility Matrix

| Feature | PostgreSQL | postgresqlite | Notes |
|---------|------------|---------------|-------|
| ENABLE RLS | ✅ | ✅ | Already implemented |
| FORCE RLS | ✅ | ✅ | Already implemented |
| CREATE POLICY | ✅ | ✅ | Already implemented |
| PERMISSIVE policies | OR combined | OR combined | Match PG semantics |
| RESTRICTIVE policies | AND combined | AND combined | Match PG semantics |
| USING clause | ✅ | ✅ | For SELECT/UPDATE/DELETE |
| WITH CHECK clause | ✅ | ⚠️ | Requires special handling |
| current_user | ✅ | ✅ | Via session context |
| session_user | ✅ | ✅ | Via session context |
| user roles | ✅ | ✅ | Via __pg_auth_members__ |
| BYPASSRLS | ✅ | ✅ | Superuser/owner check |

## Open Questions

1. **INSERT WITH CHECK**: Should we use the INSERT...SELECT pattern or triggers?
2. **Subqueries**: How do we handle RLS on tables referenced in subqueries?
3. **JOINs**: Should RLS apply to joined tables automatically?
4. **Performance**: Should we cache parsed RLS expressions?
5. **Expression rewriting**: How to handle `current_user` in RLS expressions?

## Next Steps

1. Review this design with researcher-new for PostgreSQL compatibility
2. Create proof-of-concept for AST injection
3. Implement INSERT...SELECT pattern for WITH CHECK
4. Build comprehensive test suite

## References

- [PostgreSQL RLS Documentation](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [pg_query crate documentation](https://docs.rs/pg_query/)
- Current implementation: `src/rls.rs`, `src/transpiler.rs`, `src/catalog.rs`
