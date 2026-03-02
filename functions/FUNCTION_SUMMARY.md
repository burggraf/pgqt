# PostgreSQLite Function Support - Implementation Summary

## 📋 Overview

This document summarizes the plan for implementing PostgreSQL-compatible user-defined functions in PostgreSQLite, with full support for SQL-language functions in Phase 1 and PL/pgSQL (via Lua) in Phase 2.

## 🎯 Goals

- **100% PostgreSQL compatibility** for `CREATE FUNCTION` syntax
- Support all parameter modes: `IN`, `OUT`, `INOUT`, `VARIADIC`
- Support all return types: scalar, `SETOF`, `TABLE`, `VOID`
- Store functions in catalog tables (like `__pg_functions__`)
- Intercept and execute function calls transparently
- Phase 1: SQL-language functions only
- Phase 2: PL/pgSQL via Lua runtime

---

## 📁 Deliverables

### Documentation Created

1. **`FUNCTION_IMPLEMENTATION_PLAN.md`** - Comprehensive 23KB implementation plan
   - Complete architecture and design
   - Phase 1 and Phase 2 specifications
   - Catalog schema design
   - Execution engine design
   - Testing strategy
   - Timeline and success criteria

2. **`FUNCTION_QUICK_START.md`** - 5KB quick reference guide
   - Step-by-step implementation checklist
   - File changes required
   - Key design decisions
   - Common pitfalls

3. **`FUNCTION_CODE_EXAMPLES.md`** - 27KB detailed code examples
   - Complete catalog schema implementation
   - Function metadata structures
   - Storage APIs with serde_json
   - Function execution engine
   - CREATE FUNCTION parsing
   - Integration with main handler
   - Example usage

---

## 🔧 Technical Architecture

### Phase 1: SQL Functions

#### 1. Catalog Storage (`src/catalog.rs`)
```rust
// New table: __pg_functions__
// Stores: name, params, return type, body, attributes
// Indexed by: funcname, schema_name
```

#### 2. Function Execution Engine (`src/functions.rs` - NEW)
```rust
// Executes SQL-language functions
// Handles: parameter substitution, STRICT checking, return types
// Returns: FunctionResult (Scalar/SetOf/Table/Void/Null)
```

#### 3. CREATE FUNCTION Parser (`src/transpiler.rs`)
```rust
// Parses PostgreSQL CREATE FUNCTION syntax
// Extracts: name, parameters, return type, body, attributes
// Returns: FunctionMetadata struct
```

#### 4. Integration (`src/main.rs`)
```rust
// Handles: CREATE FUNCTION, DROP FUNCTION
// Intercepts: function calls in queries
// Executes: user-defined functions
```

### Phase 2: PL/pgSQL Functions (Future)

#### Lua Runtime (`src/plpgsql.rs`)
```rust
// Parses PL/pgSQL syntax
// Transpiles to Lua
// Executes in Lua sandbox
// Supports: DECLARE, BEGIN/END, IF/THEN/ELSE, LOOP, EXCEPTION
```

---

## 📊 Implementation Checklist

### Phase 1 - Core Functionality

- [ ] **Catalog Schema**
  - [ ] Add `__pg_functions__` table to `init_catalog()`
  - [ ] Create indexes on `funcname` and `schema_name`
  - [ ] Implement `FunctionMetadata` struct
  - [ ] Implement `ParamMode` and `ReturnTypeKind` enums

- [ ] **Storage APIs**
  - [ ] `store_function()` - INSERT/UPDATE
  - [ ] `get_function()` - SELECT by name
  - [ ] `drop_function()` - DELETE
  - [ ] `list_functions()` - Query all functions

- [ ] **Function Execution**
  - [ ] `execute_sql_function()` - Main execution entry point
  - [ ] `substitute_parameters()` - Replace $1, $2, etc.
  - [ ] `execute_scalar_function()` - Single value return
  - [ ] `execute_setof_function()` - Multiple values
  - [ ] `execute_table_function()` - Multiple rows/columns
  - [ ] `execute_void_function()` - No return

