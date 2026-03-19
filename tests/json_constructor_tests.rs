//! Integration tests for JSON constructor functions
//!
//! These tests verify the PostgreSQL-compatible JSON constructor functions:
//! - to_json(anyelement) - Convert any value to JSON
//! - to_jsonb(anyelement) - Convert any value to JSONB
//! - array_to_json(anyarray) - Convert array to JSON array
//! - json_build_object(VARIADIC "any") - Build JSON object from variadic args
//! - jsonb_build_object(VARIADIC "any") - Build JSONB object from variadic args
//! - json_build_array(VARIADIC "any") - Build JSON array from variadic args
//! - jsonb_build_array(VARIADIC "any") - Build JSONB array from variadic args

use rusqlite::Connection;

fn setup_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    pgqt::jsonb::register_jsonb_functions(&conn).unwrap();
    conn
}

#[test]
fn test_to_json_basic_types() {
    let conn = setup_conn();

    // Test string
    let result: String = conn
        .query_row("SELECT to_json('hello')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "\"hello\"");

    // Test integer
    let result: String = conn
        .query_row("SELECT to_json(42)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "42");

    // Test float
    let result: String = conn
        .query_row("SELECT to_json(3.14)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "3.14");

    // Test null
    let result: String = conn
        .query_row("SELECT to_json(NULL)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "null");
}

#[test]
fn test_to_json_nested_json() {
    let conn = setup_conn();

    // Test that valid JSON strings are parsed and re-serialized
    let result: String = conn
        .query_row("SELECT to_json('[1,2,3]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "[1,2,3]");

    // Test object JSON string
    let result: String = conn
        .query_row("SELECT to_json('{\"a\": 1}')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "{\"a\":1}");
}

#[test]
fn test_to_jsonb_basic_types() {
    let conn = setup_conn();

    // Test string
    let result: String = conn
        .query_row("SELECT to_jsonb('hello')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "\"hello\"");

    // Test integer
    let result: String = conn
        .query_row("SELECT to_jsonb(42)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "42");

    // Test null
    let result: String = conn
        .query_row("SELECT to_jsonb(NULL)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "null");
}

#[test]
fn test_array_to_json() {
    let conn = setup_conn();

    // Test simple array
    let result: String = conn
        .query_row("SELECT array_to_json('[1, 2, 3]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "[1,2,3]");

    // Test string array
    let result: String = conn
        .query_row("SELECT array_to_json('[\"a\", \"b\", \"c\"]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "[\"a\",\"b\",\"c\"]");

    // Test nested array
    let result: String = conn
        .query_row("SELECT array_to_json('[[1, 2], [3, 4]]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "[[1,2],[3,4]]");
}

#[test]
fn test_json_build_object_basic() {
    let conn = setup_conn();

    // Test basic key-value pairs
    let result: String = conn
        .query_row("SELECT json_build_object('a', 1, 'b', 'text')", [], |row| row.get(0))
        .unwrap();

    // Parse and verify
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert_eq!(parsed["b"], "text");
}

#[test]
fn test_json_build_object_empty() {
    let conn = setup_conn();

    // Test empty object
    let result: String = conn
        .query_row("SELECT json_build_object()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "{}");
}

#[test]
fn test_json_build_object_with_null() {
    let conn = setup_conn();

    // Test with null value
    let result: String = conn
        .query_row("SELECT json_build_object('key', NULL)", [], |row| row.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed["key"].is_null());
}

#[test]
fn test_json_build_object_multiple_types() {
    let conn = setup_conn();

    // Test with multiple types
    let result: String = conn
        .query_row(
            "SELECT json_build_object('str', 'hello', 'num', 42, 'float', 3.14, 'bool', 1)",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["str"], "hello");
    assert_eq!(parsed["num"], 42);
    assert_eq!(parsed["float"], 3.14);
    assert_eq!(parsed["bool"], 1);
}

#[test]
fn test_jsonb_build_object() {
    let conn = setup_conn();

    let result: String = conn
        .query_row("SELECT jsonb_build_object('x', 10, 'y', 20)", [], |row| row.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["x"], 10);
    assert_eq!(parsed["y"], 20);
}

#[test]
fn test_json_build_array_basic() {
    let conn = setup_conn();

    // Test with multiple types
    let result: String = conn
        .query_row("SELECT json_build_array(1, 'two', 3.0)", [], |row| row.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed[0], 1);
    assert_eq!(parsed[1], "two");
    assert_eq!(parsed[2], 3.0);
}

#[test]
fn test_json_build_array_empty() {
    let conn = setup_conn();

    // Test empty array
    let result: String = conn
        .query_row("SELECT json_build_array()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "[]");
}

#[test]
fn test_json_build_array_single_element() {
    let conn = setup_conn();

    let result: String = conn
        .query_row("SELECT json_build_array('single')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "[\"single\"]");
}

#[test]
fn test_jsonb_build_array() {
    let conn = setup_conn();

    let result: String = conn
        .query_row("SELECT jsonb_build_array('a', 'b', 'c')", [], |row| row.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed[0], "a");
    assert_eq!(parsed[1], "b");
    assert_eq!(parsed[2], "c");
}

#[test]
fn test_json_build_array_with_null() {
    let conn = setup_conn();

    let result: String = conn
        .query_row("SELECT json_build_array(1, NULL, 3)", [], |row| row.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed[0], 1);
    assert!(parsed[1].is_null());
    assert_eq!(parsed[2], 3);
}

#[test]
fn test_nested_json_build() {
    let conn = setup_conn();

    // Test building nested structures
    let result: String = conn
        .query_row(
            "SELECT json_build_object('arr', json_build_array(1, 2, 3))",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["arr"][0], 1);
    assert_eq!(parsed["arr"][1], 2);
    assert_eq!(parsed["arr"][2], 3);
}

#[test]
fn test_variadic_arities() {
    let conn = setup_conn();

    // Test json_build_object with different arities (must be even)
    let result: String = conn
        .query_row("SELECT json_build_object('a', 1)", [], |row| row.get(0))
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);

    let result: String = conn
        .query_row("SELECT json_build_object('a', 1, 'b', 2, 'c', 3)", [], |row| row.get(0))
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert_eq!(parsed["b"], 2);
    assert_eq!(parsed["c"], 3);

    // Test json_build_array with different arities
    let result: String = conn
        .query_row("SELECT json_build_array(1, 2, 3, 4, 5)", [], |row| row.get(0))
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 5);
}
