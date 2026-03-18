# Phase 3: Boolean & Bitwise Aggregate Functions

## Overview

**Goal:** Implement boolean and bitwise aggregate functions to improve `aggregates.sql` from 79.9% to 90% and related float tests.

**Estimated Score Gain:** +4-5% overall compatibility

**Current Status:**
- aggregates.sql: 79.9% (489/612 statements)
- float4.sql: 34.0% (34/100 statements)
- float8.sql: 52.7% (97/184 statements)

---

## Sub-Phase 3.1: Boolean Aggregate Functions

### Objective
Implement boolean aggregate functions.

### Functions to Implement

| Function | Signature | Description |
|----------|-----------|-------------|
| `bool_and` | `bool_and(boolean)` | AND of all non-null values |
| `bool_or` | `bool_or(boolean)` | OR of all non-null values |
| `every` | `every(boolean)` | Equivalent to bool_and (SQL standard) |

### State Functions (for internal use)

| Function | Signature | Description |
|----------|-----------|-------------|
| `booland_statefunc` | `booland_statefunc(boolean, boolean)` | State transition for bool_and |
| `boolor_statefunc` | `boolor_statefunc(boolean, boolean)` | State transition for bool_or |

### Implementation Steps

1. **Create Aggregate Functions:**
   Look at `src/array_agg.rs` for the aggregate function pattern:
   
   ```rust
   //! Boolean aggregate functions
   
   use rusqlite::functions::{Aggregate, Context, FunctionFlags};
   use rusqlite::Connection;
   
   /// State for bool_and aggregate
   pub struct BoolAndState {
       result: Option<bool>,
   }
   
   /// bool_and aggregate implementation
   pub struct BoolAnd;
   
   impl Aggregate<BoolAndState, Option<bool>> for BoolAnd {
       fn init(&self, _ctx: &mut Context) -> BoolAndState {
           BoolAndState { result: None }
       }
       
       fn step(&self, ctx: &mut Context, state: &mut BoolAndState) {
           let val: Option<bool> = ctx.get(0).ok();
           
           if let Some(v) = val {
               state.result = Some(state.result.unwrap_or(true) && v);
           }
       }
       
       fn finalize(&self, _ctx: &mut Context, state: Option<BoolAndState>) -> Option<bool> {
           state.map(|s| s.result.unwrap_or(true))
       }
   }
   
   /// State for bool_or aggregate
   pub struct BoolOrState {
       result: Option<bool>,
   }
   
   /// bool_or aggregate implementation
   pub struct BoolOr;
   
   impl Aggregate<BoolOrState, Option<bool>> for BoolOr {
       fn init(&self, _ctx: &mut Context) -> BoolOrState {
           BoolOrState { result: None }
       }
       
       fn step(&self, ctx: &mut Context, state: &mut BoolOrState) {
           let val: Option<bool> = ctx.get(0).ok();
           
           if let Some(v) = val {
               state.result = Some(state.result.unwrap_or(false) || v);
           }
       }
       
       fn finalize(&self, _ctx: &mut Context, state: Option<BoolOrState>) -> Option<bool> {
           state.map(|s| s.result.unwrap_or(false))
       }
   }
   
   /// Register boolean aggregate functions
   pub fn register_bool_aggregates(conn: &Connection) -> rusqlite::Result<()> {
       conn.create_aggregate_function(
           "bool_and",
           1,
           FunctionFlags::SQLITE_UTF8,
           BoolAnd,
       )?;
       
       conn.create_aggregate_function(
           "bool_or",
           1,
           FunctionFlags::SQLITE_UTF8,
           BoolOr,
       )?;
       
       // every is an alias for bool_and
       conn.create_aggregate_function(
           "every",
           1,
           FunctionFlags::SQLITE_UTF8,
           BoolAnd,
       )?;
       
       Ok(())
   }
   ```

2. **State Functions (for completeness):**
   ```rust
   /// Register state transition functions
   pub fn register_bool_statefuncs(conn: &Connection) -> rusqlite::Result<()> {
       conn.create_scalar_function(
           "booland_statefunc",
           2,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let acc: Option<bool> = ctx.get(0).ok();
               let val: Option<bool> = ctx.get(1).ok();
               
               match (acc, val) {
                   (Some(a), Some(v)) => Ok(Some(a && v)),
                   (Some(a), None) => Ok(Some(a)),
                   (None, Some(v)) => Ok(Some(v)),
                   (None, None) => Ok(None::<bool>),
               }
           },
       )?;
       
       conn.create_scalar_function(
           "boolor_statefunc",
           2,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let acc: Option<bool> = ctx.get(0).ok();
               let val: Option<bool> = ctx.get(1).ok();
               
               match (acc, val) {
                   (Some(a), Some(v)) => Ok(Some(a || v)),
                   (Some(a), None) => Ok(Some(a)),
                   (None, Some(v)) => Ok(Some(v)),
                   (None, None) => Ok(None::<bool>),
               }
           },
       )?;
       
       Ok(())
   }
   ```

