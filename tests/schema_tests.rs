//! Unit tests for schema/namespace support

use pgqt::schema::{SearchPath, SchemaManager, init_schema_catalog, create_schema, drop_schema, schema_exists, list_schemas};
use pgqt::transpiler::transpile;
use rusqlite::Connection;
use std::path::Path;

// ============================================================================
// SearchPath Tests
// ============================================================================

#[test]
fn test_search_path_default() {
    let path = SearchPath::default();
    assert_eq!(path.schemas, vec!["$user", "public"]);
}

#[test]
fn test_search_path_parse_simple() {
    let path = SearchPath::parse("schema1, public").unwrap();
    assert_eq!(path.schemas, vec!["schema1", "public"]);
}

#[test]
fn test_search_path_parse_with_quotes() {
    let path = SearchPath::parse("\"$user\", public").unwrap();
    assert_eq!(path.schemas, vec!["$user", "public"]);
}

#[test]
fn test_search_path_parse_empty() {
    let path = SearchPath::parse("").unwrap();
    assert_eq!(path.schemas, vec!["$user", "public"]); // default
}

#[test]
fn test_search_path_parse_whitespace() {
    let path = SearchPath::parse("  schema1  ,  public  ").unwrap();
    assert_eq!(path.schemas, vec!["schema1", "public"]);
}

#[test]
fn test_search_path_to_string() {
    let path = SearchPath::parse("schema1, public").unwrap();
    assert_eq!(path.to_string(), "schema1, public");
}

#[test]
fn test_search_path_to_string_with_special() {
    let path = SearchPath::parse("$user, public").unwrap();
    assert_eq!(path.to_string(), "\"$user\", public");
}

// ============================================================================
// SchemaManager Tests
// ============================================================================

#[test]
fn test_schema_manager_path_public() {
    let manager = SchemaManager::new(Path::new("/data/myapp.db"));
    assert_eq!(manager.schema_db_path("public"), std::path::PathBuf::from("/data/myapp.db"));
    assert_eq!(manager.schema_db_path(""), std::path::PathBuf::from("/data/myapp.db"));
}

#[test]
fn test_schema_manager_path_custom() {
    let manager = SchemaManager::new(Path::new("/data/myapp.db"));
    assert_eq!(manager.schema_db_path("inventory"), std::path::PathBuf::from("/data/myapp_inventory.db"));
    assert_eq!(manager.schema_db_path("analytics"), std::path::PathBuf::from("/data/myapp_analytics.db"));
}

#[test]
fn test_schema_manager_path_relative() {
    let manager = SchemaManager::new(Path::new("test.db"));
    assert_eq!(manager.schema_db_path("inventory"), std::path::PathBuf::from("test_inventory.db"));
}

// ============================================================================
// Schema Catalog Tests
// ============================================================================

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    init_schema_catalog(&conn).unwrap();
    conn
}

#[test]
fn test_init_schema_catalog() {
    let conn = Connection::open_in_memory().unwrap();
    init_schema_catalog(&conn).unwrap();

    // Check default schemas exist
    assert!(schema_exists(&conn, "public").unwrap());
    assert!(schema_exists(&conn, "pg_catalog").unwrap());
    assert!(schema_exists(&conn, "information_schema").unwrap());
}

#[test]
fn test_create_schema() {
    let conn = setup_test_db();

    let oid = create_schema(&conn, "test_schema", Some(10)).unwrap();
    assert!(oid > 0);
    assert!(schema_exists(&conn, "test_schema").unwrap());
}

#[test]
fn test_create_schema_lowercase() {
    let conn = setup_test_db();

    create_schema(&conn, "TestSchema", Some(10)).unwrap();
    assert!(schema_exists(&conn, "testschema").unwrap());
}

#[test]
fn test_create_duplicate_schema() {
    let conn = setup_test_db();

    create_schema(&conn, "test_schema", Some(10)).unwrap();
    let result = create_schema(&conn, "test_schema", Some(10));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}

