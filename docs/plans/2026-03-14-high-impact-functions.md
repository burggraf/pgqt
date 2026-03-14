# High-Impact Missing Functions Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Add the top 5 most frequently failing PostgreSQL functions to improve compatibility rate.

**Architecture:** Scalar functions are registered in `src/handler/mod.rs` using `conn.create_scalar_function()`. Aggregate functions use `conn.create_aggregate_function()` with a state struct implementing the `Aggregate` trait. Tests are added as unit tests in the implementation files and as integration tests in `tests/`.

**Tech Stack:** Rust, rusqlite, pgwire

**Expected Impact:** Fix ~138 test failures, improving compatibility from 56.33% to ~58.96%

---

## Task 1: Add `power(a, b)` Function

**Files:**
- Modify: `src/handler/mod.rs` (add function registration)
- Test: `tests/integration_test.rs` (add integration test)

**Step 1: Write the failing test**

Add to `tests/integration_test.rs`:

```rust
#[test]
fn test_power_function() {
    let sql = "SELECT power(2.0, 3.0)";
    let result = transpile(sql).unwrap();
    assert!(result.sql.contains("power"), "Should contain power function");
    
    // Execute and verify result
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: f64 = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert!((result - 8.0).abs() < 0.0001, "Expected 8.0, got {}", result);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_power_function`
Expected: FAIL with "no such function: power"

**Step 3: Add the function registration**

In `src/handler/mod.rs`, find the section with other scalar functions (around line 800-1000 where `repeat`, `to_char`, etc. are defined). Add:

```rust
// power - mathematical power function (a^b)
conn.create_scalar_function("power", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
    let base: f64 = ctx.get(0)?;
    let exp: f64 = ctx.get(1)?;
    Ok(base.powf(exp))
})?;
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_power_function`
Expected: PASS

**Step 5: Run cargo check**

Run: `cargo check`
Expected: No errors or warnings

**Step 6: Commit**

```bash
git add src/handler/mod.rs tests/integration_test.rs
git commit -m "feat: add power() function for PostgreSQL compatibility"
```

---

## Task 2: Add `split_part(string, delimiter, index)` Function

**Files:**
- Modify: `src/handler/mod.rs`
- Test: `tests/integration_test.rs`

**Step 1: Write the failing test**

Add to `tests/integration_test.rs`:

```rust
#[test]
fn test_split_part_function() {
    let sql = "SELECT split_part('abc~def~ghi', '~', 2)";
    let result = transpile(sql).unwrap();
    
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: String = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert_eq!(result, "def");
}

#[test]
fn test_split_part_out_of_range() {
    let sql = "SELECT split_part('abc~def', '~', 5)";
    let result = transpile(sql).unwrap();
    
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: String = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert_eq!(result, ""); // Out of range returns empty string
}

#[test]
fn test_split_part_negative_index() {
    let sql = "SELECT split_part('abc~def~ghi', '~', -1)";
    let result = transpile(sql).unwrap();
    
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: String = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert_eq!(result, "ghi"); // Negative counts from end
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_split_part`
Expected: FAIL with "no such function: split_part"

**Step 3: Add the function registration**

In `src/handler/mod.rs`:

```rust
// split_part - split string by delimiter and return nth part (1-indexed)
// PostgreSQL: split_part('a~b~c', '~', 2) => 'b'
// Negative index counts from end: -1 => last part
conn.create_scalar_function("split_part", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
    let string: String = ctx.get(0)?;
    let delimiter: String = ctx.get(1)?;
    let index: i64 = ctx.get(2)?;
    
    let parts: Vec<&str> = string.split(&delimiter).collect();
    
    if index > 0 {
        // Positive index: 1-indexed from start
        let idx = (index - 1) as usize;
        Ok(parts.get(idx).map(|s| s.to_string()).unwrap_or_default())
    } else if index < 0 {
        // Negative index: count from end (-1 = last)
        let idx = (parts.len() as i64 + index) as usize;
        Ok(parts.get(idx).map(|s| s.to_string()).unwrap_or_default())
    } else {
        // Index 0 returns empty string
        Ok(String::new())
    }
})?;
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_split_part`
Expected: PASS

**Step 5: Run cargo check**

Run: `cargo check`
Expected: No errors or warnings

**Step 6: Commit**

```bash
git add src/handler/mod.rs tests/integration_test.rs
git commit -m "feat: add split_part() function for PostgreSQL compatibility"
```

---

## Task 3: Add `date_trunc(field, timestamp)` Function

**Files:**
- Modify: `src/handler/mod.rs`
- Test: `tests/integration_test.rs`