3. **Integration:**
   Add to `src/handler/mod.rs` in `register_custom_functions()`:
   ```rust
   register_bool_aggregates(conn)?;
   register_bool_statefuncs(conn)?;
   ```

### Testing

Create `tests/bool_aggregate_tests.rs`:
```rust
use pgqt::test_utils::setup_test_db;

#[test]
fn test_bool_and() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE bool_test (id INT, b BOOLEAN)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO bool_test VALUES (1, true), (2, true), (3, false)",
        [],
    ).unwrap();
    
    let result: Option<bool> = conn.query_row(
        "SELECT bool_and(b) FROM bool_test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, Some(false));
}

#[test]
fn test_bool_or() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE bool_test (id INT, b BOOLEAN)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO bool_test VALUES (1, false), (2, false), (3, true)",
        [],
    ).unwrap();
    
    let result: Option<bool> = conn.query_row(
        "SELECT bool_or(b) FROM bool_test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, Some(true));
}

#[test]
fn test_bool_and_all_null() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE bool_test (id INT, b BOOLEAN)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO bool_test VALUES (1, NULL), (2, NULL)",
        [],
    ).unwrap();
    
    let result: Option<bool> = conn.query_row(
        "SELECT bool_and(b) FROM bool_test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    // Empty result should return true for bool_and
    assert_eq!(result, Some(true));
}

#[test]
fn test_every_alias() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE bool_test (id INT, b BOOLEAN)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO bool_test VALUES (1, true), (2, true)",
        [],
    ).unwrap();
    
    let result: Option<bool> = conn.query_row(
        "SELECT every(b) FROM bool_test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, Some(true));
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/FUNCTIONS.md` or `docs/AGGREGATES.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 3.2: Bitwise Aggregate Functions

### Objective
Implement bitwise aggregate functions.

### Functions to Implement

| Function | Signature | Description |
|----------|-----------|-------------|
| `bit_and` | `bit_and(integer)` | Bitwise AND of all non-null values |
| `bit_or` | `bit_or(integer)` | Bitwise OR of all non-null values |
| `bit_xor` | `bit_xor(integer)` | Bitwise XOR of all non-null values |

### Implementation Steps

1. **Create Aggregate Functions:**
   ```rust
   //! Bitwise aggregate functions
   
   use rusqlite::functions::{Aggregate, Context, FunctionFlags};
   use rusqlite::Connection;
   
   /// State for bit_and aggregate
   pub struct BitAndState {
       result: Option<i64>,
   }
   
   /// bit_and aggregate implementation
   pub struct BitAnd;
   
   impl Aggregate<BitAndState, Option<i64>> for BitAnd {
       fn init(&self, _ctx: &mut Context) -> BitAndState {
           BitAndState { result: None }
       }
       
       fn step(&self, ctx: &mut Context, state: &mut BitAndState) {
           let val: Option<i64> = ctx.get(0).ok();
           
           if let Some(v) = val {
               state.result = Some(state.result.unwrap_or(v) & v);
           }
       }
       
       fn finalize(&self, _ctx: &mut Context, state: Option<BitAndState>) -> Option<i64> {
           state.and_then(|s| s.result)
       }
   }
   
   /// State for bit_or aggregate
   pub struct BitOrState {
       result: Option<i64>,
   }
   
   /// bit_or aggregate implementation
   pub struct BitOr;
   
   impl Aggregate<BitOrState, Option<i64>> for BitOr {
       fn init(&self, _ctx: &mut Context) -> BitOrState {
           BitOrState { result: None }
       }
       
       fn step(&self, ctx: &mut Context, state: &mut BitOrState) {
           let val: Option<i64> = ctx.get(0).ok();
           
           if let Some(v) = val {
               state.result = Some(state.result.unwrap_or(0) | v);
           }
       }
       
       fn finalize(&self, _ctx: &mut Context, state: Option<BitOrState>) -> Option<i64> {
           state.and_then(|s| s.result)
       }
   }
   
   /// State for bit_xor aggregate
   pub struct BitXorState {
       result: Option<i64>,
   }
   
   /// bit_xor aggregate implementation
   pub struct BitXor;
   
   impl Aggregate<BitXorState, Option<i64>> for BitXor {
       fn init(&self, _ctx: &mut Context) -> BitXorState {
           BitXorState { result: None }
       }
       
       fn step(&self, ctx: &mut Context, state: &mut BitXorState) {
           let val: Option<i64> = ctx.get(0).ok();
           
           if let Some(v) = val {
               state.result = Some(state.result.unwrap_or(0) ^ v);
           }
       }
       
       fn finalize(&self, _ctx: &mut Context, state: Option<BitXorState>) -> Option<i64> {
           state.and_then(|s| s.result)
       }
   }
   
   /// Register bitwise aggregate functions
   pub fn register_bitwise_aggregates(conn: &Connection) -> rusqlite::Result<()> {
       conn.create_aggregate_function(
           "bit_and",
           1,
           FunctionFlags::SQLITE_UTF8,
           BitAnd,
       )?;
       
       conn.create_aggregate_function(
           "bit_or",
           1,
           FunctionFlags::SQLITE_UTF8,
           BitOr,
       )?;
       
       conn.create_aggregate_function(
           "bit_xor",
           1,
           FunctionFlags::SQLITE_UTF8,
           BitXor,
       )?;
       
       Ok(())
   }
   ```

2. **Integration:**
   Add to `src/handler/mod.rs`:
   ```rust
   register_bitwise_aggregates(conn)?;
   ```

### Testing

Create tests in `tests/bool_aggregate_tests.rs` or new file:
```rust
#[test]
fn test_bit_and() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE bitwise_test (id INT, i INT)",
        [],
    ).unwrap();
    
    // 5 = 101, 3 = 011, 1 = 001
    // 5 & 3 & 1 = 001 = 1
    conn.execute(
        "INSERT INTO bitwise_test VALUES (1, 5), (2, 3), (3, 1)",
        [],
    ).unwrap();
    
    let result: Option<i64> = conn.query_row(
        "SELECT bit_and(i) FROM bitwise_test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, Some(1));
}

