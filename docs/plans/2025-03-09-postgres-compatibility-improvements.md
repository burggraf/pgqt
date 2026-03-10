# PostgreSQL Compatibility Improvements Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Increase PostgreSQL compatibility from 28% to 80%+ by fixing the top failure categories identified in the compatibility suite.

**Architecture:** Add input validation layer, expand function registry, fix column alias preservation, and expose system catalog views. Each fix will be validated against the postgres-compatibility-suite.

**Tech Stack:** Rust, pg_query for parsing, SQLite for storage, psycopg2 for testing

---

## Overview

The postgres-compatibility-suite revealed 36 failures across 50 tests (28% pass rate). This plan addresses the top issues systematically.

### Failure Categories (Prioritized)

1. **Type Validation Not Enforced** (Critical) - PG rejects invalid inputs, PGQT accepts them
2. **Missing Built-in Functions** (Critical) - corr(), to_char(), generate_series() missing
3. **Column Alias Preservation** (High) - Set operations generate non-standard column names
4. **System Catalog Access** (High) - pg_class, pg_tables not accessible
5. **UPDATE FROM Subqueries** (Medium) - Column reference aliasing issues
6. **CREATE OR REPLACE VIEW** (Medium) - View replacement not working
7. **SHOW Command** (Low) - Incomplete parameter list

---

## Phase 1: Type Validation Layer

### Task 1: Create Validation Framework Infrastructure

**Files:**
- Create: `src/validation/mod.rs`
- Create: `src/validation/types.rs`
- Modify: `src/lib.rs` - add validation module

**Step 1: Create validation module structure**

```rust
// src/validation/mod.rs
pub mod types;

use crate::catalog::ColumnMetadata;
use crate::transpiler::context::TranspileContext;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: String,  // SQLSTATE code
    pub message: String,
    pub position: Option<usize>,
}

pub trait Validator {
    fn validate(&self, ctx: &TranspileContext) -> Result<(), ValidationError>;
}

pub fn validate_varchar(value: &str, max_length: usize) -> Result<(), ValidationError> {
    if value.len() > max_length {
        return Err(ValidationError {
            code: "22001".to_string(),
            message: format!("value too long for type character varying({})", max_length),
            position: None,
        });
    }
    Ok(())
}

pub fn validate_char(value: &str, length: usize) -> Result<(), ValidationError> {
    if value.len() > length {
        return Err(ValidationError {
            code: "22001".to_string(),
            message: format!("value too long for type character({})", length),
            position: None,
        });
    }
    Ok(())
}
```

**Step 2: Add to lib.rs**

```rust
// src/lib.rs
pub mod validation;
```

**Step 3: Commit**

```bash
git add src/validation/mod.rs src/validation/types.rs src/lib.rs
git commit -m "feat(validation): add validation framework infrastructure"
```

---

### Task 2: Implement VARCHAR/CHAR Length Validation

**Files:**
- Modify: `src/transpiler/expr.rs` - add validation in INSERT/UPDATE
- Modify: `src/catalog/table.rs` - expose column metadata with lengths
- Test: `tests/validation_tests.rs`

**Step 1: Write failing test**

```rust
// tests/validation_tests.rs
#[test]
fn test_varchar_length_validation() {
    let sql = "CREATE TABLE test (name VARCHAR(1)); INSERT INTO test VALUES ('cd');";
    let result = pgqt::transpile_with_metadata(sql);
    assert!(!result.errors.is_empty(), "Should reject value too long for VARCHAR(1)");
    assert!(result.errors[0].code == "22001");
}

#[test]
fn test_char_length_validation() {
    let sql = "CREATE TABLE test (name CHAR(1)); INSERT INTO test VALUES ('ab');";
    let result = pgqt::transpile_with_metadata(sql);
    assert!(!result.errors.is_empty(), "Should reject value too long for CHAR(1)");
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test test_varchar_length_validation --test validation_tests -- --nocapture
# Expected: FAIL - no validation currently
```

**Step 3: Implement validation in transpiler**

Modify `src/transpiler/expr.rs` to validate string literals against column types:

```rust
// In expr.rs, when processing INSERT values
fn validate_insert_values(
    columns: &[ColumnRef],
    values: &[Node],
    table_name: &str,
    ctx: &TranspileContext,
) -> Result<(), ValidationError> {
    // Get column metadata from catalog
    // For each string value, check if column is VARCHAR/CHAR
    // Validate length against column definition
    Ok(())
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test test_varchar_length_validation --test validation_tests -- --nocapture
# Expected: PASS
```

**Step 5: Commit**

```bash
git add tests/validation_tests.rs src/transpiler/expr.rs src/catalog/table.rs
git commit -m "feat(validation): add VARCHAR/CHAR length validation"
```

