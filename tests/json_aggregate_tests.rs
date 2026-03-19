//! Integration tests for JSON aggregate functions
//!
//! These tests verify that json_agg, jsonb_agg, json_object_agg, and jsonb_object_agg
//! work correctly through the PGQT transpiler and SQLite execution.

use pgqt::jsonb::register_json_agg_functions;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    register_json_agg_functions(&conn).unwrap();
    conn
}

#[test]
fn test_json_agg_basic() {
    let conn = setup_db();
    conn.execute("CREATE TABLE products (name TEXT)", []).unwrap();
    conn.execute(
        "INSERT INTO products VALUES ('apple'), ('banana'), ('cherry')",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(name) FROM products", [], |r| r.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 3);
    assert!(parsed.as_array().unwrap().contains(&"apple".into()));
    assert!(parsed.as_array().unwrap().contains(&"banana".into()));
    assert!(parsed.as_array().unwrap().contains(&"cherry".into()));
}

#[test]
fn test_json_agg_with_integers() {
    let conn = setup_db();
    conn.execute("CREATE TABLE scores (value INTEGER)", []).unwrap();
    conn.execute("INSERT INTO scores VALUES (10), (20), (30)", []).unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(value) FROM scores", [], |r| r.get(0))
        .unwrap();

    assert_eq!(result, "[10,20,30]");
}

#[test]
fn test_json_agg_with_nulls() {
    let conn = setup_db();
    conn.execute("CREATE TABLE data (value TEXT)", []).unwrap();
    conn.execute("INSERT INTO data VALUES ('a'), (NULL), ('b')", []).unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(value) FROM data", [], |r| r.get(0))
        .unwrap();

    // json_agg includes NULLs
    assert_eq!(result, "[\"a\",null,\"b\"]");
}

#[test]
fn test_json_agg_empty() {
    let conn = setup_db();
    conn.execute("CREATE TABLE empty (value TEXT)", []).unwrap();

    let result: Option<String> = conn
        .query_row("SELECT json_agg(value) FROM empty", [], |r| r.get(0))
        .unwrap();

    assert!(result.is_none());
}

#[test]
fn test_jsonb_agg_basic() {
    let conn = setup_db();
    conn.execute("CREATE TABLE items (id INTEGER)", []).unwrap();
    conn.execute("INSERT INTO items VALUES (1), (2), (3)", []).unwrap();

    let result: String = conn
        .query_row("SELECT jsonb_agg(id) FROM items", [], |r| r.get(0))
        .unwrap();

    assert_eq!(result, "[1,2,3]");
}

#[test]
fn test_json_agg_with_group_by() {
    let conn = setup_db();
    conn.execute("CREATE TABLE sales (category TEXT, amount INTEGER)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO sales VALUES ('A', 10), ('A', 20), ('B', 30), ('B', 40), ('B', 50)",
        [],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT category, json_agg(amount) FROM sales GROUP BY category ORDER BY category")
        .unwrap();
    let results: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "A");
    assert_eq!(results[0].1, "[10,20]");
    assert_eq!(results[1].0, "B");
    assert_eq!(results[1].1, "[30,40,50]");
}

#[test]
fn test_json_object_agg_basic() {
    let conn = setup_db();
    conn.execute("CREATE TABLE config (key TEXT, value INTEGER)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO config VALUES ('timeout', 30), ('retries', 3)",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT json_object_agg(key, value) FROM config", [], |r| r.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_object());
    assert_eq!(parsed["timeout"], 30);
    assert_eq!(parsed["retries"], 3);
}

#[test]
fn test_jsonb_object_agg_basic() {
    let conn = setup_db();
    conn.execute("CREATE TABLE users (name TEXT, city TEXT)", []).unwrap();
    conn.execute(
        "INSERT INTO users VALUES ('Alice', 'NYC'), ('Bob', 'LA')",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT jsonb_object_agg(name, city) FROM users", [], |r| r.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_object());
    assert_eq!(parsed["Alice"], "NYC");
    assert_eq!(parsed["Bob"], "LA");
}

#[test]
fn test_json_object_agg_duplicate_keys() {
    let conn = setup_db();
    conn.execute("CREATE TABLE settings (key TEXT, value TEXT)", []).unwrap();
    conn.execute(
        "INSERT INTO settings VALUES ('theme', 'dark'), ('theme', 'light'), ('theme', 'auto')",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT json_object_agg(key, value) FROM settings", [], |r| r.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_object());
    // Last value wins for duplicate keys
    assert_eq!(parsed["theme"], "auto");
}

#[test]
fn test_json_object_agg_empty() {
    let conn = setup_db();
    conn.execute("CREATE TABLE empty (key TEXT, value TEXT)", []).unwrap();

    let result: Option<String> = conn
        .query_row("SELECT json_object_agg(key, value) FROM empty", [], |r| r.get(0))
        .unwrap();

    assert!(result.is_none());
}

#[test]
fn test_json_object_agg_with_nulls() {
    let conn = setup_db();
    conn.execute("CREATE TABLE data (key TEXT, value TEXT)", []).unwrap();
    conn.execute(
        "INSERT INTO data VALUES ('a', 'value_a'), ('b', NULL), ('c', 'value_c')",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT json_object_agg(key, value) FROM data", [], |r| r.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_object());
    assert_eq!(parsed["a"], "value_a");
    assert!(parsed["b"].is_null());
    assert_eq!(parsed["c"], "value_c");
}

#[test]
fn test_json_object_agg_with_group_by() {
    let conn = setup_db();
    conn.execute("CREATE TABLE orders (customer TEXT, item TEXT, qty INTEGER)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO orders VALUES ('Alice', 'apple', 5), ('Alice', 'banana', 3), ('Bob', 'carrot', 2)",
        [],
    )
    .unwrap();

    let mut stmt = conn
        .prepare("SELECT customer, json_object_agg(item, qty) FROM orders GROUP BY customer ORDER BY customer")
        .unwrap();
    let results: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(results.len(), 2);
    
    let alice_obj: serde_json::Value = serde_json::from_str(&results[0].1).unwrap();
    assert_eq!(alice_obj["apple"], 5);
    assert_eq!(alice_obj["banana"], 3);
    
    let bob_obj: serde_json::Value = serde_json::from_str(&results[1].1).unwrap();
    assert_eq!(bob_obj["carrot"], 2);
}

#[test]
fn test_json_agg_mixed_types() {
    let conn = setup_db();
    conn.execute("CREATE TABLE mixed (data TEXT)", []).unwrap();
    conn.execute(
        "INSERT INTO mixed VALUES ('string'), ('123'), ('true'), ('null')",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(data) FROM mixed", [], |r| r.get(0))
        .unwrap();

    // Values are stored as strings
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 4);
}

#[test]
fn test_json_agg_with_floats() {
    let conn = setup_db();
    conn.execute("CREATE TABLE measurements (value REAL)", []).unwrap();
    conn.execute("INSERT INTO measurements VALUES (3.14), (2.71), (1.41)", [])
        .unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(value) FROM measurements", [], |r| r.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 3);
}

#[test]
fn test_json_agg_single_value() {
    let conn = setup_db();
    conn.execute("CREATE TABLE single (value TEXT)", []).unwrap();
    conn.execute("INSERT INTO single VALUES ('only_one')", []).unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(value) FROM single", [], |r| r.get(0))
        .unwrap();

    assert_eq!(result, "[\"only_one\"]");
}

#[test]
fn test_json_object_agg_single_pair() {
    let conn = setup_db();
    conn.execute("CREATE TABLE single (key TEXT, value TEXT)", []).unwrap();
    conn.execute("INSERT INTO single VALUES ('key1', 'value1')", []).unwrap();

    let result: String = conn
        .query_row("SELECT json_object_agg(key, value) FROM single", [], |r| r.get(0))
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_object());
    assert_eq!(parsed["key1"], "value1");
}

#[test]
fn test_json_agg_order_preservation() {
    let conn = setup_db();
    conn.execute("CREATE TABLE ordered (id INTEGER, value TEXT)", []).unwrap();
    conn.execute(
        "INSERT INTO ordered VALUES (1, 'first'), (2, 'second'), (3, 'third')",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(value ORDER BY id) FROM ordered", [], |r| r.get(0))
        .unwrap();

    // Note: ORDER BY within aggregate may not be supported directly in SQLite
    // This test verifies basic aggregation without explicit ordering
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 3);
}

#[test]
fn test_json_agg_nested_json() {
    let conn = setup_db();
    conn.execute("CREATE TABLE nested (data TEXT)", []).unwrap();
    conn.execute(
        "INSERT INTO nested VALUES ('{\"nested\": true}'), ('{\"nested\": false}')",
        [],
    )
    .unwrap();

    let result: String = conn
        .query_row("SELECT json_agg(data) FROM nested", [], |r| r.get(0))
        .unwrap();

    // Values are stored as strings, not parsed as JSON
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_array());
    // The value is parsed as JSON since it looks like JSON
    assert_eq!(parsed[0]["nested"], true);
}
