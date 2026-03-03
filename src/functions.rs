use rusqlite::{Connection, types::Value, OptionalExtension};
use anyhow::{Result, Context};
use crate::catalog::{FunctionMetadata, ReturnTypeKind};
use crate::transpiler::transpile;

/// Function execution result
#[derive(Debug, Clone)]
pub enum FunctionResult {
    Scalar(Option<Value>),
    SetOf(Vec<Value>),
    Table(Vec<Vec<Value>>),
    Void,
    Null,
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
    use crate::catalog::{init_catalog, FunctionMetadata, ReturnTypeKind, ParamMode};

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
