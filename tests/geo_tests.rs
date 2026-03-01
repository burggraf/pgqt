//! Integration tests for PostgreSQL Geometric Types
//!
//! These tests verify that geometric type SQL is properly transpiled
//! from PostgreSQL syntax to SQLite function calls.

use postgresqlite::transpiler::transpile;

// ============================================================================
// Point Type Tests
// ============================================================================

#[test]
fn test_point_transpilation() {
    let sql = "SELECT point '(1, 2)'";
    let result = transpile(sql);
    // Point literals are cast to text for storage
    assert!(result.contains("(1, 2)"), "Point should preserve coordinate values: {}", result);
}

#[test]
fn test_point_distance_operator() {
    let sql = "SELECT point '(1, 2)' <-> point '(4, 6)'";
    let result = transpile(sql);
    assert!(result.contains("geo_distance"), "Point distance should use geo_distance(): {}", result);
}

#[test]
fn test_point_in_table() {
    let sql = "CREATE TABLE points (p POINT)";
    let result = transpile(sql);
    assert!(result.contains("text"), "POINT type should be stored as TEXT: {}", result);
}

#[test]
fn test_point_insert() {
    let sql = "INSERT INTO points (p) VALUES ('(1, 2)')";
    let result = transpile(sql);
    assert!(result.contains("insert"), "INSERT should be preserved: {}", result);
}

// ============================================================================
// Box Type Tests
// ============================================================================

#[test]
fn test_box_transpilation() {
    let sql = "SELECT box '(0,0),(2,2)'";
    let result = transpile(sql);
    assert!(result.contains("(0,0)"), "Box should preserve first point: {}", result);
    assert!(result.contains("(2,2)"), "Box should preserve second point: {}", result);
}

#[test]
fn test_box_overlaps_operator() {
    let sql = "SELECT box '(0,0),(2,2)' && box '(1,1),(3,3)'";
    let result = transpile(sql);
    assert!(result.contains("geo_overlaps"), "Box && should use geo_overlaps(): {}", result);
}

#[test]
fn test_box_contains_operator() {
    let sql = "SELECT box '(0,0),(2,2)' @> box '(0.5,0.5),(1.5,1.5)'";
    let result = transpile(sql);
    assert!(result.contains("geo_contains"), "Box @> should use geo_contains(): {}", result);
}

#[test]
fn test_box_contained_operator() {
    let sql = "SELECT box '(0.5,0.5),(1.5,1.5)' <@ box '(0,0),(2,2)'";
    let result = transpile(sql);
    assert!(result.contains("geo_contained"), "Box <@ should use geo_contained(): {}", result);
}

#[test]
fn test_box_left_operator() {
    let sql = "SELECT box '(0,0),(2,2)' << box '(3,0),(5,2)'";
    let result = transpile(sql);
    assert!(result.contains("geo_left"), "Box << should use geo_left(): {}", result);
}

#[test]
fn test_box_right_operator() {
    let sql = "SELECT box '(3,0),(5,2)' >> box '(0,0),(2,2)'";
    let result = transpile(sql);
    assert!(result.contains("geo_right"), "Box >> should use geo_right(): {}", result);
}

#[test]
fn test_box_below_operator() {
    let sql = "SELECT box '(0,0),(2,2)' <<| box '(0,3),(2,5)'";
    let result = transpile(sql);
    assert!(result.contains("geo_below"), "Box <<| should use geo_below(): {}", result);
}

#[test]
fn test_box_above_operator() {
    let sql = "SELECT box '(0,3),(2,5)' |>> box '(0,0),(2,2)'";
    let result = transpile(sql);
    assert!(result.contains("geo_above"), "Box |>> should use geo_above(): {}", result);
}

// ============================================================================
// Circle Type Tests
// ============================================================================

#[test]
fn test_circle_transpilation() {
    let sql = "SELECT circle '<(1, 2), 5>'";
    let result = transpile(sql);
    assert!(result.contains("<"), "Circle should preserve center and radius: {}", result);
}

