# RLS Implementation Review

**Reviewer**: reviewer (on rls-team)  
**Date**: 2026-02-28  
**Status**: ⚠️ In Progress - Major Refactoring Underway  

---

## Executive Summary

**UPDATE (2026-02-28)**: Implementer has started major refactoring from view-based approach to AST injection. Progress is significant but incomplete.

The implementation is being refactored from a **view-based approach** to the correct **AST injection during transpilation** architecture. The new implementation shows good progress but has several issues that need to be addressed before merging.

---

## Architecture Review

### ✅ Progress: Correct Architecture Being Implemented

The implementer has started refactoring to the correct AST injection approach:

| Component | Status | Notes |
|-----------|--------|-------|
| `__pg_rls_policies__` table | ✅ Implemented | In `catalog.rs` |
| `__pg_rls_enabled__` table | ✅ Implemented | In `catalog.rs` |
| `RlsContext` struct | ✅ Implemented | In `rls.rs` |
| `build_rls_expression()` | ✅ Implemented | Correct OR/AND logic |
| `get_rls_where_clause()` | ✅ Implemented | For transpiler use |
| DDL handlers | ⚠️ Partial | ALTER TABLE, CREATE/DROP POLICY exist but not integrated |
| Transpiler integration | ⚠️ Partial | `transpile_with_rls()` exists but not used |

### ❌ Critical Issues Remaining

| Aspect | Plan Specification | Current Implementation | Issue |
|--------|-------------------|----------------------|-------|
| ** enforcement** | AST injection in transpiler | View creation + table renaming | Security bypass possible |
| **Policy storage** | `__pg_rls_policies__` system table | In-memory HashMap only | No persistence |
| **Integration** | Transpiler injects WHERE clauses | Standalone RLS manager | No query interception |
| **RBAC** | Uses session user/role | `roles` field unused | Policies not enforced per-user |

### Security Vulnerability

```sql
-- Attacker can bypass RLS entirely:
SELECT * FROM _documents_data;  -- Direct access to base table!
```

---

## Code Review Findings

### 1. Policy Combination Logic (src/rls.rs:125-140)

**Current**:
```rust
fn build_where_clause(&self, policies: &[RlsPolicy]) -> String {
    let mut clauses = Vec::new();
    for policy in policies {
        if let Some(ref expr) = policy.using_expr {
            clauses.push(expr.clone());
        }
    }
    if clauses.is_empty() {
        String::new()
    } else {
        clauses.join(" AND ")  // ❌ WRONG: All policies combined with AND
    }
}
```

**Expected (per PostgreSQL semantics)**:
- PERMISSIVE policies → combine with `OR`
- RESTRICTIVE policies → combine with `AND`
- Final: `(permissive_clause) AND (restrictive_clause)`

**Fix Required**:
```rust
fn build_where_clause(&self, policies: &[RlsPolicy]) -> String {
    let permissive: Vec<_> = policies.iter()
        .filter(|p| p.permissive)
        .filter_map(|p| p.using_expr.as_ref())
        .collect();
    let restrictive: Vec<_> = policies.iter()
        .filter(|p| !p.permissive)
        .filter_map(|p| p.using_expr.as_ref())
        .collect();
    
    let mut parts = Vec::new();
    
    if !permissive.is_empty() {
        parts.push(format!("({})", permissive.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" OR ")));
    }
    if !restrictive.is_empty() {
        parts.push(format!("({})", restrictive.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" AND ")));
    }
    
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        _ => parts.join(" AND "),
    }
}
```

---

### 2. Missing Transpiler Integration (src/transpiler.rs)

**Issue**: Zero RLS integration in transpiler.

**Required Changes**:
1. Add `RlsManager` reference to `TranspileContext`
2. Modify `reconstruct_select_stmt`, `reconstruct_insert_stmt`, `reconstruct_update_stmt`, `reconstruct_delete_stmt` to inject RLS WHERE clauses
3. Add session user/role resolution

**Example Integration Point**:
```rust
// In reconstruct_select_stmt, after building WHERE clause:
let rls_filter = ctx.get_rls_filter(&table_name, operation_type);
let final_where = match (where_sql, rls_filter) {
    (None, None) => None,
    (Some(w), None) => Some(w),
    (None, Some(r)) => Some(r),
    (Some(w), Some(r)) => Some(format!("({}) AND ({})", w, r)),
};
```

---

### 3. Missing DDL Command Handlers

**Not Implemented**:
- `ALTER TABLE ... ENABLE ROW LEVEL SECURITY`
- `ALTER TABLE ... DISABLE ROW LEVEL SECURITY`
- `CREATE POLICY name ON table_name [AS PERMISSIVE|RESTRICTIVE] [FOR ALL|SELECT|INSERT|UPDATE|DELETE] [TO role] USING (...) [WITH CHECK (...)]`
- `DROP POLICY name ON table_name`
- `ALTER POLICY ...`

