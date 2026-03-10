# Fix Specific Failing Test Categories

## Overview
Target the top 5 most impactful failing test categories based on the compatibility suite results.

## Category 1: Column Alias Preservation (case.sql)

### Problem
CASE expressions lose their column aliases:
```sql
SELECT CASE WHEN 1 < 2 THEN 3 END AS "Simple WHEN"
-- PGQT returns: ?column?
-- PostgreSQL returns: Simple WHEN
```

### Root Cause
The transpiler generates anonymous column names (`?column?`) for expressions instead of preserving the alias from the AS clause.

### Fix
**Files:**
- `src/transpiler/expr.rs`
- `src/transpiler/dml.rs`

**Steps:**
1. In `reconstruct_res_target()`, check if the ResTarget has a name
2. If name exists, use it as the column alias
3. Ensure alias is propagated through the transpilation

**Code:**
```rust
fn reconstruct_res_target(target: &ResTarget, ctx: &mut TranspileContext) -> String {
    let val_sql = reconstruct_node(target.val.as_ref()?, ctx);
    if !target.name.is_empty() {
        format!("{} AS {}", val_sql, quote_identifier(&target.name))
    } else {
        val_sql
    }
}
```

**Verification:**
```bash
pytest runner.py -k "case.sql" -v
```

---

## Category 2: Array Type Metadata (arrays.sql)

### Problem
Array slice notation returns wrong type metadata:
```sql
select ('{{1,2,3},{4,5,6},{7,8,9}}'::int[])[1:2][2]
-- PGQT returns column type: 'array'
-- PostgreSQL returns column type: 'int4'
```

### Root Cause
The transpiler doesn't properly track element types through array operations.

### Fix
**Files:**
- `src/transpiler/expr.rs`
- `src/array.rs`

**Steps:**
1. When transpiling array subscripts, preserve the element type
2. For slices, determine if result is array or element based on dimensions
3. Return proper type metadata

**Verification:**
```bash
pytest runner.py -k "arrays.sql" -v
```

---

## Category 3: pg_class Accessibility (create_table.sql)

### Problem
System catalog queries fail:
```sql
SELECT relname FROM pg_class WHERE relname ~ '^unlogged\d'
-- PGQT: ERROR: no such table: pg_class
-- PostgreSQL: returns results
```

### Root Cause
The `pg_class` view exists but may not be accessible in all contexts or the `~` operator isn't supported.

### Fix
**Files:**
- `src/catalog/system_views.rs`
- `src/handler/mod.rs` (for operators)

**Steps:**
1. Ensure `pg_class` view is created in all schemas
2. Add `~` (regex match) operator support
3. Map to SQLite's REGEXP operator

**Code:**
```rust
// Add regex operator
conn.create_scalar_function("~", 2, ..., |ctx| {
    let text: String = ctx.get(0)?;
    let pattern: String = ctx.get(1)?;
    let regex = Regex::new(&pattern)?;
    Ok(regex.is_match(&text))
})?;
```

**Verification:**
```bash
pytest runner.py -k "create_table.sql" -v
```

---

## Category 4: DELETE Table Alias Validation (delete.sql)

### Problem
DELETE with table alias doesn't validate column references:
```sql
DELETE FROM delete_test dt WHERE delete_test.a > 25
-- PGQT: succeeds (incorrectly)
-- PostgreSQL: ERROR: invalid reference to FROM-clause entry for table "delete_test"
```

### Root Cause
The transpiler doesn't validate that column references match the table alias.

### Fix
**Files:**
- `src/transpiler/dml.rs`

**Steps:**
1. Track table aliases in DELETE statements
2. Validate that column references use the alias, not the original table name
3. Return error if mismatch detected

**Code:**
```rust
fn validate_delete_references(
    table_name: &str,
    alias: Option<&str>,
    where_clause: &Node,
) -> Result<(), ValidationError> {
    // Check that where clause doesn't reference table_name directly if alias exists
}
```

**Verification:**
```bash
pytest runner.py -k "delete.sql" -v
```

---

## Category 5: INSERT Column Count Validation (insert.sql)

### Problem
INSERT doesn't validate column count matches values:
```sql
insert into inserttest (col1, col2, col3) values (1, 2)
-- PGQT: succeeds (incorrectly)
-- PostgreSQL: ERROR: INSERT has more target columns than expressions
```

### Root Cause
The transpiler doesn't check that the number of columns matches the number of values.

### Fix
**Files:**
- `src/transpiler/dml.rs`

**Steps:**
1. In `reconstruct_insert_stmt()`, count columns and values
2. Return error if counts don't match
3. Use SQLSTATE 42601

**Code:**
```rust
if columns.len() != values.len() {
    return TranspileResult {
        sql: String::new(),
        errors: vec![ValidationError {
            code: "42601".to_string(),
            message: "INSERT has more target columns than expressions".to_string(),
            position: None,
        }],
        ...
    };
}
```

**Verification:**
```bash
pytest runner.py -k "insert.sql" -v
```

---

## Implementation Order

1. **Category 5** (INSERT validation) - Easiest, high impact
2. **Category 4** (DELETE alias validation) - Similar pattern
3. **Category 1** (Column aliases) - UI/UX improvement
4. **Category 3** (pg_class access) - System catalog
5. **Category 2** (Array types) - Most complex

## Expected Impact

| Category | Tests Fixed | Pass Rate Increase |
|----------|-------------|-------------------|
| INSERT validation | 2-3 | +4% |
| DELETE validation | 2-3 | +4% |
| Column aliases | 3-4 | +6% |
| pg_class access | 2-3 | +4% |
| Array types | 2-3 | +4% |
| **Total** | **11-16** | **~22%** |

**New Pass Rate: 28% → 50%**

---

## Quick Start Commands

```bash
# Run specific failing test
cd postgres-compatibility-suite
pytest runner.py -k "case.sql" -v

# Run all tests in a category
pytest runner.py -k "char or date or float" -v

# Generate detailed failure report
pytest runner.py --tb=long 2>&1 | tee failures.txt
```