- [ ] **CREATE FUNCTION Parsing**
  - [ ] `parse_create_function()` - Entry point
  - [ ] `parse_function_parameter()` - Extract param info
  - [ ] `parse_return_type()` - Handle all return types
  - [ ] `parse_function_attributes()` - IMMUTABLE, STRICT, etc.
  - [ ] `extract_function_body()` - Get AS $$ ... $$ content

- [ ] **Integration**
  - [ ] `handle_create_function()` in main.rs
  - [ ] `handle_drop_function()` in main.rs
  - [ ] `execute_with_function_calls()` - Intercept calls
  - [ ] `convert_function_result_to_response()` - Format output
  - [ ] Modify `execute_query()` to detect function statements

- [ ] **Testing**
  - [ ] Unit tests in `src/functions.rs`
  - [ ] Integration tests in `tests/function_tests.rs`
  - [ ] E2E tests in `tests/function_e2e_test.py`
  - [ ] Test all parameter modes (IN, OUT, INOUT, VARIADIC)
  - [ ] Test all return types (Scalar, SETOF, TABLE, VOID)
  - [ ] Test function attributes (STRICT, IMMUTABLE, etc.)
  - [ ] Test CREATE OR REPLACE
  - [ ] Test function calls in SELECT, WHERE clauses

- [ ] **Documentation**
  - [ ] `docs/functions.md` - User documentation
  - [ ] Code comments and examples
  - [ ] Update README.md with function support info

### Phase 2 - PL/pgSQL (Future)

- [ ] **PL/pgSQL Parser**
  - [ ] Parse DECLARE blocks
  - [ ] Parse BEGIN/END blocks
  - [ ] Parse control structures (IF, LOOP, WHILE, FOR)
  - [ ] Parse EXCEPTION blocks

- [ ] **Lua Transpiler**
  - [ ] Convert PL/pgSQL to Lua syntax
  - [ ] Map PostgreSQL types to Lua
  - [ ] Handle variable declarations
  - [ ] Handle control flow
  - [ ] Handle RETURN statements

- [ ] **Lua Runtime**
  - [ ] Embed Lua interpreter (mlua crate)
  - [ ] Create execution sandbox
  - [ ] Provide PostgreSQL-compatible API
  - [ ] Handle RAISE statements
  - [ ] Support dynamic SQL (EXECUTE)

- [ ] **Trigger Support**
  - [ ] Support CREATE TRIGGER
  - [ ] Provide OLD/NEW row access
  - [ ] Support trigger variables (TG_NAME, TG_OP, etc.)

---

## 🗂️ File Structure

```
postgresqlite/
├── src/
│   ├── catalog.rs          # Add function catalog tables + APIs
│   ├── transpiler.rs       # Add CREATE FUNCTION parsing
│   ├── functions.rs        # NEW: Function execution engine
│   ├── plpgsql.rs          # Future: PL/pgSQL parser + Lua transpiler
│   └── main.rs             # Integrate function handling
├── tests/
│   ├── function_tests.rs   # NEW: Integration tests
│   └── function_e2e_test.py # NEW: End-to-end tests
├── docs/
│   └── functions.md        # NEW: User documentation
└── Documentation Files:
    ├── FUNCTION_IMPLEMENTATION_PLAN.md
    ├── FUNCTION_QUICK_START.md
    ├── FUNCTION_CODE_EXAMPLES.md
    └── FUNCTION_SUMMARY.md (this file)
```

---

## 🧪 Testing Strategy

### Unit Tests
- Parameter substitution
- Value quoting
- STRICT attribute handling
- Return type conversions

### Integration Tests
- CREATE FUNCTION with various signatures
- Function execution with different argument types
- RETURN TABLE and RETURN SETOF
- CREATE OR REPLACE FUNCTION
- DROP FUNCTION

### E2E Tests
- Full wire protocol testing
- Function calls in SELECT clauses
- Function calls in WHERE clauses
- Nested function calls
- Transaction safety

