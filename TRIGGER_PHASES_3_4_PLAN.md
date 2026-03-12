# Trigger Implementation: Phase 3 & 4 Plan

## Overview

This document outlines the remaining work to complete PostgreSQL-compatible trigger execution in PGQT. The infrastructure (catalog, parsing, metadata) is complete; this plan covers the actual execution hooks and integration testing.

**Prerequisites:** Phase 1 (Catalog) and Phase 2 (Parsing) are complete.

---

## Phase 3: Execution Hooks

### Goal
Integrate trigger execution into INSERT/UPDATE/DELETE operations, enabling PL/pgSQL trigger functions to fire and modify data.

### Tasks

#### 3.1 Hook Integration in Query Handler
**File:** `src/handler/query.rs`

**Current State:** `execute_query()` runs SQL directly without trigger checks.

**Implementation:**
```rust
// In execute_query(), before executing DML:
// 1. Parse the SQL to identify table and operation type
// 2. Look up applicable BEFORE triggers
// 3. If row-level: fetch OLD row (for UPDATE/DELETE) or build NEW row (for INSERT)
// 4. Execute trigger functions in order
// 5. Process return values (modify rows or abort)
// 6. Execute the actual DML
// 7. Look up applicable AFTER triggers
// 8. Execute AFTER triggers
```

**Key Functions to Add:**
- `get_table_from_query(sql: &str) -> Option<String>` - Extract table name from DML
- `should_fire_triggers(op: OperationType) -> bool` - Check if operation needs triggers
- `execute_before_triggers(...)` - Fire BEFORE triggers
- `execute_after_triggers(...)` - Fire AFTER triggers

**Testing:**
```bash
cargo test --lib handler::query::tests
cargo test --test trigger_tests
```

#### 3.2 OLD/NEW Row Building
**Files:** `src/handler/query.rs`, `src/trigger/rows.rs` (new)

**Implementation:**
```rust
// Build row data from SQLite for trigger functions
pub fn build_old_row(
    conn: &Connection,
    table_name: &str,
    pk_columns: &[String],
    pk_values: &[Value]
) -> Result<HashMap<String, Value>>;

pub fn build_new_row(
    values: &[(String, Value)]
) -> HashMap<String, Value>;
```

**Considerations:**
- Need to query SQLite for OLD row values before UPDATE/DELETE
- NEW row comes from INSERT/UPDATE statement values
- Handle composite primary keys
- Type conversion from SQLite to trigger variables

**Testing:**
```bash
cargo test --lib trigger::rows::tests
```

#### 3.3 Complete execute_plpgsql_trigger
**File:** `src/plpgsql/mod.rs`

**Current State:** Stub that validates function exists but doesn't execute.

**Full Implementation:**
```rust
pub fn execute_plpgsql_trigger(
    conn: &Connection,
    function_name: &str,
    trigger_name: &str,
    trigger_timing: &str,
    trigger_event: &str,
    table_name: &str,
    table_schema: &str,
    trigger_args: &[String],
    old_row: Option<HashMap<String, Value>>,
    new_row: Option<HashMap<String, Value>>,
    functions_cache: &Arc<DashMap<String, FunctionMetadata>>,
) -> Result<Option<HashMap<String, Value>>> {
    // 1. Look up function in catalog
    // 2. Transpile PL/pgSQL body to Lua
    // 3. Create Lua runtime with trigger variables:
    //    - TG_NAME, TG_WHEN, TG_LEVEL, TG_OP
    //    - TG_TABLE_NAME, TG_TABLE_SCHEMA
    //    - TG_NARGS, TG_ARGV
    //    - NEW (table), OLD (table)
    // 4. Execute Lua function
    // 5. Process result:
    //    - nil → return None (abort operation)
    //    - table → convert to HashMap (modified row)
    //    - other → return original new_row
}
```

**Testing:**
```bash
cargo test --lib plpgsql::tests::test_trigger_execution
```

#### 3.4 Handle BEFORE Trigger Return Values
**File:** `src/handler/query.rs`

**Logic:**
```rust
// For BEFORE triggers:
match trigger_result {
    None => {
        // Trigger returned NULL - abort operation
        return Ok(vec![Response::Error(
            PgError::trigger_aborted(trigger_name)
        )]);
    }
    Some(modified_row) => {
        // Use modified values for DML
        update_statement_with_modified_values(&mut sql, modified_row)?;
    }
}
```

**Testing:**
- Test trigger that aborts operation
- Test trigger that modifies NEW row
- Test trigger that returns unchanged row

---

## Phase 4: Integration Testing

### Goal
Comprehensive E2E testing through the wire protocol using Python psycopg2.

### Tasks

#### 4.1 Create E2E Test Framework
**File:** `tests/trigger_e2e_test.py` (new)

