//! Aggregate function zero-argument tests
//!
//! Tests for PostgreSQL aggregate functions called with zero arguments.
//! PostgreSQL allows these and returns NULL (except count() which returns 0).

use pgqt::transpiler::transpile;

// ============================================================================
// Basic zero-argument aggregate tests
// ============================================================================

#[test]
fn test_max_zero_args() {
    let sql = "SELECT max()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for max() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for max(): {}", result);
}

#[test]
fn test_min_zero_args() {
    let sql = "SELECT min()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for min() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for min(): {}", result);
}

#[test]
fn test_sum_zero_args() {
    let sql = "SELECT sum()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for sum() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for sum(): {}", result);
}

#[test]
fn test_count_zero_args() {
    let sql = "SELECT count()";
    let result = transpile(sql);
    // PostgreSQL returns 0 for count() with no arguments
    assert!(result.contains("0"), "Expected 0 for count(): {}", result);
}

#[test]
fn test_avg_zero_args() {
    let sql = "SELECT avg()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for avg() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for avg(): {}", result);
}

// ============================================================================
// Standard deviation and variance tests
// ============================================================================

#[test]
fn test_stddev_zero_args() {
    let sql = "SELECT stddev()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for stddev() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for stddev(): {}", result);
}

#[test]
fn test_stddev_samp_zero_args() {
    let sql = "SELECT stddev_samp()";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for stddev_samp(): {}", result);
}

#[test]
fn test_stddev_pop_zero_args() {
    let sql = "SELECT stddev_pop()";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for stddev_pop(): {}", result);
}

#[test]
fn test_variance_zero_args() {
    let sql = "SELECT variance()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for variance() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for variance(): {}", result);
}

#[test]
fn test_var_samp_zero_args() {
    let sql = "SELECT var_samp()";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for var_samp(): {}", result);
}

#[test]
fn test_var_pop_zero_args() {
    let sql = "SELECT var_pop()";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for var_pop(): {}", result);
}

// ============================================================================
// Boolean aggregate tests
// ============================================================================

#[test]
fn test_bool_and_zero_args() {
    let sql = "SELECT bool_and()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for bool_and() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for bool_and(): {}", result);
}

#[test]
fn test_bool_or_zero_args() {
    let sql = "SELECT bool_or()";
    let result = transpile(sql);
    // PostgreSQL returns NULL for bool_or() with no arguments
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for bool_or(): {}", result);
}

// ============================================================================
// Context tests
// ============================================================================

#[test]
fn test_max_zero_args_from_subquery() {
    let sql = "SELECT max() FROM (SELECT 1 AS a, 2 AS b) AS v";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for max() in subquery: {}", result);
}

#[test]
fn test_aggregate_zero_args_in_group_by() {
    let sql = "SELECT p, sum() FROM (VALUES (1), (2)) AS v(p) GROUP BY p";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("NULL"), "Expected NULL for sum() in GROUP BY: {}", result);
}

// ============================================================================
// Verify normal aggregates still work
// ============================================================================

#[test]
fn test_max_with_args() {
    let sql = "SELECT max(x) FROM t";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("MAX"), "Expected MAX: {}", result);
    assert!(result.contains("x"), "Expected x: {}", result);
}

#[test]
fn test_count_with_args() {
    let sql = "SELECT count(*) FROM t";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("COUNT"), "Expected COUNT: {}", result);
}

#[test]
fn test_sum_with_args() {
    let sql = "SELECT sum(x) FROM t";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("SUM"), "Expected SUM: {}", result);
}
