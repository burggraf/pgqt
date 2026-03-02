# Function Implementation - Architecture Diagrams

## System Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    PostgreSQL Client                         │
│                    (psql, psycopg2, etc.)                    │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        │ PostgreSQL Wire Protocol
                        │
┌───────────────────────▼─────────────────────────────────────┐
│                  PostgreSQLite Proxy                         │
│                                                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Query Handler (main.rs)                   │  │
│  │                                                       │  │
│  │  ┌─────────────────┐  ┌──────────────────────────┐  │  │
│  │  │ CREATE FUNCTION │  │   Function Call in SQL    │  │  │
│  │  └────────┬────────┘  └────────────┬─────────────┘  │  │
│  │           │                        │                 │  │
│  │           ▼                        ▼                 │  │
│  │  ┌─────────────────┐  ┌──────────────────────────┐  │  │
│  │  │handle_create_   │  │execute_with_function_    │  │  │
│  │  │  function()     │  │       calls()            │  │  │
│  │  └────────┬────────┘  └────────────┬─────────────┘  │  │
│  │           │                        │                 │  │
│  └───────────┼────────────────────────┼─────────────────┘  │
│              │                        │                    │
│              ▼                        ▼                    │
│      ┌───────────────┐      ┌──────────────────┐         │
│      │   Transpiler  │      │  Function Engine │         │
│      │   (parsing)   │      │  (execution)     │         │
│      └───────┬───────┘      └────────┬─────────┘         │
│              │                        │                    │
└──────────────┼────────────────────────┼────────────────────┘
               │                        │
               │                        │
    ┌──────────▼──────────┐  ┌─────────▼──────────┐
    │   Catalog Storage   │  │  SQLite Database   │
    │  (__pg_functions__) │  │   (user data)      │
    └─────────────────────┘  └────────────────────┘
```

## Data Flow: CREATE FUNCTION

```
1. Client sends:
   CREATE FUNCTION add(a int, b int) RETURNS int AS $$ SELECT a + b $$ LANGUAGE sql;

2. Query Handler (main.rs)
   └─> detect CREATE FUNCTION statement
       └─> call handle_create_function(sql)

3. Transpiler (transpiler.rs)
   └─> parse_create_function(sql)
       ├─> extract function name: "add"
       ├─> extract parameters: [(a, int, IN), (b, int, IN)]
       ├─> extract return type: int (Scalar)
       ├─> extract body: "SELECT a + b"
       ├─> extract attributes: {language: "sql", strict: false, ...}
       └─> return FunctionMetadata

4. Catalog Storage (catalog.rs)
   └─> store_function(metadata)
       └─> INSERT INTO __pg_functions__ (...)

5. Response to Client:
   CREATE FUNCTION
```

## Data Flow: Function Execution

```
1. Client sends:
   SELECT add(5, 3);

2. Query Handler (main.rs)
   └─> transpile_with_metadata("SELECT add(5, 3)")
       └─> detect function call in AST
           └─> return "__USER_FUNC__(add, 5, 3)"

3. Query Handler detects function marker
   └─> execute_with_function_calls("__USER_FUNC__(add, 5, 3)")
       ├─> extract_function_call()
       │   └─> return ("add", [5, 3])
       │
       ├─> catalog::get_function(conn, "add", None)
       │   └─> return FunctionMetadata {
       │         name: "add",
       │         arg_types: ["int", "int"],
       │         return_type: "int",
       │         function_body: "SELECT a + b",
       │         ...
       │      }
       │
       └─> functions::execute_sql_function(conn, metadata, [5, 3])
           ├─> validate_arguments(metadata, [5, 3]) ✓
           ├─> check STRICT: false, continue
           ├─> substitute_parameters("SELECT a + b", [5, 3])
           │   └─> return "SELECT 5 + 3"
           ├─> transpile("SELECT 5 + 3")
           │   └─> return "SELECT 5 + 3" (no changes needed)
           │
           └─> execute_scalar_function(conn, "SELECT 5 + 3")
               └─> conn.prepare("SELECT 5 + 3")?
                   └─> stmt.query_row()?
                       └─> return FunctionResult::Scalar(Some(8))

4. Convert result to Response
   └─> convert_function_result_to_response(Scalar(8))
       └─> return QueryResponse with field "result" = 8

5. Response to Client:
   ┌─────────┐
   │ result  │
   ├─────────┤
   │    8    │
   └─────────┘