---

### Task 3: Implement UUID Format Validation

**Files:**
- Modify: `src/validation/types.rs` - add UUID validation
- Modify: `src/transpiler/expr.rs` - integrate UUID validation

**Step 1: Write failing test**

```rust
#[test]
fn test_uuid_validation() {
    let sql = "CREATE TABLE test (id UUID); INSERT INTO test VALUES ('11111111-1111-1111-1111-111111111111F');";
    let result = pgqt::transpile_with_metadata(sql);
    assert!(!result.errors.is_empty(), "Should reject invalid UUID format");
    assert!(result.errors[0].code == "22P02");
}
```

**Step 2: Implement UUID validation**

```rust
// src/validation/types.rs
use regex::Regex;

pub fn validate_uuid(value: &str) -> Result<(), ValidationError> {
    let uuid_regex = Regex::new(
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    ).unwrap();
    
    if !uuid_regex.is_match(value) {
        return Err(ValidationError {
            code: "22P02".to_string(),
            message: format!("invalid input syntax for type uuid: \"{}\"", value),
            position: None,
        });
    }
    Ok(())
}
```

**Step 3: Commit**

```bash
git add src/validation/types.rs tests/validation_tests.rs
git commit -m "feat(validation): add UUID format validation"
```

---

### Task 4: Implement Timezone Validation

**Files:**
- Modify: `src/validation/types.rs` - add timezone validation
- Modify: `src/transpiler/expr.rs` - integrate timezone validation

**Step 1: Write failing test**

```rust
#[test]
fn test_timezone_validation() {
    let sql = "SELECT '19970710 173201 America/Does_not_exist'::timestamptz;";
    let result = pgqt::transpile_with_metadata(sql);
    assert!(!result.errors.is_empty(), "Should reject invalid timezone");
    assert!(result.errors[0].code == "22023");
}
```

**Step 2: Implement timezone validation**

```rust
// src/validation/types.rs
pub const VALID_TIMEZONES: &[&str] = &[
    "UTC", "GMT", "America/New_York", "America/Los_Angeles",
    "America/Chicago", "America/Denver", "Europe/London", "Europe/Paris",
    "Asia/Tokyo", "Asia/Shanghai", "Australia/Sydney", // ... etc
];

pub fn validate_timezone(tz: &str) -> Result<(), ValidationError> {
    if !VALID_TIMEZONES.contains(&tz) {
        return Err(ValidationError {
            code: "22023".to_string(),
            message: format!("time zone \"{}\" not recognized", tz),
            position: None,
        });
    }
    Ok(())
}
```

**Step 3: Commit**

```bash
git add src/validation/types.rs tests/validation_tests.rs
git commit -m "feat(validation): add timezone validation"
```

---

### Task 5: Implement JSON/JSONB Validation

**Files:**
- Modify: `src/validation/types.rs` - add JSON validation

**Step 1: Write failing test**

```rust
#[test]
fn test_json_validation() {
    let sql = "SELECT '{invalid json}'::json;";
    let result = pgqt::transpile_with_metadata(sql);
    assert!(!result.errors.is_empty(), "Should reject invalid JSON");
}
```

**Step 2: Implement JSON validation**

```rust
pub fn validate_json(value: &str) -> Result<(), ValidationError> {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(_) => Ok(()),
        Err(e) => Err(ValidationError {
            code: "22P02".to_string(),
            message: format!("invalid input syntax for type json: \"{}\"", value),
            position: None,
        }),
    }
}
```

**Step 3: Commit**

```bash
git add src/validation/types.rs tests/validation_tests.rs
git commit -m "feat(validation): add JSON validation"
```

---

## Phase 2: Missing Built-in Functions

### Task 6: Add generate_series() Function

**Files:**
- Modify: `src/transpiler/func.rs` - add generate_series handling
- Modify: `src/transpiler/dml.rs` - handle set-returning functions in FROM

**Step 1: Write failing test**

```rust
#[test]
fn test_generate_series() {
    let sql = "SELECT * FROM generate_series(1, 10) AS i;";
    let result = pgqt::transpile(sql);
    // Should transpile to recursive CTE
    assert!(result.contains("WITH RECURSIVE"));
}
```

**Step 2: Implement generate_series transpilation**

```rust
// In func.rs or dml.rs
fn transpile_generate_series(args: &[Node]) -> String {
    // Transpile to: WITH RECURSIVE _series(n) AS (
    //   SELECT start UNION ALL SELECT n + step FROM _series WHERE n < stop
    // ) SELECT n FROM _series
    format!(
        "WITH RECURSIVE _series(n) AS (SELECT {} UNION ALL SELECT n + {} FROM _series WHERE n < {}) SELECT n FROM _series",
        start, step, stop
    )
}
```