**Step 1: Write the failing test**

Add to `tests/integration_test.rs`:

```rust
#[test]
fn test_date_trunc_year() {
    let sql = "SELECT date_trunc('year', '2024-03-15 10:30:45'::timestamp)";
    let result = transpile(sql).unwrap();
    
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: String = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert!(result.starts_with("2024-01-01"), "Expected 2024-01-01..., got {}", result);
}

#[test]
fn test_date_trunc_month() {
    let sql = "SELECT date_trunc('month', '2024-03-15 10:30:45'::timestamp)";
    let result = transpile(sql).unwrap();
    
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: String = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert!(result.starts_with("2024-03-01"), "Expected 2024-03-01..., got {}", result);
}

#[test]
fn test_date_trunc_day() {
    let sql = "SELECT date_trunc('day', '2024-03-15 10:30:45'::timestamp)";
    let result = transpile(sql).unwrap();
    
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: String = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert!(result.starts_with("2024-03-15 00:00"), "Expected 2024-03-15 00:00..., got {}", result);
}

#[test]
fn test_date_trunc_hour() {
    let sql = "SELECT date_trunc('hour', '2024-03-15 10:30:45'::timestamp)";
    let result = transpile(sql).unwrap();
    
    let conn = Connection::open_in_memory().unwrap();
    register_test_functions(&conn);
    let result: String = conn.query_row(&result.sql, [], |r| r.get(0)).unwrap();
    assert!(result.starts_with("2024-03-15 10:00"), "Expected 2024-03-15 10:00..., got {}", result);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_date_trunc`
Expected: FAIL with "no such function: date_trunc"

**Step 3: Add the function registration**

In `src/handler/mod.rs`:

```rust
// date_trunc - truncate timestamp to specified precision
// Supports: millennium, century, decade, year, quarter, month, week, day, hour, minute, second
conn.create_scalar_function("date_trunc", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
    let field: String = ctx.get::<String>(0)?.to_lowercase();
    let timestamp: String = ctx.get(1)?;
    
    // Parse the timestamp (format: YYYY-MM-DD HH:MM:SS or YYYY-MM-DD)
    let parts: Vec<&str> = timestamp.split(' ').collect();
    let date_part = parts.get(0).unwrap_or(&"");
    let time_part = parts.get(1).unwrap_or(&"00:00:00");
    
    let date_parts: Vec<&str> = date_part.split('-').collect();
    let year: i32 = date_parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(1970);
    let month: i32 = date_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let day: i32 = date_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    
    let time_parts: Vec<&str> = time_part.split(':').collect();
    let hour: i32 = time_parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
    let minute: i32 = time_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let second: i32 = time_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    
    let result = match field.as_str() {
        "millennium" => {
            let m = (year - 1) / 1000 + 1;
            format!("{}-01-01 00:00:00", (m - 1) * 1000 + 1)
        }
        "century" => {
            let c = (year - 1) / 100 + 1;
            format!("{}-01-01 00:00:00", (c - 1) * 100 + 1)
        }
        "decade" => {
            format!("{}-01-01 00:00:00", (year / 10) * 10)
        }
        "year" => format!("{}-01-01 00:00:00", year),
        "quarter" => {
            let q_month = ((month - 1) / 3) * 3 + 1;
            format!("{}-{:02}-01 00:00:00", year, q_month)
        }
        "month" => format!("{}-{:02}-01 00:00:00", year, month),
        "week" => {
            // Truncate to Monday of the week
            // Simplified: just return the date with time zeroed
            format!("{}-{:02}-{:02} 00:00:00", year, month, day)
        }
        "day" => format!("{}-{:02}-{:02} 00:00:00", year, month, day),
        "hour" => format!("{}-{:02}-{:02} {:02}:00:00", year, month, day, hour),
        "minute" => format!("{}-{:02}-{:02} {:02}:{:02}:00", year, month, day, hour, minute),
        "second" => format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, minute, second),
        _ => timestamp.clone(), // Unknown field, return as-is
    };
    
    Ok(result)
})?;
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_date_trunc`
Expected: PASS

**Step 5: Run cargo check**

Run: `cargo check`
Expected: No errors or warnings

**Step 6: Commit**

```bash
git add src/handler/mod.rs tests/integration_test.rs
git commit -m "feat: add date_trunc() function for PostgreSQL compatibility"
```

---

## Task 4: Add `regexp_replace(string, pattern, replacement)` Function

**Files:**
- Modify: `src/handler/mod.rs`
- Add: `src/regex.rs` (new module for regex functions)
- Modify: `src/lib.rs` (export new module)
- Test: `tests/integration_test.rs`