```

## Catalog Schema

```
┌─────────────────────────────────────────────────────────────┐
│                 __pg_functions__ Table                       │
├──────────────┬──────────────┬───────────────────────────────┤
│ Column       │ Type         │ Description                   │
├──────────────┼──────────────┼───────────────────────────────┤
│ oid          │ INTEGER PK   │ Function OID (auto-increment) │
│ funcname     │ TEXT         │ Function name                 │
│ schema_name  │ TEXT         │ Schema (default: 'public')    │
│ arg_types    │ TEXT (JSON)  │ ["int", "text", ...]          │
│ arg_names    │ TEXT (JSON)  │ ["a", "b", ...]               │
│ arg_modes    │ TEXT (JSON)  │ ["IN", "OUT", ...]            │
│ return_type  │ TEXT         │ "int", "SETOF users", etc.    │
│ return_type_ │ TEXT         │ "SCALAR", "SETOF", "TABLE"    │
│   kind       │              │                               │
│ return_table │ TEXT (JSON)  │ For TABLE: [{"name":"id",     │
│   _cols      │              │              "type":"int"},..]│
│ function_body│ TEXT         │ SQL body (AS $$ ... $$)       │
│ language     │ TEXT         │ "sql", "plpgsql" (future)     │
│ volatility   │ TEXT         │ "IMMUTABLE", "STABLE", ...    │
│ strict       │ BOOLEAN      │ STRICT / RETURNS NULL ON NULL │
│ security_    │ BOOLEAN      │ SECURITY DEFINER              │
│   definer    │              │                               │
│ parallel     │ TEXT         │ "UNSAFE", "RESTRICTED", ...   │
│ owner_oid    │ INTEGER      │ Owner role OID                │
│ created_at   │ TEXT         │ Timestamp                     │
└──────────────┴──────────────┴───────────────────────────────┘

Indexes:
  - idx_pg_functions_name: funcname
  - idx_pg_functions_schema: schema_name
```

## Function Metadata Structure

```rust
FunctionMetadata {
    oid: i64,                              // 12345
    name: String,                          // "add_numbers"
    schema: String,                        // "public"
    
    arg_types: Vec<String>,                // ["integer", "integer"]
    arg_names: Vec<String>,                // ["a", "b"]
    arg_modes: Vec<ParamMode>,             // [In, In]
    
    return_type: String,                   // "integer"
    return_type_kind: ReturnTypeKind,      // Scalar
    return_table_cols: Option<Vec<         // None
        (String, String)
    >>,
    
    function_body: String,                 // "SELECT a + b"
    language: String,                      // "sql"
    
    volatility: String,                    // "VOLATILE"
    strict: bool,                          // false
    security_definer: bool,                // false
    parallel: String,                      // "UNSAFE"
    
    owner_oid: i64,                        // 1
    created_at: Option<String>,            // "2026-03-02..."
}
```

## Function Execution Engine

```
execute_sql_function(conn, metadata, args)
│
├─> validate_arguments(metadata, args)
│   └─> check count matches ✓
│
├─> if metadata.strict && any_null(args)
│   └─> return FunctionResult::Null
│
├─> substituted_body = substitute_parameters(metadata.function_body, args)
│   └─> "SELECT $1 + $2" + [5, 3]
│       └─> "SELECT 5 + 3"
│
├─> sqlite_sql = transpile(substituted_body)
│   └─> "SELECT 5 + 3" (unchanged)
│
└─> match metadata.return_type_kind
    │
    ├─> Scalar
    │   └─> execute_scalar_function(conn, sqlite_sql)
    │       └─> prepare("SELECT 5 + 3")
    │           └─> query_row()
    │               └─> FunctionResult::Scalar(Some(8))
    │
    ├─> SetOf
    │   └─> execute_setof_function(conn, sqlite_sql)
    │       └─> prepare("SELECT id FROM users")
    │           └─> query_map()
    │               └─> FunctionResult::SetOf([1, 2, 3])
    │
    ├─> Table
    │   └─> execute_table_function(conn, sqlite_sql)
    │       └─> prepare("SELECT id, name FROM users")
    │           └─> query()
    │               └─> FunctionResult::Table([
    │                       [1, "Alice"],
    │                       [2, "Bob"]
    │                   ])
    │
    └─> Void
        └─> execute_void_function(conn, sqlite_sql)
            └─> execute("UPDATE ...")
                └─> FunctionResult::Void
