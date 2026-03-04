//! User-Defined Function (UDF) execution for PostgreSQL compatibility
//!
//! This module handles the storage and execution of user-defined functions created
//! via `CREATE FUNCTION`. It supports SQL-language functions as well as PL/pgSQL
//! functions (transpiled to Lua via the [`crate::plpgsql`] module).

// UDF functions
#![allow(dead_code)]
//!
//! ## Supported Features
//! - `CREATE FUNCTION` / `CREATE OR REPLACE FUNCTION`
//! - `DROP FUNCTION`
//! - Parameter modes: `IN`, `OUT`, `INOUT`
//! - Return types: scalar, `SETOF`, `TABLE`, `VOID`
//! - Function attributes: `STRICT`, `IMMUTABLE`, `STABLE`, `VOLATILE`
//! - `SECURITY DEFINER` / `SECURITY INVOKER`

use rusqlite::{Connection, types::Value, OptionalExtension};
use anyhow::{Result, Context};
use crate::catalog::{FunctionMetadata, ReturnTypeKind};
use crate::transpiler::transpile;
use crate::plpgsql::{parse_plpgsql_function, transpile_to_lua, PlPgSqlRuntime};

/// Function execution result
#[derive(Debug, Clone)]
pub enum FunctionResult {
    Scalar(Option<Value>),
    SetOf(Vec<Value>),
    Table(Vec<Vec<Value>>),
    Void,
    Null,
}

/// Execute a function (SQL or PL/pgSQL)
pub fn execute_function(
    conn: &Connection,
    func_metadata: &FunctionMetadata,
    args: &[Value]
) -> Result<FunctionResult> {
    match func_metadata.language.as_str() {
        "sql" => execute_sql_function(conn, func_metadata, args),
        "plpgsql" => execute_plpgsql_function(conn, func_metadata, args),
        _ => Err(anyhow::anyhow!("Unsupported function language: {}", func_metadata.language)),
    }
}

/// Execute a SQL-language function
pub fn execute_sql_function(
    conn: &Connection,
    func_metadata: &FunctionMetadata,
    args: &[Value]
) -> Result<FunctionResult> {
    // 1. Validate argument count
    validate_arguments(func_metadata, args)
        .context("Argument validation failed")?;
    
    // 2. If STRICT and any NULL args, return NULL immediately
    if func_metadata.strict && args.iter().any(|v| matches!(v, Value::Null)) {
        return Ok(FunctionResult::Null);
    }
    
    // 3. Substitute parameters in function body ($1, $2, ...)
    let substituted_body = substitute_parameters(&func_metadata.function_body, args)
        .context("Parameter substitution failed")?;
    
    
    // 4. Transpile the function body to SQLite
    let sqlite_sql = transpile(&substituted_body);
    
    
    // 5. Execute based on return type
    match func_metadata.return_type_kind {
        ReturnTypeKind::Scalar => {
            execute_scalar_function(conn, &sqlite_sql)
                .context("Scalar function execution failed")
        }
        ReturnTypeKind::SetOf => {
            execute_setof_function(conn, &sqlite_sql)
                .context("SETOF function execution failed")
        }
        ReturnTypeKind::Table => {
            execute_table_function(conn, &sqlite_sql)
                .context("TABLE function execution failed")
        }
        ReturnTypeKind::Void => {
            execute_void_function(conn, &sqlite_sql)
                .context("VOID function execution failed")
        }
    }
}

/// Execute a PL/pgSQL function
pub fn execute_plpgsql_function(
    conn: &Connection,
    func_metadata: &FunctionMetadata,
    args: &[Value]
) -> Result<FunctionResult> {
    // 1. Validate argument count
    validate_arguments(func_metadata, args)
        .context("Argument validation failed")?;
    
    // 2. If STRICT and any NULL args, return NULL immediately
    if func_metadata.strict && args.iter().any(|v| matches!(v, Value::Null)) {
        return Ok(FunctionResult::Null);
    }
    
    // 3. Parse PL/pgSQL function
    // We need to reconstruct the CREATE FUNCTION statement from metadata
    let create_sql = reconstruct_create_function(func_metadata);
    
    let func = parse_plpgsql_function(&create_sql)
        .context("Failed to parse PL/pgSQL function")?;
    
    // 4. Transpile to Lua
    let lua_code = transpile_to_lua(&func)
        .context("Failed to transpile PL/pgSQL to Lua")?;
    
    // 5. Execute in Lua runtime
    let runtime = PlPgSqlRuntime::new()
        .context("Failed to create PL/pgSQL runtime")?;
    
    let result = runtime.execute_function(conn, &lua_code, args)
        .context("Failed to execute PL/pgSQL function")?;
    
    // 6. Convert result based on return type
    match func_metadata.return_type_kind {
        ReturnTypeKind::Scalar | ReturnTypeKind::SetOf => {
            Ok(FunctionResult::Scalar(Some(result)))
        }
        ReturnTypeKind::Void => {
            Ok(FunctionResult::Void)
        }
        ReturnTypeKind::Table => {
            // For TABLE return type, we'd need to handle row types
            // For now, return as scalar
            Ok(FunctionResult::Scalar(Some(result)))
        }
    }
}

