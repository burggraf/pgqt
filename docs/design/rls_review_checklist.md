# RLS Implementation Review Checklist

## Overview
This checklist is for reviewing the Row-Level Security (RLS) implementation in postgresqlite. The implementation uses AST injection during transpilation to add WHERE clauses based on RLS policies.

---

## 1. Security Considerations (Critical)

### 1.1 RLS Bypass Prevention
- [ ] **Table owner bypass**: Verify that table owners can only bypass RLS when `FORCE ROW LEVEL SECURITY` is NOT set
- [ ] **Superuser bypass**: Confirm superuser/admin bypass behavior matches PostgreSQL semantics
- [ ] **Bypass flag validation**: Ensure `RlsContext::with_bypass()` cannot be exploited by untrusted code
- [ ] **Force RLS enforcement**: When `FORCE ROW LEVEL SECURITY` is enabled, verify NO bypass is possible (including table owner)
- [ ] **Metadata table protection**: Ensure `__pg_rls_policies__` and `__pg_relation_meta__` cannot be modified directly by regular users

### 1.2 Policy Expression Security
- [ ] **SQL injection prevention**: Verify policy expressions (`USING` and `WITH CHECK`) are properly sanitized
- [ ] **Expression evaluation context**: Ensure policy expressions execute in the correct security context
- [ ] **Function restrictions**: Check if policy expressions can call arbitrary functions (potential privilege escalation)
- [ ] **Subquery handling**: Verify subqueries in policy expressions respect RLS (no recursive bypass)

### 1.3 Role/User Context
- [ ] **Role resolution**: Verify `user_roles` in `RlsContext` correctly resolves all applicable roles
- [ ] **PUBLIC role**: Confirm `PUBLIC` role is always included for policy matching
- [ ] **CURRENT_USER vs SESSION_USER**: Verify both are supported and return correct values
- [ ] **Role inheritance**: If roles can inherit from other roles, verify inheritance chain is respected

### 1.4 Policy Combination Logic
- [ ] **PERMISSIVE policies**: Verify multiple PERMISSIVE policies are combined with `OR`
- [ ] **RESTRICTIVE policies**: Verify multiple RESTRICTIVE policies are combined with `AND`
- [ ] **Mixed policies**: Verify correct combination: `(permissive_expr) AND (restrictive_expr)`
- [ ] **Empty policy handling**: Verify behavior when no policies match (should allow all for PERMISSIVE, deny for RESTRICTIVE)

---

## 2. Code Quality Standards

### 2.1 Architecture & Design
- [ ] **Separation of concerns**: RLS logic should be separate from transpiler logic
- [ ] **Module boundaries**: `rls.rs` handles policy evaluation, `transpiler.rs` handles AST injection
- [ ] **Dependency injection**: `RlsContext` should be passed explicitly, not hidden in global state
- [ ] **Error handling**: All operations return `Result<T>` with meaningful error messages

### 2.2 Code Style
- [ ] **Rust idioms**: Follow Rust best practices (ownership, borrowing, error propagation with `?`)
- [ ] **Naming conventions**: Functions and variables follow Rust naming conventions
- [ ] **Documentation**: All public functions have doc comments with examples
- [ ] **Dead code**: No unused imports, functions, or variables (check with `cargo clippy`)

### 2.3 Function-Specific Reviews

#### `RlsContext` struct
- [ ] Constructor `new()` initializes all fields correctly
- [ ] Builder pattern (`with_roles`, `with_bypass`) is immutable (returns `Self`)
- [ ] Debug derive is appropriate (no sensitive data exposed)

#### `can_bypass_rls()` function
- [ ] Checks `ctx.bypass_rls` flag first
- [ ] Correctly queries `__pg_relation_meta__` for table owner
- [ ] Correctly resolves user OID from `__pg_authid__`
- [ ] Checks `is_rls_forced()` before allowing owner bypass
- [ ] Returns `false` by default (deny unless explicitly allowed)

