# PostgreSQLite Function Support Implementation Plan

## Overview

This document outlines a comprehensive plan to implement PostgreSQL-compatible user-defined functions in PostgreSQLite. The implementation will be done in **two phases**:

1. **Phase 1**: SQL-language functions (this document)
2. **Phase 2**: PL/pgSQL functions (using Lua as the runtime)

This plan focuses on Phase 1 - implementing `CREATE FUNCTION` for SQL-language functions with 100% PostgreSQL compatibility.

## Goals

- Support `CREATE FUNCTION` and `CREATE OR REPLACE FUNCTION` syntax
- Support all PostgreSQL function parameter modes: `IN`, `OUT`, `INOUT`, `VARIADIC`
- Support all return types: scalar, `SETOF`, `TABLE`, `VOID`
- Store function metadata in catalog tables (similar to `__pg_meta__`)
- Intercept function calls in SQL queries and execute the function body
- Support function attributes: `IMMUTABLE`, `STABLE`, `VOLATILE`, `STRICT`, etc.
- Provide full compatibility with PostgreSQL function calling conventions

## Phase 1: SQL-Language Functions

### 1. Catalog Schema Design

Create catalog tables to store function metadata. Add to `src/catalog.rs`:

```sql
CREATE TABLE IF NOT EXISTS __pg_functions__ (
    oid INTEGER PRIMARY KEY AUTOINCREMENT,
    funcname TEXT NOT NULL,                    -- Function name
    schema_name TEXT DEFAULT 'public',         -- Schema (simplified)
    arg_types TEXT,                            -- JSON: ["text", "integer", ...]
    arg_names TEXT,                            -- JSON: ["arg1", "arg2", ...]
    arg_modes TEXT,                            -- JSON: ["IN", "OUT", "INOUT", "VARIADIC"]
    return_type TEXT NOT NULL,                 -- Return type (e.g., "integer", "SETOF users")
    return_type_kind TEXT NOT NULL,            -- "SCALAR", "SETOF", "TABLE", "VOID"
    return_table_cols TEXT,                    -- JSON for TABLE returns: [{"name":"id","type":"int"},...]
    function_body TEXT NOT NULL,               -- The SQL body (e.g., "SELECT $1 + $2")
    language TEXT DEFAULT 'sql',               -- 'sql', 'plpgsql' (future)
    volatility TEXT DEFAULT 'VOLATILE',        -- 'IMMUTABLE', 'STABLE', 'VOLATILE'
    strict BOOLEAN DEFAULT FALSE,              -- STRICT / RETURNS NULL ON NULL INPUT
    security_definer BOOLEAN DEFAULT FALSE,    -- SECURITY DEFINER
    parallel TEXT DEFAULT 'UNSAFE',            -- 'UNSAFE', 'RESTRICTED', 'SAFE'
    owner_oid INTEGER NOT NULL,                -- Owner role OID
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_pg_functions_name ON __pg_functions__(funcname);
CREATE INDEX IF NOT EXISTS idx_pg_functions_schema ON __pg_functions__(schema_name);
```

**Rationale**: 
- Using JSON for arrays allows flexible storage of parameter information
- `return_type_kind` distinguishes between different return type categories
- `return_table_cols` stores column definitions for `RETURNS TABLE` functions
- All PostgreSQL function attributes are captured for future optimization

### 2. Function Metadata Structures

Add to `src/catalog.rs`:

```rust
/// Function parameter mode
#[derive(Debug, Clone, PartialEq)]
pub enum ParamMode {
    In,
    Out,
    InOut,
    Variadic,
}

/// Function return type category
#[derive(Debug, Clone, PartialEq)]
pub enum ReturnTypeKind {
    Scalar,
    SetOf,
    Table,
    Void,
}

/// Function metadata
#[derive(Debug, Clone)]
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
}

/// Function parameter information
#[derive(Debug, Clone)]
pub struct FunctionParameter {
    pub name: Option<String>,
    pub pg_type: String,
    pub mode: ParamMode,
    pub default_value: Option<String>,
}
```