/// Reconstruct CREATE FUNCTION statement from metadata
fn reconstruct_create_function(metadata: &FunctionMetadata) -> String {
    let mut sql = format!("CREATE FUNCTION {}(", metadata.name);
    
    // Add parameters
    let params: Vec<String> = metadata.arg_names.iter()
        .zip(metadata.arg_types.iter())
        .map(|(name, typ)| format!("{} {}", name, typ))
        .collect();
    sql.push_str(&params.join(", "));
    sql.push_str(&format!(") RETURNS {}", metadata.return_type));
    
    sql.push_str(" AS $$");
    sql.push_str(&metadata.function_body);
    sql.push_str("$$ LANGUAGE plpgsql;");
    
    sql
}

/// Validate function arguments
fn validate_arguments(func_metadata: &FunctionMetadata, args: &[Value]) -> Result<()> {
    // For now, just check count (could add type checking later)
    if args.len() != func_metadata.arg_types.len() {
        anyhow::bail!(
            "Function {} expects {} arguments, got {}",
            func_metadata.name,
            func_metadata.arg_types.len(),
            args.len()
        );
    }
    Ok(())
}

/// Substitute $1, $2, etc. with actual argument values
fn substitute_parameters(body: &str, args: &[Value]) -> Result<String> {
    let mut result = body.to_string();
    
    // First try positional parameters $1, $2, etc.
    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("${}", i + 1);
        let replacement = quote_value(arg);
        result = result.replace(&placeholder, &replacement);
    }
    
    // If no $1, $2 found, try named parameters (simple identifier replacement)
    // This handles cases like "SELECT a + b" where parameters are named
    if !result.contains('$') && args.len() > 0 {
        // Try to parse the SQL to find column references that match parameter names
        // For now, use a simple heuristic: if the body has simple identifiers that aren't keywords
        // This is a temporary workaround - proper solution requires SQL parsing
        // For the common case of "SELECT a + b", we can just substitute
        // We'll use a simple approach: if args are provided and body doesn't have $, assume positional
        // Actually, the proper fix is to convert named params to $1, $2 during CREATE FUNCTION parsing
        // For now, let's handle the simple case where function body is just an expression
    }
    
    Ok(result)
}

/// Quote a value for SQL substitution
fn quote_value(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => format!("'{}'", s.replace("'", "''")),
        Value::Blob(b) => format!("x'{}'", hex::encode(b)),
    }
}

/// Execute scalar function (returns single value)
fn execute_scalar_function(conn: &Connection, sql: &str) -> Result<FunctionResult> {
    let mut stmt = conn.prepare(sql)?;
    let result: Option<Value> = stmt.query_row([], |row| row.get(0)).optional()?;
    Ok(FunctionResult::Scalar(result))
}

/// Execute SETOF function (returns multiple rows of single type)
fn execute_setof_function(conn: &Connection, sql: &str) -> Result<FunctionResult> {
    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<Value> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FunctionResult::SetOf(rows))
}

/// Execute TABLE function (returns multiple rows with columns)
fn execute_table_function(conn: &Connection, sql: &str) -> Result<FunctionResult> {
    let mut stmt = conn.prepare(sql)?;
    let column_count = stmt.column_count();
    
    let mut rows = Vec::new();
    let mut query = stmt.query([])?;
    
    while let Some(row) = query.next()? {
        let mut row_values = Vec::new();
        for i in 0..column_count {
            row_values.push(row.get(i)?);
        }
        rows.push(row_values);
    }
    
    Ok(FunctionResult::Table(rows))
}

/// Execute VOID function (no return value)
fn execute_void_function(conn: &Connection, sql: &str) -> Result<FunctionResult> {
    conn.execute(sql, [])?;
    Ok(FunctionResult::Void)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::catalog::{FunctionMetadata, ReturnTypeKind, ParamMode};

    #[test]
    fn test_substitute_parameters() {
        let body = "SELECT $1 + $2 * $3";
        let args = vec![Value::Integer(10), Value::Integer(5), Value::Integer(2)];
        let result = substitute_parameters(body, &args).unwrap();
        assert_eq!(result, "SELECT 10 + 5 * 2");
    }

    #[test]
    fn test_quote_value() {
        assert_eq!(quote_value(&Value::Null), "NULL");
        assert_eq!(quote_value(&Value::Integer(42)), "42");
        assert_eq!(quote_value(&Value::Real(3.14)), "3.14");
        assert_eq!(quote_value(&Value::Text("hello".to_string())), "'hello'");
        assert_eq!(quote_value(&Value::Text("O'Brien".to_string())), "'O''Brien'");
    }

    #[test]
    fn test_execute_scalar_function() {
        let conn = Connection::open_in_memory().unwrap();
        let sql = "SELECT 5 + 3";
        let result = execute_scalar_function(&conn, sql).unwrap();
        assert!(matches!(result, FunctionResult::Scalar(Some(Value::Integer(8)))));
    }

    #[test]
    fn test_strict_function_with_null() {
        let metadata = FunctionMetadata {
            oid: 1,
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
        
        let conn = Connection::open_in_memory().unwrap();
        let args = vec![Value::Null];
        let result = execute_sql_function(&conn, &metadata, &args).unwrap();
        assert!(matches!(result, FunctionResult::Null));
    }
}
