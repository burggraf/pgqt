# RBAC Completion Design Document

**Date:** March 8, 2026  
**Status:** Design Validated ✅  
**Goal:** Complete PostgreSQL-compatible RBAC with full test coverage and documentation

---

## 1. Overview & Architecture

### Current State
- Core tables exist (`__pg_authid__`, `__pg_acl__`, `__pg_auth_members__`)
- Basic CREATE ROLE, DROP ROLE, GRANT, REVOKE on tables work
- Permission checking exists but is untested
- ALTER DEFAULT PRIVILEGES, ALTER OWNER, COMMENT ON are ignored
- Schema/function GRANTs not implemented
- No dedicated tests exist

### Proposed Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Wire Protocol                         │
│              (E2E tests verify errors)                   │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│              SqliteHandler (src/handler)                 │
│  - check_permissions() - recursive role resolution       │
│  - enforce_grants() - privilege enforcement              │
│  - handle_set_role() - session role switching            │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│              Transpiler (src/transpiler)                 │
│  - CREATE/ALTER/DROP ROLE                                │
│  - GRANT/REVOKE (table, schema, function)                │
│  - ALTER DEFAULT PRIVILEGES                              │
│  - ALTER OWNER                                           │
│  - COMMENT ON (security labels)                          │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│              Catalog (src/catalog)                       │
│  - __pg_authid__ (roles)                                 │
│  - __pg_auth_members__ (role membership)                 │
│  - __pg_acl__ (object privileges)                        │
│  - __pg_default_acl__ (default privileges)               │
│  - __pg_description__ (comments/labels)                  │
└─────────────────────────────────────────────────────────┘
```

---

## 2. Implementation Details

### 2.1 ALTER DEFAULT PRIVILEGES

PostgreSQL's `ALTER DEFAULT PRIVILEGES` allows setting privileges that apply to objects created in the future.

**Storage:** New table `__pg_default_acl__`:
```sql
CREATE TABLE __pg_default_acl__ (
    defaclrole INTEGER NOT NULL,      -- Role OID
    defaclnamespace INTEGER,          -- Schema OID (NULL for all schemas)
    defaclobjtype TEXT NOT NULL,      -- 'r'=table, 'S'=sequence, 'f'=function, 'T'=type, 'n'=schema
    defaclacl TEXT NOT NULL,          -- ACL string (PostgreSQL format)
    PRIMARY KEY (defaclrole, defaclnamespace, defaclobjtype)
);
```

**Transpilation:** Convert to INSERT/UPDATE/DELETE on `__pg_default_acl__`

**Enforcement:** When CREATE TABLE/FUNCTION/etc executes, look up matching default ACLs and populate `__pg_acl__`

### 2.2 ALTER OWNER

Changes object ownership.

**Implementation:**
- Add `relowner` updates to `__pg_relation_meta__` for tables
- Add similar ownership tracking for functions (new table or extend existing)
- Update `__pg_acl__` to reflect owner has all privileges

### 2.3 Schema & Function GRANTs

**Schema privileges:** CREATE, USAGE
- `__pg_acl__` already supports `object_type='schema'`
- Need to check on CREATE statements (requires CREATE privilege)

**Function privileges:** EXECUTE
- Extend function metadata with owner
- Check on function calls

### 2.4 SET ROLE Support

Currently session uses fixed key `0`. Need:
- `SET ROLE` updates session context
- `RESET ROLE` restores original
- `SESSION_USER` vs `CURRENT_USER` distinction

---

## 3. Testing Architecture (Three-Layer Approach)

### 3.1 Unit Tests

Fast tests for permission logic:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_superuser_bypasses_all_checks() {
        // Should return true immediately for superuser
    }
    
    #[test]
    fn test_role_inheritance_resolution() {
        // Test recursive CTE for effective_roles
    }
    
    #[test]
    fn test_privilege_check_with_grant_option() {
        // Test grantable flag handling
    }
    
    #[test]
    fn test_default_privileges_applied() {
        // Test ALTER DEFAULT PRIVILEGES logic
    }
}
```

### 3.2 Integration Tests (`tests/rbac_tests.rs`)

Test transpilation and catalog operations:

```rust
#[test]
fn test_create_role_transpilation() {
    let sql = "CREATE ROLE admin WITH SUPERUSER CREATEDB";
    let result = transpile(sql);
    // Verify it produces correct INSERT into __pg_authid__
}

#[test]
fn test_grant_table_privileges() {
    // Create table, grant SELECT to role, verify __pg_acl__ entry
}

#[test]
fn test_revoke_privileges() {
    // Grant then revoke, verify __pg_acl__ entry removed
}

#[test]
fn test_role_membership() {
    // GRANT role1 TO role2, verify __pg_auth_members__
}
```

