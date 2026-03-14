use std::sync::Arc;
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

#[test]
fn test_debug_function_body() {
    let sql = r#"
        CREATE FUNCTION test_func(x integer)
        RETURNS integer
        LANGUAGE sql
        AS $$
            SELECT x * 2
        $$;
    "#;
    
    let metadata = pgqt::transpiler::parse_create_function(sql).unwrap();
    
    println!("Function name: {}", metadata.name);
    println!("Function body: {:?}", metadata.function_body);
    println!("Arg names: {:?}", metadata.arg_names);
    
    // The body should contain the actual SQL, not "SELECT 1"
    assert!(!metadata.function_body.contains("SELECT 1"), 
            "Function body should not be default 'SELECT 1', got: {}", metadata.function_body);
}

#[test]
fn test_parse_returns_table() {
    let sql = r#"
        CREATE FUNCTION get_users()
        RETURNS TABLE(id integer, name text)
        LANGUAGE sql
        AS $$
            SELECT id, name FROM users
        $$;
    "#;
    
    let metadata = parse_create_function(sql).unwrap();
    
    assert_eq!(metadata.name, "get_users");
    assert_eq!(metadata.return_type_kind, ReturnTypeKind::Table);
    
    let cols = metadata.return_table_cols.expect("Should have return table columns");
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0].0, "id");
    assert_eq!(cols[0].1, "INT4");
    assert_eq!(cols[1].0, "name");
    assert_eq!(cols[1].1, "TEXT");
}

#[test]
fn test_parse_returns_void() {
    let sql = r#"
        CREATE FUNCTION log_it(msg text)
        RETURNS void
        LANGUAGE sql
        AS $$
            INSERT INTO logs(msg) VALUES(msg)
        $$;
    "#;
    
    let metadata = parse_create_function(sql).unwrap();
    
    assert_eq!(metadata.name, "log_it");
    assert_eq!(metadata.return_type_kind, ReturnTypeKind::Void);
    assert_eq!(metadata.return_type, "VOID");
}

#[test]
fn test_builtin_repeat_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();
    
    let mut stmt = conn.prepare("SELECT repeat('ab', 3)").unwrap();
    let result: String = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(result, "ababab");
    
    let mut stmt = conn.prepare("SELECT repeat('a', 0)").unwrap();
    let result: String = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(result, "");
    
    let mut stmt = conn.prepare("SELECT repeat('a', -5)").unwrap();
    let result: String = stmt.query_row([], |row| row.get(0)).unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_builtin_repeat_function_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "INSERT INTO t (b) VALUES (repeat('x', 5))";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("repeat('x', 5)"));
}

#[test]
fn test_builtin_power_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();
    
    // Test basic power: 2^3 = 8
    let result: f64 = conn.query_row("SELECT power(2.0, 3.0)", [], |row| row.get(0)).unwrap();
    assert!((result - 8.0).abs() < 0.0001, "Expected 8.0, got {}", result);
    
    // Test power of 0: 5^0 = 1
    let result: f64 = conn.query_row("SELECT power(5.0, 0.0)", [], |row| row.get(0)).unwrap();
    assert!((result - 1.0).abs() < 0.0001, "Expected 1.0, got {}", result);
    
    // Test negative exponent: 2^-1 = 0.5
    let result: f64 = conn.query_row("SELECT power(2.0, -1.0)", [], |row| row.get(0)).unwrap();
    assert!((result - 0.5).abs() < 0.0001, "Expected 0.5, got {}", result);
    
    // Test fractional exponent: 4^0.5 = 2
    let result: f64 = conn.query_row("SELECT power(4.0, 0.5)", [], |row| row.get(0)).unwrap();
    assert!((result - 2.0).abs() < 0.0001, "Expected 2.0, got {}", result);
}

#[test]
fn test_builtin_power_function_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT power(base, exp) FROM t";
    let result = transpile(input);
    assert!(result.contains("power"), "Transpiled SQL should contain power function");
}

