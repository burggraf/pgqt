//! Integration tests for JSON type casting and validation functions (Phase 1.5)
//!
//! Tests for:
//! - json_typeof / jsonb_typeof
//! - json_strip_nulls / jsonb_strip_nulls
//! - json_pretty / jsonb_pretty
//! - jsonb_set
//! - jsonb_insert

use rusqlite::Connection;

fn setup_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    pgqt::jsonb::register_jsonb_functions(&conn).unwrap();
    conn
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_typeof_null() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_typeof('null')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "null");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_typeof_boolean() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_typeof('true')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "boolean");

    let result: String = conn
        .query_row("SELECT json_typeof('false')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "boolean");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_typeof_number() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_typeof('42')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "number");

    let result: String = conn
        .query_row("SELECT json_typeof('3.14')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "number");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_typeof_string() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_typeof('\"hello\"')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "string");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_typeof_array() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_typeof('[1,2,3]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "array");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_typeof_object() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_typeof('{\"a\":1}')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "object");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_typeof() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_typeof('{\"x\":1}')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "object");

    let result: String = conn
        .query_row("SELECT jsonb_typeof('[1,2,3]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "array");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_strip_nulls_simple() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_strip_nulls('{\"a\":1,\"b\":null,\"c\":3}')", [], |row| row.get(0))
        .unwrap();
    
    // Parse result and verify null field is removed
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert!(parsed.get("b").is_none());
    assert_eq!(parsed["c"], 3);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_strip_nulls_nested() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_strip_nulls('{\"a\":{\"x\":1,\"y\":null},\"b\":2}')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"]["x"], 1);
    assert!(parsed["a"].get("y").is_none());
    assert_eq!(parsed["b"], 2);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_strip_nulls_preserves_array_nulls() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_strip_nulls('[1,null,3]')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed[0], 1);
    assert!(parsed[1].is_null());
    assert_eq!(parsed[2], 3);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_strip_nulls() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_strip_nulls('{\"a\":1,\"b\":null}')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert!(parsed.get("b").is_none());
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_pretty_object() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_pretty('{\"a\":1,\"b\":2}')", [], |row| row.get(0))
        .unwrap();
    
    // Should contain newlines and indentation
    assert!(result.contains('\n'));
    assert!(result.contains("  \"a\": 1") || result.contains("  \"a\":1"));
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_pretty_nested() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_pretty('{\"outer\":{\"inner\":1}}')", [], |row| row.get(0))
        .unwrap();
    
    assert!(result.contains('\n'));
    assert!(result.contains('{'));
    assert!(result.contains('}'));
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_pretty_array() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_pretty('[1,2,3]')", [], |row| row.get(0))
        .unwrap();
    
    assert!(result.contains('\n'));
    assert!(result.contains('['));
    assert!(result.contains(']'));
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_set_top_level() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_set('{\"a\":1}', '{a}', '99')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 99);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_set_nested() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_set('{\"outer\":{\"inner\":1}}', '{outer,inner}', '99')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["outer"]["inner"], 99);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_set_creates_new_field() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_set('{\"a\":1}', '{b}', '2')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert_eq!(parsed["b"], 2);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_set_array_element() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_set('[1,2,3]', '{1}', '99')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed[0], 1);
    assert_eq!(parsed[1], 99);
    assert_eq!(parsed[2], 3);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_insert_object() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_insert('{\"a\":1}', '{b}', '2')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert_eq!(parsed["b"], 2);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_insert_nested() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT jsonb_insert('{\"outer\":{\"x\":1}}', '{outer,y}', '2')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["outer"]["x"], 1);
    assert_eq!(parsed["outer"]["y"], 2);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_typeof_with_complex_json() {
    let conn = setup_conn();
    
    // Test with nested object
    let result: String = conn
        .query_row("SELECT json_typeof('{\"nested\":{\"deep\":true}}')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "object");
    
    // Test with array
    let result: String = conn
        .query_row("SELECT json_typeof('[{\"a\":1}]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "array");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_functions_edge_cases() {
    let conn = setup_conn();
    
    // Empty object
    let result: String = conn
        .query_row("SELECT json_typeof('{}')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "object");
    
    // Empty array
    let result: String = conn
        .query_row("SELECT json_typeof('[]')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "array");
    
    // Empty string
    let result: String = conn
        .query_row("SELECT json_typeof('\"\"')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "string");
    
    // Zero
    let result: String = conn
        .query_row("SELECT json_typeof('0')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "number");
    
    // Negative number
    let result: String = conn
        .query_row("SELECT json_typeof('-42')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "number");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_strip_nulls_empty() {
    let conn = setup_conn();
    
    // Empty object
    let result: String = conn
        .query_row("SELECT json_strip_nulls('{}')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "{}");
    
    // Object with only nulls
    let result: String = conn
        .query_row("SELECT json_strip_nulls('{\"a\":null,\"b\":null}')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, "{}");
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_set_complex_path() {
    let conn = setup_conn();
    
    // Three-level nesting
    let result: String = conn
        .query_row("SELECT jsonb_set('{\"a\":{\"b\":{\"c\":1}}}', '{a,b,c}', '999')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"]["b"]["c"], 999);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_json_pretty_array_formatting() {
    let conn = setup_conn();
    let result: String = conn
        .query_row("SELECT json_pretty('[1,2,3]')", [], |row| row.get(0))
        .unwrap();
    
    // Should have proper array formatting
    assert!(result.contains('['));
    assert!(result.contains(']'));
    assert!(result.contains('\n'));
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_set_with_complex_value() {
    let conn = setup_conn();
    
    // Set an object value
    let result: String = conn
        .query_row("SELECT jsonb_set('{\"a\":1}', '{b}', '{\"nested\":true}')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert_eq!(parsed["b"]["nested"], true);
}

#[test]
#[ignore = "JSON validation functions not yet fully implemented"]
fn test_jsonb_set_with_array_value() {
    let conn = setup_conn();
    
    // Set an array value
    let result: String = conn
        .query_row("SELECT jsonb_set('{\"a\":1}', '{b}', '[1,2,3]')", [], |row| row.get(0))
        .unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["a"], 1);
    assert_eq!(parsed["b"][0], 1);
    assert_eq!(parsed["b"][1], 2);
    assert_eq!(parsed["b"][2], 3);
}
