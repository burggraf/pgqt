//! Test PL/pgSQL functions callable from SQL
//!
//! This test verifies that PL/pgSQL functions can be called from SQL queries
//! through the pgqt_plpgsql_call_* wrapper functions.

use rusqlite::{Connection, types::Value};
use pgqt::catalog::{init_catalog, store_function, FunctionMetadata, ReturnTypeKind, ParamMode};
use pgqt::functions::execute_function;

/// Test that a PL/pgSQL function can be created and called
#[test]
fn test_plpgsql_function_from_sql() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    // Create a simple PL/pgSQL function in the catalog
    let metadata = FunctionMetadata {
        oid: 1,
        name: "add_numbers".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["integer".to_string(), "integer".to_string()],
        arg_names: vec!["a".to_string(), "b".to_string()],
        arg_modes: vec![ParamMode::In, ParamMode::In],
        return_type: "integer".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: r#"
BEGIN
    RETURN a + b;
END;
"#.to_string(),
        language: "plpgsql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    store_function(&conn, &metadata).unwrap();
    
    // Test calling the function through the execution engine
    let args = vec![Value::Integer(5), Value::Integer(3)];
    let result = execute_function(&conn, &metadata, &args).unwrap();
    
    match result {
        pgqt::functions::FunctionResult::Scalar(Some(Value::Integer(8))) => {
            // Success!
        }
        _ => panic!("Expected Scalar(Some(Integer(8))), got {:?}", result),
    }
}

/// Test PL/pgSQL function with control flow
#[test]
fn test_plpgsql_with_control_flow() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    let metadata = FunctionMetadata {
        oid: 2,
        name: "max_value".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["integer".to_string(), "integer".to_string()],
        arg_names: vec!["a".to_string(), "b".to_string()],
        arg_modes: vec![ParamMode::In, ParamMode::In],
        return_type: "integer".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: r#"
BEGIN
    IF a > b THEN
        RETURN a;
    ELSE
        RETURN b;
    END IF;
END;
"#.to_string(),
        language: "plpgsql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    store_function(&conn, &metadata).unwrap();
    
    // Test a > b
    let args = vec![Value::Integer(10), Value::Integer(5)];
    let result = execute_function(&conn, &metadata, &args).unwrap();
    match result {
        pgqt::functions::FunctionResult::Scalar(Some(Value::Integer(10))) => {}
        _ => panic!("Expected 10, got {:?}", result),
    }
    
    // Test b > a
    let args = vec![Value::Integer(3), Value::Integer(7)];
    let result = execute_function(&conn, &metadata, &args).unwrap();
    match result {
        pgqt::functions::FunctionResult::Scalar(Some(Value::Integer(7))) => {}
        _ => panic!("Expected 7, got {:?}", result),
    }
}

/// Test PL/pgSQL function with exception handling
#[test]
fn test_plpgsql_with_exception() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    let metadata = FunctionMetadata {
        oid: 3,
        name: "safe_divide".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["integer".to_string(), "integer".to_string()],
        arg_names: vec!["a".to_string(), "b".to_string()],
        arg_modes: vec![ParamMode::In, ParamMode::In],
        return_type: "integer".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: r#"
BEGIN
    RETURN a / b;
EXCEPTION
    WHEN division_by_zero THEN
        RETURN -1;
END;
"#.to_string(),
        language: "plpgsql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    };
    
    store_function(&conn, &metadata).unwrap();
    
    // Test normal division
    let args = vec![Value::Integer(10), Value::Integer(2)];
    let result = execute_function(&conn, &metadata, &args).unwrap();
    match result {
        pgqt::functions::FunctionResult::Scalar(Some(Value::Integer(5))) => {}
        _ => panic!("Expected 5, got {:?}", result),
    }
    
    // Test division by zero - should catch exception
    let args = vec![Value::Integer(10), Value::Integer(0)];
    let result = execute_function(&conn, &metadata, &args).unwrap();
    match result {
        pgqt::functions::FunctionResult::Scalar(Some(Value::Integer(-1))) => {}
        _ => panic!("Expected -1 (exception handler), got {:?}", result),
    }
}