### 3. Function Storage API

Add to `src/catalog.rs`:

```rust
/// Store a function definition in the catalog
pub fn store_function(conn: &Connection, metadata: &FunctionMetadata) -> Result<i64> {
    // Implementation: INSERT or UPDATE based on CREATE OR REPLACE
}

/// Retrieve function metadata by name and argument types
pub fn get_function(
    conn: &Connection,
    name: &str,
    arg_types: Option<&[String]>
) -> Result<Option<FunctionMetadata>> {
    // Implementation: Query __pg_functions__ table
}

/// Drop a function from the catalog
pub fn drop_function(
    conn: &Connection,
    name: &str,
    arg_types: Option<&[String]>
) -> Result<bool> {
    // Implementation: DELETE from __pg_functions__
}

/// List all functions in a schema
pub fn list_functions(conn: &Connection, schema: Option<&str>) -> Result<Vec<FunctionMetadata>> {
    // Implementation: Query all functions
}
```

### 4. AST Parsing - CREATE FUNCTION

Add to `src/transpiler.rs`:

```rust
use pg_query::protobuf::{CreateFunctionStmt, FunctionParameter, ObjectType, DefElem};

/// Parse CREATE FUNCTION statement and extract metadata
fn parse_create_function(stmt: &CreateFunctionStmt) -> Result<FunctionMetadata> {
    // Extract function name
    let funcname = extract_funcname(&stmt.funcname);
    
    // Extract parameters
    let params: Vec<FunctionParameter> = stmt
        .parameters
        .iter()
        .map(|p| parse_function_parameter(p))
        .collect();
    
    // Extract return type
    let (return_type, return_type_kind, return_table_cols) = 
        parse_return_type(&stmt.return_type, &stmt.return_type_attrs);
    
    // Extract function body (SQL language)
    let function_body = extract_function_body(&stmt);
    
    // Extract attributes (IMMUTABLE, STRICT, etc.)
    let attributes = parse_function_attributes(&stmt.options);
    
    // Build and return metadata
    Ok(FunctionMetadata {
        oid: 0, // Will be assigned by catalog
        name: funcname,
        schema: "public".to_string(),
        arg_types: params.iter().map(|p| p.pg_type.clone()).collect(),
        arg_names: params.iter().map(|p| p.name.clone().unwrap_or_default()).collect(),
        arg_modes: params.iter().map(|p| p.mode.clone()).collect(),
        return_type,
        return_type_kind,
        return_table_cols,
        function_body,
        language: extract_language(&stmt),
        volatility: attributes.volatility,
        strict: attributes.strict,
        security_definer: attributes.security_definer,
        parallel: attributes.parallel,
        owner_oid: 1, // TODO: Get current user
        created_at: Utc::now(),
    })
}

/// Parse function parameter
fn parse_function_parameter(param: &FunctionParameter) -> FunctionParameter {
    // Extract mode (IN, OUT, INOUT, VARIADIC)
    // Extract type
    // Extract name
    // Extract default value if present
}

/// Parse return type (handles RETURNS type, RETURNS SETOF type, RETURNS TABLE)
fn parse_return_type(
    return_type: &Option<TypeName>,
    return_attrs: &[Node]
) -> (String, ReturnTypeKind, Option<Vec<(String, String)>>) {
    // Handle different return type forms
}
```

### 5. Function Execution Engine

Create new file `src/functions.rs`:

