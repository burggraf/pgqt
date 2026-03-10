# PGQT Phase 2 Compatibility Fixes - Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Fix type system issues: CHAR/VARCHAR trimming, date/time validation, array type metadata, and system catalog tables.

**Architecture:** Extend the type validation system, add catalog views, and fix type metadata handling in the transpiler.

**Tech Stack:** Rust, SQLite views for system catalogs, pg_query for SQL parsing

---

## Task 1: Fix CHAR/VARCHAR Trimming Behavior

**Problem:** PostgreSQL automatically trims trailing spaces from CHAR/VARCHAR inputs. PGQT treats `'c     '` as too long for `character(1)`.

**Example Failure:**
```sql
INSERT INTO CHAR_TBL (f1) VALUES ('c     ')
-- Error: value too long for type character(1)
-- PostgreSQL trims to 'c' and accepts it
```

**Files:**
- Modify: `src/validation/types.rs` - Add trimming logic for CHAR/VARCHAR validation
- Modify: `src/transpiler/dml.rs` - Trim values in INSERT/UPDATE
- Test: `tests/compatibility_phase2.rs`

**Step 1: Write the failing test**

Create `tests/compatibility_phase2.rs`:

```rust
use pgqt::transpiler::transpile;

#[test]
fn test_char_trimming() {
    // PostgreSQL trims trailing spaces for CHAR
    let sql = "CREATE TABLE t (c CHAR(1)); INSERT INTO t VALUES ('c     ')";
    let result = transpile(sql);
    assert!(result.is_ok(), "CHAR should accept trimmed value");
}

#[test]
fn test_varchar_trimming() {
    // PostgreSQL also trims trailing spaces for VARCHAR
    let sql = "CREATE TABLE t (v VARCHAR(1)); INSERT INTO t VALUES ('d     ')";
    let result = transpile(sql);
    assert!(result.is_ok(), "VARCHAR should accept trimmed value");
}
```

**Step 2: Find where CHAR/VARCHAR validation happens**

```bash
grep -rn "character" src/validation/ | grep -i "length\|trim\|validate"
grep -rn "char.*varying\|varchar" src/validation/ | head -20
```

**Step 3: Add trimming logic in validation**

In `src/validation/types.rs`, find the CHAR/VARCHAR validation code and add trimming:

```rust
pub fn validate_char_value(value: &str, max_length: usize) -> Result<(), String> {
    // PostgreSQL trims trailing spaces before validation
    let trimmed = value.trim_end_matches(' ');
    if trimmed.len() > max_length {
        Err(format!("value too long for type character({})", max_length))
    } else {
        Ok(())
    }
}

pub fn validate_varchar_value(value: &str, max_length: usize) -> Result<(), String> {
    // PostgreSQL also trims trailing spaces for VARCHAR
    let trimmed = value.trim_end_matches(' ');
    if trimmed.len() > max_length {
        Err(format!("value too long for type character varying({})", max_length))
    } else {
        Ok(())
    }
}
```

**Step 4: Also trim in INSERT/UPDATE transpilation**

In `src/transpiler/dml.rs`, when processing INSERT values for CHAR/VARCHAR columns:

```rust
// When processing string values for CHAR/VARCHAR columns
if is_char_type(column_type) {
    // Trim trailing spaces to match PostgreSQL behavior
    value.trim_end_matches(' ').to_string()
}
```

**Step 5: Run tests**

```bash
cargo test test_char_trimming -- --nocapture
cargo test test_varchar_trimming -- --nocapture
```

**Step 6: Commit**

```bash
git add src/validation/types.rs src/transpiler/dml.rs tests/compatibility_phase2.rs
git commit -m "fix: trim trailing spaces for CHAR/VARCHAR types"
```

---

## Task 2: Add Date/Time Validation

**Problem:** Several date/time validation issues:
1. BC dates not handled: `'2040-04-10 BC'`
2. Timezone validation missing: `'America/Does_not_exist'` should fail
3. Time with timezone info: `'15:36:39 America/New_York'` should fail for TIME type

**Files:**
- Modify: `src/validation/types.rs` - Add timezone validation
- Modify: `src/transpiler/utils.rs` - Add date parsing with BC support
- Test: `tests/compatibility_phase2.rs`

**Step 1: Find existing date/time validation**

```bash
grep -rn "timezone\|Date\|Timestamp" src/validation/ | head -30
```

**Step 2: Add timezone validation**

Create a list of valid timezone names or use a crate like `chrono-tz`:

In `src/validation/types.rs`:

```rust
// Add valid timezone names (subset of IANA timezone database)
const VALID_TIMEZONES: &[&str] = &[
    "UTC", "GMT", "US/Eastern", "US/Central", "US/Mountain", "US/Pacific",
    "America/New_York", "America/Chicago", "America/Denver", "America/Los_Angeles",
    "Europe/London", "Europe/Paris", "Europe/Berlin", "Asia/Tokyo", "Asia/Shanghai",
    // ... add more as needed
];

pub fn validate_timezone(tz: &str) -> Result<(), String> {
    let tz_lower = tz.to_lowercase();
    if VALID_TIMEZONES.iter().any(|&t| t.to_lowercase() == tz_lower) {
        Ok(())
    } else {
        Err(format!("time zone \"{}\" not recognized", tz))
    }
}
```

**Step 3: Add BC date handling**

In date parsing code:

```rust
pub fn parse_date_with_era(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    
    // Check for BC era
    if trimmed.to_lowercase().ends_with(" bc") {
        let date_part = &trimmed[..trimmed.len() - 3].trim();
        // Parse the date and convert to appropriate representation
        // For now, return an error or handle as negative year
        return Err("BC dates not yet supported".to_string());
    }
    
    // Normal AD date
    Ok(trimmed.to_string())
}
```

**Step 4: Add time format validation**

```rust
pub fn validate_time_format(input: &str) -> Result<(), String> {
    // TIME type should not have timezone info
    if input.to_lowercase().contains("america/") ||
       input.to_lowercase().contains("europe/") ||
       input.to_lowercase().contains("asia/") ||
       input.to_lowercase().contains("gmt") ||
       input.to_lowercase().contains("utc") {
        return Err(format!("invalid input syntax for type time: \"{}\"", input));
    }
    Ok(())
}
```

**Step 5: Add tests**

```rust
#[test]
fn test_invalid_timezone() {
    let tz = "America/Does_not_exist";
    assert!(validate_timezone(tz).is_err());
}

#[test]
fn test_valid_timezone() {
    let tz = "America/New_York";
    assert!(validate_timezone(tz).is_ok());
}

#[test]
fn test_time_with_timezone_fails() {
    let time = "15:36:39 America/New_York";
    assert!(validate_time_format(time).is_err());
}
```

**Step 6: Run tests and commit**

```bash
cargo test timezone -- --nocapture
cargo test time_format -- --nocapture
git add src/validation/types.rs tests/compatibility_phase2.rs
git commit -m "feat: add timezone and time format validation"
```

---

## Task 3: Fix Array Type Metadata

**Problem:** Array slice operations return wrong column type metadata. PostgreSQL returns the element type, PGQT returns 'array'.

**Example Failure:**
```sql
select ('{{1,2,3},{4,5,6},{7,8,9}}'::int[])[1:2][2]
-- PostgreSQL: returns 'int4' (element type)
-- PGQT: returns 'array'
```

**Files:**
- Modify: `src/transpiler/expr/mod.rs` - AIndirection handling
- Modify: `src/handler/rewriter.rs` - Type metadata mapping
- Test: `tests/compatibility_phase2.rs`

**Step 1: Understand the current AIndirection handling**

```bash
grep -n "AIndirection" src/transpiler/expr/mod.rs
```

Read the current implementation around line 100-150.

**Step 2: Add type tracking for array access**

The key is to track whether array access returns:
- A single element (should return element type)
- A slice (should return array type)

In `src/transpiler/context.rs`, add a way to track expression types:

```rust
// In TranspileContext, add if not present
pub struct TranspileContext {
    // ... existing fields
    pub expression_types: HashMap<String, String>, // Maps expression to type
}
```

**Step 3: Modify AIndirection handling**

In `src/transpiler/expr/mod.rs`:

```rust
NodeEnum::AIndirection(ref ind) => {
    let arg_sql = ind.arg.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();
    
    // Determine if this is a single element access or a slice
    let mut is_single_element = true;
    let mut json_path = String::new();
    
    for node in &ind.indirection {
        if let Some(ref inner) = node.node {
            match inner {
                NodeEnum::AIndices(ref indices) => {
                    if let Some(ref lidx) = indices.lidx {
                        // This is a slice like [1:2], not a single element
                        is_single_element = false;
                    }
                    // ... rest of handling
                }
                // ... other cases
            }
        }
    }
    
    // Track the result type
    if is_single_element {
        // Result is element type, extract from arg type
        let element_type = extract_element_type(&arg_sql);
        ctx.expression_types.insert(format!("json_extract(...)", element_type);
    }
    
    // ... return the SQL
}
```

**Step 4: Update handler to use tracked types**

In `src/handler/rewriter.rs`, when mapping column types:

```rust
// Check if we have a tracked type for this expression
if let Some(tracked_type) = ctx.expression_types.get(&column_name) {
    return map_postgres_type_to_sqlite(tracked_type);
}
```

