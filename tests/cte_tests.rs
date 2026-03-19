//! Integration tests for CTE (WITH clause) functionality

use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn
}

#[test]
fn test_recursive_cte_simple() {
    let conn = setup_test_db();
    
    // Simple number sequence 1 to 5
    let results: Vec<i32> = conn
        .prepare("WITH RECURSIVE t(n) AS (VALUES (1) UNION ALL SELECT n+1 FROM t WHERE n < 5) SELECT n FROM t")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    
    assert_eq!(results, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_recursive_cte_tree_traversal() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE tree (id INT PRIMARY KEY, parent_id INT, name TEXT)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO tree VALUES (1, NULL, 'root'), (2, 1, 'child1'), (3, 1, 'child2'), (4, 2, 'grandchild')",
        [],
    ).unwrap();
    
    // Find all descendants of root
    let results: Vec<String> = conn
        .prepare(
            "WITH RECURSIVE descendants AS (
                SELECT id, parent_id, name FROM tree WHERE id = 1
                UNION ALL
                SELECT t.id, t.parent_id, t.name 
                FROM tree t 
                JOIN descendants d ON t.parent_id = d.id
            )
            SELECT name FROM descendants"
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    
    assert_eq!(results, vec!["root", "child1", "child2", "grandchild"]);
}

#[test]
fn test_multiple_ctes() {
    let conn = setup_test_db();
    
    let result: (i32, i32) = conn.query_row(
        "WITH a AS (SELECT 1 AS x), 
              b AS (SELECT x + 1 AS y FROM a)
         SELECT * FROM a, b",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();
    
    assert_eq!(result, (1, 2));
}

#[test]
fn test_cte_with_column_list() {
    let conn = setup_test_db();
    
    let result: i32 = conn.query_row(
        "WITH a(x, y) AS (SELECT 1, 2)
         SELECT x + y FROM a",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, 3);
}

#[test]
#[ignore = "Data-modifying CTEs require transpiler support for INSERT/DELETE in CTEs"]
fn test_data_modifying_cte_insert() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value INT)",
        [],
    ).unwrap();
    
    let results: Vec<i32> = conn
        .prepare(
            "WITH inserted AS (
                INSERT INTO test (value) VALUES (10), (20)
                RETURNING id
            )
            SELECT id FROM inserted"
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    
    assert_eq!(results.len(), 2);
}

#[test]
#[ignore = "Data-modifying CTEs require transpiler support for INSERT/DELETE in CTEs"]
fn test_chained_ctes() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE source (id INT, value INT)",
        [],
    ).unwrap();
    conn.execute(
        "CREATE TABLE dest (id INT, value INT)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO source VALUES (1, 100), (2, 200)",
        [],
    ).unwrap();
    
    let count: i32 = conn.query_row(
        "WITH 
            deleted AS (DELETE FROM source RETURNING *),
            inserted AS (INSERT INTO dest SELECT * FROM deleted RETURNING *)
         SELECT COUNT(*) FROM inserted",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 2);
}
