# Function Implementation - Code Examples

## 1. Catalog Table Schema

Add to `src/catalog.rs` in `init_catalog()` function:

```rust
// After existing table creations
conn.execute(
    "CREATE TABLE IF NOT EXISTS __pg_functions__ (
        oid INTEGER PRIMARY KEY AUTOINCREMENT,
        funcname TEXT NOT NULL,
        schema_name TEXT DEFAULT 'public',
        arg_types TEXT,                    -- JSON array: [\"text\", \"integer\"]
        arg_names TEXT,                    -- JSON array: [\"arg1\", \"arg2\"]
        arg_modes TEXT,                    -- JSON array: [\"IN\", \"OUT\", \"INOUT\", \"VARIADIC\"]
        return_type TEXT NOT NULL,         -- e.g., \"integer\", \"SETOF users\"
        return_type_kind TEXT NOT NULL,    -- \"SCALAR\", \"SETOF\", \"TABLE\", \"VOID\"
        return_table_cols TEXT,            -- JSON: [{\"name\":\"id\",\"type\":\"int\"},...]
        function_body TEXT NOT NULL,       -- The SQL body
        language TEXT DEFAULT 'sql',
        volatility TEXT DEFAULT 'VOLATILE',-- 'IMMUTABLE', 'STABLE', 'VOLATILE'
        strict BOOLEAN DEFAULT FALSE,
        security_definer BOOLEAN DEFAULT FALSE,
        parallel TEXT DEFAULT 'UNSAFE',    -- 'UNSAFE', 'RESTRICTED', 'SAFE'
        owner_oid INTEGER NOT NULL,
        created_at TEXT DEFAULT CURRENT_TIMESTAMP
    )",
    [],
)
.context("Failed to create __pg_functions__ table")?;

conn.execute(
    "CREATE INDEX IF NOT EXISTS idx_pg_functions_name ON __pg_functions__(funcname)",
    [],
)
.context("Failed to create index on __pg_functions__")?;

conn.execute(
    "CREATE INDEX IF NOT EXISTS idx_pg_functions_schema ON __pg_functions__(schema_name)",
    [],
)
.context("Failed to create schema index on __pg_functions__")?;
```

## 2. Function Metadata Structures