#### `build_rls_expression()` function
- [ ] Correctly separates PERMISSIVE and RESTRICTIVE policies
- [ ] Handles `for_using` flag (USING vs WITH CHECK expressions)
- [ ] Falls back to `using_expr` when `with_check_expr` is None for INSERT/UPDATE
- [ ] Returns `None` when no applicable expressions exist

#### `get_rls_where_clause()` function
- [ ] Calls `can_bypass_rls()` before building expression
- [ ] Returns `TRUE` when bypass is allowed (no restriction)
- [ ] Calls `get_applicable_policies()` with correct command type
- [ ] Wraps expression in parentheses for SQL correctness

#### `apply_rls_to_sql()` function
- [ ] Maps `OperationType` to PostgreSQL command strings correctly
- [ ] Handles all operation types (SELECT, INSERT, UPDATE, DELETE)
- [ ] Correctly injects WHERE clause into existing SQL
- [ ] Handles existing WHERE clauses (AND with new RLS clause)

### 2.4 Transpiler Integration
- [ ] **AST manipulation**: Verify `pg_query` AST nodes are modified correctly
- [ ] **SQL reconstruction**: Verify modified AST reconstructs to valid SQL
- [ ] **Edge cases**: Handle queries without WHERE clauses, nested queries, joins
- [ ] **Performance**: AST walking should not significantly impact transpilation speed

---

## 3. Test Coverage Requirements

### 3.1 Unit Tests (rls.rs)
- [ ] **RlsContext creation**: Test all constructor and builder methods
- [ ] **can_bypass_rls**: Test with bypass flag, table owner, forced RLS, non-owner
- [ ] **build_rls_expression**: Test PERMISSIVE only, RESTRICTIVE only, mixed, empty
- [ ] **get_rls_where_clause**: Test with policies, without policies, bypass enabled
- [ ] **apply_rls_to_sql**: Test SELECT, INSERT, UPDATE, DELETE operations

### 3.2 Catalog Integration Tests
- [ ] **enable_rls/disable_rls**: Verify RLS enabled/disabled flag is stored correctly
- [ ] **force_rls/no_force_rls**: Verify forced flag is stored correctly
- [ ] **store_rls_policy**: Test creating policies with all options
- [ ] **get_applicable_policies**: Test filtering by table, command, roles
- [ ] **drop_policy**: Verify policies can be removed
- [ ] **alter_policy**: Verify policies can be modified

### 3.3 Transpiler Integration Tests
- [ ] **SELECT with RLS**: Verify WHERE clause is injected
- [ ] **INSERT with RLS**: Verify WITH CHECK logic is applied
- [ ] **UPDATE with RLS**: Verify both USING and WITH CHECK are applied
- [ ] **DELETE with RLS**: Verify WHERE clause is injected
- [ ] **JOIN queries**: Verify RLS applies to correct tables
- [ ] **Subqueries**: Verify RLS applies in nested queries
- [ ] **Existing WHERE**: Verify RLS ANDs with existing conditions

### 3.4 Multi-User Integration Tests
- [ ] **Different users, same table**: Verify different policies apply to different users
- [ ] **Role-based policies**: Verify policies with TO role_name work correctly
- [ ] **PUBLIC policies**: Verify PUBLIC policies apply to all users
- [ ] **Policy precedence**: Verify correct policy selection when multiple match

### 3.5 Security Tests
- [ ] **Bypass prevention**: Attempt to bypass RLS via various methods
- [ ] **Force RLS**: Verify forced RLS cannot be bypassed by table owner
- [ ] **SQL injection**: Attempt to inject SQL via policy expressions
- [ ] **Metadata protection**: Attempt to modify RLS metadata tables

### 3.6 Edge Cases
- [ ] **Empty policies**: Table with RLS enabled but no policies
- [ ] **NULL handling**: Policy expressions with NULL values
- [ ] **Special characters**: Table/column names with special characters
- [ ] **Unicode**: Policy expressions with Unicode characters
- [ ] **Long expressions**: Very long policy expressions

---

## 4. Documentation Requirements