**Step 3: Commit**

```bash
git add src/transpiler/func.rs src/transpiler/dml.rs tests/function_tests.rs
git commit -m "feat(functions): add generate_series() support"
```

---

### Task 7: Add to_char() Function

**Files:**
- Modify: `src/transpiler/func.rs` - add to_char handling

**Step 1: Write failing test**

```rust
#[test]
fn test_to_char() {
    let sql = "SELECT to_char(1234.56, 'FM9999.99');";
    let result = pgqt::transpile(sql);
    // Should transpile to SQLite printf equivalent
    assert!(result.contains("printf") || result.contains("format"));
}
```

**Step 2: Implement to_char transpilation**

```rust
fn transpile_to_char(args: &[Node]) -> String {
    // Map PostgreSQL format patterns to SQLite printf
    // 'FM9999.99' -> '%.2f'
    format!("printf('{}', {})", format_pattern, value)
}
```

**Step 3: Commit**

```bash
git add src/transpiler/func.rs tests/function_tests.rs
git commit -m "feat(functions): add to_char() support"
```

---

### Task 8: Add corr() and Other Statistical Functions

**Files:**
- Modify: `src/transpiler/func.rs` - add statistical aggregate functions

**Step 1: Write failing test**

```rust
#[test]
fn test_corr_function() {
    let sql = "SELECT corr(x, y) FROM data;";
    let result = pgqt::transpile(sql);
    // Should use custom implementation or SQLite extension
    assert!(result.contains("corr"));
}
```

**Step 2: Implement corr() as custom aggregate**

```rust
// Register corr as a function that needs custom implementation
// May need to add to src/functions.rs for runtime support
```

**Step 3: Commit**

```bash
git add src/transpiler/func.rs src/functions.rs tests/function_tests.rs
git commit -m "feat(functions): add corr() statistical function"
```

---

## Phase 3: Column Alias Preservation

### Task 9: Fix Set Operation Column Naming

**Files:**
- Modify: `src/transpiler/dml.rs` - fix column name generation for UNION/INTERSECT

**Step 1: Write failing test**

```rust
#[test]
fn test_union_column_naming() {
    let sql = "(SELECT 1,2,3 UNION SELECT 4,5,6) INTERSECT SELECT 4,5,6;";
    let result = pgqt::transpile(sql);
    // Should have consistent column names, not ?column?:1
    assert!(!result.contains("?column?:1"));
}
```

**Step 2: Fix column naming in set operations**

```rust
// In dml.rs, when reconstructing set operations
// Ensure first query's column names are preserved
// Don't append :1, :2 suffixes
```

**Step 3: Commit**

```bash
git add src/transpiler/dml.rs tests/column_alias_tests.rs
git commit -m "fix(columns): preserve column names in set operations"
```

---

## Phase 4: System Catalog Access

### Task 10: Expose pg_class and pg_tables Views

**Files:**
- Modify: `src/catalog/system_views.rs` - add missing system views

**Step 1: Write failing test**

```rust
#[test]
fn test_pg_class_accessible() {
    let sql = "SELECT * FROM pg_class LIMIT 1;";
    let result = pgqt::transpile(sql);
    // Should not error
    assert!(!result.contains("error"));
}
```

**Step 2: Add system views**

```rust
// In system_views.rs
pub fn init_system_views(conn: &Connection) -> Result<()> {
    // Add pg_class view
    conn.execute(
        "CREATE VIEW pg_class AS 
         SELECT 
           oid,
           relname,
           relnamespace,
           reltype,
           relowner,
           relam,
           relfilenode,
           reltablespace,
           relpages,
           reltuples,
           relallvisible,
           reltoastrelid,
           relhasindex,
           relisshared,
           relpersistence,
           relkind,
           relnatts,
           relchecks,
           relhasrules,
           relhastriggers,
           relhassubclass,
           relrowsecurity,
           relforcerowsecurity,
           relispopulated,
           relreplident,
           relispartition,
           relrewrite,
           relfrozenxid,
           relminmxid,
           relacl,
           reloptions,
           relpartbound
         FROM __pg_catalog__.pg_class",
        [],
    )?;
    
    // Add pg_tables view
    conn.execute(
        "CREATE VIEW pg_tables AS
         SELECT 
           schemaname,
           tablename,
           tableowner,
           tablespace,
           hasindexes,
           hasrules,
           hastriggers,
           rowsecurity
         FROM __pg_catalog__.pg_tables",
        [],
    )?;
    Ok(())
}
```

**Step 3: Commit**

```bash
git add src/catalog/system_views.rs tests/catalog_tests.rs
git commit -m "feat(catalog): expose pg_class and pg_tables system views"
```