**Structure:**
```python
#!/usr/bin/env python3
"""End-to-end tests for PostgreSQL trigger functionality."""
import subprocess
import time
import psycopg2
import os
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_trigger_e2e.db"

def start_proxy():
    """Start PGQT proxy server."""
    # Build release binary first
    proc = subprocess.Popen(
        ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    time.sleep(2)  # Wait for startup
    return proc

def stop_proxy(proc):
    """Stop PGQT proxy server."""
    proc.send_signal(signal.SIGTERM)
    proc.wait()

def test_before_insert_trigger():
    """Test BEFORE INSERT trigger modifies data."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table
        cur.execute("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, created_at TIMESTAMP)")
        
        # Create PL/pgSQL function
        cur.execute("""
            CREATE FUNCTION set_created_at() RETURNS TRIGGER AS $$
            BEGIN
                NEW.created_at = NOW();
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)
        
        # Create trigger
        cur.execute("""
            CREATE TRIGGER before_insert_users
            BEFORE INSERT ON users
            FOR EACH ROW
            EXECUTE FUNCTION set_created_at();
        """)
        
        # Insert data
        cur.execute("INSERT INTO users (name) VALUES ('Alice')")
        conn.commit()
        
        # Verify trigger fired
        cur.execute("SELECT name, created_at FROM users WHERE name = 'Alice'")
        row = cur.fetchone()
        assert row[0] == 'Alice'
        assert row[1] is not None, "Trigger should have set created_at"
        
        cur.close()
        conn.close()
        print("test_before_insert_trigger: PASSED")
    finally:
        stop_proxy(proc)
        if os.path.exists(DB_PATH):
            os.remove(DB_PATH)

if __name__ == "__main__":
    test_before_insert_trigger()
```

#### 4.2 Create Test Cases

**Test 1: BEFORE INSERT - Set Default Value**
- Create trigger that sets a column value before insert
- Verify value is set correctly

**Test 2: BEFORE UPDATE - Validate Data**
- Create trigger that validates data and aborts if invalid
- Verify operation aborts on invalid data
- Verify operation succeeds on valid data

**Test 3: AFTER INSERT - Audit Logging**
- Create trigger that inserts into audit table after insert
- Verify audit record is created

**Test 4: Multiple Triggers - Execution Order**
- Create multiple triggers on same table/event
- Verify they fire in correct order (by OID)

**Test 5: Trigger with Arguments**
- Create trigger with custom arguments
- Verify arguments are accessible in TG_ARGV

**Test 6: DELETE Trigger**
- Create BEFORE DELETE trigger
- Verify OLD record is accessible

**Test 7: Abort Operation**
- Create trigger that returns NULL
- Verify operation is aborted

#### 4.3 Run Full Test Suite

**Command:**
```bash
./run_tests.sh
```

**Verification Steps:**
1. Unit tests pass (cargo test --lib)
2. Integration tests pass (cargo test --test)
3. E2E tests pass (python3 tests/trigger_e2e_test.py)

---

## Development Workflow

For each task, follow this workflow:

### 1. Before Starting
```bash
# Ensure clean baseline
cargo test
./run_tests.sh
```

### 2. During Development
```bash
# Run relevant tests frequently
cargo test --lib trigger

# Check transpilation output
cargo run -- --transpile "SELECT ..."
```

### 3. Before Committing
```bash
# Run full test suite
./run_tests.sh

# Check release build
cargo build --release

# Fix any warnings
cargo clippy --fix

# Clean up temporary files
rm -f *.db *.db.error.log
```

### 4. Documentation Updates

Update these files as needed:
- `README.md` - Add trigger support to feature list
- `docs/triggers.md` (new) - Comprehensive trigger documentation
- `AGENTS.md` - Update if new patterns are introduced
- `CHANGELOG.md` - Document new features

---

## Build Verification Checklist

Before marking each phase complete:

- [ ] `./run_tests.sh` passes completely
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings (or warnings are justified)
- [ ] All new code has doc comments
- [ ] README/docs updated to reflect new features
- [ ] E2E tests verify wire protocol behavior

---

## Estimated Timeline

| Phase | Task | Estimated Effort |
|-------|------|-----------------|
| 3.1 | Hook Integration | 1 session |
| 3.2 | OLD/NEW Row Building | 1 session |
| 3.3 | execute_plpgsql_trigger | 1-2 sessions |
| 3.4 | Return Value Handling | 1 session |
| 4.1 | E2E Test Framework | 0.5 session |
| 4.2 | Test Cases | 1 session |
| 4.3 | Documentation | 0.5 session |

**Total: 5-6 development sessions**

---

## Success Criteria

Phase 3 & 4 are complete when:

1. **Functionality:**
   - BEFORE INSERT triggers can modify NEW row
   - BEFORE UPDATE triggers can validate and abort
   - AFTER triggers fire after DML completes
   - Trigger functions receive correct TG_* variables
   - OLD/NEW rows are correctly populated

2. **Testing:**
   - All unit tests pass
   - All integration tests pass
   - All E2E tests pass
   - Manual testing with psql confirms behavior

3. **Quality:**
   - No build warnings
   - Code follows project patterns
   - Documentation is complete
   - Error handling is robust

4. **Performance:**
   - Trigger lookup is fast (indexed)
   - Trigger execution overhead is minimal
   - No regressions in non-trigger queries