Add to `src/catalog.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json;

/// Function parameter mode
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParamMode {
    In,
    Out,
    InOut,
    Variadic,
}

/// Function return type category
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReturnTypeKind {
    Scalar,
    SetOf,
    Table,
    Void,
}

/// Function metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetadata {
    pub oid: i64,
    pub name: String,
    pub schema: String,
    pub arg_types: Vec<String>,
    pub arg_names: Vec<String>,
    pub arg_modes: Vec<ParamMode>,
    pub return_type: String,
    pub return_type_kind: ReturnTypeKind,
    pub return_table_cols: Option<Vec<(String, String)>>, // (name, type)
    pub function_body: String,
    pub language: String,
    pub volatility: String,
    pub strict: bool,
    pub security_definer: bool,
    pub parallel: String,
    pub owner_oid: i64,
    pub created_at: Option<String>,
}

/// Store a function definition in the catalog
pub fn store_function(conn: &Connection, metadata: &FunctionMetadata) -> Result<i64> {
    let arg_types_json = serde_json::to_string(&metadata.arg_types)?;
    let arg_names_json = serde_json::to_string(&metadata.arg_names)?;
    let arg_modes_json = serde_json::to_string(&metadata.arg_modes)?;
    let return_table_cols_json = match &metadata.return_table_cols {
        Some(cols) => serde_json::to_string(cols)?,
        None => "null".to_string(),
    };

    conn.execute(
        "INSERT INTO __pg_functions__ 
         (funcname, schema_name, arg_types, arg_names, arg_modes, 
          return_type, return_type_kind, return_table_cols,
          function_body, language, volatility, strict, security_definer, parallel, owner_oid)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        [
            &metadata.name,
            &metadata.schema,
            &arg_types_json,
            &arg_names_json,
            &arg_modes_json,
            &metadata.return_type,
            &format!("{:?}", metadata.return_type_kind),
            &return_table_cols_json,
            &metadata.function_body,
            &metadata.language,
            &metadata.volatility,
            &metadata.strict,
            &metadata.security_definer,
            &metadata.parallel,
            &metadata.owner_oid,
        ],
    )?;

    // Get the assigned OID
    let oid: i64 = conn.query_row(
        "SELECT last_insert_rowid()",
        [],
        |row| row.get(0),
    )?;

    Ok(oid)
}

/// Retrieve function metadata by name
pub fn get_function(
    conn: &Connection,
    name: &str,
    arg_types: Option<&[String]>
) -> Result<Option<FunctionMetadata>> {
    let query = if arg_types.is_some() {
        "SELECT * FROM __pg_functions__ WHERE funcname = ? AND arg_types = ?"
    } else {
        "SELECT * FROM __pg_functions__ WHERE funcname = ? ORDER BY oid LIMIT 1"
    };

    let arg_types_json = arg_types.map(|types| serde_json::to_string(types).unwrap());

    let mut stmt = conn.prepare(query)?;
    
    let row = if let Some(json) = &arg_types_json {
        stmt.query_row([name, json], |row| row)
    } else {
        stmt.query_row([name], |row| row)
    };

    match row {
        Ok(row) => {
            let arg_types: Vec<String> = serde_json::from_str(&row.get::<_, String>(2)?)?;
            let arg_names: Vec<String> = serde_json::from_str(&row.get::<_, String>(3)?)?;
            let arg_modes: Vec<ParamMode> = serde_json::from_str(&row.get::<_, String>(4)?)?;
            let return_type_kind: ReturnTypeKind = 
                match row.get::<_, String>(7)?.as_str() {
                    "Scalar" => ReturnTypeKind::Scalar,
                    "SetOf" => ReturnTypeKind::SetOf,
                    "Table" => ReturnTypeKind::Table,
                    "Void" => ReturnTypeKind::Void,
                    _ => ReturnTypeKind::Scalar,
                };
            let return_table_cols: Option<Vec<(String, String)>> = 
                row.get::<_, Option<String>>(8)?
                .map(|s| serde_json::from_str(&s).unwrap());

            Ok(Some(FunctionMetadata {
                oid: row.get(0)?,
                name: row.get(1)?,
                schema: row.get::<_, String>(1)?,
                arg_types,
                arg_names,
                arg_modes,
                return_type: row.get(6)?,
                return_type_kind,
                return_table_cols,
                function_body: row.get(9)?,
                language: row.get(10)?,
                volatility: row.get(11)?,
                strict: row.get(12)?,
                security_definer: row.get(13)?,
                parallel: row.get(14)?,
                owner_oid: row.get(15)?,
                created_at: row.get(16)?,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Drop a function from the catalog
pub fn drop_function(
    conn: &Connection,
    name: &str,
    arg_types: Option<&[String]>
) -> Result<bool> {
    let query = if arg_types.is_some() {
        "DELETE FROM __pg_functions__ WHERE funcname = ? AND arg_types = ?"
    } else {
        "DELETE FROM __pg_functions__ WHERE funcname = ?"
    };

    let arg_types_json = arg_types.map(|types| serde_json::to_string(types).unwrap());

    let mut stmt = conn.prepare(query)?;
    
    let changes = if let Some(json) = &arg_types_json {
        stmt.execute([name, json])?
    } else {
        stmt.execute([name])?
    };

    Ok(changes > 0)
}
```

## 3. Function Execution Engine (src/functions.rs)

Create new file `src/functions.rs`:

```rust
use rusqlite::{Connection, types::Value};
use anyhow::{Result, Context};
use crate::catalog::FunctionMetadata;
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
    
    // Replace positional parameters $1, $2, etc.
    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("${}", i + 1);
        let replacement = quote_value(arg);
        result = result.replace(&placeholder, &replacement);
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
    
    let rows: Vec<Vec<Value>> = stmt
        .query([])?
        .map(|row| {
            let row = row?;
            (0..column_count)
                .map(|i| row.get(i))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    
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
    use crate::catalog::{init_catalog, FunctionMetadata, ReturnTypeKind};

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
            arg_modes: vec![crate::catalog::ParamMode::In],
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
```

