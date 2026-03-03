//! PL/pgSQL integration tests
//!
//! These tests verify the complete PL/pgSQL pipeline:
//! parsing → transpilation → Lua execution

use pgqt::plpgsql::{parse_plpgsql_function, transpile_to_lua, PlPgSqlRuntime};
use rusqlite::{Connection, types::Value as SqliteValue};

/// Test basic scalar function
#[test]
fn test_basic_scalar_function() {
    let sql = r#"
        CREATE FUNCTION add(a int, b int) RETURNS int AS $$
        BEGIN
            RETURN a + b;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    assert_eq!(func.fn_name, Some("add".to_string()));
    
    let lua = transpile_to_lua(&func).unwrap();
    assert!(lua.contains("local function add"));
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(
        &conn, 
        &lua, 
        &[SqliteValue::Integer(5), SqliteValue::Integer(3)]
    ).unwrap();
    
    assert_eq!(result, SqliteValue::Integer(8));
}

/// Test IF/ELSE control flow
#[test]
fn test_if_else_control_flow() {
    let sql = r#"
        CREATE FUNCTION max_val(a int, b int) RETURNS int AS $$
        BEGIN
            IF a > b THEN
                RETURN a;
            ELSE
                RETURN b;
            END IF;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    // Test a > b
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(10), SqliteValue::Integer(5)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Integer(10));
    
    // Test b > a
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(3), SqliteValue::Integer(7)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Integer(7));
}

/// Test WHILE loop
#[test]
fn test_while_loop() {
    let sql = r#"
        CREATE FUNCTION sum_to_n(n int) RETURNS int AS $$
        DECLARE
            i int := 1;
            total int := 0;
        BEGIN
            WHILE i <= n LOOP
                total := total + i;
                i := i + 1;
            END LOOP;
            RETURN total;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(5)]
    ).unwrap();
    
    // 1+2+3+4+5 = 15
    assert_eq!(result, SqliteValue::Integer(15));
}

/// Test FOR loop
#[test]
fn test_for_loop() {
    let sql = r#"
        CREATE FUNCTION factorial(n int) RETURNS int AS $$
        DECLARE
            result int := 1;
            i int;
        BEGIN
            FOR i IN 1..n LOOP
                result := result * i;
            END LOOP;
            RETURN result;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(5)]
    ).unwrap();
    
    // 5! = 120
    assert_eq!(result, SqliteValue::Integer(120));
}

/// Test RAISE NOTICE
#[test]
fn test_raise_notice() {
    let sql = r#"
        CREATE FUNCTION log_message(msg text) RETURNS void AS $$
        BEGIN
            RAISE NOTICE 'Message: %', msg;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    // Should not panic
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Text("Hello".to_string())]
    );
    
    assert!(result.is_ok());
}

/// Test exception handling with division by zero
#[test]
fn test_exception_division_by_zero() {
    let sql = r#"
        CREATE FUNCTION safe_divide(a int, b int) RETURNS int AS $$
        BEGIN
            RETURN a / b;
        EXCEPTION
            WHEN division_by_zero THEN
                RETURN -1;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    // Test normal division
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(10), SqliteValue::Integer(2)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Integer(5));
    
    // Test division by zero - should catch exception
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(10), SqliteValue::Integer(0)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Integer(-1));
}

/// Test PERFORM statement
#[test]
fn test_perform_statement() {
    let sql = r#"
        CREATE FUNCTION increment_counter() RETURNS void AS $$
        BEGIN
            PERFORM 1; -- Dummy operation
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(&conn, &lua, &[]);
    assert!(result.is_ok());
}

/// Test nested blocks
#[test]
fn test_nested_blocks() {
    let sql = r#"
        CREATE FUNCTION nested_test(x int) RETURNS int AS $$
        BEGIN
            BEGIN
                IF x > 0 THEN
                    RETURN x * 2;
                ELSE
                    RETURN x * -1;
                END IF;
            END;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(5)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Integer(10));
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(-3)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Integer(3));
}

/// Test multiple RETURN NEXT (SETOF simulation)
#[test]
fn test_return_next_accumulation() {
    let sql = r#"
        CREATE FUNCTION generate_series(start_val int, end_val int) 
        RETURNS SETOF int AS $$
        DECLARE
            i int;
        BEGIN
            FOR i IN start_val..end_val LOOP
                RETURN NEXT i;
            END LOOP;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    // Verify Lua code contains result set initialization
    assert!(lua.contains("_result_set"));
    assert!(lua.contains("table.insert"));
}

/// Test SQL expression in assignment
#[test]
fn test_sql_expression_assignment() {
    let sql = r#"
        CREATE FUNCTION compute(a int, b int) RETURNS int AS $$
        DECLARE
            result int;
        BEGIN
            result := a * b + 10;
            RETURN result;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(3), SqliteValue::Integer(4)]
    ).unwrap();
    
    // 3 * 4 + 10 = 22
    assert_eq!(result, SqliteValue::Integer(22));
}

/// Test ELSIF chain
#[test]
fn test_elsif_chain() {
    let sql = r#"
        CREATE FUNCTION grade(score int) RETURNS text AS $$
        BEGIN
            IF score >= 90 THEN
                RETURN 'A';
            ELSIF score >= 80 THEN
                RETURN 'B';
            ELSIF score >= 70 THEN
                RETURN 'C';
            ELSE
                RETURN 'F';
            END IF;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    // Test each branch
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(95)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Text("A".to_string()));
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(85)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Text("B".to_string()));
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(75)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Text("C".to_string()));
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(60)]
    ).unwrap();
    assert_eq!(result, SqliteValue::Text("F".to_string()));
}

/// Test EXIT from loop
#[test]
fn test_exit_from_loop() {
    let sql = r#"
        CREATE FUNCTION find_limit(n int, limit_val int) RETURNS int AS $$
        DECLARE
            i int := 0;
        BEGIN
            LOOP
                i := i + 1;
                IF i > limit_val THEN
                    EXIT;
                END IF;
            END LOOP;
            RETURN i;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(
        &conn, &lua, &[SqliteValue::Integer(100), SqliteValue::Integer(5)]
    ).unwrap();
    
    assert_eq!(result, SqliteValue::Integer(6));
}

/// Test function with no arguments
#[test]
fn test_no_arguments() {
    let sql = r#"
        CREATE FUNCTION get_constant() RETURNS int AS $$
        BEGIN
            RETURN 42;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    assert_eq!(func.fn_name, Some("get_constant".to_string()));
    
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(&conn, &lua, &[]).unwrap();
    assert_eq!(result, SqliteValue::Integer(42));
}

/// Test string concatenation
#[test]
fn test_string_concatenation() {
    let sql = r#"
        CREATE FUNCTION greet(first_name text, last_name text) RETURNS text AS $$
        BEGIN
            RETURN 'Hello, ' || first_name || ' ' || last_name;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let result = runtime.execute_function(
        &conn, &lua, 
        &[SqliteValue::Text("John".to_string()), SqliteValue::Text("Doe".to_string())]
    ).unwrap();
    
    assert_eq!(result, SqliteValue::Text("Hello, John Doe".to_string()));
}
