//! Statistical aggregate functions for PostgreSQL compatibility
//!
//! This module implements PostgreSQL statistical aggregate functions that
//! SQLite doesn't natively support, including:
//! - stddev_pop (population standard deviation)
//! - stddev_samp (sample standard deviation)
//! - var_pop (population variance)
//! - var_samp (sample variance)
//! - regr_* (linear regression functions)
//! - corr (correlation coefficient)
//! - covar_pop/covar_samp (covariance functions)

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

// ============================================================================
// Linear Regression Aggregates (regr_*, corr, covar_*)
// ============================================================================

/// State for linear regression aggregate functions.
/// Accumulates the necessary sums to compute regression statistics.
#[derive(Debug, Clone, Default)]
struct RegrState {
    count: i64,
    sum_x: f64,
    sum_y: f64,
    sum_x2: f64, // sum of X*X
    sum_y2: f64, // sum of Y*Y
    sum_xy: f64, // sum of X*Y
}

impl RegrState {
    fn new() -> Self {
        Self::default()
    }

    /// Add a pair of values (Y, X) to the running totals.
    /// Returns true if both values were non-null and added.
    fn add(&mut self, y: Option<f64>, x: Option<f64>) -> bool {
        match (x, y) {
            (Some(x), Some(y)) => {
                self.count += 1;
                self.sum_x += x;
                self.sum_y += y;
                self.sum_x2 += x * x;
                self.sum_y2 += y * y;
                self.sum_xy += x * y;
                true
            }
            _ => false,
        }
    }

    /// Count of rows where both X and Y are non-null.
    fn count(&self) -> i64 {
        self.count
    }

    /// Average of X values (regr_avgx).
    fn avg_x(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum_x / self.count as f64)
        } else {
            None
        }
    }

    /// Average of Y values (regr_avgy).
    fn avg_y(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum_y / self.count as f64)
        } else {
            None
        }
    }

    /// Sum of squares of deviations for X: sum((X - avg_x)^2)
    /// Computed as: sum_x2 - sum_x^2 / N
    fn sxx(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum_x2 - (self.sum_x * self.sum_x) / self.count as f64)
        } else {
            None
        }
    }

    /// Sum of squares of deviations for Y: sum((Y - avg_y)^2)
    /// Computed as: sum_y2 - sum_y^2 / N
    fn syy(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum_y2 - (self.sum_y * self.sum_y) / self.count as f64)
        } else {
            None
        }
    }

    /// Sum of products of deviations: sum((X - avg_x) * (Y - avg_y))
    /// Computed as: sum_xy - sum_x * sum_y / N
    fn sxy(&self) -> Option<f64> {
        if self.count > 0 {
            Some(self.sum_xy - (self.sum_x * self.sum_y) / self.count as f64)
        } else {
            None
        }
    }

    /// Slope of the least-squares-fit linear equation: sxy / sxx
    /// Returns NULL if sxx is zero (vertical line).
    fn slope(&self) -> Option<f64> {
        let sxx = self.sxx()?;
        if sxx.abs() > f64::EPSILON {
            self.sxy().map(|sxy| sxy / sxx)
        } else {
            None
        }
    }

    /// Y-intercept of the least-squares-fit linear equation: avg_y - slope * avg_x
    /// Returns NULL if slope is NULL.
    fn intercept(&self) -> Option<f64> {
        let slope = self.slope()?;
        let avg_x = self.avg_x()?;
        let avg_y = self.avg_y()?;
        Some(avg_y - slope * avg_x)
    }

    /// Square of the correlation coefficient: (sxy)^2 / (sxx * syy)
    /// Returns NULL if either sxx or syy is zero.
    fn r2(&self) -> Option<f64> {
        let sxx = self.sxx()?;
        let syy = self.syy()?;
        let sxy = self.sxy()?;

        if sxx.abs() > f64::EPSILON && syy.abs() > f64::EPSILON {
            Some((sxy * sxy) / (sxx * syy))
        } else {
            None
        }
    }

    /// Correlation coefficient: sxy / sqrt(sxx * syy)
    /// Returns NULL if either sxx or syy is zero.
    fn corr(&self) -> Option<f64> {
        let sxx = self.sxx()?;
        let syy = self.syy()?;
        let sxy = self.sxy()?;

        let denom = sxx * syy;
        if denom.abs() > f64::EPSILON {
            Some(sxy / denom.sqrt())
        } else {
            None
        }
    }

    /// Population covariance: sxy / N
    fn covar_pop(&self) -> Option<f64> {
        if self.count > 0 {
            self.sxy().map(|sxy| sxy / self.count as f64)
        } else {
            None
        }
    }

    /// Sample covariance: sxy / (N - 1)
    fn covar_samp(&self) -> Option<f64> {
        if self.count > 1 {
            self.sxy().map(|sxy| sxy / (self.count - 1) as f64)
        } else {
            None
        }
    }
}

