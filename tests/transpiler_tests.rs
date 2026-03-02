//! Unit tests for SQL transpilation

use pgqt::transpiler::{transpile, transpile_with_metadata};

#[test]
fn test_transpile_simple_select() {
    let input = "SELECT * FROM users";
    let result = transpile(input);
    assert_eq!(result, "select * from users");
}

#[test]
fn test_transpile_select_columns() {
    let input = "SELECT id, name, email FROM users";
    let result = transpile(input);
    assert_eq!(result, "select id, name, email from users");
}

#[test]
fn test_transpile_where_clause() {
    let input = "SELECT * FROM users WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("where"));
    assert!(result.contains("id = 1"));
}

#[test]
fn test_transpile_limit() {
    let input = "SELECT * FROM users LIMIT 10";
    let result = transpile(input);
    assert!(result.contains("limit 10"));
}

#[test]
fn test_transpile_limit_all() {
    let input = "SELECT * FROM users LIMIT ALL";
    let result = transpile(input);
    assert!(result.contains("limit -1"));
}

#[test]
fn test_transpile_offset() {
    let input = "SELECT * FROM users LIMIT 10 OFFSET 20";
    let result = transpile(input);
    assert!(result.contains("limit 10"));
    assert!(result.contains("offset 20"));
}

#[test]
fn test_transpile_order_by() {
    let input = "SELECT * FROM users ORDER BY name ASC, id DESC";
    let result = transpile(input);
    assert!(result.contains("order by"));
    assert!(result.contains("name asc"));
    assert!(result.contains("id desc"));
}

#[test]
fn test_transpile_distinct() {
    let input = "SELECT DISTINCT status FROM orders";
    let result = transpile(input);
    assert!(result.contains("distinct"));
}

#[test]
fn test_transpile_schema_public() {
    let input = "SELECT * FROM public.users";
    let result = transpile(input);
    // Should strip 'public' schema
    assert!(!result.contains("public."));
    assert!(result.contains("users"));
}

#[test]
fn test_transpile_schema_other() {
    let input = "SELECT * FROM inventory.products";
    let result = transpile(input);
    // Should preserve other schemas (attached databases)
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_now() {
    let input = "SELECT now()";
    let result = transpile(input);
    assert!(result.contains("datetime('now')"));
}

#[test]
fn test_transpile_cast_int() {
    let input = "SELECT '123'::int";
    let result = transpile(input);
    assert!(result.contains("cast("));
    assert!(result.contains("as integer"));
}

#[test]
fn test_transpile_cast_text() {
    let input = "SELECT 123::text";
    let result = transpile(input);
    assert!(result.contains("cast("));
    assert!(result.contains("as text"));
}

#[test]
fn test_transpile_like_operator() {
    let input = "SELECT * FROM users WHERE name ~~ 'Alice%'";
    let result = transpile(input);
    assert!(result.contains("like"));
}

#[test]
fn test_transpile_not_like_operator() {
    let input = "SELECT * FROM users WHERE name !~~ 'Alice%'";
    let result = transpile(input);
    assert!(result.contains("not like"));
}

#[test]
fn test_create_table_metadata() {
    let input = "CREATE TABLE test (id SERIAL, name VARCHAR(100))";
    let result = transpile_with_metadata(input);
    
    // Check SQL was generated
    assert!(result.sql.contains("create table"));
    assert!(result.sql.contains("integer primary key autoincrement"));
    assert!(result.sql.contains("text"));
    
    // Check metadata was extracted
    let metadata = result.create_table_metadata.expect("Should have metadata");
    assert_eq!(metadata.table_name, "test");
    assert_eq!(metadata.columns.len(), 2);
}

#[test]
fn test_create_table_timestamp_types() {
    let input = "CREATE TABLE events (id SERIAL, created_at TIMESTAMP WITH TIME ZONE)";
    let result = transpile_with_metadata(input);
    
    let metadata = result.create_table_metadata.expect("Should have metadata");
    let ts_col = metadata.columns.iter().find(|c| c.column_name == "created_at").unwrap();
    assert_eq!(ts_col.original_type, "TIMESTAMP WITH TIME ZONE");
}

#[test]
fn test_create_table_boolean() {
    let input = "CREATE TABLE flags (id SERIAL, is_active BOOLEAN)";
    let result = transpile_with_metadata(input);
    
    // SQLite should use INTEGER
    assert!(result.sql.contains("integer"));
    
    // But metadata should preserve BOOLEAN
    let metadata = result.create_table_metadata.expect("Should have metadata");
    let bool_col = metadata.columns.iter().find(|c| c.column_name == "is_active").unwrap();
    assert_eq!(bool_col.original_type, "BOOLEAN");
}

#[test]
fn test_create_table_json() {
    let input = "CREATE TABLE data (id SERIAL, payload JSONB)";
    let result = transpile_with_metadata(input);
    
    // SQLite stores as TEXT
    assert!(result.sql.contains("text"));
    
    // Metadata preserves JSONB
    let metadata = result.create_table_metadata.expect("Should have metadata");
    let json_col = metadata.columns.iter().find(|c| c.column_name == "payload").unwrap();
    assert_eq!(json_col.original_type, "JSONB");
}

#[test]
fn test_insert_statement() {
    let input = "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')";
    let result = transpile(input);
    assert!(result.contains("insert into"));
    assert!(result.contains("values"));
}

#[test]
fn test_update_statement() {
    let input = "UPDATE users SET name = 'Bob' WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("update"));
    assert!(result.contains("set"));
    assert!(result.contains("where"));
}

#[test]
fn test_delete_statement() {
    let input = "DELETE FROM users WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("delete from"));
    assert!(result.contains("where"));
}

#[test]
fn test_join() {
    let input = "SELECT u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id";
    let result = transpile(input);
    assert!(result.contains("join"));
    assert!(result.contains("on"));
}

#[test]
fn test_subquery() {
    let input = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)";
    let result = transpile(input);
    assert!(result.contains("select"));
    assert!(result.contains("where"));
    assert!(result.contains("in"));
}

#[test]
fn test_group_by() {
    let input = "SELECT status, COUNT(*) FROM orders GROUP BY status";
    let result = transpile(input);
    assert!(result.contains("group by"));
    assert!(result.contains("count"));
}

#[test]
fn test_alias() {
    let input = "SELECT u.name AS user_name FROM users u";
    let result = transpile(input);
    assert!(result.contains("as"));
}
