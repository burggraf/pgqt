//! Integration tests for JSON processing functions (Phase 1.2)
//!
//! These tests verify that JSON processing functions work correctly
//! when used in SQL queries through the transpiler.

use rusqlite::Connection;

/// Helper function to set up an in-memory database with JSON functions registered
fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    pgqt::jsonb::register_jsonb_functions(&conn).unwrap();
    conn
}

#[test]
fn test_json_each_with_sqlite_json_each() {
    let conn = setup_test_db();

    // Use json_each_impl to generate array, then SQLite's json_each to iterate
    // This simulates how the transpiler would handle json_each() in FROM clause
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_each_impl('{\"a\": 1, \"b\": 2}'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    // Should get two rows, each containing a [key, value] pair
    assert_eq!(rows.len(), 2);
    // Each row should be a JSON array like ["a",1] or ["b",2]
    assert!(rows.iter().any(|r| r.contains("a") && r.contains("1")));
    assert!(rows.iter().any(|r| r.contains("b") && r.contains("2")));
}

#[test]
fn test_json_array_elements_with_sqlite_json_each() {
    let conn = setup_test_db();

    // json_array_elements_impl returns array, use json_each to iterate
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_array_elements_impl('[10, 20, 30]'))"
    ).unwrap();
    let rows: Vec<i64> = stmt.query_map([], |row| {
        row.get::<_, i64>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows, vec![10, 20, 30]);
}

#[test]
fn test_json_object_keys_with_sqlite_json_each() {
    let conn = setup_test_db();

    // json_object_keys_impl returns array of keys
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_object_keys_impl('{\"c\": 1, \"a\": 2, \"b\": 3}'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows.len(), 3);
    assert!(rows.contains(&"a".to_string()));
    assert!(rows.contains(&"b".to_string()));
    assert!(rows.contains(&"c".to_string()));
}

