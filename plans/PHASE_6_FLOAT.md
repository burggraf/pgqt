# Phase 6: Float/Real Edge Cases

## Overview

**Goal:** Handle special float values and input validation to improve `float4.sql` from 34% to 60% and `float8.sql` from 52.7% to 75%.

**Estimated Score Gain:** +2-3% overall compatibility

**Current Status:**
- float4.sql: 34.0% (34/100 statements)
- float8.sql: 52.7% (97/184 statements)

**Note:** This phase is lower priority because many failures are edge cases (infinity, NaN, overflow) that don't affect typical usage.

---

## Sub-Phase 6.1: Special Float Value Handling

### Objective
Handle special float values correctly.

### Values to Support

| Value | float4 | float8 | Description |
|-------|--------|--------|-------------|
| NaN | `'NaN'::float4` | `'NaN'::float8` | Not a Number |
| Infinity | `'infinity'::float4` | `'infinity'::float8` | Positive infinity |
| -Infinity | `'-infinity'::float4` | `'-infinity'::float8` | Negative infinity |

### Implementation Steps

1. **SQLite Float Support:**
   SQLite uses IEEE 754 floats which natively support these values:
   ```sql
   -- In SQLite:
   SELECT 1.0 / 0.0;  -- Returns inf
   SELECT -1.0 / 0.0; -- Returns -inf
   SELECT 0.0 / 0.0;  -- Returns nan
   ```

2. **Parsing Special Values:**
   ```rust
   // In src/transpiler/expr.rs or type handling
   fn parse_special_float(s: &str) -> Option<f64> {
       match s.to_lowercase().as_str() {
           "nan" | "'nan'" => Some(f64::NAN),
           "infinity" | "'infinity'" | "inf" => Some(f64::INFINITY),
           "-infinity" | "'-infinity'" | "-inf" => Some(f64::NEG_INFINITY),
           _ => None,
       }
   }
   ```

3. **Transpiler Updates:**
   ```rust
   // When encountering 'NaN'::float4 or similar
   // Map to: CAST('NaN' AS REAL) or just the literal
   // SQLite will parse 'NaN', 'Inf', etc. in some builds
   ```

4. **Alternative Approach:**
   If SQLite doesn't parse these strings directly:
   ```rust
   // Create functions to generate these values
   conn.create_scalar_function(
       "nan",
       0,
       FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
       |_ctx| Ok(f64::NAN),
   )?;
   
   conn.create_scalar_function(
       "infinity",
       0,
       FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
       |_ctx| Ok(f64::INFINITY),
   )?;
   ```

### Testing

```rust
#[test]
fn test_special_float_values() {
    let conn = setup_test_db();
    
    // Test NaN
    let result: f64 = conn.query_row(
        "SELECT 'NaN'::float8",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(result.is_nan());
    
    // Test Infinity
    let result: f64 = conn.query_row(
        "SELECT 'infinity'::float8",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(result.is_infinite() && result > 0.0);
    
    // Test -Infinity
    let result: f64 = conn.query_row(
        "SELECT '-infinity'::float8",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(result.is_infinite() && result < 0.0);
}

#[test]
fn test_float_arithmetic_with_special_values() {
    let conn = setup_test_db();
    
    // Infinity + 100 = Infinity
    let result: f64 = conn.query_row(
        "SELECT 'infinity'::float8 + 100.0",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(result.is_infinite() && result > 0.0);
    
    // Infinity / Infinity = NaN
    let result: f64 = conn.query_row(
        "SELECT 'infinity'::float8 / 'infinity'::float8",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(result.is_nan());
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

## Sub-Phase 6.2: Float Input Validation

### Objective
Match PostgreSQL's float input validation.

### Invalid Inputs to Reject

| Input | Should Error? | PostgreSQL Behavior |
|-------|---------------|---------------------|
| `'xyz'::float4` | Yes | ERROR: invalid input syntax |
| `'5.0.0'::float4` | Yes | ERROR: invalid input syntax |
| `'5 . 0'::float4` | Yes | ERROR: invalid input syntax |
| `'     - 3.0'::float4` | Yes | ERROR: invalid input syntax |
| `''::float4` | Yes | ERROR: invalid input syntax |
| `'       '::float4` | Yes | ERROR: invalid input syntax |

### Implementation Steps

1. **Input Validation:**
   ```rust
   fn validate_float_input(s: &str) -> Result<f64, String> {
       let trimmed = s.trim();
       
       // Check for empty or whitespace-only
       if trimmed.is_empty() {
           return Err("invalid input syntax for type double precision: \"\"".to_string());
       }
       
       // Check for obviously invalid patterns
       if trimmed.matches('.').count() > 1 {
           return Err(format!("invalid input syntax for type double precision: \"{}\"", s));
       }
       
       // Check for spaces in the middle
       if trimmed.contains(' ') && !trimmed.starts_with('-') && !trimmed.starts_with('+') {
           return Err(format!("invalid input syntax for type double precision: \"{}\"", s));
       }
       
       // Try to parse
       match trimmed.parse::<f64>() {
           Ok(v) => Ok(v),
           Err(_) => Err(format!("invalid input syntax for type double precision: \"{}\"", s)),
       }
   }
   ```

2. **Transpiler Updates:**
   ```rust
   // When encountering ::float4 or ::float8 casts
   // Validate the input and generate appropriate error if invalid
   ```

3. **Error Handling:**
   Return PostgreSQL-compatible error messages:
   - `22003: numeric value out of range` (for overflow)
   - `22P02: invalid text representation` (for invalid syntax)

### Testing

```rust
#[test]
fn test_invalid_float_input() {
    let conn = setup_test_db();
    
    // These should all error
    let invalid_inputs = vec![
        "'xyz'::float4",
        "'5.0.0'::float4",
        "'5 . 0'::float4",
        "'     - 3.0'::float4",
        "''::float4",
    ];
    
    for input in invalid_inputs {
        let result: Result<f64, _> = conn.query_row(
            &format!("SELECT {}", input),
            [],
            |row| row.get(0),
        );
        assert!(result.is_err(), "Should have errored for: {}", input);
    }
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

## Sub-Phase 6.3: Integration & Compatibility Suite Run

### Objective
Run the full compatibility suite and verify float improvements.

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
   - Baseline: float4.sql: 34.0%, float8.sql: 52.7%
   - Target: float4.sql: 60%+, float8.sql: 75%+
   - Document improvements

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in float4.sql and float8.sql scores
- [ ] CHANGELOG.md updated with phase summary
- [ ] README.md updated with new compatibility percentage

---

## Summary

This phase focuses on float/real edge cases. By implementing these features, we expect to:

- Improve `float4.sql` from 34.0% to ~60% (+26 percentage points)
- Improve `float8.sql` from 52.7% to ~75% (+22 percentage points)
- Add ~2-3% to overall compatibility score

**Note:** This phase is lower priority because the failures are mostly edge cases that don't affect typical application usage.

**Key Implementation Files:**
- `src/transpiler/expr.rs` (type casting)
- `src/handler/mod.rs` (special value functions)
- `tests/float_tests.rs` (create)
