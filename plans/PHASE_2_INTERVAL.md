# Phase 2: Interval Type & Functions

## Overview

**Goal:** Implement comprehensive interval type support to improve `interval.sql` from 30.1% to 70%.

**Estimated Score Gain:** +3-4% overall compatibility

**Current Status:**
- interval.sql: 30.1% (135/449 statements)

---

## Sub-Phase 2.1: Interval Input Parsing

### Objective
Support PostgreSQL interval input formats.

### Input Formats to Support

| Format | Example | Description |
|--------|---------|-------------|
| Standard | `'1 day 2 hours'` | Component-based |
| At-style | `'@ 1 minute'` | At-prefix format |
| ISO 8601 | `'P1Y2M3DT4H5M6S'` | ISO standard |
| Weeks | `'1.5 weeks'` | Decimal weeks |
| Months | `'5 months'` | Months only |
| Years | `'6 years'` | Years only |
| Infinity | `'infinity'`, `'-infinity'` | Special values |

### Implementation Steps

1. **Create `src/interval.rs`:**
   ```rust
   //! Interval type support for PGQT
   
   use std::str::FromStr;
   
   /// Internal representation of an interval
   #[derive(Debug, Clone, PartialEq)]
   pub struct Interval {
       pub months: i32,      // Months component
       pub days: i32,        // Days component
       pub microseconds: i64, // Time component in microseconds
   }
   
   impl Interval {
       pub fn new(months: i32, days: i32, microseconds: i64) -> Self {
           Interval { months, days, microseconds }
       }
       
       pub fn from_str(s: &str) -> Result<Self, IntervalError> {
           // Parse various interval formats
           if s.eq_ignore_ascii_case("infinity") {
               return Ok(Interval::infinity());
           }
           if s.eq_ignore_ascii_case("-infinity") {
               return Ok(Interval::neg_infinity());
           }
           
           // Try ISO 8601 format
           if s.starts_with('P') {
               return Self::parse_iso8601(s);
           }
           
           // Try at-style format
           if s.starts_with('@') {
               return Self::parse_at_style(s);
           }
           
           // Try standard format
           Self::parse_standard(s)
       }
       
       fn parse_standard(s: &str) -> Result<Self, IntervalError> {
           // Parse formats like "1 day 2 hours 3 minutes 4 seconds"
           // "6 years 5 months"
           // "1.5 weeks"
       }
       
       fn parse_at_style(s: &str) -> Result<Self, IntervalError> {
           // Parse formats like "@ 1 minute", "@ 5 hour ago"
       }
       
       fn parse_iso8601(s: &str) -> Result<Self, IntervalError> {
           // Parse ISO 8601 duration format P1Y2M3DT4H5M6S
       }
   }
   ```

2. **Storage Format:**
   Store intervals as a delimited string or JSON in SQLite:
   ```rust
   impl ToString for Interval {
       fn to_string(&self) -> String {
           format!("{}|{}|{}", self.months, self.days, self.microseconds)
       }
   }
   ```

3. **Register Type Conversion:**
   Add to `src/handler/mod.rs`:
   ```rust
   pub fn register_interval_functions(conn: &Connection) -> rusqlite::Result<()> {
       // Register interval parsing for ::interval casts
       conn.create_scalar_function(
           "parse_interval",
           1,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let s: String = ctx.get(0)?;
               let interval = Interval::from_str(&s)
                   .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
               Ok(interval.to_string())
           },
       )?;
       Ok(())
   }
   ```

4. **Transpiler Updates:**
   In `src/transpiler/expr.rs`, handle `::interval` casts:
   ```rust
   // When encountering '1 day'::interval
   // Transform to: parse_interval('1 day')
   ```

### Testing