---

## Phase 5: UPDATE FROM Subqueries

### Task 11: Fix UPDATE FROM Column Aliasing

**Files:**
- Modify: `src/transpiler/dml.rs` - fix UPDATE FROM subquery handling

**Step 1: Write failing test**

```rust
#[test]
fn test_update_from_subquery() {
    let sql = "UPDATE update_test SET a=v.i FROM (VALUES(100, 20)) AS v(i, j) WHERE update_test.b = v.j;";
    let result = pgqt::transpile(sql);
    // Should correctly reference v.i and v.j
    assert!(result.contains("v.i") || result.contains("column1"));
}
```

**Step 2: Fix UPDATE FROM handling**

```rust
// In dml.rs, reconstruct_update_stmt
// Properly handle FROM clause with subquery aliases
// Map v.i to the correct column reference in the subquery
```

**Step 3: Commit**

```bash
git add src/transpiler/dml.rs tests/update_tests.rs
git commit -m "fix(update): handle FROM subquery column aliases correctly"
```

---

## Phase 6: CREATE OR REPLACE VIEW

### Task 12: Fix CREATE OR REPLACE VIEW

**Files:**
- Modify: `src/transpiler/ddl.rs` - handle OR REPLACE

**Step 1: Write failing test**

```rust
#[test]
fn test_create_or_replace_view() {
    let sql = "CREATE OR REPLACE VIEW v_test AS SELECT 1; CREATE OR REPLACE VIEW v_test AS SELECT 2;";
    let result = pgqt::transpile(sql);
    // Should not error about view already existing
    assert!(!result.contains("already exists"));
}
```

**Step 2: Fix CREATE OR REPLACE VIEW**

```rust
// In ddl.rs, when handling CreateStmt for views
// If OR REPLACE is specified, drop existing view first
if replace {
    format!("DROP VIEW IF EXISTS {}; CREATE VIEW {} AS {}", view_name, view_name, query)
} else {
    format!("CREATE VIEW {} AS {}", view_name, query)
}
```

**Step 3: Commit**

```bash
git add src/transpiler/ddl.rs tests/view_tests.rs
git commit -m "fix(views): implement CREATE OR REPLACE VIEW correctly"
```

---

## Phase 7: SHOW Command Expansion

### Task 13: Expand SHOW Command Parameters

**Files:**
- Modify: `src/handler/mod.rs` or relevant SHOW handler - add more parameters

**Step 1: Write failing test**

```rust
#[test]
fn test_show_all_parameters() {
    // PG returns ~378 parameters, PGQT returns 27
    // This is an integration test
}
```

**Step 2: Add more SHOW parameters**

```rust
// Add common PostgreSQL GUC parameters to SHOW response
let additional_params = vec![
    ("allow_alter_system", "off"),
    ("archive_command", ""),
    ("archive_mode", "off"),
    // ... many more
];
```

**Step 3: Commit**

```bash
git add src/handler/mod.rs tests/show_tests.rs
git commit -m "feat(show): expand SHOW command parameter list"
```

---

## Phase 8: Integration & Verification

### Task 14: Run Full Compatibility Suite

**Step 1: Run the suite**

```bash
cd postgres-compatibility-suite
source venv/bin/activate
pytest runner.py -v --tb=short 2>&1 | tee compatibility_results.txt
```

**Step 2: Compare results**

- Baseline: 14 passed, 36 failed (28%)
- Target: 40+ passed, 10- failed (80%+)

**Step 3: Document remaining issues**

Create `docs/compatibility/remaining-issues.md` with:
- Issues that couldn't be fixed
- Why they're hard (e.g., recursive CTEs)
- Workarounds

---

## Summary

This plan addresses the 36 failures through:

1. **Type Validation** (Tasks 1-5) - Enforce PostgreSQL-compatible input validation
2. **Missing Functions** (Tasks 6-8) - Add generate_series, to_char, corr
3. **Column Aliases** (Task 9) - Fix set operation column naming
4. **System Catalog** (Task 10) - Expose pg_class, pg_tables
5. **UPDATE FROM** (Task 11) - Fix subquery aliasing
6. **Views** (Task 12) - Fix CREATE OR REPLACE
7. **SHOW Command** (Task 13) - Expand parameter list
8. **Verification** (Task 14) - Validate improvements

**Estimated Timeline:** 2-3 weeks for full implementation

**Key Files to Modify:**
- `src/validation/` (new module)
- `src/transpiler/expr.rs`, `dml.rs`, `ddl.rs`, `func.rs`
- `src/catalog/system_views.rs`
- `src/handler/mod.rs`

**Testing Strategy:**
- Unit tests for each validation function
- Integration tests for transpilation
- Full compatibility suite run for verification
