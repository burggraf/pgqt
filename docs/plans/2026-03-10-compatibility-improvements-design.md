# PGQT PostgreSQL Compatibility Improvements - Design Document

**Date:** March 10, 2026  
**Status:** Design Complete - Ready for Implementation  
**Target:** Improve compatibility test pass rate from 28% to 60%+

---

## Executive Summary

This document outlines a 3-phase plan to fix critical PostgreSQL compatibility issues identified in the compatibility test suite. The primary focus is on fixing the most impactful bugs first, followed by type system improvements and polish items.

**Current State:** 14/50 tests passing (28%)  
**Phase 1 Goal:** Fix critical blocking issues  
**Phase 2 Goal:** Fix type system issues  
**Phase 3 Goal:** Polish and error message alignment

---

## Phase 1: Critical Fixes (Highest Priority)

### 1.1 Bitwise Operator Transpilation Bug

**Problem:** The `<<` and `>>` operators are being incorrectly transpiled to geometric functions when used with integer expressions.

**Example Failure:**
```sql
-- Input
SELECT (-1::int2<<15)::text

-- Current (broken) output
select cast(geo_left(json_remove(, '$.' || cast(1 as integer)), 15) as text)

-- Expected output
select cast((-1 << 15) as text)
```

**Root Cause:** The `geo::looks_like_geo()` function is too permissive:
```rust
val.contains('<') ||
(!val.contains('[') && val.contains('(') && val.contains(',') && val.contains(')'))
```

This matches `(-1::int2` because it contains `(` and `,`.

**Solution Approach:**

1. **Fix `geo::looks_like_geo()` to be more restrictive:**
   - Only match actual geometric patterns
   - Point: `(x,y)` - exactly one comma, starts with `(`, ends with `)`
   - Box/lseg: `(x1,y1),(x2,y2)` - exactly 3 commas
   - Circle: `<(x,y),r>` - starts with `<`, ends with `>`
   - Must NOT contain `::` (type cast operator)

2. **Add type-aware handling in operator reconstruction:**
   - Check if either operand contains `::int2`, `::int4`, or `::int8`
   - If so, treat `<<` and `>>` as bitwise operators
   - This provides defense in depth

**Files to Modify:**
- `src/transpiler/expr/geo.rs` - Tighten `looks_like_geo()` and `is_geo_operation()`
- `src/transpiler/expr/operators.rs` - Add type-aware handling for `<<` and `>>`

**Testing:**
- Add unit tests for `looks_like_geo()` with various inputs
- Test bitwise operations: `SELECT (1::int2 << 2), (8::int4 >> 1)`
- Test geometric operations still work: `SELECT point '(1,2)' << point '(3,4)'`

---

### 1.2 Missing `char_length()` Function

**Problem:** The `char_length(text)` function is not available, causing failures in basic SELECT statements.

**Example Failure:**
```sql
SELECT id, a, char_length(b) FROM delete_test
-- Error: no such function: char_length
```

**Solution:** Add `char_length` as an alias for `length` in the function registry.

**Implementation:**
- In `src/transpiler/func.rs`, add an entry to the function alias map
- Or handle in the function reconstruction logic to map `char_length` -> `length`

**Code Location:**
```rust
// In src/transpiler/func.rs
const FUNCTION_ALIASES: &[(&str, &str)] = &[
    ("char_length", "length"),
    // ... existing aliases
];
```

**Testing:**
- `SELECT char_length('hello')` should return 5
- `SELECT char_length('')` should return 0

---

### 1.3 Column Alias Preservation

**Problem:** Column aliases specified with `AS` are not being preserved in the result set metadata. PostgreSQL returns the alias name, but PGQT returns `?column?`.

**Example Failure:**
```sql
SELECT CASE WHEN 1 < 2 THEN 3 END AS "Simple WHEN"
-- PostgreSQL column name: "Simple WHEN"
-- PGQT column name: "?column?"
```

