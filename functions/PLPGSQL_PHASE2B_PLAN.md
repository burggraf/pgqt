# PL/pgSQL Phase 2B - Control Flow Completion

## Overview

Phase 2B completes the control flow implementation for PL/pgSQL support, building on the foundation established in Phase 2A. This phase focuses on:

1. **Exception Handling** - Full SQLSTATE mapping and EXCEPTION blocks
2. **Advanced Control Flow** - Cursors, RETURN NEXT/QUERY
3. **Integration** - Connecting PL/pgSQL to the PGQT catalog system
4. **Testing** - Comprehensive test coverage for all features

## Current Status (Post Phase 2A)

### ✅ Completed in Phase 2A
- Parser using `pg_parse::parse_plpgsql()`
- AST types with custom deserializers
- Basic transpiler (assignment, RETURN, RAISE, PERFORM)
- Lua runtime with PGQT API
- 11 unit tests passing

### ⚠️ Known Issues to Address
1. Function name not extracted from pg_parse output
2. Some field type mismatches (integer vs string)
3. FOR loop variable declaration format
4. Missing integration with PGQT catalog

## Phase 2B Implementation Tasks

### Task 1: Fix Remaining Parser Issues (1-2 days)

#### 1.1 Function Name Extraction
The pg_parse library doesn't include the function name in the PL/pgSQL AST. We need to extract it separately.

**Approach:**
- Use `pg_query` to parse the CREATE FUNCTION statement
- Extract function name, argument names/types, and return type
- Merge with pg_parse output

**Files to modify:**
- `src/plpgsql/parser.rs` - Add function metadata extraction

**Example:**
```rust
pub struct PlpgsqlFunction {
    pub fn_name: String,  // Extracted from CREATE FUNCTION
    pub fn_argnames: Vec<String>,
    pub fn_argtypes: Vec<String>,
    pub fn_rettype: String,
    pub action: PlPgSQLAction,
}
```

#### 1.2 Fix Field Type Issues
- RAISE statement `elog_level` can be integer or string
- FOR loop variable declaration uses different field names

**Files to modify:**
- `src/plpgsql/ast.rs` - Update types with custom deserializers

### Task 2: Exception Handling (2-3 days)

#### 2.1 SQLSTATE Error Code Mapping
Implement comprehensive SQLSTATE to error condition mapping.

**Files to create:**
- `src/plpgsql/sqlstate.rs` - Error code definitions

```rust
pub const SQLSTATE_DIVISION_BY_ZERO: &str = "22012";
pub const SQLSTATE_UNIQUE_VIOLATION: &str = "23505";
pub const SQLSTATE_NO_DATA_FOUND: &str = "P0002";
// ... etc
```

#### 2.2 Exception Block Transpilation
Transpile EXCEPTION blocks to Lua pcall/xpcall with proper error handling.

**Example PL/pgSQL:**
```sql
BEGIN
    -- risky code
EXCEPTION
    WHEN division_by_zero THEN
        RETURN NULL;
    WHEN OTHERS THEN
        RAISE NOTICE 'Error: %', SQLERRM;
        RETURN -1;
END;
```

**Generated Lua:**
```lua
local _ok, _err = pcall(function()
  -- risky code
end)
if not _ok then
  local _sqlstate = _err.sqlstate or "P0001"
  local _sqlerrm = _err.message or tostring(_err)
  if _sqlstate == "22012" then
    return nil
  else
    _ctx.raise("NOTICE", "Error: %s", {_sqlerrm})
    return -1
  end
end
```

**Files to modify:**
- `src/plpgsql/transpiler.rs` - emit_block() with exception handling
- `src/plpgsql/runtime.rs` - Error handling in API

### Task 3: Advanced Features (2-3 days)

#### 3.1 RETURN NEXT / RETURN QUERY
Support for functions returning SETOF.

**PL/pgSQL:**
```sql
CREATE FUNCTION get_numbers() RETURNS SETOF int AS $$
BEGIN
    RETURN NEXT 1;
    RETURN NEXT 2;
    RETURN NEXT 3;
END;
$$ LANGUAGE plpgsql;
```

**Generated Lua:**
```lua
local _result_set = {}
local function get_numbers(_ctx)
  table.insert(_result_set, 1)
  table.insert(_result_set, 2)
  table.insert(_result_set, 3)
  return _result_set
end
return get_numbers
```

**Files to modify:**
- `src/plpgsql/transpiler.rs` - emit_return_next()
- `src/plpgsql/runtime.rs` - Handle result sets

#### 3.2 Cursor Support (OPEN/FETCH/CLOSE)
Implement cursor operations for iterating over query results.

**PL/pgSQL:**
```sql
DECLARE
    cur CURSOR FOR SELECT * FROM users;
    rec RECORD;
BEGIN
    OPEN cur;
    LOOP
        FETCH cur INTO rec;
        EXIT WHEN NOT FOUND;
        -- process rec
    END LOOP;
    CLOSE cur;
END;
```

