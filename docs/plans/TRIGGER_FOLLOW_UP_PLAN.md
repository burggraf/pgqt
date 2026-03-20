# Trigger Implementation Follow-Up Plan

## Overview

This document outlines the remaining work to fully complete the trigger implementation after the initial 5-phase resolution plan. The core functionality is working, but there are known limitations and edge cases to address.

---

## Phase 1: Complete WHERE Clause Deparsing for OLD Row Fetching

### Problem
The `deparse_where_clause()` function in `src/trigger/rows.rs` is currently a stub that always returns an error. This prevents OLD row fetching from working for UPDATE/DELETE statements with WHERE clauses.

### Current State
```rust
fn deparse_where_clause(where_clause: &pg_query::protobuf::Node) -> Result<String> {
    // TODO: Implement proper deparsing of WHERE clauses
    Err(anyhow!("WHERE clause deparsing not yet fully implemented"))
}
```

### Implementation Details

1. **Use pg_query's deparse functionality**
   - pg_query can deparse a protobuf Node back to SQL
   - Create a minimal parse result containing just the WHERE clause node
   - Call pg_query::deparse() to get the SQL string

2. **Implementation approach:**
   ```rust
   fn deparse_where_clause(where_clause: &pg_query::protobuf::Node) -> Result<String> {
       // Create a minimal SELECT statement with the WHERE clause
       // pg_query requires a full statement to deparse properly
       let temp_sql = format!("SELECT 1 WHERE {}", placeholder);
       
       // Parse it to get the structure
       let parsed = pg_query::parse(&temp_sql)?;
       
       // Replace the placeholder with our actual WHERE clause
       // Then deparse back to SQL
       
       // Alternative: Use pg_query::deparse directly if possible
       pg_query::deparse(&create_minimal_parse_result(where_clause))
   }
   ```

3. **Alternative simpler approach:**
   - Extract the WHERE clause text directly from the original SQL using position info
   - pg_query provides stmt_location and stmt_len in RawStmt
   - Use string slicing to extract the WHERE clause

### Testing
- Unit test: `test_deparse_where_clause_simple`
- Unit test: `test_deparse_where_clause_complex`
- E2E test: UPDATE trigger with WHERE clause
- E2E test: DELETE trigger with WHERE clause

---

## Phase 2: Support Multi-Row Operations (FOR EACH ROW Semantics)

### Problem
Currently triggers fire once per statement. PostgreSQL's "FOR EACH ROW" semantics require firing once for each affected row.

### Implementation Details

1. **For UPDATE statements:**
   ```rust
   // Before executing UPDATE:
   // 1. Query for all rows matching WHERE clause
   // 2. For each row:
   //    a. Build OLD row
   //    b. Build NEW row (merge SET values)
   //    c. Execute BEFORE trigger
   //    d. If trigger returns NULL, skip this row
   //    e. Otherwise, execute UPDATE for this specific row
   // 3. After all rows, execute AFTER trigger for each modified row
   ```

2. **For DELETE statements:**
   ```rust
   // Before executing DELETE:
   // 1. Query for all rows matching WHERE clause
   // 2. For each row:
   //    a. Build OLD row
   //    b. Execute BEFORE trigger
   //    c. If trigger returns NULL, skip this row
   //    d. Otherwise, execute DELETE for this specific row
   // 3. After all rows, execute AFTER trigger for each deleted row
   ```

3. **Implementation location:** `src/handler/query.rs` in `execute_dml_with_triggers()`

4. **Challenge:** Need to execute DML per-row instead of single statement
   - Option A: Modify SQL to use primary key for each row
   - Option B: Use SQLite's rowid for single-row operations

### Testing
- E2E test: UPDATE affecting multiple rows with trigger
- E2E test: DELETE affecting multiple rows with trigger
- E2E test: Trigger that skips some rows (returns NULL)

---

## Phase 3: Apply Trigger-Modified NEW Row to SQL

### Problem
If a BEFORE trigger modifies the NEW row, those changes aren't reflected in the executed SQL.

### Current State (in `execute_dml_with_triggers()`):
```rust
BeforeTriggerResult::Continue(modified_new_row) => {
    // TODO: If new_row was modified, we need to update the SQL
    // For now, just execute the original SQL
    let _ = modified_new_row; // Suppress unused warning for now
    
    // Execute the DML
    let result = self.execute_statement(conn, sqlite_sql)?;
    // ...
}
```

### Implementation Details

1. **For INSERT:**
   - Compare modified_new_row with original new_row
   - If different, reconstruct the INSERT statement with new values
   - Handle case where trigger adds/modifies columns

2. **For UPDATE:**
   - Compare modified_new_row with original new_row  
   - If different, reconstruct the SET clause with new values
   - Keep the same WHERE clause

3. **Implementation approach:**
   ```rust
   fn rebuild_insert_sql(original_sql: &str, modified_row: &HashMap<String, Value>) -> Result<String> {
       // Parse original INSERT
       // Replace VALUES with modified values
       // Return new SQL
   }
   
   fn rebuild_update_sql(original_sql: &str, modified_row: &HashMap<String, Value>) -> Result<String> {
       // Parse original UPDATE
       // Replace SET clause with modified values
       // Keep WHERE clause
       // Return new SQL
   }
   ```

### Testing
- E2E test: Trigger that modifies a column value
- E2E test: Trigger that adds a default value
- E2E test: Verify modified values are persisted to database

---

## Phase 4: Add More PostgreSQL Built-in Functions