Create `tests/interval_tests.rs`:
```rust
#[test]
fn test_interval_parsing() {
    let test_cases = vec![
        ("1 day", Interval::new(0, 1, 0)),
        ("2 hours 30 minutes", Interval::new(0, 0, 9000000000)),
        ("1.5 weeks", Interval::new(0, 10, 43200000000)),
        ("@ 1 minute", Interval::new(0, 0, 60000000)),
        ("P1Y2M3DT4H5M6S", Interval::new(14, 3, 14706000000)),
    ];
    
    for (input, expected) in test_cases {
        let parsed = Interval::from_str(input).unwrap();
        assert_eq!(parsed, expected, "Failed for input: {}", input);
    }
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/INTERVAL.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 2.2: Interval Arithmetic Operators

### Objective
Support interval arithmetic.

### Operators to Implement

| Operator | Signature | Description |
|----------|-----------|-------------|
| `+` | `interval + interval` → `interval` | Addition |
| `-` | `interval - interval` → `interval` | Subtraction |
| `*` | `interval * number` → `interval` | Multiplication |
| `*` | `number * interval` → `interval` | Multiplication (commutative) |
| `/` | `interval / number` → `interval` | Division |
| `+` | `+ interval` → `interval` | Unary plus |
| `-` | `- interval` → `interval` | Unary minus |

### Implementation Steps

1. **Arithmetic Implementation:**
   ```rust
   impl Interval {
       pub fn add(&self, other: &Interval) -> Interval {
           Interval {
               months: self.months + other.months,
               days: self.days + other.days,
               microseconds: self.microseconds + other.microseconds,
           }
       }
       
       pub fn sub(&self, other: &Interval) -> Interval {
           Interval {
               months: self.months - other.months,
               days: self.days - other.days,
               microseconds: self.microseconds - other.microseconds,
           }
       }
       
       pub fn mul(&self, factor: f64) -> Interval {
           Interval {
               months: (self.months as f64 * factor) as i32,
               days: (self.days as f64 * factor) as i32,
               microseconds: (self.microseconds as f64 * factor) as i64,
           }
       }
       
       pub fn div(&self, divisor: f64) -> Result<Interval, IntervalError> {
           if divisor == 0.0 {
               return Err(IntervalError::DivisionByZero);
           }
           Ok(Interval {
               months: (self.months as f64 / divisor) as i32,
               days: (self.days as f64 / divisor) as i32,
               microseconds: (self.microseconds as f64 / divisor) as i64,
           })
       }
       
       pub fn neg(&self) -> Interval {
           Interval {
               months: -self.months,
               days: -self.days,
               microseconds: -self.microseconds,
           }
       }
   }
   ```

2. **Register SQLite Functions:**
   ```rust
   conn.create_scalar_function(
       "interval_add",
       2,
       FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
       |ctx| {
           let i1 = Interval::from_str(&ctx.get::<String>(0)?)
               .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
           let i2 = Interval::from_str(&ctx.get::<String>(1)?)
               .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
           Ok(i1.add(&i2).to_string())
       },
   )?;
   // ... register other arithmetic functions
   ```

3. **Transpiler Updates:**
   In `src/transpiler/expr.rs`, detect interval operations:
   ```rust
   // Detect when both operands are intervals and map to functions
   // interval + interval -> interval_add(i1, i2)
   // interval * number -> interval_mul(i, n)
   ```

### Testing

Test all arithmetic operations with various interval components.

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/INTERVAL.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 2.3: Interval Comparison Operators

### Objective
Support interval comparisons.

### Operators to Implement

| Operator | Description |
|----------|-------------|
| `=` | Equal |
| `<>` or `!=` | Not equal |
| `<` | Less than |
| `<=` | Less than or equal |
| `>` | Greater than |
| `>=` | Greater than or equal |

### Implementation Steps

1. **Comparison Implementation:**
   ```rust
   impl Interval {
       /// Normalize for comparison
       /// Convert to a comparable form (approximate total microseconds)
       fn normalize_for_compare(&self) -> i128 {
           // Approximate conversions for comparison
           // 1 month ≈ 30.44 days
           // 1 day = 24 * 60 * 60 * 1000000 microseconds
           let month_micros = (self.months as i128) * 30_440_000_000i128;
           let day_micros = (self.days as i128) * 86_400_000_000i128;
           month_micros + day_micros + (self.microseconds as i128)
       }
       
       pub fn eq(&self, other: &Interval) -> bool {
           self.months == other.months 
               && self.days == other.days 
               && self.microseconds == other.microseconds
       }
       
       pub fn lt(&self, other: &Interval) -> bool {
           self.normalize_for_compare() < other.normalize_for_compare()
       }
       
       pub fn le(&self, other: &Interval) -> bool {
           self.eq(other) || self.lt(other)
       }
       
       pub fn gt(&self, other: &Interval) -> bool {
           !self.le(other)
       }
       
       pub fn ge(&self, other: &Interval) -> bool {
           !self.lt(other)
       }
   }
   ```

2. **Register SQLite Functions:**
   ```rust
   conn.create_scalar_function(
       "interval_eq",
       2,
       FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
       |ctx| {
           let i1 = Interval::from_str(&ctx.get::<String>(0)?)
               .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
           let i2 = Interval::from_str(&ctx.get::<String>(1)?)
               .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
           Ok(i1.eq(&i2))
       },
   )?;
   // ... register other comparison functions
   ```

3. **Transpiler Updates:**
   Map interval comparison operators to these functions.

### Testing

Test comparisons with different units and edge cases.

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/INTERVAL.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 2.4: Interval Extraction Functions

### Objective
Support EXTRACT from intervals and related functions.

### Functions to Implement

| Function | Description |
|----------|-------------|
| `extract(EPOCH FROM interval)` | Total seconds |
| `extract(CENTURY FROM interval)` | Centuries |
| `extract(DECADE FROM interval)` | Decades |
| `extract(YEAR FROM interval)` | Years |
| `extract(MONTH FROM interval)` | Months |
| `extract(DAY FROM interval)` | Days |
| `extract(HOUR FROM interval)` | Hours |
| `extract(MINUTE FROM interval)` | Minutes |
| `extract(SECOND FROM interval)` | Seconds |
| `extract(MILLISECOND FROM interval)` | Milliseconds |
| `extract(MICROSECOND FROM interval)` | Microseconds |

### Implementation Steps

1. **Extraction Implementation:**
   ```rust
   impl Interval {
       pub fn extract(&self, field: &str) -> f64 {
           match field.to_uppercase().as_str() {
               "EPOCH" => {
                   let month_seconds = (self.months as f64) * 30.44 * 24.0 * 3600.0;
                   let day_seconds = (self.days as f64) * 24.0 * 3600.0;
                   let micros_seconds = (self.microseconds as f64) / 1_000_000.0;
                   month_seconds + day_seconds + micros_seconds
               }
               "CENTURY" => (self.months as f64) / 1200.0,
               "DECADE" => (self.months as f64) / 120.0,
               "YEAR" => (self.months as f64) / 12.0,
               "MONTH" => (self.months % 12) as f64,
               "DAY" => self.days as f64,
               "HOUR" => (self.microseconds / 3_600_000_000) as f64,
               "MINUTE" => ((self.microseconds % 3_600_000_000) / 60_000_000) as f64,
               "SECOND" => ((self.microseconds % 60_000_000) as f64) / 1_000_000.0,
               "MILLISECOND" => ((self.microseconds % 1_000_000) as f64) / 1000.0 
                   + ((self.microseconds / 1_000_000) % 60) as f64 * 1000.0,
               "MICROSECOND" => (self.microseconds % 1_000_000) as f64,
               _ => 0.0,
           }
       }
   }
   ```

2. **Register SQLite Function:**
   ```rust
   conn.create_scalar_function(
       "extract_from_interval",
       2,
       FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
       |ctx| {
           let field: String = ctx.get(0)?;
           let interval_str: String = ctx.get(1)?;
           let interval = Interval::from_str(&interval_str)
               .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
           Ok(interval.extract(&field))
       },
   )?;
   ```

3. **Transpiler Updates:**
   Update EXTRACT handling to support interval sources:
   ```rust
   // extract(EPOCH FROM column::interval)
   // -> extract_from_interval('EPOCH', parse_interval(column))
   ```

### Testing

Test extraction of all components with various intervals.

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/INTERVAL.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 2.5: Integration & Compatibility Suite Run

### Objective
Run the full compatibility suite and verify interval improvements.

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
   - Baseline: interval.sql: 30.1%
   - Target: interval.sql: 70%+
   - Document improvements

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in interval.sql score
- [ ] CHANGELOG.md updated with phase summary
- [ ] README.md updated with new compatibility percentage

---

## Summary

This phase focuses on comprehensive interval type support. By implementing these features, we expect to:

- Improve `interval.sql` from 30.1% to ~70% (+40 percentage points)
- Add ~3-4% to overall compatibility score

**Key Implementation Files:**
- `src/interval.rs` (create)
- `src/handler/mod.rs` (register functions)
- `src/transpiler/expr.rs` (operator handling)
- `tests/interval_tests.rs` (create)
- `docs/INTERVAL.md` (create)
