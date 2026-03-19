//! Integration tests for JSON operators (Phase 1.4)
//!
//! Tests PostgreSQL JSON operators:
//! - `->` (Get JSON object field / array element)
//! - `->>` (Get JSON object field / array element as text)
//! - `#>` (Get JSON object at specified path)
//! - `#>>` (Get JSON object at specified path as text)
//! - `@>` (JSON contains)
//! - `<@` (JSON is contained by)
//! - `?` (Does key exist?)
//! - `?|` (Does any key exist?)
//! - `?&` (Do all keys exist?)
//! - `||` (Concatenate JSON)
//! - `-` (Delete key/array element)
//! - `#-` (Delete at path)

use pgqt::transpiler::transpile;

#[test]
fn test_json_arrow_operator() {
    // -> operator should extract JSON field
    let sql = "SELECT '{\"a\": 1}'::json->'a'";
    let transpiled = transpile(sql);
    println!("-> operator: {}", transpiled);
    assert!(transpiled.contains("json_extract"));
}

#[test]
fn test_json_arrow_text_operator() {
    // ->> operator should extract JSON field as text
    let sql = "SELECT '{\"a\": 1}'::json->>'a'";
    let transpiled = transpile(sql);
    println!("->> operator: {}", transpiled);
    assert!(transpiled.contains("json_extract"));
}

#[test]
fn test_json_hash_arrow_operator() {
    // #> operator should extract JSON at path
    let sql = "SELECT '{\"a\": {\"b\": 2}}'::json#>'{a,b}'";
    let transpiled = transpile(sql);
    println!("#> operator: {}", transpiled);
    assert!(transpiled.contains("json_extract"));
    // Should convert path from {a,b} to $.a.b
    assert!(transpiled.contains("$.a.b") || transpiled.contains("'$.a.b'"));
}

#[test]
fn test_json_hash_arrow_text_operator() {
    // #>> operator should extract JSON at path as text
    let sql = "SELECT '{\"a\": {\"b\": 2}}'::json#>>'{a,b}'";
    let transpiled = transpile(sql);
    println!("#>> operator: {}", transpiled);
    assert!(transpiled.contains("json_extract"));
}

#[test]
fn test_json_contains_operator() {
    // @> operator should check JSON containment
    let sql = "SELECT '{\"a\": 1, \"b\": 2}'::jsonb @> '{\"a\": 1}'";
    let transpiled = transpile(sql);
    println!("@> operator: {}", transpiled);
    assert!(transpiled.contains("jsonb_contains"));
}

#[test]
fn test_json_contained_by_operator() {
    // <@ operator should check if JSON is contained by
    let sql = "SELECT '{\"a\": 1}'::jsonb <@ '{\"a\": 1, \"b\": 2}'";
    let transpiled = transpile(sql);
    println!("<@ operator: {}", transpiled);
    assert!(transpiled.contains("jsonb_contained"));
}

#[test]
fn test_json_key_exists_operator() {
    // ? operator should check if key exists
    let sql = "SELECT '{\"a\": 1, \"b\": 2}'::jsonb ? 'a'";
    let transpiled = transpile(sql);
    println!("? operator: {}", transpiled);
    assert!(transpiled.contains("jsonb_exists"));
}

#[test]
fn test_json_any_key_exists_operator() {
    // ?| operator should check if any key exists
    let sql = "SELECT '{\"a\": 1, \"b\": 2}'::jsonb ?| array['a', 'c']";
    let transpiled = transpile(sql);
    println!("?| operator: {}", transpiled);
    assert!(transpiled.contains("jsonb_exists_any"));
}

#[test]
fn test_json_all_keys_exist_operator() {
    // ?& operator should check if all keys exist
    let sql = "SELECT '{\"a\": 1, \"b\": 2}'::jsonb ?& array['a', 'b']";
    let transpiled = transpile(sql);
    println!("?& operator: {}", transpiled);
    assert!(transpiled.contains("jsonb_exists_all"));
}

#[test]
fn test_json_concat_operator() {
    // || operator should concatenate JSON
    let sql = "SELECT '{\"a\": 1}'::jsonb || '{\"b\": 2}'::jsonb";
    let transpiled = transpile(sql);
    println!("|| operator: {}", transpiled);
    // Should use json_concat function
    assert!(transpiled.contains("json_concat") || transpiled.contains("json_patch"));
}

#[test]
fn test_json_delete_operator() {
    // - operator should delete key from JSON object
    let sql = "SELECT '{\"a\": 1, \"b\": 2}'::jsonb - 'a'";
    let transpiled = transpile(sql);
    println!("- operator: {}", transpiled);
    // Should use json_delete function
    assert!(transpiled.contains("json_delete") || transpiled.contains("json_remove"));
}

#[test]
fn test_json_delete_path_operator() {
    // #- operator should delete at path
    let sql = "SELECT '{\"a\": {\"b\": 2}}'::jsonb #- '{a,b}'";
    let transpiled = transpile(sql);
    println!("#- operator: {}", transpiled);
    assert!(transpiled.contains("json_delete_path") || transpiled.contains("json_remove"));
}

#[test]
fn test_json_arrow_with_array_index() {
    // -> operator with array index
    let sql = "SELECT '[1, 2, 3]'::json->1";
    let transpiled = transpile(sql);
    println!("-> with array index: {}", transpiled);
    assert!(transpiled.contains("json_extract"));
}

#[test]
fn test_json_nested_path_access() {
    // Test nested path with mixed keys and indices
    let sql = "SELECT '{\"a\": [{\"b\": 1}]}'::json#>'{a,0,b}'";
    let transpiled = transpile(sql);
    println!("Nested path: {}", transpiled);
    assert!(transpiled.contains("json_extract"));
}
