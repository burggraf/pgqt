//! Integration tests for Row-Level Security (RLS)
//!
//! These tests verify the full RLS lifecycle including:
//! - Enabling/disabling RLS on tables
//! - Creating and applying policies
//! - RLS enforcement for SELECT, INSERT, UPDATE, DELETE

use pgqt::catalog::{self, RlsPolicy};
use pgqt::rls::{self, RlsContext};
use pgqt::rls_inject;
use pgqt::transpiler;
use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    catalog::init_catalog(&conn).unwrap();
    conn
}

fn create_test_table(conn: &Connection) {
    conn.execute(
        "CREATE TABLE documents (
            id INTEGER PRIMARY KEY,
            owner TEXT NOT NULL,
            title TEXT,
            content TEXT,
            is_public BOOLEAN DEFAULT 0
        )",
        [],
    ).unwrap();
    
    // Store metadata
    catalog::store_table_metadata(&conn, "documents", &[
        ("id".to_string(), "SERIAL".to_string(), Some("PRIMARY KEY".to_string())),
        ("owner".to_string(), "TEXT".to_string(), Some("NOT NULL".to_string())),
        ("title".to_string(), "TEXT".to_string(), None),
        ("content".to_string(), "TEXT".to_string(), None),
        ("is_public".to_string(), "BOOLEAN".to_string(), Some("DEFAULT 0".to_string())),
    ]).unwrap();
    
    // Store relation metadata with owner
    catalog::store_relation_metadata(&conn, "documents", 1).unwrap();
    
    // Create test user
    conn.execute(
        "INSERT OR IGNORE INTO __pg_authid__ (oid, rolname, rolsuper, rolcanlogin) VALUES (1, 'alice', 0, 1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO __pg_authid__ (oid, rolname, rolsuper, rolcanlogin) VALUES (2, 'bob', 0, 1)",
        [],
    ).unwrap();
    
    // Insert test data
    conn.execute(
        "INSERT INTO documents (id, owner, title, content, is_public) VALUES 
            (1, 'alice', 'Alice Doc 1', 'Private content', 0),
            (2, 'alice', 'Alice Doc 2', 'Public content', 1),
            (3, 'bob', 'Bob Doc 1', 'Bob private', 0),
            (4, 'bob', 'Bob Doc 2', 'Bob public', 1)",
        [],
    ).unwrap();
}

#[test]
fn test_rls_enable_disable() {
    let conn = setup_test_db();
    create_test_table(&conn);
    
    // Initially RLS should be disabled
    assert!(!catalog::is_rls_enabled(&conn, "documents").unwrap());
    
    // Enable RLS
    catalog::enable_rls(&conn, "documents", false).unwrap();
    assert!(catalog::is_rls_enabled(&conn, "documents").unwrap());
    
    // Disable RLS
    catalog::disable_rls(&conn, "documents").unwrap();
    assert!(!catalog::is_rls_enabled(&conn, "documents").unwrap());
}

#[test]
fn test_rls_force() {
    let conn = setup_test_db();
    create_test_table(&conn);
    
    // Enable RLS with force
    catalog::enable_rls(&conn, "documents", true).unwrap();
    assert!(catalog::is_rls_forced(&conn, "documents").unwrap());
    
    // Enable RLS without force
    catalog::enable_rls(&conn, "documents", false).unwrap();
    assert!(!catalog::is_rls_forced(&conn, "documents").unwrap());
}

#[test]
fn test_create_select_policy() {
    let conn = setup_test_db();
    create_test_table(&conn);
    
    // Enable RLS
    catalog::enable_rls(&conn, "documents", false).unwrap();
    
    // Create a policy that allows users to see their own documents
    let policy = RlsPolicy {
        name: "owner_select".to_string(),
        table_name: "documents".to_string(),
        command: "SELECT".to_string(),
        permissive: true,
        roles: vec!["PUBLIC".to_string()],
        using_expr: Some("owner = current_user".to_string()),
        with_check_expr: None,
        enabled: true,
    };
    
    catalog::store_rls_policy(&conn, &policy).unwrap();
    
    // Verify policy was stored
    let policies = catalog::get_table_policies(&conn, "documents").unwrap();
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].name, "owner_select");
}

