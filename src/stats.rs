//! Statistical aggregate functions for PostgreSQL compatibility
//!
//! This module implements PostgreSQL statistical aggregate functions that
//! SQLite doesn't natively support, including:
//! - stddev_pop (population standard deviation)
//! - stddev_samp (sample standard deviation)
//! - var_pop (population variance)
//! - var_samp (sample variance)

use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::{Connection, Result};

/// State for variance/standard deviation calculations
/// Uses Welford's online algorithm for numerical stability
#[derive(Debug, Clone)]
struct VarState {
    count: i64,
    mean: f64,
    m2: f64, // Sum of squares of differences from the current mean
}

impl VarState {
    fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            m2: 0.0,
        }
    }

    /// Add a value to the running calculation using Welford's algorithm
    fn add(&mut self, x: f64) {
        self.count += 1;
        let delta = x - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = x - self.mean;
        self.m2 += delta * delta2;
    }

    /// Get the population variance (divides by N)
    fn var_pop(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.m2 / self.count as f64)
        }
    }

    /// Get the sample variance (divides by N-1)
    fn var_samp(&self) -> Option<f64> {
        if self.count <= 1 {
            None
        } else {
            Some(self.m2 / (self.count - 1) as f64)
        }
    }
}

/// Aggregate function for population variance (var_pop)
struct VarPop;

impl Aggregate<VarState, Option<f64>> for VarPop {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<VarState> {
        Ok(VarState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut VarState) -> Result<()> {
        // Handle NULL values by ignoring them
        let val: Option<f64> = ctx.get(0)?;
        if let Some(x) = val {
            acc.add(x);
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<VarState>) -> Result<Option<f64>> {
        match acc {
            Some(state) => Ok(state.var_pop()),
            None => Ok(None),
        }
    }
}

/// Aggregate function for sample variance (var_samp)
struct VarSamp;

impl Aggregate<VarState, Option<f64>> for VarSamp {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<VarState> {
        Ok(VarState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut VarState) -> Result<()> {
        let val: Option<f64> = ctx.get(0)?;
        if let Some(x) = val {
            acc.add(x);
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<VarState>) -> Result<Option<f64>> {
        match acc {
            Some(state) => Ok(state.var_samp()),
            None => Ok(None),
        }
    }
}

/// Aggregate function for population standard deviation (stddev_pop)
struct StddevPop;

impl Aggregate<VarState, Option<f64>> for StddevPop {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<VarState> {
        Ok(VarState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut VarState) -> Result<()> {
        let val: Option<f64> = ctx.get(0)?;
        if let Some(x) = val {
            acc.add(x);
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<VarState>) -> Result<Option<f64>> {
        match acc {
            Some(state) => Ok(state.var_pop().map(|v| v.sqrt())),
            None => Ok(None),
        }
    }
}

/// Aggregate function for sample standard deviation (stddev_samp)
struct StddevSamp;

impl Aggregate<VarState, Option<f64>> for StddevSamp {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<VarState> {
        Ok(VarState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut VarState) -> Result<()> {
        let val: Option<f64> = ctx.get(0)?;
        if let Some(x) = val {
            acc.add(x);
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<VarState>) -> Result<Option<f64>> {
        match acc {
            Some(state) => Ok(state.var_samp().map(|v| v.sqrt())),
            None => Ok(None),
        }
    }
}

/// Register all statistical aggregate functions with the SQLite connection
pub fn register_statistical_functions(conn: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;

    // Population variance
    conn.create_aggregate_function("var_pop", 1, flags, VarPop)?;

    // Sample variance
    conn.create_aggregate_function("var_samp", 1, flags, VarSamp)?;

    // Population standard deviation
    conn.create_aggregate_function("stddev_pop", 1, flags, StddevPop)?;

    // Sample standard deviation
    conn.create_aggregate_function("stddev_samp", 1, flags, StddevSamp)?;

    // Aliases (PostgreSQL compatibility)
    conn.create_aggregate_function("variance", 1, flags, VarSamp)?; // alias for var_samp
    conn.create_aggregate_function("stddev", 1, flags, StddevSamp)?; // alias for stddev_samp

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_statistical_functions(&conn).unwrap();
        conn
    }

    #[test]
    fn test_var_pop() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (2.0), (4.0), (6.0), (8.0)", [])
            .unwrap();

