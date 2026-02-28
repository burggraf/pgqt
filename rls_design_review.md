# RLS Design Document Review

**Reviewer:** reviewer-v2  
**Document:** rls_design.md  
**Date:** 2026-02-28  
**Status:** ⚠️ **CONDITIONALLY APPROVED** - Address critical issues before implementation

---

## Executive Summary

The design document is **well-structured** and demonstrates a solid understanding of PostgreSQL RLS semantics. The AST injection approach is the correct direction. However, there are **critical security gaps** and **implementation ambiguities** that must be resolved before proceeding.

---

## 1. Security Review (CRITICAL)

### ✅ Strengths
- Bypass logic correctly identifies: BYPASSRLS attribute, table owner, FORCE RLS
- Policy combination logic matches PostgreSQL (PERMISSIVE=OR, RESTRICTIVE=AND)
- Session context extension with role resolution is appropriate

### ❌ Critical Issues (Must Fix)

#### 1.1 Metadata Table Protection (HIGH PRIORITY)
**Issue:** The design doesn't address how to protect `__pg_rls_policies__` and `__pg_relation_meta__` from direct modification.

**Risk:** A user could bypass RLS by:
```sql
UPDATE __pg_rls_policies__ SET polqual = '1=1';
DELETE FROM __pg_relation_meta__ WHERE relname = 'sensitive_table';
```

**Recommendation:**
- Implement these as internal-only tables (not accessible via SQL)
- OR use SQLite's `WITHOUT ROWID` with restricted access
- OR implement a trigger that rejects modifications from non-superusers
- Document this in the security model

#### 1.2 Policy Expression SQL Injection (HIGH PRIORITY)
**Issue:** No mention of sanitizing or validating policy expressions.

**Risk:** If policy expressions can be crafted by untrusted users:
```sql
CREATE POLICY bypass ON documents USING (1=1 OR owner_id = (SELECT 1 FROM __pg_rls_policies__ DELETE));
```

**Recommendation:**
- Only superusers/table owners should create policies
- Validate policy expressions don't contain DDL/DML
- Document that policy expressions execute with definer's rights

#### 1.3 Subquery Handling in Policies (MEDIUM PRIORITY)
**Issue:** Design doesn't address how RLS handles tables referenced in policy expression subqueries.

**Risk:** Infinite recursion or bypass:
```sql
CREATE POLICY p1 ON t1 USING (EXISTS (SELECT 1 FROM t2 WHERE ...));
-- Does t2 have RLS? Does it apply recursively?
```

**Recommendation:**
- Document behavior: Do subqueries in policy expressions trigger RLS on referenced tables?
- PostgreSQL applies RLS recursively - should we match this?
- Add test case for recursive RLS

#### 1.4 Empty Policy Default Behavior (HIGH PRIORITY)
**Issue:** Research findings note: "If *no* permissive policies apply, PostgreSQL defaults to **deny**". Design doesn't explicitly address this.

**Current Code Gap:** `build_rls_expression()` returns `None` when no policies match, which could be interpreted as "no filter" (allow all).

**Recommendation:**
```rust
// Explicit default-deny when RLS is enabled but no permissive policies exist
if permissive_clauses.is_empty() && !restrictive_clauses.is_empty() {
    return Some("FALSE".to_string()); // Default deny
}
```

---

## 2. Code Quality Review

### ✅ Strengths
- Clear separation of concerns (RLS engine vs AST injection)
- Good function signatures proposed
- Session context extension is clean

### ⚠️ Issues to Address

#### 2.1 Error Handling Strategy
**Issue:** Design shows `Result<()>` but doesn't define error types.