**Required**: Add handlers in `reconstruct_sql_with_metadata()` in `transpiler.rs` to:
1. Parse these DDL statements
2. Store/retrieve policies from `__pg_rls_policies__`
3. Update in-memory RLS manager

---

### 4. Incomplete Metadata Storage (src/rls.rs:175-188)

**Current**:
```rust
pub fn store_policy_metadata(conn: &Connection, policy: &RlsPolicy) -> Result<()> {
    conn.execute(
        "INSERT INTO __pg_meta__ (table_name, column_name, original_type, constraints)
         VALUES (?1, ?2, ?3, ?4)",
        (...),
    )?;
    Ok(())
}
```

**Issues**:
- Uses wrong table (`__pg_meta__` is for column metadata)
- No dedicated `__pg_rls_policies__` table
- Schema doesn't capture all policy fields

**Required Schema**:
```sql
CREATE TABLE IF NOT EXISTS __pg_rls_policies__ (
    id INTEGER PRIMARY KEY,
    policy_name TEXT NOT NULL,
    table_name TEXT NOT NULL,
    command TEXT NOT NULL,  -- ALL, SELECT, INSERT, UPDATE, DELETE
    permissive INTEGER NOT NULL,  -- 1=true, 0=false
    using_expr TEXT,
    with_check_expr TEXT,
    roles TEXT,  -- JSON array of role names
    created_at TEXT DEFAULT (datetime('now'))
);
```

---

### 5. WITH CHECK Not Implemented (src/rls.rs:153-159)

**Current**:
```rust
fn create_write_triggers(&self, _conn: &Connection, _table_name: &str) -> Result<()> {
    // TODO: Create triggers that enforce WITH CHECK expressions
    Ok(())
}
```

**Required**: Implement INSTEAD OF triggers for INSERT/UPDATE on RLS-enabled views:
```sql
CREATE TRIGGER rls_insert_check
INSTEAD OF INSERT ON documents
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (NEW.owner = current_user)  -- Policy check
        THEN INSERT INTO _documents_data (id, owner, content) VALUES (NEW.id, NEW.owner, NEW.content)
        ELSE RAISE(ABORT, 'RLS policy violation')
    END;
END;
```

---

### 6. No RBAC Integration

**Issue**: Policy `roles` field is never evaluated.

**Required**:
1. Integrate with existing `__pg_users__`, `__pg_roles__`, `__pg_permissions__` tables
2. Resolve `current_user` / `session_user` during transpilation
3. Filter policies based on current user's roles

---

## Test Review

### tests/rls_tests.rs

**Coverage**: Minimal (2 tests)

**Missing Test Cases**:
- [ ] Policy combination (OR for permissive, AND for restrictive)
- [ ] Multiple policies on same table
- [ ] Per-command policies (SELECT vs INSERT vs UPDATE vs DELETE)
- [ ] Role-based policy filtering
- [ ] WITH CHECK enforcement on INSERT/UPDATE
- [ ] RLS bypass prevention (hidden table access should fail)
- [ ] DDL commands (ENABLE/DISABLE RLS, CREATE/DROP POLICY)
- [ ] Multi-user scenarios

---

## Action Items

### For Implementer
1. **Abandon view-based approach** - Refactor to AST injection in transpiler
2. **Create `__pg_rls_policies__` system table** for persistence
3. **Implement DDL handlers** for RLS commands
4. **Integrate with transpiler** - Add RLS filter injection to all DML operations
5. **Implement WITH CHECK** via INSTEAD OF triggers
6. **Fix policy combination logic** (OR/AND for permissive/restrictive)

### For Researcher
1. **Review AST injection strategy** - Ensure transpiler can handle RLS without breaking existing functionality
2. **Define session context API** - How does transpiler access current user/role?
3. **Research trigger-based WITH CHECK** - SQLite limitations and workarounds

### For Test Writer
1. **Expand test coverage** per missing test cases above
2. **Add security tests** - Verify RLS cannot be bypassed
3. **Add multi-user integration tests** - Test with RBAC system

---

## Compliance with Implementation Plan

| Plan Requirement | Status | Notes |
|-----------------|--------|-------|
| AST injection during transpilation | ❌ Not started | Currently using views |
| `__pg_rls_policies__` system table | ❌ Not implemented | Using in-memory only |
| PERMISSIVE = OR, RESTRICTIVE = AND | ❌ Incorrect | All combined with AND |
| DDL command support | ❌ Not implemented | No CREATE POLICY, etc. |
| RBAC integration | ❌ Not implemented | Roles field unused |
| WITH CHECK for INSERT/UPDATE | ❌ Not implemented | TODO stub |
| Session context (current_user) | ❌ Not implemented | No user resolution |

---

## Recommendation

**Do not merge** current implementation. Requires significant rework to align with the approved architecture. Suggest:

1. Researcher and implementer sync on AST injection approach
2. Implementer creates new branch with transpiler-first architecture
3. Build incrementally: metadata → DDL → SELECT injection → INSERT/UPDATE/DELETE injection → WITH CHECK
4. Test writer develops security-focused test suite in parallel
