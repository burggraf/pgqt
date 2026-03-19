//! Integration tests for INSERT statement enhancements
//! 
//! These tests verify:
//! - Complex expressions in RETURNING clause
//! - Aggregate functions in RETURNING
//! - Subqueries in RETURNING
//! - Column aliases in RETURNING
//! - Interaction with triggers

use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
    conn
}

#[test]
fn test_insert_returning_simple_column() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    let result: (i64,) = conn.query_row(
        "INSERT INTO test (id, name) VALUES (1, 'test') RETURNING id",
        [],
        |row| Ok((row.get(0).unwrap(),)),
    ).unwrap();
    
    assert_eq!(result.0, 1);
}

#[test]
fn test_insert_returning_multiple_columns() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    let result: (i64, String) = conn.query_row(
        "INSERT INTO test (id, name) VALUES (1, 'hello') RETURNING id, name",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, "hello");
}

#[test]
fn test_insert_returning_expression() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value INTEGER)",
        [],
    ).unwrap();
    
    let result: (i64, i64) = conn.query_row(
        "INSERT INTO test (id, value) VALUES (1, 10) RETURNING id, value * 2",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 20);
}

#[test]
fn test_insert_returning_alias() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    let result: (i64,) = conn.query_row(
        "INSERT INTO test (id, name) VALUES (1, 'test') RETURNING id AS new_id",
        [],
        |row| Ok((row.get("new_id").unwrap(),)),
    ).unwrap();
    
    assert_eq!(result.0, 1);
}

#[test]
fn test_insert_returning_multiple_aliases() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value INTEGER)",
        [],
    ).unwrap();
    
    let result: (i64, i64) = conn.query_row(
        "INSERT INTO test (id, value) VALUES (1, 10) RETURNING id AS new_id, value * 2 AS doubled_value",
        [],
        |row| Ok((row.get("new_id").unwrap(), row.get("doubled_value").unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 20);
}

#[test]
fn test_insert_returning_function() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    let result: (String,) = conn.query_row(
        "INSERT INTO test (id, name) VALUES (1, 'hello') RETURNING UPPER(name)",
        [],
        |row| Ok((row.get(0).unwrap(),)),
    ).unwrap();
    
    assert_eq!(result.0, "HELLO");
}

#[test]
fn test_insert_returning_function_with_alias() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    let result: (String,) = conn.query_row(
        "INSERT INTO test (id, name) VALUES (1, 'hello') RETURNING UPPER(name) AS upper_name",
        [],
        |row| Ok((row.get("upper_name").unwrap(),)),
    ).unwrap();
    
    assert_eq!(result.0, "HELLO");
}

#[test]
fn test_insert_returning_star() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)",
        [],
    ).unwrap();
    
    let result: (i64, String, i64) = conn.query_row(
        "INSERT INTO test (id, name, value) VALUES (1, 'test', 42) RETURNING *",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap(), row.get(2).unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, "test");
    assert_eq!(result.2, 42);
}

#[test]
fn test_insert_returning_subquery() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY)",
        [],
    ).unwrap();
    
    conn.execute(
        "CREATE TABLE other (count INTEGER)",
        [],
    ).unwrap();
    
    conn.execute("INSERT INTO other VALUES (5)", []).unwrap();
    
    let result: (i64, i64) = conn.query_row(
        "INSERT INTO test (id) VALUES (1) RETURNING id, (SELECT count FROM other)",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 5);
}

#[test]
fn test_insert_returning_with_trigger() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, modified TEXT)",
        [],
    ).unwrap();
    
    // Create a trigger that modifies the row on insert
    conn.execute(
        "CREATE TRIGGER test_trigger 
         AFTER INSERT ON test 
         BEGIN
             UPDATE test SET modified = 'triggered' WHERE id = NEW.id;
         END",
        [],
    ).unwrap();
    
    // Note: In SQLite, AFTER INSERT trigger won't affect RETURNING values
    // because RETURNING captures values immediately after insert
    let result: (i64, String, Option<String>) = conn.query_row(
        "INSERT INTO test (id, name) VALUES (1, 'test') RETURNING id, name, modified",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap(), row.get(2).unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, "test");
    // modified should be NULL because trigger runs AFTER insert
    assert!(result.2.is_none());
}

