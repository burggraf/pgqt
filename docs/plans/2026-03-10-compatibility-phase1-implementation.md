# PGQT Phase 1 Compatibility Fixes - Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Fix the 4 critical compatibility issues causing test failures: bitwise operators, char_length function, column aliases, and float whitespace handling.

**Architecture:** Make surgical fixes to the transpiler's operator detection, function registry, and expression reconstruction. Each fix is isolated and testable.

**Tech Stack:** Rust, pg_query for SQL parsing, SQLite for execution

---

## Task 1: Fix Bitwise Operator Bug

**Problem:** `<<` and `>>` operators with integer expressions are incorrectly detected as geometric operations.

**Files:**
- Modify: `src/transpiler/expr/geo.rs:8-12` (the `looks_like_geo` function)
- Modify: `src/transpiler/expr/operators.rs:103-122` (the `<<` and `>>` match arms)
- Test: Add to `src/transpiler/expr/geo.rs` (unit tests at bottom)
- Test: `tests/compatibility_phase1.rs` (integration test)

**Step 1: Write the failing test**

Add to bottom of `src/transpiler/expr/geo.rs` in a `#[cfg(test)]` module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_geo_with_integer_cast() {
        // These should NOT be detected as geometric
        assert!(!looks_like_geo("(-1::int2"));
        assert!(!looks_like_geo("(-1::int4"));
        assert!(!looks_like_geo("(-1::int8"));
        assert!(!looks_like_geo("cast(1 as integer)"));
        
        // These SHOULD be detected as geometric
        assert!(looks_like_geo("(1,2)"));
        assert!(looks_like_geo("(1,2),(3,4)"));
        assert!(looks_like_geo("<(1,2),3>"));
    }

    #[test]
    fn test_bitwise_operators_not_geo() {
        // Integer bitwise operations
        let lexpr = "-1::int2";
        let rexpr = "15";
        assert!(!is_geo_operation(lexpr, rexpr));
        
        // Geometric operations
        let lexpr = "point '(1,2)'";
        let rexpr = "point '(3,4)'";
        assert!(is_geo_operation(lexpr, rexpr));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test test_looks_like_geo_with_integer_cast -- --nocapture
```

Expected: FAIL - the function currently returns true for `(-1::int2`

**Step 3: Fix the `looks_like_geo` function**

Modify `src/transpiler/expr/geo.rs:8-12`:

```rust
/// Check if a SQL value looks like a geometric type
pub(crate) fn looks_like_geo(val: &str) -> bool {
    let val_lower = val.to_lowercase();
    
    // Exclude type casts - these are never geometric literals
    if val_lower.contains("::") || val_lower.contains("cast(") {
        return false;
    }
    
    // Circle: <(x,y),r>
    if val.starts_with('<') && val.ends_with('>') {
        return true;
    }
    
    // Point: (x,y) - exactly one comma, starts with (, ends with )
    // Box/lseg: (x1,y1),(x2,y2) - exactly 3 commas
    if val.starts_with('(') && val.ends_with(')') && !val.contains('[') {
        let comma_count = val.matches(',').count();
        return comma_count == 1 || comma_count == 3;
    }
    
    false
}
```

**Step 4: Add type-aware handling in operators.rs**

Modify `src/transpiler/expr/operators.rs:103-122` to check for integer types:

```rust
        "<<" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            // Check if this is an integer bitwise operation
            let is_integer_op = lexpr_lower.contains("::int") || 
                               lexpr_lower.contains("::integer") ||
                               rexpr_lower.contains("::int") ||
                               rexpr_lower.contains("::integer") ||
                               lexpr_sql.parse::<i64>().is_ok() ||
                               rexpr_sql.parse::<i64>().is_ok();
            
            if is_integer_op {
                // Bitwise left shift
                format!("{} << {}", lexpr_sql, rexpr_sql)
            } else if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_left(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_left(&lexpr_sql, &rexpr_sql)
            } else {
                // Default to bitwise for unknown types
                format!("{} << {}", lexpr_sql, rexpr_sql)
            }
        },
        
        ">>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            // Check if this is an integer bitwise operation
            let is_integer_op = lexpr_lower.contains("::int") || 
                               lexpr_lower.contains("::integer") ||
                               rexpr_lower.contains("::int") ||
                               rexpr_lower.contains("::integer") ||
                               lexpr_sql.parse::<i64>().is_ok() ||
                               rexpr_sql.parse::<i64>().is_ok();
            
            if is_integer_op {
                // Bitwise right shift
                format!("{} >> {}", lexpr_sql, rexpr_sql)
            } else if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_right(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_right(&lexpr_sql, &rexpr_sql)
            } else {
                // Default to bitwise for unknown types
                format!("{} >> {}", lexpr_sql, rexpr_sql)
            }
        },