**Step 5: Add test**

```rust
#[test]
fn test_array_slice_type_metadata() {
    let sql = "SELECT ('{1,2,3}'::int[])[1]";
    let result = transpile_with_metadata(sql);
    // Should return int4 type, not array type
    assert_eq!(result.column_types[0], "int4");
}
```

**Step 6: Commit**

```bash
git add src/transpiler/expr/mod.rs src/transpiler/context.rs src/handler/rewriter.rs
git add tests/compatibility_phase2.rs
git commit -m "fix: array element access returns element type not array type"
```

---

## Task 4: Implement System Catalog Views

**Problem:** Queries against `pg_class`, `pg_attribute`, `pg_type` fail because these tables don't exist.

**Example Failure:**
```sql
SELECT relname, relkind, relpersistence FROM pg_class WHERE relname ~ '^unlogged\d'
-- Error: no such table: pg_class
```

**Files:**
- Modify: `src/catalog/system_views.rs` - Add pg_class view
- Modify: `src/catalog/init.rs` - Create views during initialization
- Test: `tests/compatibility_phase2.rs`

**Step 1: Read existing system_views.rs**

```bash
cat src/catalog/system_views.rs
```

**Step 2: Add pg_class view**

In `src/catalog/system_views.rs`:

```rust
pub const PG_CLASS_VIEW: &str = r#"
CREATE VIEW IF NOT EXISTS pg_class AS
SELECT 
    name as relname,
    CASE type
        WHEN 'table' THEN 'r'
        WHEN 'index' THEN 'i'
        WHEN 'view' THEN 'v'
        ELSE 'r'
    END as relkind,
    'p' as relpersistence,
    -- Add other columns as needed
    name as relnamespace,
    0 as relowner,
    0 as reltablespace
FROM sqlite_master
WHERE type IN ('table', 'index', 'view')
"#;
```

**Step 3: Add pg_attribute view**

```rust
pub const PG_ATTRIBUTE_VIEW: &str = r#"
CREATE VIEW IF NOT EXISTS pg_attribute AS
SELECT 
    -- Join with PGQT's column metadata table
    c.table_name as attrelid,
    c.column_name as attname,
    c.data_type as atttypid,
    -- Add other columns
    c.ordinal_position as attnum,
    CASE WHEN c.is_nullable = 'YES' THEN true ELSE false END as attnotnull
FROM __pg_catalog__.columns c
"#;
```

**Step 4: Initialize views**

In `src/catalog/init.rs`, add:

```rust
pub fn init_system_views(conn: &Connection) -> Result<(), CatalogError> {
    conn.execute_batch(PG_CLASS_VIEW)?;
    conn.execute_batch(PG_ATTRIBUTE_VIEW)?;
    // Add more views
    Ok(())
}
```

Call this in the catalog initialization.

**Step 5: Add tests**

```rust
#[test]
fn test_pg_class_view() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE test_table (id INT)", []).unwrap();
    
    let mut stmt = conn.prepare("SELECT relname, relkind FROM pg_class WHERE relname = 'test_table'").unwrap();
    let rows: Vec<(String, String)> = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).unwrap().collect();
    
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "test_table");
    assert_eq!(rows[0].1, "r"); // regular table
}
```

**Step 6: Run tests and commit**

```bash
cargo test pg_class -- --nocapture
git add src/catalog/system_views.rs src/catalog/init.rs
git add tests/compatibility_phase2.rs
git commit -m "feat: implement pg_class and pg_attribute system catalog views"
```

---

## Task 5: Run Compatibility Tests and Verify

**Step 1: Run unit tests**

```bash
cargo test
```

**Step 2: Run compatibility tests**

```bash
./run_compatibility_tests.sh
```

**Step 3: Compare results**

Target improvements:
- `char.sql` - Should now pass (trimming fix)
- `varchar.sql` - Should now pass (trimming fix)
- `date.sql` - May still have issues but BC date handling improved
- `arrays.sql` - Type metadata should improve
- `create_table.sql` - pg_class should help

**Step 4: Fix any regressions**

If any tests that were passing now fail, investigate and fix.

**Step 5: Commit final results**

```bash
git add .
git commit -m "test: add Phase 2 compatibility tests for type system fixes"
```

---

## Summary

After completing these 5 tasks:

1. **CHAR/VARCHAR trimming** - PostgreSQL-compatible trailing space handling
2. **Date/time validation** - Timezone validation, BC date handling
3. **Array type metadata** - Element type returned for single element access
4. **System catalogs** - `pg_class` and `pg_attribute` views available
5. All tests pass, compatibility rate improved

**Expected Pass Rate Improvement:** 30% → 40-45%
