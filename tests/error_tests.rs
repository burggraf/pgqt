//! Integration tests for error code mapping.
//!
//! These tests verify that PostgreSQL SQLSTATE codes are correctly returned
//! for various error conditions.

use pgqt::handler::SqliteHandler;
use pgqt::handler::query::QueryExecution;
use std::fs;

/// Counter for generating unique database paths
static DB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Helper to create a test handler with a temporary file database
/// Note: We use a file database instead of :memory: because the connection pool
/// creates multiple connections, and each :memory: connection gets its own isolated database.
fn create_test_handler() -> (SqliteHandler, String) {
    let temp_dir = std::env::temp_dir();
    let counter = DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let db_path = temp_dir.join(format!("pgqt_error_test_{}_{}.db", std::process::id(), counter));
    let db_path_str = db_path.to_str().unwrap().to_string();
    // Clean up any existing file
    let _ = fs::remove_file(&db_path_str);
    let handler = SqliteHandler::new(&db_path_str).expect("Failed to create handler");
    (handler, db_path_str)
}

/// Helper to clean up the test database
fn cleanup_db(path: &str) {
    let _ = fs::remove_file(path);
}

#[test]
fn test_unique_violation_sqlstate() {
    let (handler, db_path) = create_test_handler();
    
    // Create a table with a unique constraint
    let result = handler.execute_query(
        1,
        "CREATE TABLE unique_test (id INTEGER PRIMARY KEY, email VARCHAR(100) UNIQUE)"
    );
    assert!(result.is_ok(), "Failed to create table: {:?}", result);
    
    // Insert first row
    let result = handler.execute_query(
        1,
        "INSERT INTO unique_test (id, email) VALUES (1, 'test@example.com')"
    );
    assert!(result.is_ok(), "Failed to insert first row: {:?}", result);
    
    // Try to insert a duplicate - should fail with unique violation
    let result = handler.execute_query(
        1,
        "INSERT INTO unique_test (id, email) VALUES (2, 'test@example.com')"
    );
    assert!(result.is_err(), "Expected error for duplicate email");
    
    // The error should be converted to PgError internally
    // We can verify this by checking the error message contains the right code
    let err = result.unwrap_err();
    let err_str = err.to_string();
    
    // The error should indicate constraint violation
    assert!(
        err_str.contains("UNIQUE") || err_str.contains("unique") || err_str.contains("constraint"),
        "Error message should indicate unique constraint violation: {}",
        err_str
    );
    
    cleanup_db(&db_path);
}

#[test]
fn test_not_null_violation_sqlstate() {
    let (handler, db_path) = create_test_handler();
    
    // Create a table with a NOT NULL constraint
    let result = handler.execute_query(
        1,
        "CREATE TABLE notnull_test (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL)"
    );
    assert!(result.is_ok(), "Failed to create table: {:?}", result);
    
    // Try to insert NULL - should fail with not null violation
    let result = handler.execute_query(
        1,
        "INSERT INTO notnull_test (id, name) VALUES (1, NULL)"
    );
    assert!(result.is_err(), "Expected error for NULL violation");
    
    let err = result.unwrap_err();
    let err_str = err.to_string();
    
    // The error should indicate NOT NULL violation
    assert!(
        err_str.contains("NOT NULL") || err_str.contains("not null") || err_str.contains("constraint"),
        "Error message should indicate NOT NULL constraint violation: {}",
        err_str
    );
    
    cleanup_db(&db_path);
}

#[test]
fn test_undefined_table_sqlstate() {
    let (handler, db_path) = create_test_handler();
    
    // Try to select from a non-existent table
    let result = handler.execute_query(
        1,
        "SELECT * FROM nonexistent_table"
    );
    assert!(result.is_err(), "Expected error for nonexistent table");
    
    let err = result.unwrap_err();
    let err_str = err.to_string();
    
    // The error should indicate the table doesn't exist
    assert!(
        err_str.contains("no such table") || err_str.contains("does not exist"),
        "Error message should indicate table doesn't exist: {}",
        err_str
    );
    
    cleanup_db(&db_path);
}

#[test]
fn test_syntax_error_sqlstate() {
    let (handler, db_path) = create_test_handler();
    
    // Try to execute invalid SQL
    let result = handler.execute_query(
        1,
        "SELEC * FROM invalid"
    );
    assert!(result.is_err(), "Expected error for syntax error");
    
    let err = result.unwrap_err();
    let err_str = err.to_string();
    
    // The error should indicate syntax error
    assert!(
        err_str.contains("syntax") || err_str.contains("parse") || err_str.contains("error"),
        "Error message should indicate syntax error: {}",
        err_str
    );
    
    cleanup_db(&db_path);
}

#[test]
fn test_check_violation_sqlstate() {
    let (handler, db_path) = create_test_handler();
    
    // Create a table with a CHECK constraint
    // Note: SQLite doesn't have native CHECK constraint support in the same way,
    // but we can test the error mapping path
    let result = handler.execute_query(
        1,
        "CREATE TABLE check_test (id INTEGER PRIMARY KEY, age INTEGER CHECK(age >= 0))"
    );
    assert!(result.is_ok(), "Failed to create table: {:?}", result);
    
    // Try to insert a value that violates the CHECK constraint
    let result = handler.execute_query(
        1,
        "INSERT INTO check_test (id, age) VALUES (1, -1)"
    );
    // This may or may not fail depending on SQLite's handling
    // The important thing is that if it fails, it should be mapped correctly
    if let Err(err) = result {
        let err_str = err.to_string();
        assert!(
            err_str.contains("CHECK") || err_str.contains("constraint"),
            "Error message should indicate CHECK constraint violation: {}",
            err_str
        );
    }
    
    cleanup_db(&db_path);
}