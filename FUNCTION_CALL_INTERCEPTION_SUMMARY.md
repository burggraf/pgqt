# Function Call Interception Implementation

## Overview

This document describes the implementation of function call interception for PostgreSQL user-defined functions in PGQT.

## Problem Statement

While the `CREATE FUNCTION` infrastructure (parsing, catalog storage, execution engine) was complete, runtime interception of function calls in queries was missing. When queries like `SELECT add_numbers(5, 3)` were executed, the system didn't detect and execute the user-defined functions.

## Solution Implemented

### Approach: AST-based Interception for Simple Cases

The implementation uses AST parsing to detect and intercept simple function calls before normal query execution.

### Key Components

#### 1. Function Registry
Added to `SqliteHandler` struct:
```rust
functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>,
```

#### 2. `try_execute_simple_function_call` Method
- Parses SQL using pg_query
- Walks AST to detect simple function calls of the form `SELECT func(arg1, arg2)`
- Returns `Ok(response)` if intercepted, `Err` if not a simple function call

#### 3. `execute_function_call` Method
- Extracts function name from AST
- Looks up function metadata from catalog
- Extracts literal arguments (integers, floats, strings)
- Executes function using existing `functions::execute_sql_function`
- Converts result to pgwire Response

#### 4. Integration with `execute_query`
Modified to call `try_execute_simple_function_call` before normal transpilation and execution.

## Files Modified

- `src/main.rs`: Added function interception logic

## What Works

✅ Simple function calls with literal arguments: `SELECT add(5, 3)`
✅ Function lookup from catalog
✅ Function execution using existing engine
✅ Result conversion to pgwire protocol
✅ STRICT attribute handling

## Current Limitations

### Query Patterns
❌ Function calls in WHERE clauses: `SELECT * FROM t WHERE is_valid(x)`
❌ Function calls in FROM clauses: `SELECT * FROM get_users()`
❌ Nested function calls: `SELECT add(multiply(2,3), 4)`
❌ Multiple function calls in one query

### Argument Types
❌ Column references: `SELECT add(id, 1) FROM users`
❌ Expressions as arguments
❌ Subqueries as arguments

### Return Types
❌ SETOF functions (partially handled)
❌ TABLE functions (partially handled)
❌ VOID functions

### Other
❌ The `register_sqlite_function` stub method is incomplete

## Testing

### Integration Tests
The existing integration tests (`tests/function_tests.rs`) test:
- Function parsing
- Catalog storage
- Function execution engine

These tests were already failing before this implementation (pre-existing issue).

### E2E Tests
The E2E tests (`tests/function_e2e_test.py`) test:
- Creating functions via CREATE FUNCTION
- Calling functions in queries
- Various function types and attributes

To run E2E tests:
```bash
python3 tests/function_e2e_test.py
```

Note: Requires the pgqt proxy to be running on port 5434.

## Next Steps

### Phase 1: Expand Simple Case Support
1. Handle function calls with column references (requires query context)
2. Support function calls in WHERE clauses
3. Handle nested function calls (recursive AST walking)

### Phase 2: Full SQL Context Support
1. Implement proper argument evaluation in query context
2. Support all PostgreSQL function call contexts (SELECT, WHERE, FROM, etc.)
3. Handle complex argument types (expressions, subqueries)

### Phase 3: Performance Optimization
1. Cache function metadata lookups
2. Optimize AST parsing for common patterns
3. Consider SQLite custom function approach for better performance

## Alternative Approaches Considered

### Option 1: SQLite Custom Functions (Not Fully Implemented)
**Concept**: Register each UDF as a SQLite custom function
**Status**: Started implementation but ran into closure state management issues
**Pros**: Leverages SQLite's native mechanism, works in all contexts
**Cons**: Complex connection/closure management, threading concerns

### Option 2: Transpiler Markers (Not Implemented)
**Concept**: Mark UDF calls during transpilation, intercept at execution
**Status**: Not implemented
**Pros**: Clean separation of concerns
**Cons**: Requires extensive transpiler changes

### Option 3: AST-based Interception (Implemented)
**Concept**: Parse SQL, find function calls, execute separately
**Status**: Partially implemented for simple cases
**Pros**: Works with existing architecture, no SQLite closure issues
**Cons**: Complex to handle all cases, performance overhead

## Conclusion

The current implementation provides a working foundation for function call interception. It handles the simplest and most common case (simple SELECT with literal arguments) and can be expanded to handle more complex scenarios.

The implementation follows the project's existing patterns and integrates well with the existing function execution engine. Future work should focus on expanding support for more complex query patterns and argument types.