```rust
use rusqlite::{Connection, Row};
use anyhow::Result;
use crate::catalog::FunctionMetadata;
use crate::transpiler::transpile;

/// Execute a SQL-language function
pub fn execute_sql_function(
    conn: &Connection,
    func_metadata: &FunctionMetadata,
    args: &[rusqlite::types::Value]
) -> Result<FunctionResult> {
    // 1. Validate argument count and types
    validate_arguments(func_metadata, args)?;
    
    // 2. If STRICT and any NULL args, return NULL immediately
    if func_metadata.strict && args.iter().any(|v| matches!(v, rusqlite::types::Value::Null)) {
        return Ok(FunctionResult::Null);
    }
    
    // 3. Substitute parameters in function body ($1, $2, ... or named params)
    let substituted_body = substitute_parameters(&func_metadata.function_body, args)?;
    
    // 4. Transpile the function body to SQLite
    let sqlite_sql = transpile(&substituted_body);
    
    // 5. Execute based on return type
    match func_metadata.return_type_kind {
        ReturnTypeKind::Scalar => {
            execute_scalar_function(conn, &sqlite_sql)
        }
        ReturnTypeKind::SetOf => {
            execute_setof_function(conn, &sqlite_sql, &func_metadata.return_type)
        }
        ReturnTypeKind::Table => {
            execute_table_function(conn, &sqlite_sql, &func_metadata.return_table_cols)
        }
        ReturnTypeKind::Void => {
            execute_void_function(conn, &sqlite_sql)
        }
    }
}

/// Substitute $1, $2, etc. with actual argument values
fn substitute_parameters(body: &str, args: &[rusqlite::types::Value]) -> Result<String> {
    let mut result = body.to_string();
    
    // Replace positional parameters $1, $2, etc.
    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("${}", i + 1);
        let replacement = format!("{}", quote_value(arg));
        result = result.replace(&placeholder, &replacement);
    }
    
    Ok(result)
}

/// Execute scalar function (returns single value)
fn execute_scalar_function(conn: &Connection, sql: &str) -> Result<FunctionResult> {
    let mut stmt = conn.prepare(sql)?;
    let result: Option<rusqlite::types::Value> = stmt.query_row([], |row| row.get(0)).optional()?;
    Ok(FunctionResult::Scalar(result))
}

/// Execute SETOF function (returns multiple rows of single type)
fn execute_setof_function(
    conn: &Connection,
    sql: &str,
    return_type: &str
) -> Result<FunctionResult> {
    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<rusqlite::types::Value> = stmt.query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FunctionResult::SetOf(rows))
}

/// Execute TABLE function (returns multiple rows with columns)
fn execute_table_function(
    conn: &Connection,
    sql: &str,
    columns: &Option<Vec<(String, String)>>
) -> Result<FunctionResult> {
    let mut stmt = conn.prepare(sql)?;
    let column_count = stmt.column_count();
    
    let rows: Vec<Vec<rusqlite::types::Value>> = stmt.query([])?
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

/// Function execution result
pub enum FunctionResult {
    Scalar(Option<rusqlite::types::Value>),
    SetOf(Vec<rusqlite::types::Value>),
    Table(Vec<Vec<rusqlite::types::Value>>),
    Void,
    Null,
}
```

### 6. Function Call Transpilation

Modify `src/transpiler.rs` to detect and handle function calls:

```rust
/// Detect function calls in SQL and prepare for execution
fn reconstruct_func_call(func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
    let func_name = extract_func_name(&func_call.func);
    
    // Check if this is a user-defined function in our catalog
    if let Ok(conn) = get_connection() {
        if let Ok(Some(metadata)) = catalog::get_function(&conn, &func_name, None) {
            // This is a user-defined function
            // We need to handle this specially during execution, not transpilation
            // Return a marker that will be intercepted at execution time
            return format!("__USER_FUNC__({})", func_name);
        }
    }
    
    // Not a user-defined function, transpile normally
    reconstruct_func_call_normal(func_call, ctx)
}

/// During execution in main.rs, intercept function calls
fn execute_with_function_interception(
    conn: &Connection,
    sql: &str,
    session: &SessionContext
) -> Result<Vec<Response>> {
    // Check if SQL contains function call markers
    if sql.contains("__USER_FUNC__") {
        // Parse to extract function name and arguments
        let (func_name, args) = extract_function_call(sql)?;
        
        // Look up function metadata
        if let Some(metadata) = catalog::get_function(conn, &func_name, None)? {
            // Execute the function
            let result = functions::execute_sql_function(conn, &metadata, &args)?;
            
            // Return result as appropriate Response
            return convert_function_result_to_response(result);
        }
    }
    
    // No user function, execute normally
    execute_normal_query(conn, sql, session)
}
```

