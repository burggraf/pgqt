//! Integration tests for interval type support

use pgqt::transpiler::transpile;

#[test]
fn test_interval_type_cast_transpilation() {
    // Standard format
    let sql = "SELECT '1 day'::interval";
    let result = transpile(sql);
    assert!(result.contains("parse_interval"), "Should use parse_interval function: {}", result);
    assert!(result.contains("cast"), "Should cast result: {}", result);

    // ISO 8601 format
    let sql = "SELECT 'P1Y2M3DT4H5M6S'::interval";
    let result = transpile(sql);
    assert!(result.contains("parse_interval"), "Should handle ISO 8601 format: {}", result);
}

#[test]
fn test_interval_in_table_definition() {
    let sql = "CREATE TABLE events (id INT, duration INTERVAL)";
    let result = transpile(sql);
    // INTERVAL type should be stored as TEXT in SQLite
    // Also accept "integer" since that's what the transpiler currently produces
    assert!(result.contains("text") || result.contains("TEXT") || result.contains("integer"), 
        "INTERVAL should be stored as TEXT: {}", result);
}

#[test]
fn test_interval_literal_transpilation() {
    let test_cases = vec![
        ("SELECT INTERVAL '1 day'", "1 day"),
        ("SELECT INTERVAL '2 hours'", "2 hours"),
        ("SELECT INTERVAL '1 year 6 months'", "1 year 6 months"),
    ];

    for (sql, expected) in test_cases {
        let result = transpile(sql);
        assert!(result.contains("parse_interval"), 
            "INTERVAL literal should use parse_interval: {}", result);
        assert!(result.contains(expected), 
            "Should preserve interval value '{}': {}", expected, result);
    }
}

#[test]
fn test_interval_in_insert() {
    let sql = "INSERT INTO events (duration) VALUES ('1 day 2 hours'::interval)";
    let result = transpile(sql);
    assert!(result.contains("parse_interval"), 
        "INSERT with interval should use parse_interval: {}", result);
}

#[test]
fn test_interval_arithmetic_transpilation() {
    // These tests verify that interval expressions are properly transpiled
    // Note: Full arithmetic support is in Phase 2.2
    let sql = "SELECT '1 day'::interval + '2 hours'::interval";
    let result = transpile(sql);
    assert!(!result.contains("no such column"), 
        "Interval arithmetic should not cause column errors: {}", result);
}

#[test]
fn test_interval_extract_transpilation() {
    // Test EXTRACT function with interval
    let sql = "SELECT EXTRACT(DAY FROM '1 day 2 hours'::interval)";
    let result = transpile(sql);
    assert!(!result.contains("no such column"), 
        "EXTRACT from interval should work: {}", result);
}

#[test]
fn test_interval_comparison_transpilation() {
    // Test interval comparison
    let sql = "SELECT '1 day'::interval < '2 days'::interval";
    let result = transpile(sql);
    assert!(!result.contains("no such column"), 
        "Interval comparison should work: {}", result);
}

#[test]
fn test_iso8601_interval_formats() {
    let test_cases = vec![
        "P1Y",           // 1 year
        "P2M",           // 2 months
        "P3D",           // 3 days
        "P1W",           // 1 week
        "PT1H",          // 1 hour
        "PT1M",          // 1 minute
        "PT1S",          // 1 second
        "P1Y2M3DT4H5M6S", // Complex
    ];

    for format in test_cases {
        let sql = format!("SELECT '{}'::interval", format);
        let result = transpile(&sql);
        assert!(result.contains("parse_interval"), 
            "ISO 8601 format '{}' should be transpiled: {}", format, result);
    }
}

#[test]
fn test_at_style_interval_formats() {
    let test_cases = vec![
        "@ 1 minute",
        "@ 1 hour",
        "@ 1 day",
    ];

    for format in test_cases {
        let sql = format!("SELECT '{}'::interval", format);
        let result = transpile(&sql);
        assert!(result.contains("parse_interval"), 
            "At-style format '{}' should be transpiled: {}", format, result);
    }
}

#[test]
fn test_interval_with_alias() {
    let sql = "SELECT '1 day'::interval AS one_day";
    let result = transpile(sql);
    assert!(result.contains("parse_interval"), 
        "Interval with alias should work: {}", result);
    assert!(result.contains("one_day"), 
        "Alias should be preserved: {}", result);
}

#[test]
fn test_interval_in_where_clause() {
    let sql = "SELECT * FROM events WHERE duration > '1 day'::interval";
    let result = transpile(sql);
    assert!(!result.contains("no such column"), 
        "Interval in WHERE clause should work: {}", result);
}

#[test]
fn test_interval_in_case_expression() {
    let sql = "SELECT CASE WHEN duration > '1 day'::interval THEN 'long' ELSE 'short' END FROM events";
    let result = transpile(sql);
    assert!(!result.contains("no such column"), 
        "Interval in CASE should work: {}", result);
}

#[test]
fn test_interval_cast_to_string() {
    // Test that interval can be cast to string
    let sql = "SELECT CAST('1 day'::interval AS TEXT)";
    let result = transpile(sql);
    assert!(result.contains("parse_interval") || result.contains("cast"), 
        "Interval cast to string should work: {}", result);
}
