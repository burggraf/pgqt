//! Integration tests for boolean aggregate functions
//!
//! Tests bool_and, bool_or, every, booland_statefunc, boolor_statefunc

use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    pgqt::bool_aggregates::register_bool_aggregates(&conn).unwrap();
    pgqt::bool_aggregates::register_bool_statefuncs(&conn).unwrap();
    pgqt::bool_aggregates::register_bitwise_aggregates(&conn).unwrap();
    conn
}

#[test]
fn test_bool_and_all_true() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute("INSERT INTO bool_test VALUES (1, true), (2, true), (3, true)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_and(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(true));
}

#[test]
fn test_bool_and_all_false() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute("INSERT INTO bool_test VALUES (1, false), (2, false), (3, false)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_and(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(false));
}

#[test]
fn test_bool_and_mixed() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO bool_test VALUES (1, true), (2, false), (3, true)",
        [],
    )
    .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_and(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(false));
}

#[test]
fn test_bool_or_all_true() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute("INSERT INTO bool_test VALUES (1, true), (2, true), (3, true)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_or(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(true));
}

#[test]
fn test_bool_or_all_false() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute("INSERT INTO bool_test VALUES (1, false), (2, false), (3, false)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_or(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(false));
}

#[test]
fn test_bool_or_mixed() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO bool_test VALUES (1, false), (2, true), (3, false)",
        [],
    )
    .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_or(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(true));
}

#[test]
fn test_bool_and_with_nulls() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO bool_test VALUES (1, true), (2, NULL), (3, true)",
        [],
    )
    .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_and(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    // NULLs are skipped, so result is bool_and(true, true) = true
    assert_eq!(result, Some(true));
}

#[test]
fn test_bool_or_with_nulls() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO bool_test VALUES (1, false), (2, NULL), (3, false)",
        [],
    )
    .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_or(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    // NULLs are skipped, so result is bool_or(false, false) = false
    assert_eq!(result, Some(false));
}

#[test]
fn test_bool_and_all_nulls() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute("INSERT INTO bool_test VALUES (1, NULL), (2, NULL)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_and(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    // All NULLs means no non-null values, returns true (identity for AND)
    assert_eq!(result, Some(true));
}

#[test]
fn test_bool_or_all_nulls() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute("INSERT INTO bool_test VALUES (1, NULL), (2, NULL)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_or(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    // All NULLs means no non-null values, returns false (identity for OR)
    assert_eq!(result, Some(false));
}

#[test]
fn test_bool_and_empty_table() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_and(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    // Empty result set returns true for bool_and
    assert_eq!(result, Some(true));
}

#[test]
fn test_bool_or_empty_table() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT bool_or(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    // Empty result set returns false for bool_or
    assert_eq!(result, Some(false));
}

#[test]
fn test_every_alias_all_true() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute("INSERT INTO bool_test VALUES (1, true), (2, true)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT every(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(true));
}

#[test]
fn test_every_alias_with_false() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO bool_test VALUES (1, true), (2, false), (3, true)",
        [],
    )
    .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT every(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(false));
}

#[test]
fn test_every_alias_empty_table() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bool_test (id INT, b BOOLEAN)", [])
        .unwrap();

    let result: Option<bool> = conn
        .query_row("SELECT every(b) FROM bool_test", [], |row| row.get(0))
        .unwrap();

    // every is an alias for bool_and, so empty table returns true
    assert_eq!(result, Some(true));
}