**Root Cause:** The transpiler is likely not preserving the `ResTarget.name` field when reconstructing SELECT expressions, or the handler is not reading column names from the result metadata correctly.

**Investigation Steps:**
1. Check `src/transpiler/expr/stmt.rs` - `reconstruct_res_target()` function
2. Verify that `ResTarget.name` is being used to set the output column name
3. Check `src/handler/mod.rs` - how result column metadata is constructed

**Solution:**
- Ensure `ResTarget.name` is preserved during transpilation
- If SQLite doesn't support column aliases in the way needed, store the alias mapping and apply it when returning results

**Files to Modify:**
- `src/transpiler/expr/stmt.rs` - Preserve alias names
- `src/handler/mod.rs` - Apply alias names to result metadata

**Testing:**
```sql
SELECT 1 AS "One", 'test' AS "String Column", 3.14 AS pi
-- All three column names should match the aliases
```

---

### 1.4 Float Whitespace Handling

**Problem:** PostgreSQL accepts whitespace-padded numeric strings in INSERT statements, but PGQT rejects them.

**Example Failure:**
```sql
INSERT INTO FLOAT4_TBL(f1) VALUES ('    0.0')
-- Error: invalid input syntax for type real: "    0.0"
```

**Solution:** Trim whitespace from numeric string inputs before parsing in type cast handling.

**Implementation:**
- In `src/transpiler/expr/mod.rs` or type cast handling code
- When casting to REAL/DOUBLE PRECISION, trim whitespace from string literals

**Code Location:**
```rust
// In type cast reconstruction
NodeEnum::TypeCast(ref type_cast) => {
    let val = reconstruct_type_cast(type_cast, ctx);
    // Trim whitespace for numeric casts
    if is_numeric_cast(type_cast) {
        val.trim().to_string()
    } else {
        val
    }
}
```

**Testing:**
- `INSERT INTO t VALUES ('  0.0  '::real)`
- `SELECT '  123.456  '::double precision`

---

## Phase 2: Type System Improvements

### 2.1 CHAR/VARCHAR Trimming Behavior

**Problem:** PostgreSQL automatically trims trailing spaces from CHAR/VARCHAR inputs. PGQT treats `'c     '` as too long for `character(1)`.

**Example Failure:**
```sql
INSERT INTO CHAR_TBL (f1) VALUES ('c     ')
-- Error: value too long for type character(1)
-- PostgreSQL trims to 'c' and accepts it
```

**Solution:** Apply PostgreSQL-compatible trimming rules:
- For `CHAR(n)`/`CHARACTER(n)`: Trim trailing spaces before length check
- For `VARCHAR(n)`: Also trim trailing spaces (PostgreSQL behavior)

**Implementation:**
- In INSERT/UPDATE value handling
- When the target column is CHAR/VARCHAR type, apply `rtrim()` to string inputs
- Or store the trimmed value

**Files to Modify:**
- `src/transpiler/dml.rs` - INSERT/UPDATE value handling
- May need catalog lookup to know column types

**Testing:**
```sql
CREATE TABLE t (c char(1), v varchar(1));
INSERT INTO t VALUES ('c     ', 'd     ');
-- Both should succeed and store 'c' and 'd'
```

---

### 2.2 Date/Time Validation

**Problem:** Several date/time validation issues:
1. BC dates not supported: `'2040-04-10 BC'`
2. Timezone validation missing: `'America/Does_not_exist'` should fail
3. Time with timezone info: `'15:36:39 America/New_York'` should fail

**Solution:**

1. **BC Date Support:**
   - Parse BC suffix and convert to appropriate SQLite representation
   - Or return proper error if not supported

2. **Timezone Validation:**
   - Add a list of valid timezone names
   - Validate timezone names in timestamp parsing

3. **Time Format Validation:**
   - Reject time values with timezone information when parsing TIME (not TIMESTAMPTZ)

