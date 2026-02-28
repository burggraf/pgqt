//! Unit tests for Row-Level Security

use postgresqlite::rls::{RlsManager, RlsPolicy, RlsCommand};
use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    
    // Create test table
    conn.execute(
        "CREATE TABLE documents (id INTEGER PRIMARY KEY, owner TEXT, content TEXT)",
        [],
    ).unwrap();
    
    conn
}

#[test]
fn test_enable_rls() {
    let conn = setup_test_db();
    
    RlsManager::enable_rls(&conn, "documents").unwrap();
    
    // Verify view was created
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='documents'",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 1);
    
    // Verify original table was renamed
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_documents_data'",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 1);
}

#[test]
fn test_create_policy() {
    let conn = setup_test_db();
    let mut manager = RlsManager::new();
    
    RlsManager::enable_rls(&conn, "documents").unwrap();
    
    let policy = RlsPolicy {
        name: "owner_policy".to_string(),
        table_name: "documents".to_string(),
        command: RlsCommand::Select,
        permissive: true,
        using_expr: Some("owner = 'alice'".to_string()),
        with_check_expr: None,
        roles: vec![],
    };
    
    manager.create_policy(&conn, policy).unwrap();
    
    assert!(manager.is_rls_enabled("documents"));
    assert_eq!(manager.get_policies("documents").unwrap().len(), 1);
}