#[test]
fn test_policy_combination_permissive() {
    let _conn = setup_test_db();
    
    // Create multiple permissive policies
    let policies = vec![
        RlsPolicy {
            name: "policy1".to_string(),
            table_name: "docs".to_string(),
            command: "SELECT".to_string(),
            permissive: true,
            roles: vec!["PUBLIC".to_string()],
            using_expr: Some("owner = 'alice'".to_string()),
            with_check_expr: None,
            enabled: true,
        },
        RlsPolicy {
            name: "policy2".to_string(),
            table_name: "docs".to_string(),
            command: "SELECT".to_string(),
            permissive: true,
            roles: vec!["PUBLIC".to_string()],
            using_expr: Some("is_public = 1".to_string()),
            with_check_expr: None,
            enabled: true,
        },
    ];
    
    // Build RLS expression
    let expr = rls::build_rls_expression(&policies, true).unwrap();
    
    // Should combine with OR
    assert!(expr.contains("OR"));
    assert!(expr.contains("owner = 'alice'"));
    assert!(expr.contains("is_public = 1"));
}

#[test]
fn test_policy_combination_restrictive() {
    let _conn = setup_test_db();
    
    // Create mixed policies
    let policies = vec![
        RlsPolicy {
            name: "permissive1".to_string(),
            table_name: "docs".to_string(),
            command: "SELECT".to_string(),
            permissive: true,
            roles: vec!["PUBLIC".to_string()],
            using_expr: Some("owner = 'alice'".to_string()),
            with_check_expr: None,
            enabled: true,
        },
        RlsPolicy {
            name: "restrictive1".to_string(),
            table_name: "docs".to_string(),
            command: "SELECT".to_string(),
            permissive: false,
            roles: vec!["PUBLIC".to_string()],
            using_expr: Some("is_public = 1".to_string()),
            with_check_expr: None,
            enabled: true,
        },
    ];
    
    // Build RLS expression
    let expr = rls::build_rls_expression(&policies, true).unwrap();
    
    // Should combine: (permissive) AND (restrictive)
    // With single permissive, no OR needed
    assert!(expr.contains("AND"));
    assert!(expr.contains("owner = 'alice'"));
    assert!(expr.contains("is_public = 1"));
}

#[test]
fn test_inject_rls_into_select() {
    let original_sql = "SELECT * FROM documents WHERE id > 0";
    let rls_expr = "owner = 'alice'";
    
    let result = rls_inject::inject_rls_into_select_sql(original_sql, rls_expr);
    
    // Should contain both conditions
    assert!(result.contains("id > 0"));
    assert!(result.contains("owner = 'alice'"));
    assert!(result.contains("AND"));
}

#[test]
fn test_inject_rls_into_update() {
    let original_sql = "UPDATE documents SET title = 'New' WHERE id = 1";
    let rls_expr = "owner = 'alice'";
    
    let result = rls_inject::inject_rls_into_update_sql(original_sql, rls_expr);
    
    // Should contain both conditions
    assert!(result.contains("id = 1"));
    assert!(result.contains("owner = 'alice'"));
}

#[test]
fn test_inject_rls_into_delete() {
    let original_sql = "DELETE FROM documents WHERE id = 1";
    let rls_expr = "owner = 'alice'";
    
    let result = rls_inject::inject_rls_into_delete_sql(original_sql, rls_expr);
    
    // Should contain both conditions
    assert!(result.contains("id = 1"));
    assert!(result.contains("owner = 'alice'"));
}

#[test]
fn test_rewrite_current_user() {
    let expr = "owner = current_user";
    let rewritten = rls_inject::rewrite_rls_expression(expr, "alice", "alice");
    
    assert_eq!(rewritten, "owner = 'alice'");
}

