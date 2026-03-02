# Function Implementation - Quick Start Guide

## Overview
This guide provides a quick reference for implementing PostgreSQL-compatible functions in PostgreSQLite.

## Phase 1: SQL Functions (Current Focus)

### File Changes Required

1. **`src/catalog.rs`** - Add function catalog tables and APIs
2. **`src/transpiler.rs`** - Parse CREATE FUNCTION, detect function calls
3. **`src/functions.rs`** - NEW FILE - Function execution engine
4. **`src/main.rs`** - Integrate function handling into query execution
5. **`tests/function_tests.rs`** - NEW FILE - Integration tests
6. **`tests/function_e2e_test.py`** - NEW FILE - End-to-end tests

### Step-by-Step Implementation

#### Step 1: Catalog Schema (src/catalog.rs)
```rust
// Add to init_catalog()
conn.execute(
    "CREATE TABLE IF NOT EXISTS __pg_functions__ (...)",
    []
)?;

// Add FunctionMetadata struct
pub struct FunctionMetadata { ... }

// Add storage APIs
pub fn store_function(...) -> Result<i64> { ... }
pub fn get_function(...) -> Result<Option<FunctionMetadata>> { ... }
pub fn drop_function(...) -> Result<bool> { ... }
```

#### Step 2: Function Execution Engine (src/functions.rs - NEW)
```rust
pub fn execute_sql_function(
    conn: &Connection,
    func_metadata: &FunctionMetadata,
    args: &[Value]
) -> Result<FunctionResult> {
    // 1. Validate args
    // 2. Check STRICT
    // 3. Substitute parameters ($1, $2, ...)
    // 4. Transpile body
    // 5. Execute based on return type
}
```

#### Step 3: Parse CREATE FUNCTION (src/transpiler.rs)
```rust
fn parse_create_function(stmt: &CreateFunctionStmt) -> Result<FunctionMetadata> {
    // Extract name, params, return type, body, attributes
}
```

#### Step 4: Integrate with Main (src/main.rs)
```rust
fn execute_query(&self, sql: &str) -> Result<Vec<Response>> {
    if is_create_function(sql) {
        return self.handle_create_function(sql);
    }
    if is_drop_function(sql) {
        return self.handle_drop_function(sql);
    }
    // ... rest of execution
}
```

### Key Design Decisions

1. **Catalog Storage**: Use JSON columns for flexible parameter storage
2. **Parameter Substitution**: Replace `$1`, `$2` with quoted values in function body
3. **Return Types**: Handle 4 categories - Scalar, SetOf, Table, Void
4. **Function Attributes**: Store but only implement STRICT in Phase 1
5. **Execution**: Transpile function body each time (can optimize later with caching)

### Testing Checklist

- [ ] Simple scalar function: `CREATE FUNCTION add(a int, b int) RETURNS int ...`
- [ ] Function with OUT params
- [ ] RETURNS TABLE function
- [ ] RETURNS SETOF function
- [ ] STRICT attribute (returns NULL on NULL input)
- [ ] CREATE OR REPLACE FUNCTION
- [ ] DROP FUNCTION
- [ ] Call function in SELECT clause
- [ ] Call function in WHERE clause
- [ ] Nested function calls

### Example Test Case

```rust
// tests/function_tests.rs
#[test]
fn test_simple_addition_function() {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    
    // Create function
    let sql = r#"
        CREATE FUNCTION add_numbers(a integer, b integer)
        RETURNS integer
        LANGUAGE sql
        AS $$
            SELECT a + b
        $$;
    "#;
    
    // Parse and store
    let metadata = transpiler::parse_create_function(sql).unwrap();
    catalog::store_function(&conn, &metadata).unwrap();
    
    // Retrieve and execute
    let retrieved = catalog::get_function(&conn, "add_numbers", None).unwrap().unwrap();
    let result = functions::execute_sql_function(&conn, &retrieved, &[10.into(), 5.into()]).unwrap();
    
    assert_eq!(result, FunctionResult::Scalar(Some(15.into())));
}
```

## Phase 2: PL/pgSQL Functions (Future)

### Overview
Use Lua as runtime for PL/pgSQL execution.

### Key Files
- `src/plpgsql.rs` - Parser and Lua transpiler
- `src/functions.rs` - Extended with Lua runtime support

### Dependencies
- Add `mlua` crate for Lua embedding

### Example
```sql
CREATE FUNCTION factorial(n integer)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    result integer := 1;
    i integer;
BEGIN
    FOR i IN 1..n LOOP
        result := result * i;
    END LOOP;
    RETURN result;
END;
$$;
```

## Common Pitfalls

1. **Parameter numbering**: PostgreSQL uses `$1`, `$2`, ... (1-indexed)
2. **Type compatibility**: Ensure argument types match function signature
3. **SQL injection**: Always quote/escape substituted values
4. **Transaction safety**: Functions should respect transaction boundaries
5. **Error handling**: Propagate errors properly from function execution

## Resources

- PostgreSQL CREATE FUNCTION docs: https://www.postgresql.org/docs/current/sql-createfunction.html
- pg_query Rust docs: https://docs.rs/pg_query/
- SQLite custom functions: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.create_scalar_function