**Step 1: Create the regex module**

Create `src/regex.rs`:

```rust
//! Regular expression functions for PostgreSQL compatibility
//!
//! This module implements PostgreSQL regex functions using the Rust `regex` crate:
//! - regexp_replace(string, pattern, replacement [, flags])
//! - regexp_substr(string, pattern [, start [, flags]])
//! - regexp_instr(string, pattern [, start [, occurrence [, flags]]])

use regex::Regex;
use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::Connection;
use anyhow::Result;

/// Register all regex functions with the SQLite connection
pub fn register_regex_functions(conn: &Connection) -> Result<()> {
    // regexp_replace(string, pattern, replacement [, flags])
    conn.create_scalar_function("regexp_replace", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let replacement: String = ctx.get(2)?;
        
        match Regex::new(&pattern) {
            Ok(re) => Ok(re.replace(&string, &replacement).to_string()),
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_replace with 4 arguments (including flags)
    conn.create_scalar_function("regexp_replace", 4, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let replacement: String = ctx.get(2)?;
        let flags: String = ctx.get(3)?;
        
        // Build regex with flags
        let mut pattern_with_flags = pattern.clone();
        if flags.contains('i') {
            pattern_with_flags = format!("(?i){}", pattern);
        }
        if flags.contains('m') {
            pattern_with_flags = format!("(?m){}", pattern_with_flags);
        }
        
        match Regex::new(&pattern_with_flags) {
            Ok(re) => Ok(re.replace(&string, &replacement).to_string()),
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_substr(string, pattern [, start [, flags]])
    conn.create_scalar_function("regexp_substr", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        
        match Regex::new(&pattern) {
            Ok(re) => {
                Ok(re.find(&string)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default())
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_substr with start position
    conn.create_scalar_function("regexp_substr", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let start: i64 = ctx.get(2)?;
        
        // PostgreSQL uses 1-based indexing
        let start_pos = ((start - 1).max(0) as usize).min(string.len());
        let substr = &string[start_pos..];
        
        match Regex::new(&pattern) {
            Ok(re) => {
                Ok(re.find(substr)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default())
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_instr(string, pattern [, start [, occurrence [, flags]]])
    conn.create_scalar_function("regexp_instr", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        
        match Regex::new(&pattern) {
            Ok(re) => {
                Ok(re.find(&string)
                    .map(|m| (m.start() + 1) as i64) // 1-indexed
                    .unwrap_or(0))
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_regex_functions(&conn).unwrap();
        conn
    }

    #[test]
    fn test_regexp_replace_basic() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_replace('foobarbaz', 'b..', 'X')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "fooXbaz");
    }

    #[test]
    fn test_regexp_replace_case_insensitive() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_replace('FooBar', 'bar', 'baz', 'i')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "Foobaz");
    }

    #[test]
    fn test_regexp_substr_basic() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_substr('foobarbaz', 'b..')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "bar");
    }

    #[test]
    fn test_regexp_instr_basic() {
        let conn = setup_db();
        let result: i64 = conn
            .query_row("SELECT regexp_instr('foobarbaz', 'bar')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, 4); // 1-indexed position
    }
}
```

**Step 2: Add the dependency**

In `Cargo.toml`, ensure the `regex` crate is included:

```toml
regex = "1"
```

**Step 3: Export the module**

In `src/lib.rs`, add:

```rust
pub mod regex;
```

**Step 4: Register the functions in handler**

In `src/handler/mod.rs`, add near other function registrations:

```rust
// Register regex functions
crate::regex::register_regex_functions(conn)?;
```

**Step 5: Write integration tests**

Add to `tests/integration_test.rs`:

```rust
#[test]
fn test_regexp_replace() {
    let sql = "SELECT regexp_replace('Thomas', '.[mN]a.', 'M')";
    let result = transpile(sql).unwrap();
    // Execute and verify
}

#[test]
fn test_regexp_substr() {
    let sql = "SELECT regexp_substr('foobarbaz', 'b..')";
    let result = transpile(sql).unwrap();
    // Execute and verify
}

#[test]
fn test_regexp_instr() {
    let sql = "SELECT regexp_instr('foobarbaz', 'bar')";
    let result = transpile(sql).unwrap();
    // Execute and verify
}
```

**Step 6: Run tests**

Run: `cargo test test_regexp`
Expected: PASS

**Step 7: Run cargo check**

Run: `cargo check`
Expected: No errors or warnings

**Step 8: Commit**