#[test]
fn test_bit_or() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE bitwise_test (id INT, i INT)",
        [],
    ).unwrap();
    
    // 1 = 001, 2 = 010, 4 = 100
    // 1 | 2 | 4 = 111 = 7
    conn.execute(
        "INSERT INTO bitwise_test VALUES (1, 1), (2, 2), (3, 4)",
        [],
    ).unwrap();
    
    let result: Option<i64> = conn.query_row(
        "SELECT bit_or(i) FROM bitwise_test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, Some(7));
}

#[test]
fn test_bit_xor() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE bitwise_test (id INT, i INT)",
        [],
    ).unwrap();
    
    // 5 = 101, 3 = 011
    // 5 ^ 3 = 110 = 6
    conn.execute(
        "INSERT INTO bitwise_test VALUES (1, 5), (2, 3)",
        [],
    ).unwrap();
    
    let result: Option<i64> = conn.query_row(
        "SELECT bit_xor(i) FROM bitwise_test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, Some(6));
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

## Sub-Phase 3.3: Statistical Aggregate Functions

### Objective
Implement internal aggregate functions used by PostgreSQL for statistics.

### Functions to Implement

| Function | Signature | Description |
|----------|-----------|-------------|
| `float8_accum` | `float8_accum(real[], real)` | Accumulate for statistical aggregates |
| `float8_regr_accum` | `float8_regr_accum(real[], real, real)` | Accumulate for regression |
| `float8_combine` | `float8_combine(real[], real[])` | Combine accumulators |
| `float8_regr_combine` | `float8_regr_combine(real[], real[])` | Combine regression accumulators |

### Implementation Steps

1. **Understanding the Functions:**
   These are internal functions used by PostgreSQL's statistical aggregates.
   They operate on arrays representing running statistics:
   - `float8_accum` accumulates values for computing variance, stddev, etc.
   - The array format is typically: `[count, sum, sum of squares, ...]`

2. **Implementation:**
   ```rust
   //! Statistical aggregate support functions
   
   use rusqlite::functions::FunctionFlags;
   use rusqlite::Connection;
   use serde_json::Value as JsonValue;
   
   /// float8_accum - accumulate a value for statistical computation
   /// Input: accum array [n, sum, sum_sqr], new value
   /// Output: updated accum array
   pub fn register_float8_accum(conn: &Connection) -> rusqlite::Result<()> {
       conn.create_scalar_function(
           "float8_accum",
           2,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let accum_str: String = ctx.get(0)?;
               let val: f64 = ctx.get(1)?;
               
               let mut accum: Vec<f64> = serde_json::from_str(&accum_str)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
               
               // Ensure accum has at least 3 elements
               while accum.len() < 3 {
                   accum.push(0.0);
               }
               
               // Update: [count, sum, sum_of_squares]
               accum[0] += 1.0;                    // count
               accum[1] += val;                    // sum
               accum[2] += val * val;              // sum of squares
               
               serde_json::to_string(&accum)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
           },
       )?;
       
       Ok(())
   }
   
   /// float8_combine - combine two accumulators
   pub fn register_float8_combine(conn: &Connection) -> rusqlite::Result<()> {
       conn.create_scalar_function(
           "float8_combine",
           2,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let accum1_str: String = ctx.get(0)?;
               let accum2_str: String = ctx.get(1)?;
               
               let accum1: Vec<f64> = serde_json::from_str(&accum1_str)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
               let accum2: Vec<f64> = serde_json::from_str(&accum2_str)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
               
               // Combine accumulators element-wise
               let len = accum1.len().max(accum2.len());
               let mut result = Vec::with_capacity(len);
               
               for i in 0..len {
                   let v1 = accum1.get(i).copied().unwrap_or(0.0);
                   let v2 = accum2.get(i).copied().unwrap_or(0.0);
                   result.push(v1 + v2);
               }
               
               serde_json::to_string(&result)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
           },
       )?;
       
       Ok(())
   }
   
   /// float8_regr_accum - accumulate for regression
   /// Input: accum array, y value, x value
   pub fn register_float8_regr_accum(conn: &Connection) -> rusqlite::Result<()> {
       conn.create_scalar_function(
           "float8_regr_accum",
           3,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let accum_str: String = ctx.get(0)?;
               let y: f64 = ctx.get(1)?;
               let x: f64 = ctx.get(2)?;
               
               let mut accum: Vec<f64> = serde_json::from_str(&accum_str)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
               
               // Ensure accum has at least 8 elements for regression
               while accum.len() < 8 {
                   accum.push(0.0);
               }
               
               // Update: [n, sum_x, sum_x2, sum_y, sum_y2, sum_xy, 0, 0]
               accum[0] += 1.0;                    // n
               accum[1] += x;                      // sum_x
               accum[2] += x * x;                  // sum_x2
               accum[3] += y;                      // sum_y
               accum[4] += y * y;                  // sum_y2
               accum[5] += x * y;                  // sum_xy
               
               serde_json::to_string(&accum)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
           },
       )?;
       
       Ok(())
   }
   
   /// float8_regr_combine - combine two regression accumulators
   pub fn register_float8_regr_combine(conn: &Connection) -> rusqlite::Result<()> {
       conn.create_scalar_function(
           "float8_regr_combine",
           2,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let accum1_str: String = ctx.get(0)?;
               let accum2_str: String = ctx.get(1)?;
               
               let accum1: Vec<f64> = serde_json::from_str(&accum1_str)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
               let accum2: Vec<f64> = serde_json::from_str(&accum2_str)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
               
               // Combine element-wise
               let len = accum1.len().max(accum2.len());
               let mut result = Vec::with_capacity(len);
               
               for i in 0..len {
                   let v1 = accum1.get(i).copied().unwrap_or(0.0);
                   let v2 = accum2.get(i).copied().unwrap_or(0.0);
                   result.push(v1 + v2);
               }
               
               serde_json::to_string(&result)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
           },
       )?;
       
       Ok(())
   }
   ```

3. **Integration:**
   Add to `src/handler/mod.rs`:
   ```rust
   register_float8_accum(conn)?;
   register_float8_combine(conn)?;
   register_float8_regr_accum(conn)?;
   register_float8_regr_combine(conn)?;
   ```

### Testing

Test with sample data and verify results match expected patterns:
```rust
#[test]
fn test_float8_accum() {
    let conn = setup_test_db();
    
    // Start with empty array
    let result: String = conn.query_row(
        "SELECT float8_accum('[]', 10.0)",
        [],
        |row| row.get(0),
    ).unwrap();
    
    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 1.0);  // count
    assert_eq!(accum[1], 10.0); // sum
    assert_eq!(accum[2], 100.0); // sum of squares
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

## Sub-Phase 3.4: Integration & Compatibility Suite Run

### Objective
Run the full compatibility suite and verify aggregate improvements.

### Tasks

1. **Build and Test:**
   ```bash
   cargo build --release
   cargo clippy --release
   ./run_tests.sh
   ```

2. **Run Compatibility Suite:**
   ```bash
   cd postgres-compatibility-suite
   source venv/bin/activate
   python3 runner_with_stats.py
   ```

3. **Compare Results:**
   - Baseline: aggregates.sql: 79.9%
   - Target: aggregates.sql: 90%+
   - Document improvements

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in aggregates.sql score
- [ ] CHANGELOG.md updated with phase summary
- [ ] README.md updated with new compatibility percentage

---

## Summary

This phase focuses on boolean, bitwise, and statistical aggregate functions. By implementing these features, we expect to:

- Improve `aggregates.sql` from 79.9% to ~90% (+10 percentage points)
- Improve `float4.sql` and `float8.sql` through statistical function support
- Add ~4-5% to overall compatibility score

**Key Implementation Files:**
- `src/aggregates.rs` (create) or extend existing modules
- `src/handler/mod.rs` (register functions)
- `tests/bool_aggregate_tests.rs` (create)
- `docs/AGGREGATES.md` (create)