#[test]
fn test_builtin_split_part_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();
    
    // Test basic split: split_part('abc~def~ghi', '~', 2) => 'def'
    let result: String = conn.query_row("SELECT split_part('abc~def~ghi', '~', 2)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "def");
    
    // Test first part: split_part('abc~def', '~', 1) => 'abc'
    let result: String = conn.query_row("SELECT split_part('abc~def', '~', 1)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "abc");
    
    // Test last part with negative index: split_part('abc~def~ghi', '~', -1) => 'ghi'
    let result: String = conn.query_row("SELECT split_part('abc~def~ghi', '~', -1)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "ghi");
    
    // Test out of range (returns empty string): split_part('abc~def', '~', 5) => ''
    let result: String = conn.query_row("SELECT split_part('abc~def', '~', 5)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "");
    
    // Test index 0 (returns empty string)
    let result: String = conn.query_row("SELECT split_part('abc~def', '~', 0)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "");
    
    // Test single element
    let result: String = conn.query_row("SELECT split_part('abc', '~', 1)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "abc");
}

#[test]
fn test_builtin_split_part_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT split_part(name, '-', 2) FROM users";
    let result = transpile(input);
    assert!(result.contains("split_part"), "Transpiled SQL should contain split_part function");
}

#[test]
fn test_builtin_date_trunc_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();
    
    // Test year truncation: date_trunc('year', '2024-03-15 10:30:45') => '2024-01-01 00:00:00'
    let result: String = conn.query_row("SELECT date_trunc('year', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-01-01 00:00:00");
    
    // Test month truncation: date_trunc('month', '2024-03-15 10:30:45') => '2024-03-01 00:00:00'
    let result: String = conn.query_row("SELECT date_trunc('month', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-01 00:00:00");
    
    // Test day truncation: date_trunc('day', '2024-03-15 10:30:45') => '2024-03-15 00:00:00'
    let result: String = conn.query_row("SELECT date_trunc('day', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15 00:00:00");
    
    // Test hour truncation: date_trunc('hour', '2024-03-15 10:30:45') => '2024-03-15 10:00:00'
    let result: String = conn.query_row("SELECT date_trunc('hour', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15 10:00:00");
    
    // Test quarter truncation: Q2 starts in April
    let result: String = conn.query_row("SELECT date_trunc('quarter', '2024-05-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-04-01 00:00:00");
    
    // Test week truncation (should return Monday)
    let result: String = conn.query_row("SELECT date_trunc('week', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    // March 15, 2024 is a Friday, so week should start on Monday March 11
    assert!(result.starts_with("2024-03-11"), "Expected Monday of that week, got {}", result);
}

#[test]
fn test_builtin_date_trunc_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT date_trunc('month', created_at) FROM users";
    let result = transpile(input);
    assert!(result.contains("date_trunc"), "Transpiled SQL should contain date_trunc function");
}

#[test]
fn test_builtin_date_part_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();
    
    // Test year extraction
    let result: f64 = conn.query_row("SELECT date_part('year', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert!((result - 2024.0).abs() < 0.1, "Expected 2024, got {}", result);
    
    // Test month extraction
    let result: f64 = conn.query_row("SELECT date_part('month', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert!((result - 3.0).abs() < 0.1, "Expected 3, got {}", result);
    
    // Test day extraction
    let result: f64 = conn.query_row("SELECT date_part('day', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert!((result - 15.0).abs() < 0.1, "Expected 15, got {}", result);
    
    // Test hour extraction
    let result: f64 = conn.query_row("SELECT date_part('hour', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert!((result - 10.0).abs() < 0.1, "Expected 10, got {}", result);
    
    // Test minute extraction
    let result: f64 = conn.query_row("SELECT date_part('minute', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert!((result - 30.0).abs() < 0.1, "Expected 30, got {}", result);
    
    // Test second extraction (with fractional seconds)
    let result: f64 = conn.query_row("SELECT date_part('second', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert!((result - 45.0).abs() < 0.1, "Expected 45, got {}", result);
    
    // Test quarter extraction
    let result: f64 = conn.query_row("SELECT date_part('quarter', '2024-05-15 10:30:45')", [], |row| row.get(0)).unwrap();
    assert!((result - 2.0).abs() < 0.1, "Expected 2 (Q2), got {}", result);
    
    // Test day of week (dow) - 0 = Sunday
    let result: f64 = conn.query_row("SELECT date_part('dow', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    // March 15, 2024 is a Friday, so dow = 5
    assert!((result - 5.0).abs() < 0.1, "Expected 5 (Friday), got {}", result);
    
    // Test day of year (doy)
    let result: f64 = conn.query_row("SELECT date_part('doy', '2024-03-15 10:30:45')", [], |row| row.get(0)).unwrap();
    // March 15 is day 75 in a non-leap year (Jan 31 + Feb 29 + Mar 15 = 75 in 2024 leap year)
    // Actually 2024 is a leap year, so Feb has 29 days
    // Jan 31 + Feb 29 + Mar 15 = 31 + 29 + 15 = 75
    assert!((result - 75.0).abs() < 0.1, "Expected 75, got {}", result);
}

#[test]
fn test_builtin_date_part_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT date_part('year', created_at) FROM users";
    let result = transpile(input);
    assert!(result.contains("date_part"), "Transpiled SQL should contain date_part function");
}

#[test]
fn test_builtin_date_bin_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();

    // Test 15-minute bins
    let result: String = conn.query_row("SELECT date_bin('15 minutes', '2024-03-15 10:23:45', '2000-01-01')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15 10:15:00");

    // Test 1-hour bins
    let result: String = conn.query_row("SELECT date_bin('1 hour', '2024-03-15 10:23:45', '2000-01-01')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15 10:00:00");

    // Test 1-day bins
    let result: String = conn.query_row("SELECT date_bin('1 day', '2024-03-15 10:23:45', '2000-01-01')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15 00:00:00");

    // Test with date-only format
    let result: String = conn.query_row("SELECT date_bin('1 day', '2024-03-15', '2000-01-01')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15 00:00:00");
}

#[test]
fn test_builtin_date_bin_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT date_bin('15 minutes', created_at, '2000-01-01') FROM users";
    let result = transpile(input);
    assert!(result.contains("date_bin"), "Transpiled SQL should contain date_bin function");
}

#[test]
fn test_builtin_to_date_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();

    // Test basic YYYY-MM-DD format
    let result: String = conn.query_row("SELECT to_date('2024-03-15', 'YYYY-MM-DD')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15");

    // Test DD/MM/YYYY format
    let result: String = conn.query_row("SELECT to_date('15/03/2024', 'DD/MM/YYYY')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15");

    // Test YY format
    let result: String = conn.query_row("SELECT to_date('24-03-15', 'YY-MM-DD')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "2024-03-15");
}

#[test]
fn test_builtin_to_date_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT to_date(date_str, 'YYYY-MM-DD') FROM users";
    let result = transpile(input);
    assert!(result.contains("to_date"), "Transpiled SQL should contain to_date function");
}

#[test]
fn test_builtin_reverse_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();

    // Test basic reversal
    let result: String = conn.query_row("SELECT reverse('abcde')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "edcba");

    // Test with spaces
    let result: String = conn.query_row("SELECT reverse('hello world')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "dlrow olleh");

    // Test empty string
    let result: String = conn.query_row("SELECT reverse('')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "");

    // Test single character
    let result: String = conn.query_row("SELECT reverse('x')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "x");
}

#[test]
fn test_builtin_reverse_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT reverse(name) FROM users";
    let result = transpile(input);
    assert!(result.contains("reverse"), "Transpiled SQL should contain reverse function");
}

#[test]
fn test_builtin_left_right_functions() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();

    // Test left - basic case
    let result: String = conn.query_row("SELECT left('hello', 2)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "he");

    // Test right - basic case
    let result: String = conn.query_row("SELECT right('hello', 2)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "lo");

    // Test left with negative n (all but last 2)
    let result: String = conn.query_row("SELECT left('hello', -2)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "hel");

    // Test right with negative n (all but first 2)
    let result: String = conn.query_row("SELECT right('hello', -2)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "llo");

    // Test left with n > length
    let result: String = conn.query_row("SELECT left('hello', 100)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "hello");

    // Test right with n > length
    let result: String = conn.query_row("SELECT right('hello', 100)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "hello");

    // Test left with n = 0
    let result: String = conn.query_row("SELECT left('hello', 0)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "");

    // Test right with n = 0
    let result: String = conn.query_row("SELECT right('hello', 0)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "");
}

#[test]
fn test_builtin_left_right_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT left(name, 3), right(name, 3) FROM users";
    let result = transpile(input);
    assert!(result.contains("left"), "Transpiled SQL should contain left function");
    assert!(result.contains("right"), "Transpiled SQL should contain right function");
}

#[test]
fn test_builtin_concat_function() {
    use pgqt::handler::SqliteHandler;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let functions = Arc::new(dashmap::DashMap::new());
    let sessions = Arc::new(dashmap::DashMap::new());
    SqliteHandler::register_builtin_functions(&conn, functions, sessions).unwrap();

    // Test basic concatenation
    let result: String = conn.query_row("SELECT concat('a', 'b', 'c')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "abc");

    // Test with spaces
    let result: String = conn.query_row("SELECT concat('hello', ' ', 'world')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "hello world");

    // Test with numbers (converted to strings)
    let result: String = conn.query_row("SELECT concat(1, 2, 3)", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "123");

    // Test empty concat
    let result: String = conn.query_row("SELECT concat()", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "");

    // Test single argument
    let result: String = conn.query_row("SELECT concat('hello')", [], |row| row.get(0)).unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn test_builtin_concat_transpilation() {
    use pgqt::transpiler::transpile;
    let input = "SELECT concat(first_name, ' ', last_name) FROM users";
    let result = transpile(input);
    assert!(result.contains("concat"), "Transpiled SQL should contain concat function");
}