```

**Step 5: Run tests to verify they pass**

```bash
cargo test test_looks_like_geo_with_integer_cast -- --nocapture
cargo test test_bitwise_operators_not_geo -- --nocapture
```

Expected: PASS

**Step 6: Test the actual transpilation**

```bash
cargo run -- --transpile "SELECT (-1::int2<<15)::text"
```

Expected output should NOT contain `geo_left` or `json_remove`

**Step 7: Commit**

```bash
git add src/transpiler/expr/geo.rs src/transpiler/expr/operators.rs
git commit -m "fix: bitwise operators << and >> no longer confused with geometric ops

- Tighten looks_like_geo() to exclude type casts
- Add integer type detection for << and >> operators
- Fixes int2, int4, int8 bitwise shift operations"
```

---

## Task 2: Add char_length() Function

**Problem:** The `char_length(text)` function is missing but commonly used.

**Files:**
- Modify: `src/transpiler/func.rs` (find the function mapping/alias section)
- Test: `tests/compatibility_phase1.rs`

**Step 1: Find where function aliases are defined**

```bash
grep -n "length" src/transpiler/func.rs | head -20
```

Look for a function name mapping table or alias handling.

**Step 2: Write the failing test**

Create `tests/compatibility_phase1.rs`:

```rust
use pgqt::transpiler::transpile;

#[test]
fn test_char_length_function() {
    let sql = "SELECT char_length('hello')";
    let result = transpile(sql);
    assert!(result.is_ok());
    let transpiled = result.unwrap().sql;
    // Should use SQLite's length function
    assert!(transpiled.contains("length") || transpiled.contains("char_length"));
}
```

**Step 3: Run test to verify it fails**

```bash
cargo test test_char_length_function -- --nocapture
```

Expected: FAIL - function not found or not recognized

**Step 4: Add the function alias**

In `src/transpiler/func.rs`, find where function names are mapped and add:

```rust
// In the function that reconstructs function calls, add this mapping:
fn get_function_name(name: &str) -> &str {
    match name.to_lowercase().as_str() {
        "char_length" => "length",
        "character_length" => "length",
        // ... existing mappings
        _ => name,
    }
}
```

If there's an existing alias table, add to it:
```rust
const FUNCTION_ALIASES: &[(&str, &str)] = &[
    ("char_length", "length"),
    ("character_length", "length"),
    // ... existing aliases
];
```

**Step 5: Run test to verify it passes**

```bash
cargo test test_char_length_function -- --nocapture
```

Expected: PASS

**Step 6: Test transpilation output**

```bash
cargo run -- --transpile "SELECT char_length('hello'), character_length('world')"
```

Expected: Both should use `length()` in output

**Step 7: Commit**

```bash
git add src/transpiler/func.rs tests/compatibility_phase1.rs
git commit -m "feat: add char_length and character_length as aliases for length

Maps PostgreSQL char_length() and character_length() to SQLite length()"
```

---

## Task 3: Fix Column Alias Preservation

**Problem:** Column aliases like `AS "Simple WHEN"` are not preserved in result metadata.

**Files:**
- Read: `src/transpiler/expr/stmt.rs` (ResTarget handling)
- Read: `src/handler/mod.rs` (result metadata construction)
- Modify: TBD based on investigation
- Test: `tests/compatibility_phase1.rs`

**Step 1: Investigate how ResTarget is handled**

```bash
grep -n "ResTarget" src/transpiler/expr/stmt.rs | head -20
```

Read the `reconstruct_res_target` function:
```rust
pub(crate) fn reconstruct_res_target(res_target: &ResTarget, ctx: &mut TranspileContext) -> String {
    // ... existing code
}
```

**Step 2: Check if alias is being preserved in transpilation**

The transpiled SQL should contain the alias. Test:
```bash
cargo run -- --transpile "SELECT 1 AS \"Simple WHEN\""
```

If the alias is in the transpiled SQL, the issue is in the handler. If not, the issue is in the transpiler.

**Step 3: Write the failing test**

Add to `tests/compatibility_phase1.rs`:

```rust
#[test]
fn test_column_alias_preservation() {
    let sql = r#"SELECT CASE WHEN 1 < 2 THEN 3 END AS "Simple WHEN""#;
    let result = transpile(sql);
    assert!(result.is_ok());
    let transpiled = result.unwrap().sql;
    // The alias should be preserved in the output SQL
    assert!(transpiled.contains("Simple WHEN"), 
        "Alias not preserved in: {}", transpiled);
}
```

**Step 4: Run test to verify it fails (if transpiler issue)**

```bash
cargo test test_column_alias_preservation -- --nocapture
```

**Step 5: Fix the transpiler if needed**

In `src/transpiler/expr/stmt.rs`, the `reconstruct_res_target` function should preserve the name field:

```rust
pub(crate) fn reconstruct_res_target(res_target: &ResTarget, ctx: &mut TranspileContext) -> String {
    let val_sql = res_target
        .val
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    
    // Preserve the alias if present
    if let Some(ref name) = res_target.name {
        format!("{} AS {}", val_sql, name)
    } else {
        val_sql
    }
}
```

**Step 6: If transpiler is correct, check handler**

In `src/handler/mod.rs`, find where result column metadata is constructed. The column names should come from the query's ResTarget names.

Look for code that creates the RowDescription message for PostgreSQL wire protocol.

**Step 7: Run test to verify it passes**

```bash
cargo test test_column_alias_preservation -- --nocapture
```

**Step 8: Commit**

```bash
git add src/transpiler/expr/stmt.rs tests/compatibility_phase1.rs
git commit -m "fix: preserve column aliases in SELECT output