#[test]
fn test_create_schema_pg_prefix() {
    let conn = setup_test_db();

    let result = create_schema(&conn, "pg_myschema", Some(10));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unacceptable schema name"));
}

#[test]
fn test_drop_schema() {
    let conn = setup_test_db();

    create_schema(&conn, "test_schema", Some(10)).unwrap();
    assert!(schema_exists(&conn, "test_schema").unwrap());

    drop_schema(&conn, "test_schema").unwrap();
    assert!(!schema_exists(&conn, "test_schema").unwrap());
}

#[test]
fn test_drop_nonexistent_schema() {
    let conn = setup_test_db();

    let result = drop_schema(&conn, "nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}

#[test]
fn test_drop_public_schema() {
    let conn = setup_test_db();

    let result = drop_schema(&conn, "public");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot drop schema \"public\""));
}

#[test]
fn test_drop_system_schema() {
    let conn = setup_test_db();

    let result = drop_schema(&conn, "pg_catalog");
    assert!(result.is_err());

    let result = drop_schema(&conn, "information_schema");
    assert!(result.is_err());
}

#[test]
fn test_list_schemas() {
    let conn = setup_test_db();

    create_schema(&conn, "alpha", Some(10)).unwrap();
    create_schema(&conn, "beta", Some(10)).unwrap();

    let schemas = list_schemas(&conn).unwrap();
    let names: Vec<&str> = schemas.iter().map(|s| s.nspname.as_str()).collect();

    assert!(names.contains(&"public"));
    assert!(names.contains(&"pg_catalog"));
    assert!(names.contains(&"information_schema"));
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

// ============================================================================
// Transpiler Tests
// ============================================================================

#[test]
fn test_transpile_schema_public() {
    let input = "SELECT * FROM public.users";
    let result = transpile(input);
    // public schema should be stripped
    assert!(!result.contains("public."));
    assert!(result.contains("users"));
}

#[test]
fn test_transpile_schema_pg_catalog() {
    let input = "SELECT * FROM pg_catalog.pg_class";
    let result = transpile(input);
    // pg_catalog should be stripped
    assert!(!result.contains("pg_catalog."));
}

#[test]
fn test_transpile_schema_custom() {
    let input = "SELECT * FROM inventory.products";
    let result = transpile(input);
    // Custom schema should be preserved
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_create_table_in_schema() {
    let input = "CREATE TABLE inventory.products (id SERIAL, name TEXT)";
    let result = transpile(input);
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_insert_into_schema() {
    let input = "INSERT INTO inventory.products (name) VALUES ('Widget')";
    let result = transpile(input);
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_update_schema() {
    let input = "UPDATE inventory.products SET name = 'Gadget' WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_delete_from_schema() {
    let input = "DELETE FROM inventory.products WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_join_across_schemas() {
    let input = "SELECT p.name, o.total FROM inventory.products p JOIN public.orders o ON p.id = o.product_id";
    let result = transpile(input);
    assert!(result.contains("inventory.products"));
    assert!(result.contains("orders")); // public.orders should become just orders
}

#[test]
fn test_transpile_multiple_schema_references() {
    let input = "SELECT a.name, b.name FROM schema1.table1 a, schema2.table2 b";
    let result = transpile(input);
    assert!(result.contains("schema1.table1"));
    assert!(result.contains("schema2.table2"));
}

#[test]
fn test_transpile_schema_with_alias() {
    let input = "SELECT p.name FROM inventory.products AS p";
    let result = transpile(input);
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_schema_case_insensitive() {
    let input = "SELECT * FROM INVENTORY.Products";
    let result = transpile(input);
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_schema_quoted() {
    let input = "SELECT * FROM \"MySchema\".\"MyTable\"";
    let result = transpile(input);
    // Should preserve the schema prefix
    assert!(result.contains("myschema.mytable"));
}