**Recommendation:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum RlsError {
    #[error("Policy not found: {0}")]
    PolicyNotFound(String),
    #[error("RLS bypass attempt detected for table {0}")]
    BypassAttempt(String),
    #[error("Policy expression validation failed: {0}")]
    InvalidExpression(String),
    #[error("WITH CHECK violation on table {0}")]
    WithCheckViolation(String),
}
```

#### 2.2 AST Injection Implementation Details
**Issue:** Design shows function signatures but not the actual AST manipulation logic.

**Missing Details:**
- How to parse RLS expression string into `Node` objects?
- How to handle `current_user()` function in expressions?
- How to combine multiple AST nodes with AND/OR?

**Recommendation:** Provide a concrete example:
```rust
// Example: Injecting "owner_id = current_user()" into WHERE clause
fn inject_rls_into_select(stmt: &mut SelectStmt, rls_expr: &str) -> Result<()> {
    // 1. Parse RLS expression
    let parsed = pg_query::parse(&format!("SELECT WHERE {}", rls_expr))?;
    let rls_node = extract_where_node(parsed)?;
    
    // 2. Combine with existing WHERE
    let new_where = match &stmt.where_clause {
        None => rls_node,
        Some(existing) => create_and_node(existing, &rls_node),
    };
    
    // 3. Update statement
    stmt.where_clause = Some(new_where);
    Ok(())
}
```

#### 2.3 INSERT...SELECT Pattern Concerns
**Issue:** The proposed INSERT...SELECT conversion has edge cases:

```sql
-- Original
INSERT INTO t (a, b) VALUES (1, 2), (3, 4);

-- Proposed conversion
INSERT INTO t (a, b) 
SELECT * FROM (VALUES (1, 2), (3, 4)) AS v(a, b)
WHERE (with_check_expr);
```

**Problems:**
1. `VALUES` in FROM clause may not work in all SQLite versions
2. Column aliasing in subquery may not match
3. DEFAULT values in INSERT need special handling

**Recommendation:** Consider trigger-based approach for INSERT WITH CHECK:
```rust
// Create INSTEAD OF INSERT trigger on RLS-enabled tables
CREATE TRIGGER rls_check_t
BEFORE INSERT ON t
FOR EACH ROW
BEGIN
    SELECT CASE 
        WHEN NOT (NEW.col = current_user()) 
        THEN RAISE(ABORT, 'WITH CHECK violation')
    END;
END;
```

---

## 3. Test Coverage Review

### ⚠️ Issues

#### 3.1 Missing Test Categories
The design mentions "comprehensive test suite" but doesn't specify:

**Required Test Categories:**
1. **Security Tests:**
   - Attempt to modify `__pg_rls_policies__` directly
   - Attempt RLS bypass via subquery
   - Attempt SQL injection in policy expression
   - Verify FORCE RLS prevents owner bypass

2. **Edge Case Tests:**
   - Empty policy set (default deny)
   - NULL handling in policy expressions
   - Unicode in policy expressions
   - Very long policy expressions

3. **Integration Tests:**
   - RLS on joined tables
   - RLS in subqueries
   - RLS with views (if supported)
   - RLS with CTEs

4. **Performance Tests:**
   - Overhead measurement (with/without RLS)
   - Multiple policies on same table
   - Complex policy expressions

**Recommendation:** Add a "Test Plan" section to the design document.

---

## 4. Documentation Review

### ⚠️ Issues

#### 4.1 Missing User Documentation Outline
The design doesn't include what `docs/RLS.md` should contain.

**Required Sections:**
1. What is RLS and when to use it
2. Quick start example
3. Complete SQL syntax reference
4. Policy expression examples (multi-tenant, user-owned rows, role-based)
5. Security considerations
6. Limitations and differences from PostgreSQL
7. Troubleshooting guide

#### 4.2 Migration Guide
**Issue:** No mention of migrating from the current view-based approach.

**Recommendation:** Add section on:
- How to disable view-based RLS
- How to enable AST-based RLS
- Compatibility considerations

---

## 5. Performance Review

### ⚠️ Issues

#### 5.1 Caching Strategy
**Issue:** Design lists caching as an "open question" but this should be decided before implementation.

**Recommendation:**
```rust
// Cache parsed RLS expressions per (table, roles, command)
struct RlsCache {
    expressions: HashMap<(String, Vec<String>, String), Node>,
}