#[test]
fn test_insert_returning_complex_expressions() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value1 INTEGER, value2 INTEGER)",
        [],
    ).unwrap();
    
    let result: (i64, i64, i64) = conn.query_row(
        "INSERT INTO test (id, value1, value2) VALUES (1, 10, 20) 
         RETURNING id, value1 + value2 AS sum, value1 * value2 AS product",
        [],
        |row| Ok((row.get(0).unwrap(), row.get("sum").unwrap(), row.get("product").unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 30);
    assert_eq!(result.2, 200);
}

#[test]
fn test_update_returning_expression() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value INTEGER)",
        [],
    ).unwrap();
    
    conn.execute("INSERT INTO test (id, value) VALUES (1, 10)", []).unwrap();
    
    let result: (i64, i64) = conn.query_row(
        "UPDATE test SET value = 20 WHERE id = 1 RETURNING id, value * 2 AS doubled",
        [],
        |row| Ok((row.get(0).unwrap(), row.get("doubled").unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 40);
}

#[test]
fn test_delete_returning_expression() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value INTEGER)",
        [],
    ).unwrap();
    
    conn.execute("INSERT INTO test (id, value) VALUES (1, 10)", []).unwrap();
    
    let result: (i64, i64) = conn.query_row(
        "DELETE FROM test WHERE id = 1 RETURNING id, value * 3 AS tripled",
        [],
        |row| Ok((row.get(0).unwrap(), row.get("tripled").unwrap())),
    ).unwrap();
    
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 30);
}

// Multi-row INSERT tests for Phase 4.3

#[test]
fn test_multi_row_insert_basic() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO test (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c')",
        [],
    ).unwrap();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 3);
    
    let names: Vec<String> = conn
        .prepare("SELECT name FROM test ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok(row.get::<_, String>(0).unwrap()))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    
    assert_eq!(names, vec!["a", "b", "c"]);
}

#[test]
#[ignore = "SQLite doesn't support DEFAULT in multi-row INSERT - requires transpiler fix to split into separate INSERTs"]
fn test_multi_row_insert_with_default() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT DEFAULT 'default_value')",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO test (id, name) VALUES (1, DEFAULT), (2, 'a')",
        [],
    ).unwrap();
    
    let name1: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    
    let name2: String = conn.query_row(
        "SELECT name FROM test WHERE id = 2",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(name1, "default_value");
    assert_eq!(name2, "a");
}

#[test]
fn test_multi_row_insert_with_expressions() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value INTEGER, name TEXT)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO test (id, value, name) VALUES (1, 1+1, UPPER('a')), (2, 2*2, 'b')",
        [],
    ).unwrap();
    
    let value1: i64 = conn.query_row(
        "SELECT value FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    
    let name1: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    
    let value2: i64 = conn.query_row(
        "SELECT value FROM test WHERE id = 2",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(value1, 2);  // 1+1
    assert_eq!(name1, "A"); // UPPER('a')
    assert_eq!(value2, 4);  // 2*2
}

#[test]
#[ignore = "SQLite doesn't support DEFAULT in multi-row INSERT - requires transpiler fix to split into separate INSERTs"]
fn test_multi_row_insert_mixed_values() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT DEFAULT 'default_name')",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO test (id, name) VALUES (1, 'a'), (DEFAULT, 'b')",
        [],
    ).unwrap();
    
    let name1: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    
    let id2: i64 = conn.query_row(
        "SELECT id FROM test WHERE name = 'b'",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(name1, "a");
    assert!(id2 > 1); // DEFAULT should generate next value
}

#[test]
fn test_multi_row_insert_different_column_orders() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)",
        [],
    ).unwrap();
    
    // This tests that column order is respected regardless of VALUES order
    conn.execute(
        "INSERT INTO test (value, name, id) VALUES (10, 'a', 1), (20, 'b', 2)",
        [],
    ).unwrap();
    
    let value1: i64 = conn.query_row(
        "SELECT value FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    
    let name2: String = conn.query_row(
        "SELECT name FROM test WHERE id = 2",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(value1, 10);
    assert_eq!(name2, "b");
}

#[test]
fn test_multi_row_insert_with_returning() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    // Note: SQLite doesn't support RETURNING with multi-row directly,
    // but the proxy should handle it
    let mut stmt = conn.prepare(
        "INSERT INTO test (id, name) VALUES (1, 'a'), (2, 'b') RETURNING id, name"
    ).unwrap();
    
    let results: Vec<(i64, String)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0).unwrap(), row.get::<_, String>(1).unwrap()))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    
    assert_eq!(results.len(), 2);
    assert!(results.contains(&(1, "a".to_string())));
    assert!(results.contains(&(2, "b".to_string())));
}