## 4. CREATE FUNCTION Parsing (src/transpiler.rs)

Add to `src/transpiler.rs`:

```rust
use pg_query::protobuf::{CreateFunctionStmt, FunctionParameter, ObjectType, DefElem, TypeName};
use crate::catalog::{FunctionMetadata, ParamMode, ReturnTypeKind};

/// Parse CREATE FUNCTION statement
pub fn parse_create_function(sql: &str) -> Result<FunctionMetadata> {
    let result = pg_query::parse(sql)?;
    
    if let Some(raw_stmt) = result.protobuf.stmts.first() {
        if let Some(NodeEnum::CreateFunctionStmt(stmt)) = &raw_stmt.stmt.as_ref().and_then(|s| s.node.as_ref()) {
            return parse_create_function_stmt(stmt);
        }
    }
    
    anyhow::bail!("Not a CREATE FUNCTION statement")
}

/// Parse CreateFunctionStmt protobuf
fn parse_create_function_stmt(stmt: &CreateFunctionStmt) -> Result<FunctionMetadata> {
    // Extract function name
    let funcname = extract_funcname(&stmt.funcname)?;
    
    // Extract parameters
    let mut arg_types = Vec::new();
    let mut arg_names = Vec::new();
    let mut arg_modes = Vec::new();
    
    for param in &stmt.parameters {
        let (name, pg_type, mode) = parse_function_parameter(param)?;
        arg_names.push(name.unwrap_or_default());
        arg_types.push(pg_type);
        arg_modes.push(mode);
    }
    
    // Extract return type
    let (return_type, return_type_kind, return_table_cols) = 
        parse_return_type(&stmt.return_type, &stmt.return_type_attrs)?;
    
    // Extract function body
    let function_body = extract_function_body(stmt)?;
    
    // Extract attributes
    let attributes = parse_function_attributes(&stmt.options)?;
    
    Ok(FunctionMetadata {
        oid: 0,
        name: funcname,
        schema: "public".to_string(),
        arg_types,
        arg_names,
        arg_modes,
        return_type,
        return_type_kind,
        return_table_cols,
        function_body,
        language: extract_language(stmt)?,
        volatility: attributes.volatility,
        strict: attributes.strict,
        security_definer: attributes.security_definer,
        parallel: attributes.parallel,
        owner_oid: 1, // TODO: Get current user OID
        created_at: None,
    })
}

/// Extract function name from ObjectWithArgs
fn extract_funcname(funcname: &[Node]) -> Result<String> {
    if let Some(NodeEnum::String(s)) = funcname.first().and_then(|n| n.node.as_ref()) {
        Ok(s.sval.clone())
    } else {
        anyhow::bail!("Could not extract function name")
    }
}

/// Parse function parameter
fn parse_function_parameter(param: &FunctionParameter) -> Result<(Option<String>, String, ParamMode)> {
    let name = if !param.name.is_empty() {
        Some(param.name.clone())
    } else {
        None
    };
    
    let pg_type = extract_type_name(&param.arg_type)?;
    
    let mode = match param.mode() {
        pg_query::protobuf::FunctionParameterMode::FuncParamIn => ParamMode::In,
        pg_query::protobuf::FunctionParameterMode::FuncParamOut => ParamMode::Out,
        pg_query::protobuf::FunctionParameterMode::FuncParamInout => ParamMode::InOut,
        pg_query::protobuf::FunctionParameterMode::FuncParamVariadic => ParamMode::Variadic,
        _ => ParamMode::In,
    };
    
    Ok((name, pg_type, mode))
}

/// Extract type name from TypeName
fn extract_type_name(type_name: &Option<TypeName>) -> Result<String> {
    if let Some(tn) = type_name {
        let names: Vec<String> = tn.names
            .iter()
            .filter_map(|n| n.node.as_ref())
            .map(|n| {
                if let NodeEnum::String(s) = n {
                    s.sval.clone()
                } else {
                    String::new()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        
        Ok(names.last().unwrap_or(&String::new()).to_uppercase())
    } else {
        Ok("UNKNOWN".to_string())
    }
}

/// Parse return type
fn parse_return_type(
    return_type: &Option<TypeName>,
    return_attrs: &[Node]
) -> Result<(String, ReturnTypeKind, Option<Vec<(String, String)>>)> {
    // Check if this is RETURNS TABLE
    if !return_attrs.is_empty() {
        // RETURNS TABLE case
        let cols = parse_table_return_columns(return_attrs)?;
        let col_types: Vec<String> = cols.iter().map(|(_, t)| t.clone()).collect();
        let return_type = format!("TABLE({})", col_types.join(", "));
        return Ok((return_type, ReturnTypeKind::Table, Some(cols)));
    }
    
    // Check if this is RETURNS SETOF
    // (This requires checking the TypeName for SETOF indicator)
    // For now, assume scalar unless we detect TABLE
    
    if let Some(tn) = return_type {
        let return_type_str = extract_type_name(&Some(tn.clone()))?;
        Ok((return_type_str, ReturnTypeKind::Scalar, None))
    } else {
        Ok(("VOID".to_string(), ReturnTypeKind::Void, None))
    }
}

/// Parse RETURNS TABLE columns
fn parse_table_return_columns(attrs: &[Node]) -> Result<Vec<(String, String)>> {
    // This is complex - need to parse TableFunc or similar
    // For now, return empty and refine later
    Ok(Vec::new())
}

/// Extract function body (the AS $$ ... $$ part)
fn extract_function_body(stmt: &CreateFunctionStmt) -> Result<String> {
    // The function body is in the "source" field or similar
    // This requires examining the pg_query protobuf structure
    // For now, return a placeholder
    Ok("SELECT 1".to_string())
}

/// Parse function attributes (IMMUTABLE, STRICT, etc.)
struct FunctionAttributes {
    volatility: String,
    strict: bool,
    security_definer: bool,
    parallel: String,
}

fn parse_function_attributes(options: &[DefElem]) -> Result<FunctionAttributes> {
    let mut volatility = "VOLATILE".to_string();
    let mut strict = false;
    let mut security_definer = false;
    let mut parallel = "UNSAFE".to_string();
    
    for option in options {
        if let Some(ref defname) = option.defname {
            match defname.as_str() {
                "volatility" => {
                    if let Some(NodeEnum::String(s)) = option.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        volatility = s.sval.clone().to_uppercase();
                    }
                }
                "strict" => {
                    if let Some(NodeEnum::Boolean(b)) = option.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        strict = b.boolval;
                    }
                }
                "security" => {
                    if let Some(NodeEnum::String(s)) = option.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        security_definer = s.sval.eq_ignore_ascii_case("definer");
                    }
                }
                "parallel" => {
                    if let Some(NodeEnum::String(s)) = option.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        parallel = s.sval.clone().to_uppercase();
                    }
                }
                _ => {}
            }
        }
    }
    
    Ok(FunctionAttributes {
        volatility,
        strict,
        security_definer,
        parallel,
    })
}

/// Extract language (should be 'sql' for Phase 1)
fn extract_language(stmt: &CreateFunctionStmt) -> Result<String> {
    // Extract from the language field
    Ok("sql".to_string())
}
```