**Implementation:**
- Create a timezone validation module
- Update date/time parsing in type handling code

**Files to Modify:**
- New: `src/datetime.rs` or extend existing date handling
- `src/transpiler/utils.rs` - Type parsing

**Testing:**
```sql
-- Should fail with invalid timezone
INSERT INTO t VALUES ('19970710 173201 America/Does_not_exist');

-- Should fail (time with timezone)
INSERT INTO TIME_TBL VALUES ('15:36:39 America/New_York');
```

---

### 2.3 Array Type Metadata

**Problem:** Array slice operations return wrong column type metadata.

**Example Failure:**
```sql
select ('{{1,2,3},{4,5,6},{7,8,9}}'::int[])[1:2][2]
-- PostgreSQL: returns 'int4' (element type)
-- PGQT: returns 'array'
```

**Root Cause:** The type detection for array slicing returns the array type instead of the element type.

**Solution:**
- When an array slice operation returns a single element (not a slice), use the element type
- When it returns a slice (range), use the array type

**Implementation:**
- In `src/transpiler/expr/mod.rs` - `AIndirection` handling
- Detect if the result is a single element vs a slice
- Return appropriate type metadata

**Files to Modify:**
- `src/transpiler/expr/mod.rs`
- May need to track type information through the transpilation

**Testing:**
```sql
-- Single element access - should return int4
SELECT ('{1,2,3}'::int[])[1];

-- Slice access - should return int[]
SELECT ('{1,2,3}'::int[])[1:2];
```

---

### 2.4 System Catalog Tables

**Problem:** Queries against `pg_class` and other system catalogs fail.

**Example Failure:**
```sql
SELECT relname, relkind, relpersistence FROM pg_class WHERE relname ~ '^unlogged\d'
-- Error: no such table: pg_class
```

**Solution:** Implement shadow catalog tables/views:

1. **Create views in SQLite** that expose the catalog information:
   - `pg_class` - from `sqlite_master` + PGQT's catalog tables
   - `pg_attribute` - from PGQT's column metadata
   - `pg_type` - from PGQT's type registry

2. **Map the views** to PostgreSQL-compatible schema

**Implementation:**
- In `src/catalog/system_views.rs` or new module
- Create views during catalog initialization
- Map SQLite schema to PostgreSQL catalog structure

**Files to Modify:**
- `src/catalog/system_views.rs` - Add pg_class view
- `src/catalog/init.rs` - Initialize system views

**Testing:**
```sql
SELECT * FROM pg_class WHERE relname = 'mytable';
SELECT * FROM pg_attribute WHERE attrelid = 'mytable'::regclass;
```

---

## Phase 3: Polish and Feature Completeness

### 3.1 UPDATE Row Constructors

**Problem:** PostgreSQL row constructor syntax in UPDATE not supported.

**Example Failure:**
```sql
UPDATE update_test SET (c,b,a) = ('bugle', b+11, DEFAULT) WHERE c = 'foo'
-- Transpiled to (broken): update update_test set c = , b = , a =  where c = 'foo'
```

**Solution:** Add transpilation support for row constructor syntax.

**Implementation:**
- Detect multi-column SET syntax
- Expand to individual column assignments
- Handle DEFAULT keyword

**Files to Modify:**
- `src/transpiler/dml.rs` - UPDATE statement handling

---

### 3.2 `generate_series()` Function

**Problem:** The `generate_series()` set-returning function is not available.

**Example Failure:**
```sql
SELECT corr(g, 'NaN') FROM generate_series(1, 30) g
-- Error: no such table: generate_series
```

**Solution:** Implement `generate_series()` as a SQLite recursive CTE.

**Implementation:**
- Detect `generate_series()` calls
- Transpile to recursive CTE: `WITH RECURSIVE _series(n) AS (SELECT start UNION ALL SELECT n + 1 FROM _series WHERE n < stop)`