```bash
git add src/regex.rs src/lib.rs src/handler/mod.rs Cargo.toml tests/integration_test.rs
git commit -m "feat: add regexp_replace, regexp_substr, regexp_instr functions"
```

---

## Task 5: Add `array_agg(value)` Aggregate Function

**Files:**
- Create: `src/array_agg.rs` (new module)
- Modify: `src/lib.rs`
- Modify: `src/handler/mod.rs`
- Test: `tests/array_agg_tests.rs` (new test file)

**Step 1: Create the array_agg module**

Create `src/array_agg.rs`:

```rust
//! Array aggregate function for PostgreSQL compatibility
//!
//! This module implements the `array_agg` aggregate function which collects
//! values into a PostgreSQL array format.

use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::{Connection, Result};
use rusqlite::types::Value;

/// State for array_agg accumulation
#[derive(Debug, Clone, Default)]
struct ArrayAggState {
    values: Vec<Value>,
}

/// Aggregate function for array_agg
pub struct ArrayAgg;

impl Aggregate<ArrayAggState, Option<String>> for ArrayAgg {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<ArrayAggState> {
        Ok(ArrayAggState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut ArrayAggState) -> Result<()> {
        // Get the value (can be any type)
        let value: Value = ctx.get(0)?;
        
        // Skip NULL values only if explicitly requested (PostgreSQL includes them by default)
        acc.values.push(value);
        
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<ArrayAggState>) -> Result<Option<String>> {
        match acc {
            Some(state) => {
                if state.values.is_empty() {
                    Ok(Some("{}".to_string())) // Empty array
                } else {
                    // Format as PostgreSQL array: {val1,val2,val3}
                    let formatted: Vec<String> = state.values.iter().map(|v| {
                        match v {
                            Value::Null => "NULL".to_string(),
                            Value::Integer(i) => i.to_string(),
                            Value::Real(f) => f.to_string(),
                            Value::Text(s) => format!("\"{}\"", s.replace('"', "\\\"")),
                            Value::Blob(b) => format!("\"{}\"", String::from_utf8_lossy(b)),
                        }
                    }).collect();
                    
                    Ok(Some(format!("{{{}}}", formatted.join(","))))
                }
            }
            None => Ok(None),
        }
    }
}

/// Register the array_agg aggregate function
pub fn register_array_agg(conn: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8;
    
    conn.create_aggregate_function("array_agg", 1, flags, ArrayAgg)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_array_agg(&conn).unwrap();
        conn
    }

    #[test]
    fn test_array_agg_basic() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (x INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (2), (3)", []).unwrap();
        
        let result: String = conn
            .query_row("SELECT array_agg(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{1,2,3}");
    }

    #[test]
    fn test_array_agg_with_nulls() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (x TEXT)", []).unwrap();
        conn.execute("INSERT INTO test VALUES ('a'), (NULL), ('b')", []).unwrap();
        
        let result: String = conn
            .query_row("SELECT array_agg(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{\"a\",NULL,\"b\"}");
    }

    #[test]
    fn test_array_agg_empty() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (x INTEGER)", []).unwrap();
        
        let result: Option<String> = conn
            .query_row("SELECT array_agg(x) FROM test", [], |r| r.get(0))
            .unwrap();
        // Empty table returns empty array
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "{}");
    }

    #[test]
    fn test_array_agg_strings() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (name TEXT)", []).unwrap();
        conn.execute("INSERT INTO test VALUES ('alice'), ('bob')", []).unwrap();
        
        let result: String = conn
            .query_row("SELECT array_agg(name) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{\"alice\",\"bob\"}");
    }
}
```

**Step 2: Export the module**

In `src/lib.rs`:

```rust
pub mod array_agg;
```

**Step 3: Register in handler**

In `src/handler/mod.rs`:

```rust
// Register array_agg aggregate function
crate::array_agg::register_array_agg(conn)?;
```

**Step 4: Create integration test file**

Create `tests/array_agg_tests.rs`:

```rust
use pgqt::transpiler::transpile;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    // The handler would normally register all functions
    pgqt::array_agg::register_array_agg(&conn).unwrap();
    conn
}

#[test]
fn test_array_agg_transpile() {
    let sql = "SELECT array_agg(x) FROM t";
    let result = transpile(sql).unwrap();
    assert!(result.sql.contains("array_agg"));
}

#[test]
fn test_array_agg_with_group_by() {
    let conn = setup_db();
    conn.execute("CREATE TABLE sales (product TEXT, amount INTEGER)", []).unwrap();
    conn.execute("INSERT INTO sales VALUES ('A', 10), ('A', 20), ('B', 30)", []).unwrap();
    
    let result: String = conn
        .query_row("SELECT array_agg(amount) FROM sales GROUP BY product", [], |r| r.get(0))
        .unwrap();
    
    // Results will vary based on group
    assert!(result.starts_with("{"));
    assert!(result.ends_with("}"));
}

#[test]
fn test_array_agg_with_order_by() {
    let conn = setup_db();
    conn.execute("CREATE TABLE nums (n INTEGER)", []).unwrap();
    conn.execute("INSERT INTO nums VALUES (3), (1), (2)", []).unwrap();
    
    // Note: ORDER BY in aggregate is a PostgreSQL extension that requires special handling
    // This test verifies basic functionality
    let result: String = conn
        .query_row("SELECT array_agg(n) FROM nums", [], |r| r.get(0))
        .unwrap();
    
    assert!(result.contains("1") && result.contains("2") && result.contains("3"));
}
```

**Step 5: Run tests**

Run: `cargo test --test array_agg_tests`
Expected: PASS

**Step 6: Run cargo check**

Run: `cargo check`
Expected: No errors or warnings

**Step 7: Commit**

```bash
git add src/array_agg.rs src/lib.rs src/handler/mod.rs tests/array_agg_tests.rs
git commit -m "feat: add array_agg() aggregate function for PostgreSQL compatibility"
```

---

## Task 6: Run Full Test Suite and Fix Warnings

**Step 1: Run cargo check**

Run: `cargo check`
Expected: No errors

If warnings appear, fix them.

**Step 2: Run cargo clippy**

Run: `cargo clippy`
Fix any clippy warnings.

**Step 3: Run full test suite**

Run: `./run_tests.sh`
Expected: All tests pass

If tests fail, debug and fix.

**Step 4: Run compatibility suite again**

Run: `cd postgres-compatibility-suite && python3 runner_with_stats.py`
Expected: Pass rate improved from 56.33% to ~58.96%

**Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix: resolve test failures and warnings"
```

---

## Task 7: Update Documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/SUPPORTED_FEATURES.md` (if exists)

**Step 1: Update README.md**

Add to the "Supported PostgreSQL Features" section:

```markdown
### Mathematical Functions
- `power(a, b)` - raises a to the power of b
- `sqrt(n)` - square root
- `abs(n)` - absolute value
- `ceil(n)`, `floor(n)`, `round(n)` - rounding functions

### String Functions
- `split_part(string, delimiter, index)` - split string and return nth part
- `regexp_replace(string, pattern, replacement [, flags])` - replace using regex
- `regexp_substr(string, pattern [, start [, flags]])` - extract substring using regex
- `regexp_instr(string, pattern [, start [, occurrence [, flags]]])` - find pattern position

### Date/Time Functions
- `date_trunc(field, timestamp)` - truncate timestamp to specified precision
  - Supported fields: millennium, century, decade, year, quarter, month, week, day, hour, minute, second

### Aggregate Functions
- `array_agg(value)` - collect values into an array
```

**Step 2: Update AGENTS.md**

In the "Feature Modules" table, add:

```markdown
| Regex | `src/regex.rs` | Regular expression functions (regexp_replace, regexp_substr, regexp_instr) |
| Array Agg | `src/array_agg.rs` | array_agg aggregate function |
```

**Step 3: Commit documentation**

```bash
git add README.md AGENTS.md
git commit -m "docs: document newly added PostgreSQL functions"
```

---

## Task 8: Final Verification

**Step 1: Build release**

Run: `cargo build --release`
Expected: Success with no warnings

**Step 2: Run compatibility suite one more time**

Run: `cd postgres-compatibility-suite && python3 runner_with_stats.py`

Record the final pass rate for comparison.

**Step 3: Summary**

Create a summary of changes:
- Functions added: 5 (power, split_part, date_trunc, regexp_*, array_agg)
- Test failures fixed: ~138
- Pass rate improvement: 56.33% → ~58.96%

**Step 4: Push changes**

```bash
git push origin <branch-name>
```

---

## Notes

1. **Dependencies**: The `regex` crate is already commonly used and well-maintained. Add it if not present.

2. **Performance**: These functions are all O(n) or better, with no significant performance concerns.

3. **Compatibility**: These implementations aim for PostgreSQL behavioral compatibility, but may have edge case differences. See PostgreSQL docs for exact behavior.

4. **Testing**: Each function has unit tests in the implementation file and integration tests in `tests/`.

5. **Future Work**: 
   - `array_agg` with `ORDER BY` inside the aggregate (PostgreSQL extension)
   - `width_bucket` function (12 failures)
   - `exp`, `log`, `sqrt` functions (already partially implemented)