### 4.1 Code Documentation
- [ ] **Module doc**: `rls.rs` has module-level documentation explaining purpose
- [ ] **Public API**: All public functions have doc comments with:
  - Description of what the function does
  - Parameter descriptions
  - Return value description
  - Error conditions
  - Example usage (where applicable)
- [ ] **Complex logic**: Inline comments for non-obvious logic (policy combination, bypass checks)

### 4.2 User Documentation (`docs/RLS.md`)
- [ ] **Overview**: What is RLS and why use it
- [ ] **PostgreSQL compatibility**: What features are supported/not supported
- [ ] **Quick start**: Basic example of enabling RLS and creating a policy
- [ ] **SQL syntax**: Complete syntax for:
  - `ALTER TABLE ... ENABLE/DISABLE ROW LEVEL SECURITY`
  - `ALTER TABLE ... FORCE/NO FORCE ROW LEVEL SECURITY`
  - `CREATE POLICY` with all options
  - `ALTER POLICY`
  - `DROP POLICY`
- [ ] **Policy expressions**: How to write USING and WITH CHECK expressions
- [ ] **Role system**: How RLS integrates with RBAC
- [ ] **Examples**: Multiple real-world examples (multi-tenant, user-owned rows, role-based access)
- [ ] **Limitations**: Known limitations and differences from PostgreSQL

### 4.3 API Documentation
- [ ] **RlsContext**: How to create and configure RLS context
- [ ] **Public functions**: When to use `apply_rls_to_sql` vs `get_rls_where_clause`
- [ ] **Integration**: How to integrate RLS with application code

### 4.4 Migration Guide
- [ ] **From views**: If applicable, how to migrate from view-based RLS to AST injection
- [ ] **Version notes**: Any breaking changes from previous RLS implementation

---

## 5. Performance Considerations

- [ ] **Query overhead**: Measure performance impact of RLS on simple queries
- [ ] **Complex policies**: Test performance with complex policy expressions
- [ ] **Multiple tables**: Test performance with joins across multiple RLS-enabled tables
- [ ] **Caching**: Consider if policy lookup results can be cached
- [ ] **Index usage**: Verify RLS WHERE clauses can use indexes

---

## 6. PostgreSQL Compatibility Checklist

- [ ] `ALTER TABLE ... ENABLE ROW LEVEL SECURITY`
- [ ] `ALTER TABLE ... DISABLE ROW LEVEL SECURITY`
- [ ] `ALTER TABLE ... FORCE ROW LEVEL SECURITY`
- [ ] `ALTER TABLE ... NO FORCE ROW LEVEL SECURITY`
- [ ] `CREATE POLICY ... ON table_name`
- [ ] `CREATE POLICY ... AS PERMISSIVE`
- [ ] `CREATE POLICY ... AS RESTRICTIVE`
- [ ] `CREATE POLICY ... FOR SELECT`
- [ ] `CREATE POLICY ... FOR INSERT`
- [ ] `CREATE POLICY ... FOR UPDATE`
- [ ] `CREATE POLICY ... FOR DELETE`
- [ ] `CREATE POLICY ... FOR ALL`
- [ ] `CREATE POLICY ... TO role_name`
- [ ] `CREATE POLICY ... TO PUBLIC`
- [ ] `CREATE POLICY ... USING (expr)`
- [ ] `CREATE POLICY ... WITH CHECK (expr)`
- [ ] `ALTER POLICY ... RENAME TO`
- [ ] `DROP POLICY ... ON table_name`
- [ ] Policy combination: PERMISSIVE = OR, RESTRICTIVE = AND
- [ ] Table owner bypass (when not forced)
- [ ] Superuser bypass

---

## Review Process

1. **First Pass**: Security considerations (all items must pass)
2. **Second Pass**: Code quality and architecture
3. **Third Pass**: Test coverage (verify tests exist and pass)
4. **Fourth Pass**: Documentation completeness
5. **Final**: Run full test suite and verify PostgreSQL compatibility

## Sign-off

- [ ] Security review complete
- [ ] Code quality review complete
- [ ] Test coverage review complete
- [ ] Documentation review complete
- [ ] PostgreSQL compatibility verified
- [ ] Ready for merge