ResTarget.name is now preserved in the transpiled SQL"
```

---

## Task 4: Fix Float Whitespace Handling

**Problem:** PostgreSQL accepts whitespace-padded numeric strings like `'  0.0  '`, PGQT rejects them.

**Files:**
- Modify: `src/transpiler/expr/mod.rs` (TypeCast handling)
- Test: `tests/compatibility_phase1.rs`

**Step 1: Write the failing test**

Add to `tests/compatibility_phase1.rs`:

```rust
#[test]
fn test_float_whitespace_trim() {
    let sql = "SELECT '  0.0  '::real, '  123.456  '::double precision";
    let result = transpile(sql);
    assert!(result.is_ok());
    let transpiled = result.unwrap().sql;
    // Should trim whitespace in the cast
    assert!(transpiled.contains("0.0") || transpiled.contains("'0.0'"));
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test test_float_whitespace_trim -- --nocapture
```

Expected: FAIL or the transpiled SQL contains the whitespace

**Step 3: Find TypeCast handling code**

```bash
grep -n "TypeCast" src/transpiler/expr/mod.rs | head -10
```

Look at the `reconstruct_type_cast` function or the `NodeEnum::TypeCast` handling.

**Step 4: Add whitespace trimming for numeric types**

In `src/transpiler/expr/mod.rs`, modify the TypeCast handling:

```rust
NodeEnum::TypeCast(ref type_cast) => {
    let val = reconstruct_type_cast(type_cast, ctx);
    
    // Trim whitespace for numeric type casts
    let type_name = get_type_name(type_cast).to_lowercase();
    if is_numeric_type(&type_name) && val.starts_with('\'') && val.ends_with('\'') {
        let inner = &val[1..val.len()-1];
        format!("'{}'", inner.trim())
    } else {
        val
    }
}
```

Or modify `reconstruct_type_cast` in `src/transpiler/expr/utils.rs`:

```rust
pub(crate) fn reconstruct_type_cast(type_cast: &TypeCast, ctx: &mut TranspileContext) -> String {
    let arg_sql = type_cast
        .arg
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    
    let type_name = get_type_name(type_cast);
    
    // Trim whitespace from string literals being cast to numeric types
    let trimmed_arg = if is_numeric_type(&type_name) {
        if arg_sql.starts_with('\'') && arg_sql.ends_with('\'') {
            let inner = &arg_sql[1..arg_sql.len()-1];
            format!("'{}'", inner.trim())
        } else {
            arg_sql
        }
    } else {
        arg_sql
    };
    
    format!("CAST({} AS {})", trimmed_arg, type_name)
}
```

Add helper function:
```rust
fn is_numeric_type(type_name: &str) -> bool {
    let lower = type_name.to_lowercase();
    lower.contains("real") || 
    lower.contains("double") || 
    lower.contains("float") ||
    lower.contains("numeric") ||
    lower.contains("decimal") ||
    lower.contains("int")
}
```

**Step 5: Run test to verify it passes**

```bash
cargo test test_float_whitespace_trim -- --nocapture
```

**Step 6: Test transpilation**

```bash
cargo run -- --transpile "SELECT '  0.0  '::real"
```

Expected: Should show `'0.0'` without surrounding whitespace

**Step 7: Commit**

```bash
git add src/transpiler/expr/mod.rs tests/compatibility_phase1.rs
git commit -m "fix: trim whitespace in numeric type casts

PostgreSQL accepts '  0.0  '::real, now PGQT does too"
```

---

## Task 5: Run Full Test Suite

**Step 1: Run unit tests**

```bash
cargo test
```

Expected: All existing tests pass + new tests pass

**Step 2: Run the compatibility tests**

```bash
./run_compatibility_tests.sh
```

**Step 3: Check results**

Compare pass rate to baseline (28%). Target: 35-40% after Phase 1.

**Step 4: Fix any regressions**

If any existing tests fail, fix them before proceeding.

**Step 5: Commit final changes**

```bash
git add .
git commit -m "test: add Phase 1 compatibility tests

- Bitwise operator tests
- char_length function tests  
- Column alias preservation tests
- Float whitespace handling tests"
```

---

## Summary

After completing these 5 tasks:

1. **Bitwise operators** `<<` and `>>` work correctly with integers
2. **`char_length()`** function is available as alias for `length()`
3. **Column aliases** are preserved in result metadata
4. **Float whitespace** is trimmed in numeric casts
5. All tests pass, compatibility rate improved

**Next Steps:**
- Review Phase 2 plan (Type System Improvements)
- Continue with CHAR/VARCHAR trimming, date/time validation, array metadata, system catalogs
