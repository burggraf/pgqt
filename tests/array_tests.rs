//! Integration tests for PostgreSQL array support
//!
//! Tests array operators and functions through the SQLite handler

use pgqt::transpiler::transpile;

/// Test array overlap operator transpilation
#[test]
fn test_array_overlap_transpile() {
    // Simple overlap check
    let sql = "SELECT ARRAY[1,2,3] && ARRAY[3,4]";
    let result = transpile(sql);
    assert!(result.contains("array_overlap"));
}

/// Test array contains operator transpilation
#[test]
fn test_array_contains_transpile() {
    let sql = "SELECT ARRAY[1,2,3] @> ARRAY[1,2]";
    let result = transpile(sql);
    assert!(result.contains("array_contains"));
}

/// Test array contained by operator transpilation
#[test]
fn test_array_contained_transpile() {
    let sql = "SELECT ARRAY[1,2] <@ ARRAY[1,2,3]";
    let result = transpile(sql);
    assert!(result.contains("array_contained"));
}

/// Test array in WHERE clause
#[test]
fn test_array_in_where_clause() {
    let sql = "SELECT * FROM users WHERE tags @> '[\"admin\"]'";
    let result = transpile(sql);
    assert!(result.contains("array_contains"));
    assert!(result.contains("where"));
}

/// Test multiple array operators
#[test]
fn test_multiple_array_operators() {
    let sql = "SELECT * FROM items WHERE categories && '[1,2]' AND tags @> '[\"featured\"]'";
    let result = transpile(sql);
    assert!(result.contains("array_overlap"));
    assert!(result.contains("array_contains"));
}

/// Test array function calls
#[test]
fn test_array_append_function() {
    // array_append should be passed through
    let sql = "SELECT array_append(tags, 'new_tag') FROM items";
    let result = transpile(sql);
    assert!(result.contains("array_append"));
}

#[test]
fn test_array_prepend_function() {
    let sql = "SELECT array_prepend('first', items) FROM data";
    let result = transpile(sql);
    assert!(result.contains("array_prepend"));
}

#[test]
fn test_array_cat_function() {
    let sql = "SELECT array_cat(arr1, arr2) FROM data";
    let result = transpile(sql);
    assert!(result.contains("array_cat"));
}

#[test]
fn test_array_remove_function() {
    let sql = "SELECT array_remove(tags, 'old') FROM items";
    let result = transpile(sql);
    assert!(result.contains("array_remove"));
}

#[test]
fn test_array_replace_function() {
    let sql = "SELECT array_replace(tags, 'old', 'new') FROM items";
    let result = transpile(sql);
    assert!(result.contains("array_replace"));
}

#[test]
fn test_array_length_function() {
    let sql = "SELECT array_length(arr, 1) FROM data";
    let result = transpile(sql);
    assert!(result.contains("array_length"));
}

#[test]
fn test_cardinality_function() {
    let sql = "SELECT cardinality(arr) FROM data";
    let result = transpile(sql);
    assert!(result.contains("cardinality"));
}

#[test]
fn test_array_position_function() {
    let sql = "SELECT array_position(arr, 'value') FROM data";
    let result = transpile(sql);
    assert!(result.contains("array_position"));
}

#[test]
fn test_array_positions_function() {
    let sql = "SELECT array_positions(arr, 'value') FROM data";
    let result = transpile(sql);
    assert!(result.contains("array_positions"));
}

#[test]
fn test_array_to_string_function() {
    let sql = "SELECT array_to_string(tags, ', ') FROM items";
    let result = transpile(sql);
    assert!(result.contains("array_to_string"));
}

#[test]
fn test_string_to_array_function() {
    let sql = "SELECT string_to_array('a,b,c', ',')";
    let result = transpile(sql);
    assert!(result.contains("string_to_array"));
}

#[test]
fn test_array_dims_function() {
    let sql = "SELECT array_dims(arr) FROM data";
    let result = transpile(sql);
    assert!(result.contains("array_dims"));
}

#[test]
fn test_array_ndims_function() {
    let sql = "SELECT array_ndims(arr) FROM data";
    let result = transpile(sql);
    assert!(result.contains("array_ndims"));
}

#[test]
fn test_array_fill_function() {
    let sql = "SELECT array_fill(0, ARRAY[3,3])";
    let result = transpile(sql);
    assert!(result.contains("array_fill"));
}

#[test]
fn test_trim_array_function() {
    let sql = "SELECT trim_array(arr, 2) FROM data";
    let result = transpile(sql);
    assert!(result.contains("trim_array"));
}

/// Test array in INSERT
#[test]
fn test_array_in_insert() {
    let sql = "INSERT INTO items (name, tags) VALUES ('test', '[\"a\",\"b\"]')";
    let result = transpile(sql);
    assert!(result.contains("insert"));
}

/// Test array in UPDATE
#[test]
fn test_array_in_update() {
    let sql = "UPDATE items SET tags = array_append(tags, 'new') WHERE id = 1";
    let result = transpile(sql);
    assert!(result.contains("update"));
    assert!(result.contains("array_append"));
}

/// Test complex array expressions
#[test]
fn test_complex_array_expression() {
    let sql = "SELECT * FROM items WHERE array_length(tags, 1) > 0 AND tags @> '[\"active\"]'";
    let result = transpile(sql);
    assert!(result.contains("array_length"));
    assert!(result.contains("array_contains"));
}

/// Test nested array functions
#[test]
fn test_nested_array_functions() {
    let sql = "SELECT array_remove(array_append(tags, 'temp'), 'old')";
    let result = transpile(sql);
    assert!(result.contains("array_remove"));
    assert!(result.contains("array_append"));
}

/// Test array with subquery
#[test]
fn test_array_with_subquery() {
    let sql = "SELECT * FROM items WHERE id IN (SELECT unnest(tag_ids) FROM categories)";
    let result = transpile(sql);
    assert!(result.contains("select"));
    assert!(result.contains("in"));
}