### 3.3 E2E Tests (`tests/rbac_e2e_test.py`)

Full wire protocol tests:

```python
def test_permission_denied_error_format():
    """Verify error matches PostgreSQL format exactly."""
    # Create table as admin
    # Create limited user
    # Try SELECT without GRANT
    # Should get: ERROR:  permission denied for table users
    # SQLSTATE: 42501

def test_superuser_bypass():
    """Superuser should bypass all permission checks."""

def test_role_inheritance_effective_permissions():
    """Test that role membership grants inherited privileges."""

def test_set_role_changes_effective_user():
    """SET ROLE should change current_user and permissions."""

def test_cascade_revoke():
    """Test that REVOKE CASCADE removes dependent grants."""

def test_default_privileges_on_new_objects():
    """ALTER DEFAULT PRIVILEGES should apply to new tables."""
```

---

## 4. Documentation Structure

### 4.1 Update `docs/RBAC-PROGRESS.md`
Refresh the progress tracker with:
- ✅ Completed items (already done)
- 🚧 In Progress (what we're implementing now)
- ⏳ Pending (future enhancements)
- Clear completion percentages

### 4.2 New `docs/RBAC.md` (User Guide)
Similar to docs/RLS.md structure with:
- Overview
- Creating Roles (CREATE ROLE syntax and attributes)
- Granting Privileges (table, schema, function)
- Revoking Privileges
- Default Privileges
- System Catalog Views
- Permission Enforcement rules

### 4.3 Update README.md
Update the RBAC section to reflect full implementation status.

---

## 5. Files to Modify

### New Files
- `tests/rbac_tests.rs` - Integration tests
- `tests/rbac_e2e_test.py` - E2E tests
- `docs/RBAC.md` - User documentation

### Modified Files
- `src/catalog/init.rs` - Add `__pg_default_acl__`, `__pg_description__` tables
- `src/catalog/system_views.rs` - Add pg_default_acl view
- `src/transpiler/rls/utils.rs` - Add ALTER DEFAULT PRIVILEGES, ALTER OWNER
- `src/transpiler/mod.rs` - Remove "IGNORED" for AlterDefaultPrivilegesStmt, AlterOwnerStmt, CommentStmt
- `src/transpiler/ddl.rs` - Apply default privileges on CREATE
- `src/handler/utils.rs` - Enhance check_permissions(), add SET ROLE handling
- `src/handler/mod.rs` - Add SET ROLE, RESET ROLE command handling
- `docs/RBAC-PROGRESS.md` - Update progress
- `README.md` - Update RBAC section

---

## 6. Success Criteria

- [ ] All existing tests pass
- [ ] New unit tests for permission logic: 100% coverage of check_permissions()
- [ ] New integration tests: 20+ test cases
- [ ] New E2E tests: 15+ test cases
- [ ] ALTER DEFAULT PRIVILEGES works end-to-end
- [ ] ALTER OWNER works end-to-end
- [ ] Schema GRANT (USAGE, CREATE) works end-to-end
- [ ] Function GRANT (EXECUTE) works end-to-end
- [ ] SET ROLE / RESET ROLE works end-to-end
- [ ] Documentation is complete and accurate
- [ ] Error messages match PostgreSQL format (SQLSTATE 42501, etc.)

---

## 7. PostgreSQL Compatibility Reference

### Error Codes
| Scenario | SQLSTATE | Message |
|----------|----------|---------|
| Permission denied | 42501 | permission denied for table %s |
| Role does not exist | 42704 | role "%s" does not exist |
| Cannot drop role | 2BP01 | role "%s" cannot be dropped because some objects depend on it |
| Duplicate role | 42710 | role "%s" already exists |

### System Views
| View | Columns |
|------|---------|
| pg_roles | oid, rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin, rolconnlimit, rolvaliduntil, rolreplication, rolbypassrls |
| pg_authid | Same as pg_roles + rolpassword |
| pg_auth_members | oid, roleid, member, grantor, admin_option |
| pg_default_acl | oid, defaclrole, defaclnamespace, defaclobjtype, defaclacl |
| information_schema.table_privileges | Standard ISO view |

---

**Next Step:** Create detailed implementation plan using `/skill:writing-plans`