**Files to Modify:**
- `src/transpiler/func.rs` - Function handling
- Or create special handling in `src/transpiler/dml.rs`

---

### 3.3 Error Message Alignment

**Problem:** Many tests expect specific PostgreSQL error messages that PGQT doesn't match.

**Examples:**
- JSON parsing errors
- UUID validation errors
- Type mismatch errors
- PL/pgSQL syntax errors

**Solution:** Review and align error codes and messages with PostgreSQL where practical.

**Priority:** Low - functionality is often correct, just error messages differ

---

### 3.4 Remaining Items

1. **UNION Column Naming** - Match PostgreSQL's `?column?:1`, `?column?:2` convention
2. **EXPLAIN Support** - Extend to work with DISTINCT queries
3. **SHOW Parameters** - Add more configuration parameters to match PostgreSQL's 378
4. **Window Function Frames** - Support interval-based ranges
5. **Empty Table Definitions** - Support `CREATE TEMP TABLE onerow()`
6. **Numeric Precision** - Review numeric calculation precision

---

## Implementation Order

### Week 1: Phase 1 - Critical Fixes
1. Bitwise operator bug fix
2. Add `char_length()` function
3. Float whitespace handling
4. Column alias preservation (investigate)

### Week 2: Phase 2 - Type System
1. CHAR/VARCHAR trimming
2. Date/time validation
3. Array type metadata
4. System catalog tables (pg_class)

### Week 3: Phase 3 - Features & Polish
1. UPDATE row constructors
2. `generate_series()` function
3. Error message alignment (selected items)
4. Remaining polish items as time permits

---

## Testing Strategy

### Unit Tests
- Add tests for each fixed function in the relevant `#[cfg(test)]` module
- Test edge cases and error conditions

### Integration Tests
- Create `tests/compatibility_phase1.rs` for Phase 1 fixes
- Create `tests/compatibility_phase2.rs` for Phase 2 fixes
- Each test should verify the specific failing query from the compatibility suite

### E2E Tests
- Run the full `run_compatibility_tests.sh` after each phase
- Track pass rate improvement

### Regression Tests
- Ensure existing tests still pass: `cargo test`
- Run `./run_tests.sh` before committing

---

## Success Criteria

| Phase | Target | Metric |
|-------|--------|--------|
| Phase 1 | 40% pass rate | Critical blocking issues resolved |
| Phase 2 | 55% pass rate | Type system issues resolved |
| Phase 3 | 65% pass rate | Feature completeness improved |

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Bitwise operator fix breaks geometric operations | Comprehensive tests for both cases |
| Column alias fix requires handler changes | Isolate changes, test thoroughly |
| System catalog views impact performance | Use SQLite views, optimize queries |
| Date/time validation adds complexity | Start with basic validation, expand iteratively |

---

## Appendix: Quick Reference

### Files by Fix

| Fix | Primary Files |
|-----|---------------|
| Bitwise operators | `src/transpiler/expr/geo.rs`, `src/transpiler/expr/operators.rs` |
| char_length | `src/transpiler/func.rs` |
| Column aliases | `src/transpiler/expr/stmt.rs`, `src/handler/mod.rs` |
| Float whitespace | `src/transpiler/expr/mod.rs` |
| CHAR/VARCHAR trim | `src/transpiler/dml.rs` |
| Date/time validation | `src/transpiler/utils.rs`, new `src/datetime.rs` |
| Array metadata | `src/transpiler/expr/mod.rs` |
| System catalogs | `src/catalog/system_views.rs` |
| UPDATE row constructors | `src/transpiler/dml.rs` |
| generate_series | `src/transpiler/func.rs` |

### Test Commands

```bash
# Run all tests
cargo test

# Run compatibility tests
./run_compatibility_tests.sh

# Run specific test
cargo test test_name

# Check transpilation output
cargo run -- --transpile "SELECT ..."
```
