# Function Call Interception Implementation Plan

## Problem
User-defined functions created with CREATE FUNCTION are stored in the catalog and can be executed, but when queries call these functions (e.g., SELECT add(1, 2)), the calls are not intercepted and executed.

## Solution: AST-based Interception

### Approach
1. Before executing a query, parse it with pg_query to get the AST
2. Walk the AST to find all FuncCall nodes
3. For each FuncCall, check if it's a user-defined function (query catalog)
4. If it is a UDF:
   - Extract the arguments
   - Execute the function using the execution engine
   - Replace the FuncCall in the AST with the result
5. Reconstruct SQL from the modified AST
6. Execute the modified SQL

### Implementation Steps

#### 1. Add function to detect and execute UDFs in AST
File: `src/transpiler.rs`

```rust
/// Check if a function call is to a user-defined function and execute it if possible
fn try_execute_udf(func_call: &FuncCall, conn: &Connection) -> Option<String> {
    // Extract function name
    let func_name = extract_funcname(&func_call.funcname).ok()?;
    
    // Check if it's a user-defined function
    let metadata = catalog::get_function(conn, &func_name, None).ok()??;
    
    // Collect arguments (this is the tricky part - need to evaluate them first)
    // For now, handle simple literal arguments only
    
    // Execute the function
    // let result = functions::execute_sql_function(conn, &metadata, &args).ok()?;
    
    // Return the result as a string
    // Some(result_to_string(result))
    
    None
}
```

#### 2. Modify execute_query in main.rs
File: `src/main.rs`

```rust
fn execute_query(&self, sql: &str) -> Result<Vec<Response>> {
    // ... existing code ...
    
    // Check if query contains function calls that need interception
    if self.query_contains_udf_calls(sql)? {
        return self.execute_query_with_udf_interception(sql);
    }
    
    // ... rest of existing code ...
}

fn execute_query_with_udf_interception(&self, sql: &str) -> Result<Vec<Response>> {
    let conn = self.conn.lock().unwrap();
    
    // Parse SQL to AST
    let result = pg_query::parse(sql)?;
    
    // Walk AST and execute UDFs
    let modified_ast = self.intercept_udf_calls(&result.protobuf, &conn)?;
    
    // Reconstruct SQL from modified AST
    let modified_sql = modified_ast.deparse()?;
    
    // Execute modified SQL
    self.execute_normal_query(&modified_sql)
}

fn intercept_udf_calls(&self, protobuf: &Protobuf, conn: &Connection) -> Result<Node> {
    // Walk the AST and replace UDF calls with their results
    // This requires recursively visiting all nodes
    // ...
}
```

### Challenges

1. **Argument Evaluation**: Need to evaluate function arguments before executing the UDF
2. **Context Sensitivity**: Arguments might reference columns or other query elements
3. **Complex Queries**: Nested function calls, function calls in WHERE clauses, etc.
4. **Performance**: AST parsing and walking adds overhead

### Limitations of This Approach

- Only works for simple cases initially (literal arguments)
- Complex cases (column references, subqueries) require more sophisticated handling
- May need multiple passes for nested function calls

### Alternative: Simpler First Iteration

Start with a simpler approach that only handles:
- Simple SELECT queries: `SELECT func(arg1, arg2)`
- Literal arguments only
- Single function call per query

Then expand to handle more complex cases.

## Recommendation

Start with the **simpler first iteration**:
1. Detect queries of the form `SELECT func(args)`
2. Parse the function call
3. Execute the UDF
4. Return the result

This will pass the basic E2E tests and can be expanded later.