### Current Functions (Implemented)
- NOW(), CURRENT_TIMESTAMP → _ctx.now()
- CURRENT_DATE → _ctx.current_date()
- CURRENT_TIME → _ctx.current_time()
- COALESCE() → _ctx.coalesce()
- NULLIF() → _ctx.nullif()

### Additional Functions to Add

#### String Functions
| PostgreSQL | Lua Equivalent |
|------------|----------------|
| UPPER(s) | `string.upper(s)` |
| LOWER(s) | `string.lower(s)` |
| LENGTH(s) | `#s` or `string.len(s)` |
| SUBSTRING(s, start, len) | `string.sub(s, start, start + len - 1)` |
| TRIM(s) | `(s:gsub("^%s*(.-)%s*$", "%1"))` |
| REPLACE(s, from, to) | `(s:gsub(from, to))` |

#### Math Functions
| PostgreSQL | Lua Equivalent |
|------------|----------------|
| ABS(x) | `math.abs(x)` |
| ROUND(x) | `math.floor(x + 0.5)` |
| CEIL(x) | `math.ceil(x)` |
| FLOOR(x) | `math.floor(x)` |
| GREATEST(a, b, ...) | custom implementation |
| LEAST(a, b, ...) | custom implementation |

#### Date/Time Functions
| PostgreSQL | Lua Equivalent |
|------------|----------------|
| EXTRACT(field FROM date) | custom implementation |
| DATE_TRUNC(field, date) | custom implementation |
| AGE(timestamp) | custom implementation |

### Implementation Details

1. **Add to transpiler** (`src/plpgsql/transpiler.rs`):
   - Extend `map_postgres_functions()` with new mappings

2. **Add to runtime** (`src/plpgsql/runtime.rs`):
   - Add Lua implementations for each function
   - Register in `create_api_table()`

### Testing
- Unit test for each function mapping
- E2E test using functions in trigger context

---

## Phase 5: Fix Build Warnings

### Current Warnings to Address

Run `cargo build --release` and fix all warnings. Common types:
- Unused imports
- Unused variables
- Dead code warnings
- Deprecated function usage

### Process
```bash
cargo build --release 2>&1 | grep -E "^warning:|^error:"
# Fix each warning
# Rebuild until clean
```

---

## Phase 6: Documentation Updates

### Files to Update

1. **README.md**
   - Update trigger feature description
   - Add examples of working trigger patterns
   - Document any remaining limitations

2. **docs/TRIGGERS.md** (create if doesn't exist)
   - Comprehensive trigger documentation
   - Supported trigger types
   - Examples:
     - Timestamp trigger
     - Validation trigger
     - Audit trigger
   - Known limitations

3. **CHANGELOG.md** (create if doesn't exist)
   - Document trigger improvements
   - List new supported functions

### Documentation Content

```markdown
## Trigger Support

PGQT now supports PostgreSQL-compatible triggers with the following capabilities:

### Supported Trigger Types
- BEFORE INSERT/UPDATE/DELETE
- AFTER INSERT/UPDATE/DELETE
- FOR EACH ROW (statement-level triggers not yet supported)

### Supported Features
- Accessing NEW and OLD row data
- Modifying NEW row values in BEFORE triggers
- Using PostgreSQL built-in functions (NOW, CURRENT_TIMESTAMP, etc.)
- Raising exceptions to abort operations

### Example Triggers

#### Automatic Timestamp
```sql
CREATE FUNCTION set_timestamp() RETURNS TRIGGER AS $$
BEGIN
    NEW.created_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER before_insert
BEFORE INSERT ON my_table
FOR EACH ROW EXECUTE FUNCTION set_timestamp();
```

#### Data Validation
```sql
CREATE FUNCTION check_price() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.price < 0 THEN
        RAISE EXCEPTION 'Price cannot be negative';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

### Known Limitations
- FOR EACH STATEMENT triggers not supported
- Complex WHERE clause deparsing has limited support
- Multi-row operations execute trigger once per statement, not per row
```

---

## Implementation Order

1. **Phase 5: Fix Build Warnings** (Do first - quick win)
2. **Phase 1: WHERE Clause Deparsing** (Critical for OLD row support)
3. **Phase 3: Apply Modified NEW Row** (Important for data integrity)
4. **Phase 2: Multi-Row Operations** (Complex, lower priority)
5. **Phase 4: More Built-in Functions** (Nice to have)
6. **Phase 6: Documentation** (Final step)

---

## Verification Checklist

Before considering complete:

- [ ] `cargo build --release` produces no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] New E2E tests pass against live proxy
- [ ] Documentation is updated
- [ ] CHANGELOG is updated

---

## Success Criteria

All of these should work without issues:

```sql
-- 1. Timestamp trigger with NOW()
CREATE TRIGGER set_timestamp
BEFORE INSERT ON orders
FOR EACH ROW EXECUTE FUNCTION set_created_at();

-- 2. Validation trigger reading NEW values
CREATE TRIGGER check_price
BEFORE INSERT ON products
FOR EACH ROW EXECUTE FUNCTION validate_price();

-- 3. Audit trigger accessing OLD values
CREATE TRIGGER audit_changes
AFTER UPDATE ON customers
FOR EACH ROW EXECUTE FUNCTION log_changes();

-- 4. Trigger modifying NEW values
CREATE TRIGGER set_defaults
BEFORE INSERT ON users
FOR EACH ROW EXECUTE FUNCTION apply_defaults();

-- 5. Multi-row UPDATE with per-row trigger
UPDATE products SET price = price * 1.1 WHERE category = 'electronics';
```
