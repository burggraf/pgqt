use pgqt::transpiler::transpile_with_metadata;

#[test]
fn test_anonymous_column_naming() {
    let sql = "SELECT 1 + 1, 'hello', CAST(1 AS TEXT)";
    let result = transpile_with_metadata(sql);
    println!("SQL: {}", result.sql);
    // PostgreSQL style: SELECT 1 + 1 AS "?column?", 'hello' AS "?column?", cast(1 as text) AS "text"
    assert!(result.sql.contains("AS \"?column?\""));
    // Count occurrences of ?column? - should be 2
    let count = result.sql.matches("AS \"?column?\"").count();
    assert_eq!(count, 2);
    // Cast should have type name
    assert!(result.sql.contains("AS \"text\""));
}

#[test]
fn test_values_column_naming() {
    let sql = "SELECT * FROM (VALUES (1, 'a')) AS v";
    let result = transpile_with_metadata(sql);
    println!("SQL: {}", result.sql);
    // In a subquery without column aliases, they should be column1, column2
    assert!(result.sql.contains("AS \"column1\""));
}

#[test]
fn test_mixed_column_naming() {
    let sql = "SELECT id, 1 + 1, name as my_name FROM users";
    let result = transpile_with_metadata(sql);
    println!("SQL: {}", result.sql);
    // id should NOT be renamed
    // 1 + 1 SHOULD be renamed to ?column?
    // my_name SHOULD be preserved
    assert!(!result.sql.contains("id AS \"?column?\""));
    assert!(result.sql.contains("1 + 1 AS \"?column?\""));
    assert!(result.sql.contains("name as \"my_name\""));
}