---

## 📈 Success Metrics

### Phase 1 Must-Haves
- ✅ CREATE FUNCTION works for simple scalar functions
- ✅ CREATE OR REPLACE FUNCTION works
- ✅ DROP FUNCTION works
- ✅ Functions callable in SELECT and WHERE clauses
- ✅ IN, OUT, INOUT parameters work
- ✅ RETURNS TABLE works
- ✅ RETURNS SETOF works
- ✅ STRICT attribute works
- ✅ All tests pass (unit + integration + E2E)

### Phase 1 Nice-to-Haves
- ✅ VARIADIC parameters
- ✅ IMMUTABLE/STABLE/VOLATILE attributes
- ✅ SECURITY DEFINER
- ✅ PARALLEL attributes
- ✅ Function overloading by argument types

---

## ⚠️ Technical Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Complex function body parsing | High | Start simple, iterate |
| SQL injection via parameter substitution | Critical | Use proper quoting, never concat |
| Performance of function interception | Medium | Cache metadata, optimize lookup |
| Type compatibility issues | High | Use catalog type system |
| Nested function calls complexity | Medium | Implement call stack tracking |

---

## 🚀 Next Steps

1. **Review documentation** - Read all three detailed docs
2. **Set up worktree** - Isolated development environment
3. **Implement catalog schema** - Start with `src/catalog.rs`
4. **Build execution engine** - Create `src/functions.rs`
5. **Add parsing** - Extend `src/transpiler.rs`
6. **Integrate** - Modify `src/main.rs`
7. **Test comprehensively** - Unit, integration, E2E
8. **Document** - Write user-facing docs

---

## 📚 Resources

- **PostgreSQL CREATE FUNCTION**: https://www.postgresql.org/docs/current/sql-createfunction.html
- **pg_query Rust**: https://docs.rs/pg_query/
- **SQLite Custom Functions**: https://docs.rs/rusqlite/
- **Existing PostgreSQLite Code**: Study `src/catalog.rs`, `src/transpiler.rs`, `src/main.rs`

---

## 🎓 Key Design Decisions

1. **JSON for Arrays**: Use serde_json for flexible parameter storage
2. **Parameter Substitution**: Replace `$1`, `$2` with quoted values (1-indexed)
3. **Return Type Categories**: Scalar, SetOf, Table, Void
4. **Execution Strategy**: Transpile function body each execution (can optimize later)
5. **Catalog Storage**: Per-database storage (not separate schema catalogs yet)
6. **Phase 1 Scope**: SQL-language only, defer PL/pgSQL to Phase 2

---

## 💡 Example Usage

```sql
-- Simple scalar function
CREATE FUNCTION add(a int, b int) RETURNS int AS $$ SELECT a + b $$ LANGUAGE sql;
SELECT add(5, 3);  -- Returns 8

-- RETURNS TABLE
CREATE FUNCTION get_users() RETURNS TABLE(id int, name text) 
AS $$ SELECT id, name FROM users $$ LANGUAGE sql;
SELECT * FROM get_users();

-- STRICT function
CREATE FUNCTION square(x int) RETURNS int STRICT 
AS $$ SELECT x * x $$ LANGUAGE sql;
SELECT square(NULL);  -- Returns NULL

-- OUT parameters
CREATE FUNCTION get_user_info(id int, OUT name text, OUT email text)
AS $$ SELECT name, email FROM users WHERE user_id = id $$ LANGUAGE sql;
SELECT * FROM get_user_info(1);
```

---

## 📞 Support

For questions or issues:
- Review the detailed documentation files
- Check existing PostgreSQLite code patterns
- Refer to PostgreSQL and pg_query documentation
- Test incrementally with small examples first

---

**Status**: Planning Complete ✅  
**Phase 1 Ready**: Yes ✅  
**Phase 2 Planned**: Yes (Future)  
**Documentation**: Complete (3 files, 55KB total) ✅
