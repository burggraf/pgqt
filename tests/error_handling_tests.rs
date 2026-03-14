//! Tests for error handling scenarios
//!
//! These tests document cases where PGQT may be more permissive than PostgreSQL.

use pgqt::transpiler::transpile;

#[test]
fn test_group_by_validation_permissive() {
    // This should fail in PostgreSQL - column not in GROUP BY
    // PGQT may accept this due to SQLite's permissive nature
    let sql = "SELECT t1.f1 FROM t1 LEFT JOIN t2 USING (f1) GROUP BY f1";
    let result = transpile(sql);
    // For now, just ensure it transpiles (strict checking can be added later)
    assert!(!result.is_empty(), "Should transpile: {}", result);
}

#[test]
fn test_type_checking_permissive() {
    // Type mismatches that PostgreSQL catches
    let test_cases = vec![
        "SELECT 'text'::int",
        "SELECT 1 + 'hello'",
    ];
    
    for sql in test_cases {
        let result = transpile(sql);
        // Currently PGQT may accept these - documented as known issue
        println!("{}: {}", sql, result);
    }
}