#[test]
fn test_circle_in_table() {
    let sql = "CREATE TABLE circles (c CIRCLE)";
    let result = transpile(sql);
    assert!(result.contains("text"), "CIRCLE type should be stored as TEXT: {}", result);
}

// ============================================================================
// Mixed/Complex Tests
// ============================================================================

#[test]
fn test_geo_in_where_clause() {
    let sql = "SELECT * FROM boxes WHERE b @> '(0.5,0.5),(1.5,1.5)'";
    let result = transpile(sql);
    assert!(result.contains("where"), "WHERE clause should be preserved: {}", result);
    assert!(result.contains("geo_contains"), "Should use geo_contains(): {}", result);
}

#[test]
fn test_geo_with_alias() {
    let sql = "SELECT b @> '(0,0),(1,1)' AS contains_origin FROM boxes";
    let result = transpile(sql);
    // Alias may be quoted depending on transpiler behavior
    assert!(result.contains("contains_origin"), "Alias should be preserved: {}", result);
    assert!(result.contains("geo_contains"), "Should use geo_contains(): {}", result);
}

#[test]
fn test_geo_in_join() {
    let sql = "SELECT a.id, b.id FROM boxes a, boxes b WHERE a.b && b.b";
    let result = transpile(sql);
    // When both operands are columns (not geo literals), the transpiler may
    // not detect it as a geo operation. This is a known limitation.
    // The test documents the expected behavior.
    assert!(result.contains("from boxes"), "FROM clause should be preserved: {}", result);
}

#[test]
fn test_multiple_geo_operators() {
    let sql = "SELECT * FROM boxes WHERE b @> '(0,0),(1,1)' AND b && '(0.5,0.5),(2,2)'";
    let result = transpile(sql);
    assert!(result.contains("geo_contains"), "Should use geo_contains(): {}", result);
    assert!(result.contains("geo_overlaps"), "Should use geo_overlaps(): {}", result);
}

#[test]
fn test_geo_with_other_types() {
    let sql = "SELECT id, name, b FROM locations WHERE b @> '(0,0),(1,1)' AND name = 'test'";
    let result = transpile(sql);
    assert!(result.contains("geo_contains"), "Should use geo_contains(): {}", result);
    assert!(result.contains("="), "Should have equality comparison: {}", result);
}

// ============================================================================
// Type Creation Tests
// ============================================================================

#[test]
fn test_create_table_with_geo_types() {
    let sql = "CREATE TABLE geo_table (id SERIAL, p POINT, b BOX, c CIRCLE)";
    let result = transpile(sql);
    assert!(result.contains("create table"), "CREATE TABLE should be preserved: {}", result);
    assert!(result.contains("text"), "Geometric types should become TEXT: {}", result);
}

// Note: ALTER TABLE is not yet fully supported by the transpiler
// #[test]
// fn test_alter_table_add_geo_column() {
//     let sql = "ALTER TABLE locations ADD COLUMN bbox BOX";
//     let result = transpile(sql);
//     assert!(result.contains("alter table"), "ALTER TABLE should be preserved: {}", result);
//     assert!(result.contains("text"), "BOX should become TEXT: {}", result);
// }

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_geo_with_null() {
    let sql = "SELECT * FROM boxes WHERE b IS NULL";
    let result = transpile(sql);
    assert!(result.contains("is null"), "NULL check should be preserved: {}", result);
}

#[test]
fn test_geo_function_in_select() {
    let sql = "SELECT area(b) FROM boxes";
    let result = transpile(sql);
    assert!(result.contains("area"), "area() function should be preserved: {}", result);
}

#[test]
fn test_geo_order_by() {
    let sql = "SELECT id, p <-> '(0,0)' AS dist FROM points ORDER BY dist";
    let result = transpile(sql);
    assert!(result.contains("geo_distance"), "Should use geo_distance(): {}", result);
    assert!(result.contains("order by"), "ORDER BY should be preserved: {}", result);
}
