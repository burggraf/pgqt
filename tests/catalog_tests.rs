//! Unit tests for shadow catalog

use pgqt::catalog::{init_catalog, store_column_metadata, get_column_metadata, ColumnMetadata};
use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    conn
}

#[test]
fn test_init_catalog_creates_table() {
    let conn = setup_test_db();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = '__pg_meta__'",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 1);
}

#[test]
fn test_init_catalog_creates_pg_description() {
    let conn = setup_test_db();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = '__pg_description__'",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 1);
}

#[test]
fn test_store_and_retrieve_column() {
    let conn = setup_test_db();
    
    let metadata = ColumnMetadata {
        table_name: "users".to_string(),
        column_name: "email".to_string(),
        original_type: "VARCHAR(255)".to_string(),
        constraints: Some("UNIQUE NOT NULL".to_string()),
    };
    
    store_column_metadata(&conn, &metadata).unwrap();
    
    let retrieved = get_column_metadata(&conn, "users", "email")
        .unwrap()
        .expect("Should find metadata");
    
    assert_eq!(retrieved.table_name, "users");
    assert_eq!(retrieved.column_name, "email");
    assert_eq!(retrieved.original_type, "VARCHAR(255)");
    assert_eq!(retrieved.constraints, Some("UNIQUE NOT NULL".to_string()));
}

#[test]
fn test_get_nonexistent_column() {
    let conn = setup_test_db();
    
    let result = get_column_metadata(&conn, "nonexistent", "col").unwrap();
    assert!(result.is_none());
}
