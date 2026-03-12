# Trigger Limitations Resolution Plan

## Overview

This document outlines the plan to address the current limitations in PGQT's trigger implementation. The core infrastructure is complete and working; these are enhancements to support more complex trigger scenarios.

## Current Limitations

### 1. Assignments to NEW.column in Trigger Functions

**Problem:** The PL/pgSQL transpiler generates invalid Lua code for assignments like:
```sql
NEW.created_at = NOW();
```

This generates:
```lua
var_3 = NEW.created_at = '2024-01-01 00:00:00'  -- Invalid Lua!
```

**Root Cause:** The transpiler treats the assignment as an expression and tries to assign it to a temporary variable, which is invalid Lua syntax.

**Impact:** Triggers cannot modify row data before insertion/update.

---

### 2. Accessing NEW.column Values for Validation

**Problem:** Reading `NEW.column` values in conditionals fails because the NEW table is empty (not populated with actual values from the INSERT/UPDATE statement).

**Example that fails:**
```sql
CREATE FUNCTION check_price() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.price < 0 THEN
        RETURN NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

**Root Cause:** The `execute_dml_with_triggers()` function creates an empty HashMap for `new_row` instead of parsing the actual values from the SQL statement.

**Impact:** Validation triggers cannot check the actual data being inserted/updated.

---

### 3. The OLD Record for UPDATE/DELETE Triggers

**Problem:** UPDATE and DELETE triggers need access to the OLD row (the existing data), but this is not implemented.

**Example that would fail:**
```sql
CREATE FUNCTION audit_delete() RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO audit_log (old_data) VALUES (OLD.name);
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;
```

**Root Cause:** `build_old_row()` exists but is not called. We need to:
1. Parse the WHERE clause from UPDATE/DELETE statements
2. Query SQLite for the matching row(s)
3. Handle the case where multiple rows might be affected

**Impact:** Audit triggers, soft-delete triggers, and data validation triggers that need old values don't work.

---

### 4. Built-in PostgreSQL Functions in Trigger Bodies

**Problem:** PostgreSQL functions like `NOW()`, `CURRENT_TIMESTAMP`, etc. are not recognized by the transpiler.

**Example that fails:**
```sql
NEW.created_at = NOW();
```

**Root Cause:** The transpiler doesn't have a mapping from PostgreSQL built-in functions to Lua equivalents.

**Impact:** Common trigger patterns like timestamp tracking don't work.

---

## Implementation Plan

### Phase 1: Fix Assignment Statements in Transpiler (Priority: HIGH)

**Estimated Effort:** 1-2 sessions

**Tasks:**
1. Modify `src/plpgsql/transpiler.rs` to handle assignment statements correctly
2. Change the code generation for `PlPgSQLStmtAssign` to not wrap in a variable assignment
3. Generate proper Lua: `NEW["column"] = value` instead of `var_x = NEW.column = value`

**Implementation Details:**
```rust
// In emit_assignment() or similar function
// Current (broken):
ctx.emit_line(&format!("var_{} = {} = {}", var_id, target, value));

