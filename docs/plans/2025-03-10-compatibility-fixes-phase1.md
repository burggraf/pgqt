# PostgreSQL Compatibility Fixes - Phase 1

## Objective
Integrate the validation framework into the transpiler to enforce type validation during INSERT/UPDATE operations.

## Current State
- Validation framework exists in `src/validation/` but is NOT called during query execution
- Tests show PGQT accepts invalid data that PostgreSQL rejects:
  - `INSERT INTO CHAR_TBL (f1) VALUES ('cd')` - should fail for CHAR(1)
  - `INSERT INTO FLOAT4_TBL(f1) VALUES ('10e70')` - should fail (overflow)
  - `INSERT INTO DATE_TBL VALUES ('1997-02-29')` - should fail (invalid date)

## Implementation Plan

### Task 1: Extract Column Metadata with Lengths

**Files:**
- Modify: `src/catalog/table.rs`
- Modify: `src/catalog/mod.rs`

**Steps:**
1. Add function to get column metadata including type modifiers (lengths)
2. Store VARCHAR(n) and CHAR(n) lengths in catalog
3. Expose API: `get_column_metadata(table: &str, column: &str) -> ColumnMetadata`

**Verification:**
```rust
#[test]
fn test_get_column_metadata() {
    let meta = get_column_metadata("test_table", "name");
    assert_eq!(meta.type_name, "varchar");
    assert_eq!(meta.max_length, Some(255));
}
```

---

### Task 2: Integrate Validation into INSERT Processing

**Files:**
- Modify: `src/transpiler/dml.rs`
- Modify: `src/transpiler/expr.rs`

**Steps:**
1. In `reconstruct_insert_stmt()`, after parsing values:
   - Get target table column metadata
   - For each string value, check if target column is VARCHAR/CHAR
   - Call validation functions
2. Return validation errors in `TranspileResult.errors`

**Code Structure:**
```rust
fn validate_insert_values(
    table_name: &str,
    columns: &[ColumnRef],
    values: &[Node],
    ctx: &TranspileContext,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let column_meta = get_table_columns(table_name);
    
    for (i, (col, val)) in columns.iter().zip(values.iter()).enumerate() {
        if let Some(meta) = column_meta.get(&col.name) {
            if let Some(string_val) = extract_string_literal(val) {
                match validate_value(string_val, meta) {
                    Ok(()) => {},
                    Err(e) => errors.push(e),
                }
            }
        }
    }
    errors
}
```

**Verification:**
```rust
#[test]
fn test_insert_validation_rejects_too_long() {
    let sql = "CREATE TABLE t (c CHAR(1)); INSERT INTO t VALUES ('ab');";
    let result = transpile_with_metadata(sql);
    assert!(!result.errors.is_empty());
    assert_eq!(result.errors[0].code, "22001");
}
```

---

### Task 3: Integrate Validation into UPDATE Processing

**Files:**
- Modify: `src/transpiler/dml.rs`

**Steps:**
1. Similar to INSERT, validate SET clause values
2. Check WHERE clause values if they reference table columns

**Verification:**
```rust
#[test]
fn test_update_validation_rejects_invalid() {
    let sql = "UPDATE t SET c = 'ab' WHERE id = 1;";
    let result = transpile_with_metadata(sql);
    // Should have validation error if c is CHAR(1)
}
```

---

### Task 4: Add Date/Time Validation

**Files:**
- Modify: `src/validation/types.rs`
- Modify: `src/transpiler/expr.rs`

**Steps:**
1. Add `validate_date()`, `validate_timestamp()`, `validate_timestamptz()` functions
2. Check for invalid dates (Feb 29 in non-leap years)
3. Validate timezone names
4. Integrate into INSERT/UPDATE validation

**Verification:**
```rust
#[test]
fn test_date_validation_rejects_invalid() {
    let result = validate_date("1997-02-29"); // 1997 is not a leap year
    assert!(result.is_err());
}
```

---

### Task 5: Add Numeric Range Validation

**Files:**
- Modify: `src/validation/types.rs`

**Steps:**
1. Add `validate_float4()`, `validate_float8()`, `validate_int2()`, etc.
2. Check for overflow conditions
3. Integrate into validation pipeline

**Verification:**
```rust
#[test]
fn test_float4_validation_rejects_overflow() {
    let result = validate_float4("10e70");
    assert!(result.is_err());
}
```

---

### Task 6: Error Handling in Handler

**Files:**
- Modify: `src/handler/mod.rs`

**Steps:**
1. Check `TranspileResult.errors` before executing
2. Return proper PostgreSQL error responses with SQLSTATE codes
3. Ensure errors are propagated to client

**Verification:**
Run compatibility test suite and verify:
- `char.sql` tests now pass
- `date.sql` tests now pass  
- `float4.sql` and `float8.sql` tests now pass

---

## Success Criteria

1. **Test Results:**
   - `char.sql` compatibility tests pass
   - `date.sql` compatibility tests pass
   - `float4.sql` and `float8.sql` compatibility tests pass
   - `insert.sql` column count validation tests pass

2. **Metrics:**
   - Pass rate increases from 28% to 35%+
   - Type validation errors return correct SQLSTATE codes (22001, 22003, 22008)

3. **Code Quality:**
   - All existing tests still pass
   - New validation tests added
   - No regressions in functionality

---

## Estimated Timeline

- Tasks 1-2: 2-3 days
- Task 3: 1 day
- Tasks 4-5: 2-3 days
- Task 6: 1 day
- Testing & Debugging: 2 days

**Total: 8-10 days**

---

## Dependencies

- Requires validation framework from previous implementation
- Requires catalog access to column metadata
- May need to extend catalog schema to store type modifiers
