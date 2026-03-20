# Implementation Plan: 3.1 Statistics & Aggregates

## Overview
Implement missing statistical aggregate functions (`regr_*` and `corr`) to complete PostgreSQL-compatible statistical analysis capabilities.

## Background
PostgreSQL provides a comprehensive set of linear regression and correlation functions for statistical analysis. These are commonly used in data analysis workloads and are expected by tools that rely on PostgreSQL's statistical functions.

## Missing Functions to Implement

### Linear Regression Functions
- `regr_count(Y, X)` - Number of input rows where both expressions are non-null
- `regr_sxx(Y, X)` - Sum of squares of the independent variable
- `regr_syy(Y, X)` - Sum of squares of the dependent variable
- `regr_sxy(Y, X)` - Sum of products of independent and dependent variables
- `regr_avgx(Y, X)` - Average of the independent variable (sum(X)/N)
- `regr_avgy(Y, X)` - Average of the dependent variable (sum(Y)/N)
- `regr_r2(Y, X)` - Square of the correlation coefficient
- `regr_slope(Y, X)` - Slope of the least-squares-fit linear equation
- `regr_intercept(Y, X)` - Y-intercept of the least-squares-fit linear equation

### Correlation Function
- `corr(Y, X)` - Correlation coefficient

## Implementation Details

### Mathematical Formulas

```
count = N
avg_x = sum(X) / N
avg_y = sum(Y) / N
sxx = sum((X - avg_x)^2) = sum(X^2) - sum(X)^2 / N
syy = sum((Y - avg_y)^2) = sum(Y^2) - sum(Y)^2 / N
sxy = sum((X - avg_x) * (Y - avg_y)) = sum(X*Y) - sum(X)*sum(Y) / N

slope = sxy / sxx
intercept = avg_y - slope * avg_x
r2 = (sxy * sxy) / (sxx * syy)  [when sxx > 0 and syy > 0]
corr = sxy / sqrt(sxx * syy)
```

### Implementation Strategy

Since SQLite doesn't support custom aggregate functions directly through its C API in the same way as PostgreSQL, we'll implement these as:

1. **State Accumulation**: Create aggregate functions that accumulate state across rows:
   - Count of rows (N)
   - Sum of X values
   - Sum of Y values
   - Sum of X*X values
   - Sum of Y*Y values
   - Sum of X*Y values

2. **Final Calculation**: Compute the final result from accumulated state.

### File: src/stats.rs

The implementation will extend the existing `src/stats.rs` file (if it exists) or create it with:

```rust
use rusqlite::functions::Aggregate;
use rusqlite::types::Value;

/// State structure for linear regression aggregates
struct RegrState {
    n: i64,
    sum_x: f64,
    sum_y: f64,
    sum_x2: f64,
    sum_y2: f64,
    sum_xy: f64,
}

impl RegrState {
    fn new() -> Self {
        Self {
            n: 0,
            sum_x: 0.0,
            sum_y: 0.0,
            sum_x2: 0.0,
            sum_y2: 0.0,
            sum_xy: 0.0,
        }
    }
    
    fn add(&mut self, x: f64, y: f64) {
        self.n += 1;
        self.sum_x += x;
        self.sum_y += y;
        self.sum_x2 += x * x;
        self.sum_y2 += y * y;
        self.sum_xy += x * y;
    }
    
    fn sxx(&self) -> f64 {
        self.sum_x2 - (self.sum_x * self.sum_x) / self.n as f64
    }
    
    fn syy(&self) -> f64 {
        self.sum_y2 - (self.sum_y * self.sum_y) / self.n as f64
    }
    
    fn sxy(&self) -> f64 {
        self.sum_xy - (self.sum_x * self.sum_y) / self.n as f64
    }
}
```

### Registration in src/handler/mod.rs

Add registration calls in the `SqliteHandler::new()` or connection initialization:

```rust
// In the connection setup code
conn.create_aggregate_function(
    "regr_count",
    2,
    FunctionFlags::SQLITE_UTF8,
    RegrCountAggregate,
)?;
conn.create_aggregate_function(
    "regr_sxx",
    2,
    FunctionFlags::SQLITE_UTF8,
    RegrSxxAggregate,
)?;
// ... etc for all functions
```

## Implementation Steps

### Step 1: Create/Update src/stats.rs
- [ ] Define `RegrState` struct for accumulating regression statistics
- [ ] Implement aggregate function structs for each `regr_*` function
- [ ] Implement the `corr` function
- [ ] Handle edge cases (division by zero, null inputs)

