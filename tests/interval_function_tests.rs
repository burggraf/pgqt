//! Interval function tests
//!
//! Tests for PostgreSQL interval functions:
//! - make_interval()
//! - justify_interval()
//! - justify_days()
//! - justify_hours()

use pgqt::transpiler::transpile;

// ============================================================================
// make_interval() tests
// ============================================================================

#[test]
fn test_make_interval_basic() {
    let sql = "SELECT make_interval(days => 1)";
    let result = transpile(sql);
    // Should construct an interval string
    assert!(!result.is_empty());
    // The result should contain a function call
    assert!(result.contains("make_interval") || result.contains("format"));
}

#[test]
fn test_make_interval_all_params() {
    let sql = "SELECT make_interval(1, 2, 3, 4, 5, 6, 7.5)";
    let result = transpile(sql);
    // years=1, months=2, weeks=3, days=4, hours=5, mins=6, secs=7.5
    assert!(!result.is_empty());
}

#[test]
fn test_make_interval_named_params() {
    let sql = "SELECT make_interval(years := 1, months := 2, days := 3)";
    let result = transpile(sql);
    assert!(!result.is_empty());
}

#[test]
fn test_make_interval_zero() {
    let sql = "SELECT make_interval(0, 0, 0, 0, 0, 0, 0)";
    let result = transpile(sql);
    assert!(!result.is_empty());
}

// ============================================================================
// justify_interval() tests
// ============================================================================

#[test]
fn test_justify_interval() {
    let sql = "SELECT justify_interval('1 month -30 days'::interval)";
    let result = transpile(sql);
    assert!(!result.is_empty());
    // Should contain justify_interval function call
    assert!(result.contains("justify_interval"));
}

#[test]
fn test_justify_interval_negative() {
    let sql = "SELECT justify_interval('-1 month 30 days'::interval)";
    let result = transpile(sql);
    assert!(!result.is_empty());
}

// ============================================================================
// justify_days() tests
// ============================================================================

#[test]
fn test_justify_days() {
    let sql = "SELECT justify_days('35 days'::interval)";
    let result = transpile(sql);
    // Should convert 35 days to 1 month 5 days
    assert!(!result.is_empty());
    assert!(result.contains("justify_days"));
}

#[test]
fn test_justify_days_negative() {
    let sql = "SELECT justify_days('-35 days'::interval)";
    let result = transpile(sql);
    assert!(!result.is_empty());
}

// ============================================================================
// justify_hours() tests
// ============================================================================

#[test]
fn test_justify_hours() {
    let sql = "SELECT justify_hours('27 hours'::interval)";
    let result = transpile(sql);
    // Should convert 27 hours to 1 day 3 hours
    assert!(!result.is_empty());
    assert!(result.contains("justify_hours"));
}

#[test]
fn test_justify_hours_negative() {
    let sql = "SELECT justify_hours('-27 hours'::interval)";
    let result = transpile(sql);
    assert!(!result.is_empty());
}

// ============================================================================
// Combined/Integration tests
// ============================================================================

#[test]
fn test_interval_functions_in_select() {
    let sql = "SELECT make_interval(1, 0, 0, 0, 0, 0, 0) as one_year";
    let result = transpile(sql);
    assert!(!result.is_empty());
}

#[test]
fn test_interval_functions_in_where() {
    let sql = "SELECT * FROM t WHERE justify_days(interval_col) > '30 days'";
    let result = transpile(sql);
    assert!(!result.is_empty());
}
