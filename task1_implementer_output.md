# Task 1: Fix Bitwise Operator Bug - Implementation Report

## Summary
Successfully fixed the bitwise operator bug where `<<` and `>>` operators were incorrectly being treated as geometric operators when used with integer expressions.

## Changes Made

### 1. `src/transpiler/expr/geo.rs`

Modified the `looks_like_geo()` function to:
- Exclude integer type casts (`::int`, `::integer`, `::int2`, `::int4`, `::int8`, `::smallint`, `::bigint`)
- Exclude `cast()` expressions to integer types
- Handle `cast('...' as text)` expressions by extracting and checking the inner string content for geometric patterns
- Properly detect geometric patterns:
  - Circle: `<(x,y),r>` format
  - Point: `(x,y)` - exactly 1 comma
  - Box/lseg: `(x1,y1),(x2,y2)` - exactly 3 commas

Added unit tests:
```rust
#[test]
fn test_looks_like_geo_with_integer_cast() {
    assert!(!looks_like_geo("(-1::int2"));
    assert!(!looks_like_geo("(-1::int4"));
    assert!(looks_like_geo("(1,2)"));
    assert!(looks_like_geo("<(1,2),3>"));
}
```

### 2. `src/transpiler/expr/operators.rs`

Added `is_integer_expression()` helper function to detect integer expressions:
- Checks for integer type casts (`::int`, `::integer`, etc.)
- Checks for `cast(... as int/integer/smallint/bigint)` patterns
- Checks for simple integer literals

Modified the `<<` and `>>` operator handlers to:
- First check if either operand is an integer expression using `is_integer_expression()`
- If integer operation: use bitwise shift (`{} << {}` or `{} >> {}`)
- Otherwise: check for geometric operations, then range operations, then default to bitwise shift

### 3. `tests/transpiler_tests.rs`

Added integration test:
```rust
#[test]
fn test_bitwise_shift_not_geo() {
    let sql = "SELECT (1::int2 << 15)::text";
    let result = transpile(sql);
    // Verify geo_left is NOT in output
    assert!(!result.contains("geo_left"));
    // Verify << operator is preserved
    assert!(result.contains("<<"));
}
```

## Test Results

All tests pass:
- Unit tests: 280 passed
- Integration tests: All passed
- Geo tests: 23 passed (geometric operations still work correctly)
- New bitwise shift test: PASSED

## Verification

Input: `SELECT (1::int2 << 15)::text`

Before fix:
```sql
select cast(geo_left(json_remove(, '$.' || cast(1 as integer)), 15) as text)
```

After fix:
```sql
select cast(cast(1 as integer) << 15 as text) AS "text"
```

The fix correctly:
1. Preserves the bitwise shift operator `<<`
2. Does not generate `geo_left` or `json_remove` for integer operations
3. Maintains compatibility with existing geometric operations
4. Maintains compatibility with existing range operations
