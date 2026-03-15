//! Cast operator tests
//!
//! Tests for PostgreSQL :: cast operator transpilation to SQLite CAST()

use pgqt::transpiler::transpile;

// ============================================================================
// Basic cast tests
// ============================================================================

#[test]
fn test_cast_int() {
    let sql = "SELECT 1::int";
    let result = transpile(sql);
    // Should map to CAST(1 AS INTEGER)
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
    assert!(result.to_uppercase().contains("AS"), "Expected AS in: {}", result);
}

#[test]
fn test_cast_numeric() {
    let sql = "SELECT x::numeric FROM t";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
    assert!(result.to_uppercase().contains("AS"), "Expected AS in: {}", result);
}

#[test]
fn test_cast_float8() {
    let sql = "SELECT g::float8 FROM data";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
}

#[test]
fn test_cast_int4() {
    let sql = "SELECT 7::int4";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
}

#[test]
fn test_cast_varchar() {
    let sql = "SELECT 'hello'::varchar(50)";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
    assert!(result.contains("text"), "Expected text type in: {}", result);
}

#[test]
fn test_cast_in_expression() {
    let sql = "SELECT g::numeric / 2 FROM agg_data";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
}

#[test]
fn test_cast_in_aggregate() {
    let sql = "SELECT sum(g::numeric) FROM agg_data";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
    assert!(result.to_uppercase().contains("SUM"), "Expected SUM in: {}", result);
}

// ============================================================================
// Type mapping tests
// ============================================================================

#[test]
fn test_cast_to_integer() {
    let sql = "SELECT '123'::integer";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
    assert!(result.contains("integer"), "Expected integer type in: {}", result);
}

#[test]
fn test_cast_to_real() {
    let sql = "SELECT '3.14'::real";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
    assert!(result.contains("real"), "Expected real type in: {}", result);
}

#[test]
fn test_cast_to_text() {
    let sql = "SELECT 123::text";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
    assert!(result.contains("text"), "Expected text type in: {}", result);
}

#[test]
fn test_cast_to_boolean() {
    let sql = "SELECT 1::boolean";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
}

// ============================================================================
// Complex cast tests
// ============================================================================

#[test]
fn test_cast_chained() {
    let sql = "SELECT '123'::int::float";
    let result = transpile(sql);
    // Should have nested CAST calls
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
}

#[test]
fn test_cast_in_where_clause() {
    let sql = "SELECT * FROM t WHERE id::text = '123'";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
}

#[test]
fn test_cast_in_join() {
    let sql = "SELECT * FROM a JOIN b ON a.id::int = b.id";
    let result = transpile(sql);
    assert!(result.to_uppercase().contains("CAST"), "Expected CAST in: {}", result);
}