/// Aggregate for regr_count(Y, X) - count of rows where both are non-null
struct RegrCount;

impl Aggregate<RegrState, Option<i64>> for RegrCount {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<i64>> {
        Ok(acc.map(|s| s.count()))
    }
}

/// Aggregate for regr_sxx(Y, X) - sum of squares of X deviations
struct RegrSxx;

impl Aggregate<RegrState, Option<f64>> for RegrSxx {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.sxx()))
    }
}

/// Aggregate for regr_syy(Y, X) - sum of squares of Y deviations
struct RegrSyy;

impl Aggregate<RegrState, Option<f64>> for RegrSyy {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.syy()))
    }
}

/// Aggregate for regr_sxy(Y, X) - sum of products of X and Y deviations
struct RegrSxy;

impl Aggregate<RegrState, Option<f64>> for RegrSxy {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.sxy()))
    }
}

/// Aggregate for regr_avgx(Y, X) - average of X values
struct RegrAvgx;

impl Aggregate<RegrState, Option<f64>> for RegrAvgx {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.avg_x()))
    }
}

/// Aggregate for regr_avgy(Y, X) - average of Y values
struct RegrAvgy;

impl Aggregate<RegrState, Option<f64>> for RegrAvgy {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.avg_y()))
    }
}

/// Aggregate for regr_slope(Y, X) - slope of least-squares fit
struct RegrSlope;

impl Aggregate<RegrState, Option<f64>> for RegrSlope {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.slope()))
    }
}

/// Aggregate for regr_intercept(Y, X) - y-intercept of least-squares fit
struct RegrIntercept;

impl Aggregate<RegrState, Option<f64>> for RegrIntercept {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.intercept()))
    }
}

/// Aggregate for regr_r2(Y, X) - square of correlation coefficient
struct RegrR2;

impl Aggregate<RegrState, Option<f64>> for RegrR2 {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.r2()))
    }
}

/// Aggregate for corr(Y, X) - correlation coefficient
struct Corr;

impl Aggregate<RegrState, Option<f64>> for Corr {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.corr()))
    }
}

/// Aggregate for covar_pop(Y, X) - population covariance
struct CovarPop;

impl Aggregate<RegrState, Option<f64>> for CovarPop {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.covar_pop()))
    }
}

/// Aggregate for covar_samp(Y, X) - sample covariance
struct CovarSamp;

impl Aggregate<RegrState, Option<f64>> for CovarSamp {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<RegrState> {
        Ok(RegrState::new())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut RegrState) -> Result<()> {
        let y: Option<f64> = ctx.get(0)?;
        let x: Option<f64> = ctx.get(1)?;
        acc.add(y, x);
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<RegrState>) -> Result<Option<f64>> {
        Ok(acc.and_then(|s| s.covar_samp()))
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

    // Linear regression functions
    conn.create_aggregate_function("regr_count", 2, flags, RegrCount)?;
    conn.create_aggregate_function("regr_sxx", 2, flags, RegrSxx)?;
    conn.create_aggregate_function("regr_syy", 2, flags, RegrSyy)?;
    conn.create_aggregate_function("regr_sxy", 2, flags, RegrSxy)?;
    conn.create_aggregate_function("regr_avgx", 2, flags, RegrAvgx)?;
    conn.create_aggregate_function("regr_avgy", 2, flags, RegrAvgy)?;
    conn.create_aggregate_function("regr_slope", 2, flags, RegrSlope)?;
    conn.create_aggregate_function("regr_intercept", 2, flags, RegrIntercept)?;
    conn.create_aggregate_function("regr_r2", 2, flags, RegrR2)?;

    // Correlation
    conn.create_aggregate_function("corr", 2, flags, Corr)?;

    // Covariance
    conn.create_aggregate_function("covar_pop", 2, flags, CovarPop)?;
    conn.create_aggregate_function("covar_samp", 2, flags, CovarSamp)?;

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

    // ========================================================================
    // Linear Regression Tests (regr_*, corr, covar_*)
    // ========================================================================

    fn setup_regression_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_statistical_functions(&conn).unwrap();
        conn.execute("CREATE TABLE data (x REAL, y REAL)", []).unwrap();
        conn
    }