**Files to modify:**
- `src/plpgsql/ast.rs` - Add cursor statement types
- `src/plpgsql/transpiler.rs` - Cursor transpilation
- `src/plpgsql/runtime.rs` - Cursor state management

### Task 4: GET DIAGNOSTICS (1 day)

Implement GET DIAGNOSTICS for accessing execution information.

**PL/pgSQL:**
```sql
GET DIAGNOSTICS row_count = ROW_COUNT;
GET DIAGNOSTICS sqlstate = RETURNED_SQLSTATE;
```

**Generated Lua:**
```lua
row_count = _ctx.ROW_COUNT
sqlstate = _ctx.SQLSTATE
```

**Files to modify:**
- `src/plpgsql/transpiler.rs` - emit_get_diag()

### Task 5: Catalog Integration (2-3 days)

#### 5.1 Store PL/pgSQL Functions in Catalog
Extend the catalog to store PL/pgSQL function metadata.

**Files to modify:**
- `src/catalog.rs` - Add language field to FunctionMetadata

```rust
pub struct FunctionMetadata {
    // ... existing fields
    pub language: String,  // "sql" or "plpgsql"
    pub lua_body: Option<String>,  // Pre-transpiled Lua code
}
```

#### 5.2 CREATE FUNCTION Integration
Parse CREATE FUNCTION ... LANGUAGE plpgsql and store in catalog.

**Files to modify:**
- `src/transpiler.rs` - Handle CREATE FUNCTION for plpgsql

#### 5.3 Function Execution Integration
Modify function execution to use PL/pgSQL runtime when language is plpgsql.

**Files to modify:**
- `src/functions.rs` - execute_plpgsql_function()
- `src/main.rs` - Route function calls appropriately

### Task 6: Testing (2-3 days)

#### 6.1 Unit Tests
Add comprehensive unit tests for all new features.

**Files to create/modify:**
- `src/plpgsql/tests.rs` or inline tests

#### 6.2 Integration Tests
Create Rust integration tests.

**Files to create:**
- `tests/plpgsql_tests.rs`

#### 6.3 E2E Tests
Create Python E2E tests for wire protocol testing.

**Files to create:**
- `tests/plpgsql_e2e_test.py`

## Implementation Order

1. **Week 1:**
   - Day 1-2: Fix parser issues (Task 1)
   - Day 3-5: Exception handling (Task 2)

2. **Week 2:**
   - Day 1-3: Advanced features (Task 3)
   - Day 4: GET DIAGNOSTICS (Task 4)
   - Day 5: Catalog integration start (Task 5)

3. **Week 3:**
   - Day 1-2: Complete catalog integration
   - Day 3-5: Testing (Task 6)

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_exception_handling() {
    let sql = r#"
        CREATE FUNCTION safe_divide(a int, b int) RETURNS int AS $$
        BEGIN
            RETURN a / b;
        EXCEPTION
            WHEN division_by_zero THEN
                RETURN NULL;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    let lua = transpile_to_lua(&func).unwrap();
    
    // Execute and test
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    let result = runtime.execute_function(&conn, &lua, &[
        SqliteValue::Integer(10),
        SqliteValue::Integer(0),
    ]).unwrap();
    
    assert_eq!(result, SqliteValue::Null);
}
```

### E2E Tests
```python
def test_plpgsql_exception():
    conn = psycopg2.connect(...)
    cur = conn.cursor()
    
    cur.execute("""
        CREATE FUNCTION safe_divide(a int, b int) RETURNS int AS $$
        BEGIN
            RETURN a / b;
        EXCEPTION
            WHEN division_by_zero THEN
                RETURN NULL;
        END;
        $$ LANGUAGE plpgsql;
    """)
    
    cur.execute("SELECT safe_divide(10, 0)")
    result = cur.fetchone()[0]
    assert result is None
    print("test_plpgsql_exception: PASSED")
```

## Success Criteria

- [ ] All parser issues resolved
- [ ] Exception handling with SQLSTATE mapping
- [ ] RETURN NEXT/QUERY for SETOF functions
- [ ] Cursor support (OPEN/FETCH/CLOSE)
- [ ] GET DIAGNOSTICS implementation
- [ ] Catalog integration for PL/pgSQL functions
- [ ] 90%+ test coverage for new code
- [ ] All tests passing

## Dependencies

- `pg_parse` - Already added in Phase 2A
- `mlua` - Already added in Phase 2A
- No new dependencies required

## Risks and Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| pg_parse limitations | High | Use pg_query for metadata extraction |
| Lua sandbox complexity | Medium | Comprehensive testing, gradual feature rollout |
| Performance concerns | Medium | Benchmark early, optimize hot paths |
| SQLSTATE mapping completeness | Low | Start with common errors, add as needed |

## Notes

- Keep the transpiler simple - generate straightforward Lua
- Use the PGQT API (_ctx) consistently
- Test edge cases thoroughly
- Document any PostgreSQL-specific behavior that differs from standard