#[test]
fn test_booland_statefunc_both_values() {
    let conn = setup_test_db();

    // true && true = true
    let result: Option<bool> = conn
        .query_row("SELECT booland_statefunc(true, true)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));

    // true && false = false
    let result: Option<bool> = conn
        .query_row("SELECT booland_statefunc(true, false)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(false));

    // false && true = false
    let result: Option<bool> = conn
        .query_row("SELECT booland_statefunc(false, true)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(false));

    // false && false = false
    let result: Option<bool> = conn
        .query_row("SELECT booland_statefunc(false, false)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(false));
}

#[test]
fn test_booland_statefunc_with_nulls() {
    let conn = setup_test_db();

    // First NULL
    let result: Option<bool> = conn
        .query_row("SELECT booland_statefunc(NULL, true)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));

    // Second NULL
    let result: Option<bool> = conn
        .query_row("SELECT booland_statefunc(true, NULL)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));

    // Both NULL
    let result: Option<bool> = conn
        .query_row("SELECT booland_statefunc(NULL, NULL)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_boolor_statefunc_both_values() {
    let conn = setup_test_db();

    // false || false = false
    let result: Option<bool> = conn
        .query_row("SELECT boolor_statefunc(false, false)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(false));

    // false || true = true
    let result: Option<bool> = conn
        .query_row("SELECT boolor_statefunc(false, true)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));

    // true || false = true
    let result: Option<bool> = conn
        .query_row("SELECT boolor_statefunc(true, false)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));

    // true || true = true
    let result: Option<bool> = conn
        .query_row("SELECT boolor_statefunc(true, true)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));
}

#[test]
fn test_boolor_statefunc_with_nulls() {
    let conn = setup_test_db();

    // First NULL
    let result: Option<bool> = conn
        .query_row("SELECT boolor_statefunc(NULL, true)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));

    // Second NULL
    let result: Option<bool> = conn
        .query_row("SELECT boolor_statefunc(true, NULL)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, Some(true));

    // Both NULL
    let result: Option<bool> = conn
        .query_row("SELECT boolor_statefunc(NULL, NULL)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_bool_with_group_by() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE sales (product TEXT, in_stock BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO sales VALUES ('A', true), ('A', true), ('B', true), ('B', false)",
        [],
    )
    .unwrap();

    let results: Vec<(String, Option<bool>)> = conn
        .prepare("SELECT product, bool_and(in_stock) FROM sales GROUP BY product ORDER BY product")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(results.len(), 2);
    // Product A: all in_stock = true, so bool_and = true
    assert_eq!(results[0].0, "A");
    assert_eq!(results[0].1, Some(true));
    // Product B: one in_stock = false, so bool_and = false
    assert_eq!(results[1].0, "B");
    assert_eq!(results[1].1, Some(false));
}

#[test]
fn test_bool_or_with_group_by() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE sales (product TEXT, has_sale BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO sales VALUES ('A', false), ('A', false), ('B', false), ('B', true)",
        [],
    )
    .unwrap();

    let results: Vec<(String, Option<bool>)> = conn
        .prepare("SELECT product, bool_or(has_sale) FROM sales GROUP BY product ORDER BY product")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(results.len(), 2);
    // Product A: all has_sale = false, so bool_or = false
    assert_eq!(results[0].0, "A");
    assert_eq!(results[0].1, Some(false));
    // Product B: one has_sale = true, so bool_or = true
    assert_eq!(results[1].0, "B");
    assert_eq!(results[1].1, Some(true));
}

#[test]
fn test_bool_aggregates_with_having() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE sales (product TEXT, in_stock BOOLEAN)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO sales VALUES ('A', true), ('A', true), ('B', false), ('B', false)",
        [],
    )
    .unwrap();

    let results: Vec<String> = conn
        .prepare("SELECT product FROM sales GROUP BY product HAVING bool_and(in_stock) = true")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0], "A");
}

// ============================================================================
// Bitwise Aggregate Integration Tests
// ============================================================================

#[test]
fn test_bit_and_integration() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bitwise_test (id INT, i INT)", [])
        .unwrap();
    // 5 = 101, 3 = 011, 1 = 001
    // 5 & 3 & 1 = 001 = 1
    conn.execute(
        "INSERT INTO bitwise_test VALUES (1, 5), (2, 3), (3, 1)",
        [],
    )
    .unwrap();

    let result: Option<i64> = conn
        .query_row("SELECT bit_and(i) FROM bitwise_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(1));
}

#[test]
fn test_bit_or_integration() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bitwise_test (id INT, i INT)", [])
        .unwrap();
    // 1 = 001, 2 = 010, 4 = 100
    // 1 | 2 | 4 = 111 = 7
    conn.execute(
        "INSERT INTO bitwise_test VALUES (1, 1), (2, 2), (3, 4)",
        [],
    )
    .unwrap();

    let result: Option<i64> = conn
        .query_row("SELECT bit_or(i) FROM bitwise_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(7));
}

#[test]
fn test_bit_xor_integration() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bitwise_test (id INT, i INT)", [])
        .unwrap();
    // 5 = 101, 3 = 011
    // 5 ^ 3 = 110 = 6
    conn.execute(
        "INSERT INTO bitwise_test VALUES (1, 5), (2, 3)",
        [],
    )
    .unwrap();

    let result: Option<i64> = conn
        .query_row("SELECT bit_xor(i) FROM bitwise_test", [], |row| row.get(0))
        .unwrap();

    assert_eq!(result, Some(6));
}

#[test]
fn test_bit_and_with_nulls_integration() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bitwise_test (id INT, i INT)", [])
        .unwrap();
    // NULL values should be skipped
    conn.execute(
        "INSERT INTO bitwise_test VALUES (1, 7), (2, NULL), (3, 3)",
        [],
    )
    .unwrap();

    let result: Option<i64> = conn
        .query_row("SELECT bit_and(i) FROM bitwise_test", [], |row| row.get(0))
        .unwrap();

    // 7 & 3 = 3
    assert_eq!(result, Some(3));
}

#[test]
fn test_bit_aggregates_empty_table_integration() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bitwise_test (id INT, i INT)", [])
        .unwrap();

    // Empty table should return NULL for all bitwise aggregates
    let result: Option<i64> = conn
        .query_row("SELECT bit_and(i) FROM bitwise_test", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, None);

    let result: Option<i64> = conn
        .query_row("SELECT bit_or(i) FROM bitwise_test", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, None);

    let result: Option<i64> = conn
        .query_row("SELECT bit_xor(i) FROM bitwise_test", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_bit_aggregates_with_group_by() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE bitwise_test (category TEXT, i INT)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO bitwise_test VALUES ('A', 1), ('A', 2), ('B', 4), ('B', 8)",
        [],
    )
    .unwrap();

    let results: Vec<(String, Option<i64>)> = conn
        .prepare("SELECT category, bit_or(i) FROM bitwise_test GROUP BY category ORDER BY category")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(results.len(), 2);
    // Category A: 1 | 2 = 3
    assert_eq!(results[0], ("A".to_string(), Some(3)));
    // Category B: 4 | 8 = 12
    assert_eq!(results[1], ("B".to_string(), Some(12)));
}