    #[test]
    fn test_regr_count() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4), (NULL, 5), (3, NULL)", [])
            .unwrap();

        // Only rows where both X and Y are non-null should be counted
        let result: i64 = conn
            .query_row("SELECT regr_count(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, 4, "Expected 4 non-null pairs, got {}", result);
    }

    #[test]
    fn test_regr_count_empty() {
        let conn = setup_regression_db();
        let result: Option<i64> = conn
            .query_row("SELECT regr_count(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!(result.is_none(), "Expected NULL for empty table");
    }

    #[test]
    fn test_regr_avgx_avgy() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 4), (3, 6)", [])
            .unwrap();

        let avg_x: f64 = conn
            .query_row("SELECT regr_avgx(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((avg_x - 2.0).abs() < 0.0001, "Expected avg_x = 2.0, got {}", avg_x);

        let avg_y: f64 = conn
            .query_row("SELECT regr_avgy(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((avg_y - 4.0).abs() < 0.0001, "Expected avg_y = 4.0, got {}", avg_y);
    }

    #[test]
    fn test_regr_sxx() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // X values: 1, 2, 3, 4 -> mean = 2.5
        // sxx = sum((X - mean)^2) = (1-2.5)^2 + (2-2.5)^2 + (3-2.5)^2 + (4-2.5)^2
        //     = 2.25 + 0.25 + 0.25 + 2.25 = 5.0
        // Or: sum_x2 - sum_x^2/N = (1+4+9+16) - 100/4 = 30 - 25 = 5.0
        let result: f64 = conn
            .query_row("SELECT regr_sxx(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 5.0).abs() < 0.0001, "Expected sxx = 5.0, got {}", result);
    }

    #[test]
    fn test_regr_syy() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // Y values: 2, 3, 5, 4 -> mean = 3.5
        // syy = sum((Y - mean)^2) = (2-3.5)^2 + (3-3.5)^2 + (5-3.5)^2 + (4-3.5)^2
        //     = 2.25 + 0.25 + 2.25 + 0.25 = 5.0
        let result: f64 = conn
            .query_row("SELECT regr_syy(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 5.0).abs() < 0.0001, "Expected syy = 5.0, got {}", result);
    }

    #[test]
    fn test_regr_sxy() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // X: 1, 2, 3, 4 -> mean_x = 2.5
        // Y: 2, 3, 5, 4 -> mean_y = 3.5
        // sxy = sum((X - mean_x) * (Y - mean_y))
        //     = (-1.5)(-1.5) + (-0.5)(-0.5) + (0.5)(1.5) + (1.5)(0.5)
        //     = 2.25 + 0.25 + 0.75 + 0.75 = 4.0
        let result: f64 = conn
            .query_row("SELECT regr_sxy(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 4.0).abs() < 0.0001, "Expected sxy = 4.0, got {}", result);
    }

    #[test]
    fn test_regr_slope() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // slope = sxy / sxx = 4.0 / 5.0 = 0.8
        let result: f64 = conn
            .query_row("SELECT regr_slope(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 0.8).abs() < 0.0001, "Expected slope = 0.8, got {}", result);
    }

    #[test]
    fn test_regr_intercept() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // intercept = avg_y - slope * avg_x = 3.5 - 0.8 * 2.5 = 3.5 - 2.0 = 1.5
        let result: f64 = conn
            .query_row("SELECT regr_intercept(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 1.5).abs() < 0.0001, "Expected intercept = 1.5, got {}", result);
    }

    #[test]
    fn test_corr_perfect_positive() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 4), (3, 6)", [])
            .unwrap();

        // Perfect positive correlation: corr = 1.0
        let result: f64 = conn
            .query_row("SELECT corr(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 1.0).abs() < 0.0001, "Expected corr = 1.0, got {}", result);
    }

    #[test]
    fn test_corr_perfect_negative() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 6), (2, 4), (3, 2)", [])
            .unwrap();

        // Perfect negative correlation: corr = -1.0
        let result: f64 = conn
            .query_row("SELECT corr(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - (-1.0)).abs() < 0.0001, "Expected corr = -1.0, got {}", result);
    }

    #[test]
    fn test_corr_general() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // corr = sxy / sqrt(sxx * syy) = 4.0 / sqrt(5.0 * 5.0) = 4.0 / 5.0 = 0.8
        let result: f64 = conn
            .query_row("SELECT corr(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 0.8).abs() < 0.0001, "Expected corr = 0.8, got {}", result);
    }

    #[test]
    fn test_regr_r2() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // r2 = (sxy)^2 / (sxx * syy) = 16 / 25 = 0.64
        let result: f64 = conn
            .query_row("SELECT regr_r2(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 0.64).abs() < 0.0001, "Expected r2 = 0.64, got {}", result);
    }

    #[test]
    fn test_regr_r2_perfect() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 4), (3, 6)", [])
            .unwrap();

        // Perfect correlation: r2 = 1.0
        let result: f64 = conn
            .query_row("SELECT regr_r2(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 1.0).abs() < 0.0001, "Expected r2 = 1.0, got {}", result);
    }

    #[test]
    fn test_covar_pop() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // covar_pop = sxy / N = 4.0 / 4 = 1.0
        let result: f64 = conn
            .query_row("SELECT covar_pop(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 1.0).abs() < 0.0001, "Expected covar_pop = 1.0, got {}", result);
    }

    #[test]
    fn test_covar_samp() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)", [])
            .unwrap();

        // covar_samp = sxy / (N-1) = 4.0 / 3 = 1.333...
        let result: f64 = conn
            .query_row("SELECT covar_samp(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 1.3333333).abs() < 0.0001, "Expected covar_samp = 1.333, got {}", result);
    }

    #[test]
    fn test_regr_null_handling() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (NULL, 3), (3, NULL), (4, 5)", [])
            .unwrap();

        // Only (1, 2) and (4, 5) are valid pairs
        // slope = (5-2)/(4-1) = 1.0
        let result: f64 = conn
            .query_row("SELECT regr_slope(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((result - 1.0).abs() < 0.0001, "Expected slope = 1.0, got {}", result);

        let count: i64 = conn
            .query_row("SELECT regr_count(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_regr_single_row() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (5, 10)", []).unwrap();

        // With single row:
        // - sxx = 0 (no variance in X)
        // - slope = NULL (division by zero)
        // - intercept = NULL (depends on slope)
        // - corr = NULL
        // - r2 = NULL
        let slope: Option<f64> = conn
            .query_row("SELECT regr_slope(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!(slope.is_none(), "Expected NULL slope for single row");

        let intercept: Option<f64> = conn
            .query_row("SELECT regr_intercept(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!(intercept.is_none(), "Expected NULL intercept for single row");

        let corr: Option<f64> = conn
            .query_row("SELECT corr(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!(corr.is_none(), "Expected NULL corr for single row");

        // But avgx and avgy should work
        let avgx: f64 = conn
            .query_row("SELECT regr_avgx(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((avgx - 5.0).abs() < 0.0001);

        let avgy: f64 = conn
            .query_row("SELECT regr_avgy(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!((avgy - 10.0).abs() < 0.0001);
    }

    #[test]
    fn test_regr_constant_x() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 2), (1, 3), (1, 4)", [])
            .unwrap();

        // All X values are the same -> sxx = 0 -> slope is NULL
        let slope: Option<f64> = conn
            .query_row("SELECT regr_slope(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!(slope.is_none(), "Expected NULL slope when all X values are identical");
    }

    #[test]
    fn test_regr_constant_y() {
        let conn = setup_regression_db();
        conn.execute("INSERT INTO data VALUES (1, 5), (2, 5), (3, 5)", [])
            .unwrap();

        // All Y values are the same -> syy = 0 -> corr is NULL
        let corr: Option<f64> = conn
            .query_row("SELECT corr(y, x) FROM data", [], |r| r.get(0))
            .unwrap();
        assert!(corr.is_none(), "Expected NULL corr when all Y values are identical");
    }
}