### 7. Integration with Main Handler

Modify `src/main.rs`:

```rust
mod functions; // Add this import

// In SqliteHandler::execute_query or similar:
fn execute_query(&self, sql: &str) -> Result<Vec<Response>> {
    // Check if this is a CREATE FUNCTION statement
    if is_create_function_statement(sql) {
        return self.handle_create_function(sql);
    }
    
    // Check if this is a DROP FUNCTION statement
    if is_drop_function_statement(sql) {
        return self.handle_drop_function(sql);
    }
    
    // Transpile as normal
    let transpile_result = transpile_with_metadata(sql);
    
    // Check if transpiled SQL contains function calls
    if transpile_result.sql.contains("__USER_FUNC__") {
        return self.execute_with_function_calls(&transpile_result.sql);
    }
    
    // Execute normally
    self.execute_normal_query(&transpile_result.sql)
}

fn handle_create_function(&self, sql: &str) -> Result<Vec<Response>> {
    // Parse CREATE FUNCTION
    let metadata = transpiler::parse_create_function(sql)?;
    
    // Store in catalog
    let conn = self.conn.lock().unwrap();
    catalog::store_function(&conn, &metadata)?;
    
    Ok(vec![Response::Execution(Tag::new("CREATE FUNCTION"))])
}

fn handle_drop_function(&self, sql: &str) -> Result<Vec<Response>> {
    // Parse DROP FUNCTION
    let (name, arg_types) = parse_drop_function(sql)?;
    
    // Remove from catalog
    let conn = self.conn.lock().unwrap();
    catalog::drop_function(&conn, &name, arg_types.as_deref())?;
    
    Ok(vec![Response::Execution(Tag::new("DROP FUNCTION"))])
}
```

### 8. Test Suite

Create comprehensive tests:

#### Unit Tests (`src/functions.rs`):
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_create_function_simple() {}
    #[test]
    fn test_parse_create_function_with_params() {}
    #[test]
    fn test_parse_create_function_returns_table() {}
    #[test]
    fn test_substitute_parameters() {}
    #[test]
    fn test_execute_scalar_function() {}
    #[test]
    fn test_execute_setof_function() {}
    #[test]
    fn test_execute_table_function() {}
    #[test]
    fn test_strict_function_null_handling() {}
}
```

#### Integration Tests (`tests/function_tests.rs`):
```rust
#[test]
fn test_create_simple_function() {}
#[test]
fn test_create_function_with_in_params() {}
#[test]
fn test_create_function_with_out_params() {}
#[test]
fn test_create_function_with_inout_params() {}
#[test]
fn test_create_function_variadic() {}
#[test]
fn test_create_function_returns_table() {}
#[test]
fn test_create_function_returns_setof() {}
#[test]
fn test_function_strict_attribute() {}
#[test]
fn test_function_immutable_attribute() {}
#[test]
fn test_drop_function() {}
#[test]
fn test_create_or_replace_function() {}
```

#### E2E Tests (`tests/function_e2e_test.py`):
```python
def test_simple_addition_function():
    """Test CREATE FUNCTION with simple arithmetic"""
    
def test_function_with_multiple_params():
    """Test function with multiple IN parameters"""
    
def test_function_returns_table():
    """Test RETURNS TABLE function"""
    
def test_function_with_out_params():
    """Test function with OUT parameters"""
    
def test_function_strict_null_handling():
    """Test STRICT attribute behavior"""
    
def test_function_in_select_clause():
    """Test calling function in SELECT"""
    
def test_function_in_where_clause():
    """Test calling function in WHERE clause"""
    
def test_nested_function_calls():
    """Test nested function calls"""
    
def test_create_or_replace():
    """Test CREATE OR REPLACE FUNCTION"""
```

### 9. Documentation

Create `docs/functions.md`:

```markdown
# PostgreSQLite Function Support

## Overview
PostgreSQLite supports user-defined functions using `CREATE FUNCTION`, compatible with PostgreSQL syntax.