```

## Component Interaction Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        PostgreSQLite                             │
├─────────────────┬─────────────────┬─────────────────────────────┤
│   Frontend      │   Middleware    │         Backend             │
│   (Protocol)    │   (Logic)       │         (Storage)           │
├─────────────────┼─────────────────┼─────────────────────────────┤
│                 │                 │                             │
│  SimpleQuery    │  Transpiler     │  Catalog                    │
│  Handler        │  ──────────────>│  ───────────────────────────>│
│                 │  • Parse CREATE │  • __pg_functions__         │
│                 │    FUNCTION     │  • Store metadata           │
│                 │  • Parse calls  │  • Retrieve metadata        │
│                 │                 │                             │
│                 │  Functions      │  SQLite                     │
│  ExtendedQuery  │  Engine         │  Connection                 │
│  Handler        │  ──────────────>│  ───────────────────────────>│
│                 │  • Execute      │  • Execute function body    │
│                 │    functions    │  • Return results           │
│                 │  • Substitute   │                             │
│                 │    params       │                             │
│                 │                 │                             │
└─────────────────┴─────────────────┴─────────────────────────────┘
         │                 │                 │
         │                 │                 │
         ▼                 ▼                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Client Applications                         │
│  • psql                                                         │
│  • psycopg2 (Python)                                            │
│  • pgAdmin                                                      │
│  • Any PostgreSQL client                                        │
└─────────────────────────────────────────────────────────────────┘
```

## State Diagram: Function Lifecycle

```
                    ┌─────────────┐
                    │   Initial   │
                    └──────┬──────┘
                           │
                           │ CREATE FUNCTION
                           ▼
                    ┌─────────────┐
                    │   Parsed    │
                    │  (metadata  │
                    │  extracted) │
                    └──────┬──────┘
                           │
                           │ Store in catalog
                           ▼
                    ┌─────────────┐
                    │   Stored    │
                    │   (in       │
                    │__pg_funcs__)│
                    └──────┬──────┘
                           │
           ┌───────────────┼───────────────┐
           │               │               │
           │               │               │
           ▼               ▼               ▼
    ┌──────────┐   ┌─────────────┐  ┌──────────┐
    │ Called   │   │ CREATE OR   │  │  DROP    │
    │  (exec)  │<──│   REPLACE   │  │ FUNCTION │
    └─────┬────┘   └─────────────┘  └──────────┘
          │               │
          │               │ (updates metadata)
          │               ▼
          │         ┌─────────────┐
          │         │   Updated   │
          │         │   (replace  │
          │         │   existing) │
          │         └──────┬──────┘
          │                │
          └────────────────┘
          (continues execution)
```

## Error Handling Flow

```
Function Call
│
├─> Function not found in catalog?
│   └─> Error: "function does not exist"
│
├─> Argument count mismatch?
│   └─> Error: "function expects N arguments"
│
├─> STRICT function with NULL argument?
│   └─> Return NULL (not error)
│
├─> Function body execution fails?
│   ├─> SQL syntax error?
│   │   └─> Error: propagate from SQLite
│   │
│   ├─> Type mismatch?
│   │   └─> Error: "operator does not exist"
│   │
│   └─> Constraint violation?
│       └─> Error: propagate from SQLite
│
└─> Success
    └─> Return FunctionResult
```

## Phase 1 vs Phase 2 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Phase 1: SQL Functions                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  CREATE FUNCTION ... LANGUAGE sql                           │
│  │                                                          │
│  ├─> Parse (transpiler.rs)                                 │
│  ├─> Store metadata (catalog.rs)                           │
│  └─> Execute: substitute params → transpile → run in SQLite│
│                                                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                   Phase 2: PL/pgSQL Functions                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  CREATE FUNCTION ... LANGUAGE plpgsql                       │
│  │                                                          │
│  ├─> Parse PL/pgSQL syntax (plpgsql.rs)                    │
│  ├─> Transpile to Lua (plpgsql.rs)                         │
│  ├─> Store Lua code (catalog.rs)                           │
│  └─> Execute: run in Lua sandbox (plpgsql.rs)              │
│        ├─> Access SQLite via connection                    │
│        ├─> Handle control flow (IF, LOOP, etc.)            │
│        ├─> Handle exceptions                               │
│        └─> Return results                                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Performance Considerations

```
Current Approach (Phase 1):
┌─────────────────────────────────────────┐
│ Query: SELECT add(5, 3)                 │
│                                         │
│ 1. Parse function call                  │
│ 2. Lookup metadata in catalog           │
│ 3. Substitute parameters ($1→5, $2→3)   │
│ 4. Transpile body                       │
│ 5. Execute in SQLite                    │
│                                         │
│ Total: ~5 steps per call                │
└─────────────────────────────────────────┘

Optimization Opportunities:
┌─────────────────────────────────────────┐
│ 1. Cache function metadata              │
│    - Reduce catalog lookups             │
│                                         │
│ 2. Pre-transpile function bodies        │
│    - Store SQLite version in catalog    │
│                                         │
│ 3. Inline simple functions              │
│    - Replace call with body directly    │
│                                         │
│ 4. Connection pooling                   │
│    - Reuse SQLite connections           │
└─────────────────────────────────────────┘
```

---

These diagrams provide a visual understanding of how the function system will work. Use them alongside the detailed implementation documents for complete context.
