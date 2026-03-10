# PGQT Phase 3 Compatibility Fixes - Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Implement polish items: UPDATE row constructors, generate_series function, error message alignment, and EXPLAIN improvements.

**Architecture:** Add transpilation support for row constructors, implement generate_series as CTE, align error codes with PostgreSQL.

**Tech Stack:** Rust, SQL transpilation, CTEs for set-returning functions

---

## Task 1: UPDATE Row Constructors

**Problem:** PostgreSQL row constructor syntax in UPDATE not supported:
```sql
UPDATE update_test SET (c,b,a) = ('bugle', b+11, DEFAULT) WHERE c = 'foo'
-- Currently transpiled to: update update_test set c = , b = , a =  where c = 'foo'
```

**Files:**
- Modify: `src/transpiler/dml.rs` - UPDATE statement handling

**Implementation:**

1. Find `reconstruct_update_stmt` function
2. Detect multi-column SET syntax (when target is a list)
3. Expand to individual column assignments
4. Handle DEFAULT keyword

**Test:**
```rust
#[test]
fn test_update_row_constructor() {
    let sql = "UPDATE t SET (a,b) = (1, 2)";
    let result = transpile(sql);
    assert!(result.contains("set a = 1, b = 2"));
}
```

---

## Task 2: generate_series() Function

**Problem:** `generate_series()` set-returning function not available:
```sql
SELECT * FROM generate_series(1, 10)
-- Error: no such table: generate_series
```

**Files:**
- Modify: `src/transpiler/dml.rs` or `src/transpiler/func.rs`

**Implementation:**

Detect `generate_series()` calls and transpile to recursive CTE:
```sql
-- Input
SELECT * FROM generate_series(1, 10)

-- Output
WITH RECURSIVE _series(n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM _series WHERE n < 10
)
SELECT n FROM _series
```

**Test:**
```rust
#[test]
fn test_generate_series() {
    let sql = "SELECT * FROM generate_series(1, 3)";
    let result = transpile(sql);
    assert!(result.contains("WITH RECURSIVE"));
    assert!(result.contains("_series"));
}
```

---

## Task 3: Error Message Alignment

**Problem:** Error codes and messages don't match PostgreSQL exactly.

**Files:**
- Modify: `src/validation/types.rs` - Update error codes
- Modify: `src/handler/errors.rs` - Map SQLite errors to PostgreSQL codes

**Key Error Codes to Align:**
- `22001` - string_data_right_truncation
- `22P02` - invalid_text_representation  
- `22007` - invalid_datetime_format
- `22008` - datetime_field_overflow
- `42601` - syntax_error

**Test:**
Verify error messages match PostgreSQL format in compatibility tests.

---

## Task 4: EXPLAIN Support

**Problem:** EXPLAIN doesn't work for all query types.

**Files:**
- Modify: `src/transpiler/dml.rs` - Add EXPLAIN handling

**Implementation:**

Add support for EXPLAIN with various options:
```sql
EXPLAIN (costs off) SELECT * FROM t
EXPLAIN ANALYZE SELECT * FROM t
```

---

## Summary

After Phase 3:
- UPDATE row constructors work
- generate_series() available
- Error codes match PostgreSQL
- EXPLAIN works for more queries

**Expected Pass Rate:** 30% → 35-40%