## Creating Functions

### Simple Scalar Function
```sql
CREATE FUNCTION add_numbers(a integer, b integer)
RETURNS integer
LANGUAGE sql
AS $$
    SELECT a + b
$$;
```

### Function with OUT Parameters
```sql
CREATE FUNCTION get_user_info(user_id integer, 
                              OUT username text, 
                              OUT email text)
LANGUAGE sql
AS $$
    SELECT username, email FROM users WHERE id = user_id
$$;
```

### RETURNS TABLE Function
```sql
CREATE FUNCTION get_active_users()
RETURNS TABLE(id integer, username text, email text)
LANGUAGE sql
AS $$
    SELECT id, username, email FROM users WHERE active = true
$$;
```

### RETURNS SETOF Function
```sql
CREATE FUNCTION get_user_ids()
RETURNS SETOF integer
LANGUAGE sql
AS $$
    SELECT id FROM users
$$;
```

### Function with Attributes
```sql
CREATE FUNCTION square(x integer)
RETURNS integer
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT x * x
$$;
```

## Calling Functions

### In SELECT Clause
```sql
SELECT add_numbers(5, 3);  -- Returns 8
```

### In WHERE Clause
```sql
SELECT * FROM users WHERE get_user_score(id) > 100;
```

### With OUT Parameters
```sql
SELECT * FROM get_user_info(1);
-- Returns row with username and email columns
```

### RETURNS TABLE
```sql
SELECT * FROM get_active_users();
-- Returns multiple rows with id, username, email
```

## Supported Features

- ✅ `CREATE FUNCTION` and `CREATE OR REPLACE FUNCTION`
- ✅ `DROP FUNCTION`
- ✅ Parameter modes: `IN`, `OUT`, `INOUT`, `VARIADIC`
- ✅ Return types: scalar, `SETOF`, `TABLE`, `VOID`
- ✅ Function attributes: `IMMUTABLE`, `STABLE`, `VOLATILE`
- ✅ `STRICT` / `RETURNS NULL ON NULL INPUT`
- ✅ `SECURITY INVOKER` / `SECURITY DEFINER`
- ✅ `PARALLEL UNSAFE`, `RESTRICTED`, `SAFE`

## Limitations

- Functions are stored per-database (not in separate schema catalogs yet)
- Overloading by argument types is supported but schema resolution is simplified
- PL/pgSQL functions not yet supported (Phase 2)
```

## Phase 2: PL/pgSQL Functions (Detailed)

### Overview
Implement full PL/pgSQL support using Lua as the execution runtime.

**📋 Complete implementation plan available in:** [`PLPGSQL_PHASE2_PLAN.md`](PLPGSQL_PHASE2_PLAN.md)

This document contains the high-level overview. For detailed specifications including:
- Complete AST type definitions
- Lua transpilation algorithms
- Runtime API design
- SQLSTATE error code mapping
- Trigger variable handling
- Security sandboxing
- 4-week implementation timeline

See the detailed Phase 2 plan document.

### Key Components

1. **PL/pgSQL Parser** (`src/plpgsql/parser.rs`)
   - Uses `pg_parse::parse_plpgsql()` to get JSON AST
   - Deserializes to Rust AST types
   - Full coverage of PL/pgSQL statements

2. **Lua Transpiler** (`src/plpgsql/transpiler.rs`)
   - Converts PL/pgSQL AST to Lua source code
   - Maps SQL expressions to `_ctx.scalar()` calls
   - Transforms control flow (IF, LOOP, WHILE, FOR)
   - Handles exception blocks via `pcall/xpcall`

3. **Lua Runtime** (`src/plpgsql/runtime.rs`)
   - Uses `mlua` with Luau backend for sandboxing
   - Provides PGQT API for database access
   - Manages special variables (SQLSTATE, SQLERRM)
   - Caches compiled functions

4. **Trigger Support** (`src/plpgsql/trigger.rs`)
   - Populates TG_* variables
   - Handles OLD/NEW row data
   - Returns modified rows

### Architecture Summary

```
PL/pgSQL Source
      │
      ▼