        // Population variance: sum of squared deviations / N
        // Mean = 5.0
        // Deviations: -3, -1, 1, 3
        // Squared: 9, 1, 1, 9
        // Sum = 20
        // var_pop = 20 / 4 = 5.0
        let result: f64 = conn
            .query_row("SELECT var_pop(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!((result - 5.0).abs() < 0.0001, "Expected 5.0, got {}", result);
    }

    #[test]
    fn test_var_samp() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (2.0), (4.0), (6.0), (8.0)", [])
            .unwrap();

        // Sample variance: sum of squared deviations / (N - 1)
        // Mean = 5.0
        // Deviations: -3, -1, 1, 3
        // Squared: 9, 1, 1, 9
        // Sum = 20
        // var_samp = 20 / 3 = 6.666...
        let result: f64 = conn
            .query_row("SELECT var_samp(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!(
            (result - 6.6666667).abs() < 0.0001,
            "Expected ~6.667, got {}",
            result
        );
    }

    #[test]
    fn test_stddev_pop() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (2.0), (4.0), (6.0), (8.0)", [])
            .unwrap();

        // stddev_pop = sqrt(var_pop) = sqrt(5.0)
        let result: f64 = conn
            .query_row("SELECT stddev_pop(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!(
            (result - 2.2360679).abs() < 0.0001,
            "Expected ~2.236, got {}",
            result
        );
    }

    #[test]
    fn test_stddev_samp() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (2.0), (4.0), (6.0), (8.0)", [])
            .unwrap();

        // stddev_samp = sqrt(var_samp) = sqrt(6.666...)
        let result: f64 = conn
            .query_row("SELECT stddev_samp(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!(
            (result - 2.5819889).abs() < 0.0001,
            "Expected ~2.582, got {}",
            result
        );
    }

    #[test]
    fn test_variance_alias() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (2.0), (4.0), (6.0), (8.0)", [])
            .unwrap();

        // variance is an alias for var_samp
        let var_samp: f64 = conn
            .query_row("SELECT var_samp(x) FROM test", [], |r| r.get(0))
            .unwrap();
        let variance: f64 = conn
            .query_row("SELECT variance(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!((var_samp - variance).abs() < 0.0001);
    }

    #[test]
    fn test_stddev_alias() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (2.0), (4.0), (6.0), (8.0)", [])
            .unwrap();

        // stddev is an alias for stddev_samp
        let stddev_samp: f64 = conn
            .query_row("SELECT stddev_samp(x) FROM test", [], |r| r.get(0))
            .unwrap();
        let stddev: f64 = conn
            .query_row("SELECT stddev(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!((stddev_samp - stddev).abs() < 0.0001);
    }

    #[test]
    fn test_null_handling() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute(
            "INSERT INTO test VALUES (2.0), (NULL), (4.0), (NULL), (6.0)",
            [],
        )
        .unwrap();

        // NULL values should be ignored
        let result: f64 = conn
            .query_row("SELECT var_pop(x) FROM test", [], |r| r.get(0))
            .unwrap();
        // Values: 2, 4, 6 -> mean = 4
        // Deviations: -2, 0, 2 -> squared: 4, 0, 4 -> sum = 8
        // var_pop = 8 / 3 = 2.666...
        assert!((result - 2.6666667).abs() < 0.0001);
    }

    #[test]
    fn test_empty_table() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();

        // Empty table should return NULL
        let result: Option<f64> = conn
            .query_row("SELECT var_pop(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_single_row_pop() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (5.0)", []).unwrap();

        // Single row: var_pop should return 0
        let result: f64 = conn
            .query_row("SELECT var_pop(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!(result.abs() < 0.0001, "Expected 0, got {}", result);
    }

    #[test]
    fn test_single_row_samp() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x REAL)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (5.0)", []).unwrap();

        // Single row: var_samp should return NULL (division by N-1 = 0)
        let result: Option<f64> = conn
            .query_row("SELECT var_samp(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_integer_values() {
        let conn = setup_test_db();
        conn.execute("CREATE TABLE test (x INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (2), (3), (4), (5)", [])
            .unwrap();

        // Mean = 3
        // Deviations: -2, -1, 0, 1, 2
        // Squared: 4, 1, 0, 1, 4
        // Sum = 10
        // var_pop = 10 / 5 = 2.0
        let result: f64 = conn
            .query_row("SELECT var_pop(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert!((result - 2.0).abs() < 0.0001);
    }
}