#[test]
fn test_rls_context() {
    let ctx = RlsContext::new("alice".to_string())
        .with_roles(vec!["admin".to_string(), "editor".to_string()])
        .with_bypass(false);
    
    assert_eq!(ctx.current_user, "alice");
    assert!(ctx.user_roles.contains(&"admin".to_string()));
    assert!(ctx.user_roles.contains(&"editor".to_string()));
    assert!(ctx.user_roles.contains(&"PUBLIC".to_string())); // Always added
    assert!(!ctx.bypass_rls);
}

#[test]
fn test_get_applicable_policies() {
    let conn = setup_test_db();
    create_test_table(&conn);
    
    // Create policies for different roles
    let policy1 = RlsPolicy {
        name: "public_policy".to_string(),
        table_name: "documents".to_string(),
        command: "SELECT".to_string(),
        permissive: true,
        roles: vec!["PUBLIC".to_string()],
        using_expr: Some("is_public = 1".to_string()),
        with_check_expr: None,
        enabled: true,
    };
    
    let policy2 = RlsPolicy {
        name: "admin_policy".to_string(),
        table_name: "documents".to_string(),
        command: "SELECT".to_string(),
        permissive: true,
        roles: vec!["admin".to_string()],
        using_expr: Some("1 = 1".to_string()), // Admin sees all
        with_check_expr: None,
        enabled: true,
    };
    
    catalog::store_rls_policy(&conn, &policy1).unwrap();
    catalog::store_rls_policy(&conn, &policy2).unwrap();
    
    // Get policies for PUBLIC user
    let public_policies = catalog::get_applicable_policies(
        &conn, "documents", "SELECT", &["PUBLIC".to_string()]
    ).unwrap();
    assert_eq!(public_policies.len(), 1); // Only public_policy
    
    // Get policies for admin user
    let admin_policies = catalog::get_applicable_policies(
        &conn, "documents", "SELECT", &["PUBLIC".to_string(), "admin".to_string()]
    ).unwrap();
    assert_eq!(admin_policies.len(), 2); // Both policies
}

#[test]
fn test_drop_policy() {
    let conn = setup_test_db();
    create_test_table(&conn);
    
    // Create and store a policy
    let policy = RlsPolicy {
        name: "test_policy".to_string(),
        table_name: "documents".to_string(),
        command: "SELECT".to_string(),
        permissive: true,
        roles: vec!["PUBLIC".to_string()],
        using_expr: Some("1 = 1".to_string()),
        with_check_expr: None,
        enabled: true,
    };
    
    catalog::store_rls_policy(&conn, &policy).unwrap();
    assert_eq!(catalog::get_table_policies(&conn, "documents").unwrap().len(), 1);
    
    // Drop the policy
    catalog::drop_rls_policy(&conn, "test_policy", "documents").unwrap();
    assert_eq!(catalog::get_table_policies(&conn, "documents").unwrap().len(), 0);
}

#[test]
fn test_transpile_select() {
    let result = transpiler::transpile_with_metadata("SELECT * FROM documents");
    assert!(result.sql.to_lowercase().contains("select"));
    assert!(result.referenced_tables.contains(&"documents".to_string()));
    assert_eq!(result.operation_type, transpiler::OperationType::SELECT);
}

#[test]
fn test_transpile_update() {
    let result = transpiler::transpile_with_metadata("UPDATE documents SET title = 'test' WHERE id = 1");
    assert!(result.sql.to_lowercase().contains("update"));
    assert!(result.referenced_tables.contains(&"documents".to_string()));
    assert_eq!(result.operation_type, transpiler::OperationType::UPDATE);
}

#[test]
fn test_transpile_delete() {
    let result = transpiler::transpile_with_metadata("DELETE FROM documents WHERE id = 1");
    assert!(result.sql.to_lowercase().contains("delete"));
    assert!(result.referenced_tables.contains(&"documents".to_string()));
    assert_eq!(result.operation_type, transpiler::OperationType::DELETE);
}