pg_parse::parse_plpgsql()
      │
      ▼
JSON AST ──► Rust AST (serde)
      │
      ▼
Transpile to Lua
      │
      ▼
Store Lua code in __pg_functions__
      │
      ▼ (Function Call)
mlua::Lua::load() ──► sandboxed execution
      │
      ▼
Return result
```

### Dependencies

Add to `Cargo.toml`:
```toml
pg_parse = "0.16"  # PL/pgSQL parsing
mlua = { version = "0.10", features = ["luau", "serialize", "send"] }
```

### Quick Example

**PL/pgSQL Input:**
```sql
CREATE FUNCTION add(a int, b int) RETURNS int AS $$
BEGIN
    RETURN a + b;
END;
$$ LANGUAGE plpgsql;
```

**Generated Lua:**
```lua
local function add(_ctx, ...)
  local a = select(1, ...)
  local b = select(2, ...)
  return _ctx.scalar("SELECT $1 + $2", {a, b})
end
return add
```

### Implementation Timeline

| Phase | Duration | Focus |
|-------|----------|-------|
| 2A | Week 1 | Parser, basic transpiler |
| 2B | Week 2 | Control flow, runtime API |
| 2C | Week 3 | Advanced features, exceptions |
| 2D | Week 4 | Trigger support |

**📖 See [PLPGSQL_PHASE2_PLAN.md](PLPGSQL_PHASE2_PLAN.md) for complete details.**

## Implementation Timeline

### Phase 1 (SQL Functions): 2-3 weeks
1. Week 1: Catalog schema + metadata structures + storage API
2. Week 2: AST parsing + transpilation integration
3. Week 3: Execution engine + testing + documentation

### Phase 2 (PL/pgSQL): 3-4 weeks (future)
1. Week 1: PL/pgSQL parser
2. Week 2: Lua transpiler
3. Week 3: Lua runtime + sandboxing
4. Week 4: Trigger support + comprehensive testing

## Success Criteria

### Phase 1 Must-Haves:
- [ ] `CREATE FUNCTION` works for simple scalar functions
- [ ] `CREATE OR REPLACE FUNCTION` works
- [ ] `DROP FUNCTION` works
- [ ] Functions can be called in SELECT statements
- [ ] Functions can be called in WHERE clauses
- [ ] IN, OUT, INOUT parameters work
- [ ] RETURNS TABLE works
- [ ] RETURNS SETOF works
- [ ] STRICT attribute works
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] All E2E tests pass

### Phase 1 Nice-to-Haves:
- [ ] VARIADIC parameters
- [ ] IMMUTABLE/STABLE/VOLATILE attributes (for optimization hints)
- [ ] SECURITY DEFINER
- [ ] PARALLEL attributes
- [ ] Function overloading by argument types

### Phase 2 (Future):
- [ ] PL/pgSQL syntax parsing
- [ ] Lua transpilation
- [ ] Lua runtime execution
- [ ] Trigger support
- [ ] Exception handling
- [ ] Dynamic SQL (EXECUTE)

## Technical Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Complex function body parsing | Start with simple SQL bodies, add complexity incrementally |
| Parameter substitution security | Use proper quoting and parameterization, never string concatenation |
| Performance of function interception | Cache function metadata, optimize lookup |
| Type compatibility issues | Use PostgreSQL type system from catalog, careful type mapping |
| Nested function calls | Implement proper call stack tracking |

## Dependencies

- `pg_query` crate (already in use) - for parsing CREATE FUNCTION
- `serde_json` (already in use) - for storing array data in catalog
- `rusqlite` (already in use) - for catalog storage and execution
- `mlua` (Phase 2 only) - for Lua runtime

## Next Steps

1. Create catalog table schema in `src/catalog.rs`
2. Implement FunctionMetadata structures
3. Implement storage/retrieval API
4. Add CREATE FUNCTION parsing to transpiler
5. Create function execution engine
6. Integrate with main query handler
7. Write comprehensive tests
8. Document usage