impl RlsCache {
    fn get_or_parse(&mut self, table: &str, roles: &[String], cmd: &str, expr: &str) -> Result<&Node> {
        // Cache key, check, parse if miss
    }
}
```

#### 5.2 Index Usage
**Issue:** No mention of ensuring RLS WHERE clauses can use indexes.

**Recommendation:**
- Document that policy expressions should use indexed columns
- Add test verifying index usage with RLS

---

## 6. PostgreSQL Compatibility Review

### ✅ Strengths
- Good compatibility matrix
- Correct policy combination logic
- Proper bypass semantics

### ⚠️ Gaps

#### 6.1 WITH CHECK Implementation
**Issue:** Design marks WITH CHECK as "⚠️ Requires special handling" but doesn't commit to an approach.

**Recommendation:** Make a decision before implementation:
- **Option A (INSERT...SELECT):** Cleaner SQL, but edge cases with VALUES
- **Option B (Triggers):** More robust, but requires trigger infrastructure
- **Option C (Pre-check):** Simpler, but race conditions possible

**My Recommendation:** Use **triggers** for WITH CHECK - more robust and matches PostgreSQL semantics better.

#### 6.2 CURRENT_USER Function
**Issue:** Design mentions custom `current_user()` function but doesn't detail implementation.

**Recommendation:**
```rust
// In main.rs or connection setup
conn.create_scalar_function("current_user", 0, rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
    let session = ctx.get_aux::<SessionContext>()?;
    Ok(session.current_user.clone())
})?;
```

---

## 7. Open Questions Resolution

| Question | My Recommendation |
|----------|-------------------|
| INSERT WITH CHECK: Pattern or triggers? | **Triggers** - more robust |
| Subqueries in policies | Apply RLS recursively (match PostgreSQL) |
| JOINs: RLS on joined tables? | **Yes** - each table's RLS applies independently |
| Cache parsed expressions? | **Yes** - cache per (table, roles, command) |
| Expression rewriting for current_user | Register `current_user()` as SQLite function |

---

## 8. Action Items for implementer-3

### Before Implementation Starts:
1. [ ] **Decide on WITH CHECK strategy** (recommend: triggers)
2. [ ] **Define RlsError enum** for error handling
3. [ ] **Add metadata table protection** mechanism
4. [ ] **Document default-deny behavior** for empty policies

### During Implementation:
5. [ ] **Implement AST parsing** for RLS expressions (concrete example needed)
6. [ ] **Register current_user() function** in SQLite connection
7. [ ] **Add caching layer** for parsed expressions
8. [ ] **Handle recursive RLS** in subqueries

### Testing Requirements:
9. [ ] **Security test suite** (bypass attempts, injection, etc.)
10. [ ] **Edge case tests** (NULL, Unicode, empty policies)
11. [ ] **Performance benchmarks** (overhead measurement)

### Documentation:
12. [ ] **Write docs/RLS.md** with examples and troubleshooting
13. [ ] **Add migration guide** from view-based approach

---

## 9. Approval Status

### ✅ Approved Aspects
- Overall architecture (AST injection)
- Policy combination logic
- Session context extension
- Bypass logic

### ⚠️ Conditional Approval (Must Address)
- Metadata table protection mechanism
- Default-deny for empty policies
- WITH CHECK implementation decision
- Error handling strategy

### ❌ Missing (Should Add)
- Concrete AST injection code examples
- Test plan with security tests
- User documentation outline
- Performance caching decision

---

## 10. Next Steps

1. **implementer-3** should address critical issues before starting implementation
2. **test-writer-3** should review this and create test plan
3. **documenter-3** should start outline for `docs/RLS.md`
4. Schedule a quick sync to resolve open questions (WITH CHECK strategy, caching)

---

## Summary

The design is **80% there** - the core architecture is sound. The remaining 20% (security hardening, implementation details, test coverage) is critical for a production-ready RLS implementation. Please address the critical issues before proceeding with implementation.

**Re-review requested after:** Critical issues are addressed and implementation begins.