### Step 2: Register Functions in Handler
- [ ] Add registration calls in `src/handler/mod.rs`
- [ ] Ensure functions are registered for each new connection

### Step 3: Add Unit Tests
- [ ] Test each function with known datasets
- [ ] Test edge cases (single row, all same values, null values)
- [ ] Verify mathematical correctness against PostgreSQL results

### Step 4: Run Verification
- [ ] Run `./run_tests.sh` to ensure no regressions
- [ ] Run `cargo build --release` and fix any warnings
- [ ] Run postgres-compatibility-suite tests related to statistics

## Testing

### Unit Test Examples

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_regr_slope() {
        // Dataset: (1,2), (2,3), (3,5), (4,4)
        // Expected slope: 0.8
        let mut state = RegrState::new();
        state.add(1.0, 2.0);
        state.add(2.0, 3.0);
        state.add(3.0, 5.0);
        state.add(4.0, 4.0);
        
        let slope = state.sxy() / state.sxx();
        assert!((slope - 0.8).abs() < 0.0001);
    }
    
    #[test]
    fn test_corr() {
        // Perfect correlation
        let mut state = RegrState::new();
        state.add(1.0, 2.0);
        state.add(2.0, 4.0);
        state.add(3.0, 6.0);
        
        let corr = state.sxy() / (state.sxx() * state.syy()).sqrt();
        assert!((corr - 1.0).abs() < 0.0001);
    }
}
```

### Integration Test

Create `tests/stats_tests.rs`:

```rust
use pgqt::transpiler::transpile;

#[test]
fn test_regr_functions_transpilation() {
    // Verify the functions are recognized and not modified
    let sql = "SELECT regr_slope(y, x) FROM data";
    let result = transpile(sql);
    assert!(result.contains("regr_slope"));
}
```

### E2E Test

Create `tests/stats_e2e_test.py`:

```python
#!/usr/bin/env python3
"""End-to-end tests for statistical aggregates."""
import subprocess
import time
import psycopg2
import os
import sys
import signal
import math

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_stats_e2e.db"

def start_proxy():
    env = os.environ.copy()
    env["RUST_LOG"] = "info"
    proc = subprocess.Popen(
        ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    time.sleep(1)
    return proc

def stop_proxy(proc):
    proc.send_signal(signal.SIGTERM)
    proc.wait()

def test_regr_functions():
    """Test linear regression functions."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table
        cur.execute("CREATE TABLE data (x FLOAT, y FLOAT)")
        cur.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)")
        conn.commit()
        
        # Test regr_count
        cur.execute("SELECT regr_count(y, x) FROM data")
        result = cur.fetchone()[0]
        assert result == 4, f"Expected 4, got {result}"
        
        # Test regr_slope
        cur.execute("SELECT regr_slope(y, x) FROM data")
        result = cur.fetchone()[0]
        expected = 0.8
        assert abs(result - expected) < 0.0001, f"Expected {expected}, got {result}"
        
        # Test corr
        cur.execute("SELECT corr(y, x) FROM data")
        result = cur.fetchone()[0]
        expected = 0.8 / math.sqrt(1.0 * 1.25)  # sxy / sqrt(sxx * syy)
        assert abs(result - expected) < 0.0001, f"Expected {expected}, got {result}"
        
        cur.close()
        conn.close()
        print("test_regr_functions: PASSED")
    finally:
        stop_proxy(proc)
        if os.path.exists(DB_PATH):
            os.remove(DB_PATH)

if __name__ == "__main__":
    test_regr_functions()
```

## Verification Checklist

- [ ] `./run_tests.sh` passes all tests
- [ ] `cargo build --release` completes with no warnings
- [ ] postgres-compatibility-suite stats-related tests pass
- [ ] Unit tests cover all functions
- [ ] E2E tests verify wire protocol compatibility
- [ ] COMPATIBILITY_STATUS_PLAN.md updated with completion status

## Progress Update Template

After completing this item, update COMPATIBILITY_STATUS_PLAN.md:

```markdown
### 3.1 Statistics & Aggregates
- **Problem**: Missing `regr_*` and `corr` functions.
- **Action**: Complete the implementation of statistical aggregates in `src/stats.rs`.
- **Status**: Completed (Implemented all regr_* functions and corr as SQLite aggregate UDFs).
- **Metric**: Statistical functions return correct results matching PostgreSQL behavior.
```

## References

- PostgreSQL Documentation: https://www.postgresql.org/docs/current/functions-aggregate.html
- SQLite Custom Functions: https://docs.rs/rusqlite/latest/rusqlite/functions/index.html
- Linear Regression Formulas: https://en.wikipedia.org/wiki/Simple_linear_regression