// Fixed:
ctx.emit_line(&format!("{}[\"{}\"] = {}", table_name, column_name, value));
```

**Testing:**
- Add unit test for simple assignment
- Add E2E test for trigger that sets a timestamp
- Add E2E test for trigger that modifies multiple columns

---

### Phase 2: Parse INSERT/UPDATE Values to Populate NEW Row (Priority: HIGH)

**Estimated Effort:** 2-3 sessions

**Tasks:**
1. Enhance `build_new_row_from_insert()` in `src/trigger/rows.rs`
2. Add `build_new_row_from_update()` for UPDATE statements
3. Integrate these into `execute_dml_with_triggers()` in `src/handler/query.rs`

**Implementation Details:**
```rust
// In execute_dml_with_triggers()
let new_row = match operation {
    OperationType::Insert => {
        build_new_row_from_insert(conn, &table_name, original_sql)?
    }
    OperationType::Update => {
        build_new_row_from_update(conn, &table_name, original_sql)?
    }
    _ => None,
};
```

**Parsing Logic:**
- For INSERT: Parse VALUES clause to extract column names and values
- For UPDATE: Parse SET clause to get modified columns, query DB for unmodified columns
- Handle parameters (?) by looking at the params passed to execute_query_params

**Testing:**
- Unit tests for value extraction
- E2E test for validation trigger that checks NEW values
- E2E test for trigger that modifies NEW values

---

### Phase 3: Implement OLD Row Fetching (Priority: MEDIUM)

**Estimated Effort:** 2-3 sessions

**Tasks:**
1. Complete `build_old_row_from_where()` in `src/trigger/rows.rs`
2. Parse WHERE clause from UPDATE/DELETE statements
3. Query SQLite for the row(s) being modified
4. Handle multiple rows (triggers fire once per row)

**Implementation Details:**
```rust
pub fn build_old_row_from_update(
    conn: &Connection,
    table_name: &str,
    sql: &str,
) -> Result<Option<HashMap<String, Value>>> {
    // Parse UPDATE statement to get WHERE clause
    // Execute SELECT * FROM table WHERE ... to get the row
    // Return as HashMap
}
```

**Challenge:** SQLite doesn't support "FOR EACH ROW" semantics natively. We need to:
1. Query for all matching rows before executing the UPDATE/DELETE
2. Fire the trigger for each row
3. Execute the DML for each row individually (or skip if trigger returns NULL)

**Testing:**
- Unit test for WHERE clause parsing
- E2E test for audit trigger that logs OLD values
- E2E test for soft-delete trigger

---

### Phase 4: Add PostgreSQL Built-in Function Mappings (Priority: MEDIUM)

**Estimated Effort:** 1-2 sessions

**Tasks:**
1. Create a mapping table in `src/plpgsql/transpiler.rs`
2. Map common PostgreSQL functions to Lua equivalents
3. Add the Lua implementations to the runtime

**Function Mappings:**
| PostgreSQL | Lua Equivalent |
|------------|----------------|
| NOW() | `_ctx.now()` |
| CURRENT_TIMESTAMP | `_ctx.now()` |
| CURRENT_DATE | `_ctx.current_date()` |
| CURRENT_TIME | `_ctx.current_time()` |
| COALESCE(a, b) | `(a or b)` |
| NULLIF(a, b) | `(a == b and nil or a)` |

**Implementation Details:**
```rust
// In transpile_expr() or similar
match func_name.as_str() {
    "NOW" | "CURRENT_TIMESTAMP" => Ok("_ctx.now()".to_string()),
    "CURRENT_DATE" => Ok("_ctx.current_date()".to_string()),
    // ... etc
}
```

**Add to Lua runtime:**
```rust
// In execute_plpgsql_trigger()
let now_fn = lua.create_function(|_lua, ()| {
    Ok(chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.f").to_string())
})?;
api.set("now", now_fn)?;

let current_date_fn = lua.create_function(|_lua, ()| {
    Ok(chrono::Local::now().format("%Y-%m-%d").to_string())
})?;
api.set("current_date", current_date_fn)?;
```

**Testing:**
- Unit tests for each function mapping
- E2E test for timestamp trigger using NOW()

---

### Phase 5: Enhanced Test Suite (Priority: LOW)

**Estimated Effort:** 1 session

**Tasks:**
1. Re-enable the advanced trigger tests that were simplified
2. Add tests for edge cases:
   - Multiple triggers firing in order
   - Trigger that modifies PK
   - Trigger that raises exceptions
   - Recursive triggers (trigger causes another trigger)

---

## Implementation Order

1. **Phase 1** (Assignments) - Unblocks basic data modification triggers
2. **Phase 2** (NEW row population) - Unblocks validation triggers
3. **Phase 4** (Built-in functions) - Easy win, adds NOW() support
4. **Phase 3** (OLD row) - More complex, needed for audit triggers
5. **Phase 5** (Enhanced tests) - Final validation

## Success Criteria

All of the following should work:

```sql
-- Set timestamp on insert
CREATE FUNCTION set_created_at() RETURNS TRIGGER AS $$
BEGIN
    NEW.created_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Validate data
CREATE FUNCTION check_price() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.price < 0 THEN
        RAISE EXCEPTION 'Price cannot be negative';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Audit logging
CREATE FUNCTION audit_changes() RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO audit_log (table_name, old_data, new_data, changed_at)
    VALUES (TG_TABLE_NAME, OLD.name, NEW.name, NOW());
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

## Notes

- Each phase can be developed independently
- Phases 1-2 are critical for basic trigger functionality
- Phase 3 requires careful handling of multi-row operations
- Consider adding a "trigger mode" flag to disable triggers for bulk operations (performance)
