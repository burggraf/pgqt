//! Integration tests for ON CONFLICT (Upsert) enhancements
//!
//! These tests verify:
//! - Multiple conflict targets (e.g., ON CONFLICT (col1, col2))
//! - Complex WHERE clauses in DO UPDATE
//! - Subqueries in DO UPDATE SET
//! - ON CONFLICT with RETURNING
//! - EXCLUDED table references

use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
    conn
}

#[test]
fn test_upsert_do_nothing() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();

    // First insert should succeed
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice') ON CONFLICT (id) DO NOTHING",
        [],
    ).unwrap();

    // Second insert with same id should be ignored
    conn.execute(
        "INSERT INTO test VALUES (1, 'Bob') ON CONFLICT (id) DO NOTHING",
        [],
    ).unwrap();

    let name: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();

    assert_eq!(name, "Alice");
}

#[test]
fn test_upsert_do_update() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, counter INTEGER)",
        [],
    ).unwrap();

    // Initial insert
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice', 1)",
        [],
    ).unwrap();

    // Upsert should update the name
    conn.execute(
        "INSERT INTO test VALUES (1, 'Bob', 2) ON CONFLICT (id) DO UPDATE SET name = excluded.name, counter = excluded.counter",
        [],
    ).unwrap();

    let (name, counter): (String, i64) = conn.query_row(
        "SELECT name, counter FROM test WHERE id = 1",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();

    assert_eq!(name, "Bob");
    assert_eq!(counter, 2);
}

#[test]
fn test_upsert_multiple_conflict_targets() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER, email TEXT, name TEXT, PRIMARY KEY (id, email))",
        [],
    ).unwrap();

    // Initial insert
    conn.execute(
        "INSERT INTO test VALUES (1, 'alice@example.com', 'Alice')",
        [],
    ).unwrap();

    // Upsert with multiple conflict targets
    conn.execute(
        "INSERT INTO test VALUES (1, 'alice@example.com', 'Alice Updated') ON CONFLICT (id, email) DO UPDATE SET name = excluded.name",
        [],
    ).unwrap();

    let name: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1 AND email = 'alice@example.com'",
        [],
        |row| row.get(0),
    ).unwrap();

    assert_eq!(name, "Alice Updated");
}

#[test]
fn test_upsert_with_where_clause() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, version INTEGER)",
        [],
    ).unwrap();

    // Initial insert
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice', 1)",
        [],
    ).unwrap();

    // Upsert with WHERE - should update because version < excluded.version
    conn.execute(
        "INSERT INTO test VALUES (1, 'Bob', 2) ON CONFLICT (id) DO UPDATE SET name = excluded.name WHERE test.version < excluded.version",
        [],
    ).unwrap();

    let name: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();

    // Should be updated because 1 < 2
    assert_eq!(name, "Bob");
}

#[test]
fn test_upsert_with_where_clause_no_update() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, version INTEGER)",
        [],
    ).unwrap();

    // Initial insert with version 5
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice', 5)",
        [],
    ).unwrap();

    // Upsert with WHERE - should NOT update because 5 is not < 2
    conn.execute(
        "INSERT INTO test VALUES (1, 'Bob', 2) ON CONFLICT (id) DO UPDATE SET name = excluded.name WHERE test.version < excluded.version",
        [],
    ).unwrap();

    let name: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();

    // Should NOT be updated because 5 is not < 2
    assert_eq!(name, "Alice");
}

#[test]
fn test_upsert_with_returning() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();

    // Initial insert
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice')",
        [],
    ).unwrap();

    // Upsert with RETURNING
    let result: (i64, String) = conn.query_row(
        "INSERT INTO test VALUES (1, 'Bob') ON CONFLICT (id) DO UPDATE SET name = excluded.name RETURNING id, name",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();

    assert_eq!(result.0, 1);
    assert_eq!(result.1, "Bob");
}

#[test]
fn test_upsert_with_returning_star() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)",
        [],
    ).unwrap();

    // Initial insert
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice', 100)",
        [],
    ).unwrap();

    // Upsert with RETURNING *
    let result: (i64, String, i64) = conn.query_row(
        "INSERT INTO test VALUES (1, 'Bob', 200) ON CONFLICT (id) DO UPDATE SET name = excluded.name, value = excluded.value RETURNING *",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap(), row.get(2).unwrap())),
    ).unwrap();

    assert_eq!(result.0, 1);
    assert_eq!(result.1, "Bob");
    assert_eq!(result.2, 200);
}

#[test]
fn test_upsert_excluded_reference() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, counter INTEGER DEFAULT 0)",
        [],
    ).unwrap();

    // Initial insert
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice', 5)",
        [],
    ).unwrap();

    // Upsert using excluded to increment counter
    conn.execute(
        "INSERT INTO test VALUES (1, 'Bob', 10) ON CONFLICT (id) DO UPDATE SET name = excluded.name, counter = test.counter + excluded.counter",
        [],
    ).unwrap();

    let (name, counter): (String, i64) = conn.query_row(
        "SELECT name, counter FROM test WHERE id = 1",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();

    assert_eq!(name, "Bob");
    assert_eq!(counter, 15); // 5 + 10
}

#[test]
fn test_upsert_do_update_subquery() {
    let conn = setup_test_db();

    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();

    conn.execute(
        "CREATE TABLE defaults (default_name TEXT)",
        [],
    ).unwrap();

    conn.execute("INSERT INTO defaults VALUES ('DefaultName')", []).unwrap();

    // Initial insert
    conn.execute(
        "INSERT INTO test VALUES (1, 'Alice')",
        [],
    ).unwrap();

    // Upsert with subquery in SET
    conn.execute(
        "INSERT INTO test VALUES (1, 'Bob') ON CONFLICT (id) DO UPDATE SET name = (SELECT default_name FROM defaults)",
        [],
    ).unwrap();

    let name: String = conn.query_row(
        "SELECT name FROM test WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap();

    assert_eq!(name, "DefaultName");
}
