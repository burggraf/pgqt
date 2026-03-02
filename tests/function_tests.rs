use pgqt::catalog::{init_catalog, FunctionMetadata, ParamMode, ReturnTypeKind};
use pgqt::transpiler::parse_create_function;
use rusqlite::Connection;

#[test]
fn test_parse_simple_function() {
    let sql = r#"
        CREATE FUNCTION add_numbers(a integer, b integer)
        RETURNS integer
        LANGUAGE sql
        AS $$
            SELECT a + b
        $$;
    "#;
    
    let metadata = parse_create_function(sql).unwrap();
    
    assert_eq!(metadata.name, "add_numbers");
    assert_eq!(metadata.arg_types, vec!["INT4", "INT4"]);
    assert_eq!(metadata.arg_modes, vec![ParamMode::In, ParamMode::In]);
    assert_eq!(metadata.return_type, "INT4");
    assert_eq!(metadata.return_type_kind, ReturnTypeKind::Scalar);
    assert_eq!(metadata.language, "sql");
    assert!(!metadata.strict);
}

#[test]
fn test_parse_strict_function() {
    let sql = r#"
        CREATE FUNCTION square(x integer)
        RETURNS integer
        LANGUAGE sql
        STRICT
        AS $$
            SELECT x * x
        $$;
    "#;
    
    let metadata = parse_create_function(sql).unwrap();
    
    assert_eq!(metadata.name, "square");
    assert!(metadata.strict);
}

#[test]
fn test_parse_function_with_out_params() {
    let sql = r#"
        CREATE FUNCTION get_user_info(user_id integer, OUT username text, OUT email text)
        LANGUAGE sql
        AS $$
            SELECT username, email FROM users WHERE id = user_id
        $$;
    "#;
    
    let metadata = parse_create_function(sql).unwrap();
    
    assert_eq!(metadata.name, "get_user_info");
    assert_eq!(metadata.arg_types.len(), 3);
    assert_eq!(metadata.arg_modes[0], ParamMode::In);
    assert_eq!(metadata.arg_modes[1], ParamMode::Out);
    assert_eq!(metadata.arg_modes[2], ParamMode::Out);
}

#[test]
fn test_store_and_retrieve_function() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    let metadata = FunctionMetadata {
        oid: 0,
        name: "test_func".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["integer".to_string()],
        arg_names: vec!["x".to_string()],
        arg_modes: vec![ParamMode::In],
        return_type: "integer".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: "SELECT $1 * 2".to_string(),
        language: "sql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    // Store function
    let oid = pgqt::catalog::store_function(&conn, &metadata).unwrap();
    assert!(oid > 0);
    
    // Debug: Check what's in the database
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM __pg_functions__", [], |row| row.get(0)).unwrap();
    println!("Function count: {}", count);
    
    // Retrieve function
    let retrieved = pgqt::catalog::get_function(&conn, "test_func", None).unwrap().unwrap();
    assert_eq!(retrieved.name, "test_func");
    assert_eq!(retrieved.arg_types, vec!["integer"]);
}

#[test]
fn test_drop_function() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    let metadata = FunctionMetadata {
        oid: 0,
        name: "test_func".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["integer".to_string()],
        arg_names: vec!["x".to_string()],
        arg_modes: vec![ParamMode::In],
        return_type: "integer".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: "SELECT $1 * 2".to_string(),
        language: "sql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    pgqt::catalog::store_function(&conn, &metadata).unwrap();
    
    // Verify function exists
    let exists = pgqt::catalog::get_function(&conn, "test_func", None).unwrap();
    assert!(exists.is_some());
    
    // Drop function
    let dropped = pgqt::catalog::drop_function(&conn, "test_func", None).unwrap();
    assert!(dropped);
    
    // Verify function no longer exists
    let exists = pgqt::catalog::get_function(&conn, "test_func", None).unwrap();
    assert!(exists.is_none());
}

#[test]
fn test_execute_simple_function() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    // Create a table for testing
    conn.execute("CREATE TABLE test_table (id INTEGER, value INTEGER)", []).unwrap();
    conn.execute("INSERT INTO test_table VALUES (1, 10), (2, 20)", []).unwrap();
    
    let metadata = FunctionMetadata {
        oid: 0,
        name: "double_value".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["integer".to_string()],
        arg_names: vec!["x".to_string()],
        arg_modes: vec![ParamMode::In],
        return_type: "integer".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: "SELECT $1 * 2".to_string(),
        language: "sql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    let result = pgqt::functions::execute_sql_function(&conn, &metadata, &[10.into()]).unwrap();
    
    match result {
        pgqt::functions::FunctionResult::Scalar(Some(val)) => {
            assert_eq!(val, rusqlite::types::Value::Integer(20));
        }
        _ => panic!("Expected Scalar result"),
    }
}

#[test]
fn test_execute_strict_function_with_null() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    let metadata = FunctionMetadata {
        oid: 0,
        name: "test_func".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["integer".to_string()],
        arg_names: vec!["x".to_string()],
        arg_modes: vec![ParamMode::In],
        return_type: "integer".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: "SELECT $1 * 2".to_string(),
        language: "sql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: true,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    let result = pgqt::functions::execute_sql_function(&conn, &metadata, &[rusqlite::types::Value::Null]).unwrap();
    
    match result {
        pgqt::functions::FunctionResult::Null => {
            // Correct behavior for STRICT function with NULL input
        }
        _ => panic!("Expected Null result for STRICT function with NULL input"),
    }
}

#[test]
fn test_execute_table_function() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    // Create a table for testing
    conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", []).unwrap();
    conn.execute("INSERT INTO users VALUES (1, 'Alice'), (2, 'Bob')", []).unwrap();
    
    let metadata = FunctionMetadata {
        oid: 0,
        name: "get_users".to_string(),
        schema: "public".to_string(),
        arg_types: vec![],
        arg_names: vec![],
        arg_modes: vec![],
        return_type: "TABLE".to_string(),
        return_type_kind: ReturnTypeKind::Table,
        return_table_cols: None,
        function_body: "SELECT id, name FROM users".to_string(),
        language: "sql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    let result = pgqt::functions::execute_sql_function(&conn, &metadata, &[]).unwrap();
    
    match result {
        pgqt::functions::FunctionResult::Table(rows) => {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].len(), 2);
        }
        _ => panic!("Expected Table result"),
    }
}

#[test]
fn test_create_or_replace_function() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    let sql1 = r#"
        CREATE FUNCTION test_func(x integer)
        RETURNS integer
        LANGUAGE sql
        AS $$
            SELECT x * 2
        $$;
    "#;
    
    let metadata1 = parse_create_function(sql1).unwrap();
    pgqt::catalog::store_function(&conn, &metadata1).unwrap();
    
    // Replace with new implementation
    let sql2 = r#"
        CREATE OR REPLACE FUNCTION test_func(x integer)
        RETURNS integer
        LANGUAGE sql
        AS $$
            SELECT x * 3
        $$;
    "#;
    
    let metadata2 = parse_create_function(sql2).unwrap();
    pgqt::catalog::store_function(&conn, &metadata2).unwrap();
    
    // Should have two versions (INSERT, not UPDATE)
    let functions = pgqt::catalog::get_function(&conn, "test_func", None).unwrap();
    assert!(functions.is_some());
}