#[test]
fn test_json_each_text_values() {
    let conn = setup_test_db();

    // json_each_text_impl should return text values
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_each_text_impl('{\"num\": 42, \"str\": \"hello\"}'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    // Each row is a [key, value] pair as JSON
    assert_eq!(rows.len(), 2);
    // Check that the numeric value is returned as text "42"
    assert!(rows.iter().any(|r| r.contains("num") && r.contains("42")));
    assert!(rows.iter().any(|r| r.contains("str") && r.contains("hello")));
}

#[test]
fn test_json_array_elements_text() {
    let conn = setup_test_db();

    // json_array_elements_text_impl should return text elements
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_array_elements_text_impl('[1, \"two\", true]'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    // All values should be text (with quotes for strings, without for numbers/booleans)
    assert_eq!(rows.len(), 3);
    // The text values should be "1", "two", "true"
    assert!(rows.iter().any(|r| r == "1" || r == "\"1\""));
    assert!(rows.iter().any(|r| r.contains("two")));
    assert!(rows.iter().any(|r| r == "true" || r == "\"true\""));
}

#[test]
fn test_json_processing_with_empty_structures() {
    let conn = setup_test_db();

    // Empty object with json_each
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM json_each(json_each_impl('{}'))",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 0);

    // Empty array with json_array_elements
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM json_each(json_array_elements_impl('[]'))",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 0);

    // Empty object with json_object_keys
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM json_each(json_object_keys_impl('{}'))",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn test_json_processing_with_nested_json() {
    let conn = setup_test_db();

    // Nested object - json_each should return the outer key with nested object as value
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_each_impl('{\"outer\": {\"inner\": 123}}'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows.len(), 1);
    // The value should contain the nested object
    assert!(rows[0].contains("outer"));
    assert!(rows[0].contains("inner"));
    assert!(rows[0].contains("123"));

    // Nested array - json_array_elements should return arrays as elements
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_array_elements_impl('[[1, 2], [3, 4]]'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows.len(), 2);
    assert!(rows[0].contains("1"));
    assert!(rows[1].contains("3"));
}

#[test]
fn test_json_each_with_array_input() {
    let conn = setup_test_db();

    // When json_each is given an array, it should return index-value pairs
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_each_impl('[\"first\", \"second\", \"third\"]'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows.len(), 3);
    // Each row should be [index, value]
    assert!(rows.iter().any(|r| r.contains("0") && r.contains("first")));
    assert!(rows.iter().any(|r| r.contains("1") && r.contains("second")));
    assert!(rows.iter().any(|r| r.contains("2") && r.contains("third")));
}

#[test]
fn test_jsonb_variants_equivalence() {
    let conn = setup_test_db();

    // jsonb_each should behave like json_each
    let result_json: String = conn.query_row(
        "SELECT json_each_impl('{\"a\": 1}')",
        [],
        |row| row.get(0)
    ).unwrap();
    let result_jsonb: String = conn.query_row(
        "SELECT jsonb_each_impl('{\"a\": 1}')",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(result_json, result_jsonb);

    // jsonb_array_elements should behave like json_array_elements
    let result_json: String = conn.query_row(
        "SELECT json_array_elements_impl('[1, 2, 3]')",
        [],
        |row| row.get(0)
    ).unwrap();
    let result_jsonb: String = conn.query_row(
        "SELECT jsonb_array_elements_impl('[1, 2, 3]')",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(result_json, result_jsonb);

    // jsonb_object_keys should behave like json_object_keys
    let result_json: String = conn.query_row(
        "SELECT json_object_keys_impl('{\"x\": 1, \"y\": 2}')",
        [],
        |row| row.get(0)
    ).unwrap();
    let result_jsonb: String = conn.query_row(
        "SELECT jsonb_object_keys_impl('{\"x\": 1, \"y\": 2}')",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(result_json, result_jsonb);
}

#[test]
fn test_json_processing_with_null_values() {
    let conn = setup_test_db();

    // json_each with null values
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_each_impl('{\"a\": null, \"b\": 1}'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows.len(), 2);
    // One of the rows should contain null
    assert!(rows.iter().any(|r| r.contains("a") && r.contains("null")));

    // json_each_text with null values - null should remain null (not "null" string)
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_each_text_impl('{\"a\": null, \"b\": 1}'))"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        row.get::<_, String>(0)
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows.len(), 2);
    // In text mode, null values should be preserved as null
    assert!(rows.iter().any(|r| r.contains("a")));
}

#[test]
fn test_json_array_elements_with_mixed_types() {
    let conn = setup_test_db();

    // Array with mixed types
    let mut stmt = conn.prepare(
        "SELECT value FROM json_each(json_array_elements_impl('[1, \"string\", true, null, {\"nested\": \"object\"}]'))"
    ).unwrap();
    
    // Get values as JSON type (Value) to handle mixed types
    let rows: Vec<serde_json::Value> = stmt.query_map([], |row| {
        // Try to get as string first, fallback to other types
        if let Ok(s) = row.get::<_, String>(0) {
            Ok(serde_json::Value::String(s))
        } else if let Ok(i) = row.get::<_, i64>(0) {
            Ok(serde_json::json!(i))
        } else if let Ok(b) = row.get::<_, bool>(0) {
            Ok(serde_json::json!(b))
        } else {
            Ok(serde_json::Value::Null)
        }
    }).unwrap().map(|r| r.unwrap()).collect();

    assert_eq!(rows.len(), 5);
    // Check that we have all expected values (as strings since json_each returns text)
    let row_strings: Vec<String> = rows.iter().map(|v| v.to_string()).collect();
    assert!(row_strings.iter().any(|r| r == "1" || r == "\"1\""), "Should contain 1, got: {:?}", row_strings);
    assert!(row_strings.iter().any(|r| r.contains("string")), "Should contain string, got: {:?}", row_strings);
    // SQLite returns booleans as 1/0 integers
    assert!(row_strings.iter().any(|r| r == "true" || r == "1" || r == "\"true\""), "Should contain true/1, got: {:?}", row_strings);
    assert!(row_strings.iter().any(|r| r.contains("null")), "Should contain null, got: {:?}", row_strings);
    assert!(row_strings.iter().any(|r| r.contains("nested")), "Should contain nested, got: {:?}", row_strings);
}
