use rusqlite::Connection;
use pgqt::catalog::init_catalog;
use pgqt::catalog::init_system_views;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    init_system_views(&conn).unwrap();
    conn
}

#[test]
fn test_pg_class_columns() {
    let conn = setup_test_db();
    
    // Check if critical columns exist in pg_class
    let stmt = conn.prepare("SELECT * FROM pg_class LIMIT 0").unwrap();
    let column_names = stmt.column_names();
    
    assert!(column_names.contains(&"reltoastrelid"));
    assert!(column_names.contains(&"reltoastidxid"));
    assert!(column_names.contains(&"relhasindex"));
    assert!(column_names.contains(&"relkind"));
    assert!(column_names.contains(&"relnatts"));
    assert!(column_names.contains(&"parttype"));
    assert!(column_names.contains(&"relrowsecurity"));
}

#[test]
fn test_pg_attribute_columns() {
    let conn = setup_test_db();
    
    // Check if critical columns exist in pg_attribute
    let stmt = conn.prepare("SELECT * FROM pg_attribute LIMIT 0").unwrap();
    let column_names = stmt.column_names();
    
    assert!(column_names.contains(&"attrelid"));
    assert!(column_names.contains(&"attname"));
    assert!(column_names.contains(&"atttypid"));
    assert!(column_names.contains(&"attnum"));
    assert!(column_names.contains(&"attcompression"));
}

#[test]
fn test_pg_proc_columns() {
    let conn = setup_test_db();
    
    // Check if critical columns exist in pg_proc
    let stmt = conn.prepare("SELECT * FROM pg_proc LIMIT 0").unwrap();
    let column_names = stmt.column_names();
    
    assert!(column_names.contains(&"proname"));
    assert!(column_names.contains(&"prorettype"));
    assert!(column_names.contains(&"prosrc"));
}

#[test]
fn test_pg_trigger_columns() {
    let conn = setup_test_db();
    
    // Check if critical columns exist in pg_trigger
    let stmt = conn.prepare("SELECT * FROM pg_trigger LIMIT 0").unwrap();
    let column_names = stmt.column_names();
    
    assert!(column_names.contains(&"tgname"));
    assert!(column_names.contains(&"tgrelid"));
}

#[test]
fn test_pg_class_content() {
    let conn = setup_test_db();
    
    // Create a test table
    conn.execute("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)", []).unwrap();
    
    // pg_class should contain the table
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_class WHERE relname = 'test_table'",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 1);
    
    // relkind should be 'r' for table
    let kind: String = conn.query_row(
        "SELECT relkind FROM pg_class WHERE relname = 'test_table'",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(kind, "r");
}

#[test]
fn test_pg_proc_content() {
    let conn = setup_test_db();
    
    // Insert a test function into __pg_functions__
    conn.execute(
        "INSERT INTO __pg_functions__ 
         (funcname, schema_name, arg_types, return_type, return_type_kind, function_body, owner_oid)
         VALUES ('test_func', 'public', '[\"text\"]', 'integer', 'SCALAR', 'SELECT 1', 10)",
        [],
    ).unwrap();
    
    // pg_proc should contain the function
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_proc WHERE proname = 'test_func'",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 1);
    
    // Check other columns
    let (proretset, pronargs): (bool, i64) = conn.query_row(
        "SELECT proretset, pronargs FROM pg_proc WHERE proname = 'test_func'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?))
    ).unwrap();
    assert_eq!(proretset, false);
    assert_eq!(pronargs, 1);
}