## 5. Integration with Main Handler (src/main.rs)

Add to `src/main.rs`:

```rust
// Add import at top
mod functions;

// Add to SqliteHandler implementation
impl SqliteHandler {
    fn handle_create_function(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse CREATE FUNCTION
        let metadata = transpiler::parse_create_function(sql)?;
        
        // Store in catalog
        let conn = self.conn.lock().unwrap();
        catalog::store_function(&conn, &metadata)?;
        
        Ok(vec![Response::Execution(Tag::new("CREATE FUNCTION"))])
    }
    
    fn handle_drop_function(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse function name from DROP FUNCTION
        let name = extract_function_name_from_drop(sql)?;
        
        // Remove from catalog
        let conn = self.conn.lock().unwrap();
        catalog::drop_function(&conn, &name, None)?;
        
        Ok(vec![Response::Execution(Tag::new("DROP FUNCTION"))])
    }
    
    fn execute_with_function_calls(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse to extract function name and arguments
        let (func_name, args) = extract_function_call(sql)?;
        
        // Look up function metadata
        let conn = self.conn.lock().unwrap();
        if let Some(metadata) = catalog::get_function(&conn, &func_name, None)? {
            // Execute the function
            let result = functions::execute_sql_function(&conn, &metadata, &args)?;
            
            // Convert result to Response
            return self.convert_function_result_to_response(result);
        }
        
        anyhow::bail!("Function {} not found", func_name)
    }
    
    fn convert_function_result_to_response(
        &self,
        result: functions::FunctionResult
    ) -> Result<Vec<Response>> {
        use functions::FunctionResult;
        
        match result {
            FunctionResult::Scalar(Some(value)) => {
                // Return single value
                let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
                    "result".to_string(),
                    None,
                    None,
                    Type::UNKNOWN, // TODO: Determine actual type
                    FieldFormat::Text,
                )]);
                
                let mut encoder = DataRowEncoder::new(fields.clone());
                encoder.encode_field(&Some(format!("{:?}", value)))?;
                let data_rows = vec![Ok(encoder.take_row())];
                
                Ok(vec![Response::Query(QueryResponse::new(
                    fields,
                    stream::iter(data_rows),
                ))])
            }
            FunctionResult::Table(rows) => {
                // Return multiple rows
                // TODO: Build proper field info from function metadata
                let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
                    "result".to_string(),
                    None,
                    None,
                    Type::UNKNOWN,
                    FieldFormat::Text,
                )]);
                
                let data_rows: Vec<_> = rows.into_iter()
                    .map(|row| {
                        let mut encoder = DataRowEncoder::new(fields.clone());
                        // TODO: Encode all columns
                        encoder.encode_field(&Some(format!("{:?}", row)))?;
                        Ok(encoder.take_row())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                
                Ok(vec![Response::Query(QueryResponse::new(
                    fields,
                    stream::iter(data_rows),
                ))])
            }
            FunctionResult::Void | FunctionResult::Null => {
                Ok(vec![Response::Execution(Tag::new(""))])
            }
            _ => Ok(vec![Response::Execution(Tag::new(""))]),
        }
    }
}

// Modify execute_query to handle function statements
fn execute_query(&self, sql: &str) -> Result<Vec<Response>> {
    let upper_sql = sql.trim().to_uppercase();
    
    // Handle CREATE FUNCTION
    if upper_sql.starts_with("CREATE FUNCTION") || upper_sql.starts_with("CREATE OR REPLACE FUNCTION") {
        return self.handle_create_function(sql);
    }
    
    // Handle DROP FUNCTION
    if upper_sql.starts_with("DROP FUNCTION") {
        return self.handle_drop_function(sql);
    }
    
    // ... rest of existing execute_query logic
}
```

## 6. Example Usage

```sql
-- Create a simple addition function
CREATE FUNCTION add_numbers(a integer, b integer)
RETURNS integer
LANGUAGE sql
AS $$
    SELECT a + b
$$;

-- Call the function
SELECT add_numbers(5, 3);  -- Returns 8

-- Create a function with OUT parameters
CREATE FUNCTION get_user_info(user_id integer, 
                              OUT username text, 
                              OUT email text)
LANGUAGE sql
AS $$
    SELECT username, email FROM users WHERE id = user_id
$$;

-- Call it
SELECT * FROM get_user_info(1);

-- Create a RETURNS TABLE function
CREATE FUNCTION get_active_users()
RETURNS TABLE(id integer, username text, email text)
LANGUAGE sql
AS $$
    SELECT id, username, email FROM users WHERE active = true
$$;

-- Call it
SELECT * FROM get_active_users();

-- Create a STRICT function
CREATE FUNCTION square(x integer)
RETURNS integer
LANGUAGE sql
STRICT
AS $$
    SELECT x * x
$$;

-- This returns NULL (not an error)
SELECT square(NULL);
```
