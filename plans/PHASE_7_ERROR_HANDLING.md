# Phase 7: Error Handling Alignment

## Overview

**Goal:** Align input validation with PostgreSQL semantics for stricter compatibility.

**Estimated Score Gain:** +3-5% overall compatibility

**Note:** This phase is lower priority because PGQT being more permissive than PostgreSQL doesn't break working queries—it just means PGQT accepts some inputs that PostgreSQL rejects.

---

## Sub-Phase 7.1: Input Validation Improvements

### Objective
Align input validation with PostgreSQL semantics.

### Areas to Address

| Area | PostgreSQL Behavior | PGQT Current Behavior |
|------|---------------------|----------------------|
| Invalid interval strings | ERROR: invalid input syntax | May accept silently |
| Invalid JSON | ERROR: invalid input syntax | May accept or parse differently |
| Out-of-range numeric | ERROR: numeric value out of range | May truncate or accept |
| Type casting validation | Strict validation | May be more permissive |

### Implementation Steps

1. **Interval Validation:**
   ```rust
   // In src/interval.rs
   impl Interval {
       pub fn from_str(s: &str) -> Result<Self, IntervalError> {
           // Current: May accept partial/invalid input
           // New: Strict validation matching PostgreSQL
           
           let trimmed = s.trim();
           
           // Reject obviously invalid inputs
           if trimmed.is_empty() {
               return Err(IntervalError::InvalidInput(
                   "invalid input syntax for type interval".to_string()
               ));
           }
           
           // Try each parser, but be strict about what we accept
           if let Ok(interval) = Self::parse_standard(trimmed) {
               return Ok(interval);
           }
           
           if let Ok(interval) = Self::parse_at_style(trimmed) {
               return Ok(interval);
           }
           
           if let Ok(interval) = Self::parse_iso8601(trimmed) {
               return Ok(interval);
           }
           
           Err(IntervalError::InvalidInput(format!(
               "invalid input syntax for type interval: \"{}\"",
               s
           )))
       }
   }
   ```

2. **JSON Validation:**
   ```rust
   // Ensure strict JSON parsing
   fn validate_json(s: &str) -> Result<String, String> {
       match serde_json::from_str::<serde_json::Value>(s) {
           Ok(_) => Ok(s.to_string()),
           Err(e) => Err(format!(
               "invalid input syntax for type json: \"{}\"",
               s
           )),
       }
   }
   ```

3. **Numeric Range Validation:**
   ```rust
   fn validate_numeric_range(val: &str, type_name: &str) -> Result<f64, String> {
       match val.parse::<f64>() {
           Ok(v) => {
               if v.is_infinite() && !val.to_lowercase().contains("inf") {
                   // Value overflowed to infinity but wasn't specified as infinity
                   Err(format!(
                       "\"{}\" is out of range for type {}",
                       val, type_name
                   ))
               } else {
                   Ok(v)
               }
           }
           Err(_) => Err(format!(
               "invalid input syntax for type {}: \"{}\"",
               type_name, val
           )),
       }
   }
   ```

4. **Error Code Mapping:**
   Return appropriate PostgreSQL error codes:
   - `22001: string_data_right_truncation`
   - `22003: numeric_value_out_of_range`
   - `22007: invalid_datetime_format`
   - `22P02: invalid_text_representation`
   - `42601: syntax_error`

### Testing

```rust
#[test]
fn test_interval_validation() {
    let conn = setup_test_db();
    
    // Valid intervals
    let valid = vec![
        "'1 day'::interval",
        "'1 hour'::interval",
        "'@ 1 minute'::interval",
    ];
    
    for input in valid {
        let result: Result<String, _> = conn.query_row(
            &format!("SELECT {}", input),
            [],
            |row| row.get(0),
        );
        assert!(result.is_ok(), "Should accept: {}", input);
    }
    
    // Invalid intervals
    let invalid = vec![
        "'invalid'::interval",
        "'xyz'::interval",
        "''::interval",
    ];
    
    for input in invalid {
        let result: Result<String, _> = conn.query_row(
            &format!("SELECT {}", input),
            [],
            |row| row.get(0),
        );
        assert!(result.is_err(), "Should reject: {}", input);
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

## Sub-Phase 7.2: Final Compatibility Suite Run & Summary

### Objective
Run the full compatibility suite and document final results.

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
   python3 runner_with_stats.py > /tmp/final_results.txt
   ```

3. **Compare Results:**
   - Baseline: 66.68%
   - Target: 85%+
   - Document all improvements

4. **Create Summary Report:**
   ```markdown
   # Compatibility Improvement Summary
   
   ## Overall Results
   - Starting Score: 66.68%
   - Final Score: XX.XX%
   - Improvement: +XX.XX percentage points
   
   ## File-by-File Improvements
   | File | Before | After | Improvement |
   |------|--------|-------|-------------|
   | json.sql | 38.5% | XX% | +XX% |
   | jsonb.sql | 58.5% | XX% | +XX% |
   | interval.sql | 30.1% | XX% | +XX% |
   | aggregates.sql | 79.9% | XX% | +XX% |
   | insert.sql | 57.8% | XX% | +XX% |
   | with.sql | 52.5% | XX% | +XX% |
   | float4.sql | 34.0% | XX% | +XX% |
   | float8.sql | 52.7% | XX% | +XX% |
   
   ## Key Features Added
   - JSON/JSONB constructor functions
   - JSON operators (->, ->>, #>, #>>, @>, <@, etc.)
   - JSON aggregation functions
   - Interval type with full arithmetic
   - Boolean aggregates (bool_and, bool_or, every)
   - Bitwise aggregates (bit_and, bit_or, bit_xor)
   - Recursive CTEs
   - Data-modifying CTEs
   - Float special values (NaN, Infinity)
   ```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Final compatibility score documented
- [ ] CHANGELOG.md updated with final summary
- [ ] README.md updated with new compatibility percentage
- [ ] All phase documentation complete

---

## Summary

This phase focuses on error handling alignment and final verification. By implementing these features and completing all phases, we expect to:

- Add ~3-5% to overall compatibility score
- Reach target of 85%+ overall compatibility
- Provide comprehensive documentation of all improvements

**Final Target:** 85-90% PostgreSQL compatibility

**Key Documentation Updates:**
- `CHANGELOG.md` - Complete summary of all changes
- `README.md` - Updated compatibility percentage
- `docs/` - Complete documentation for all new features
