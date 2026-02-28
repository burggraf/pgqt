# Schema (Namespace) Support Implementation Plan

## Overview

This document outlines the implementation of PostgreSQL schema/namespace compatibility in PGlite Proxy. Schemas are a fundamental PostgreSQL feature for organizing database objects into logical namespaces.

## Current State

Currently, the proxy:
- Strips `public` and `pg_catalog` schema prefixes from table references
- Passes through other schema prefixes as-is (`schema.table`)
- Does not support CREATE SCHEMA, DROP SCHEMA, or schema management
- Has a hardcoded `pg_namespace` view with only system schemas
- Does not implement `search_path` resolution

## Design Decision: ATTACH DATABASE Approach

After researching options, we will use **SQLite ATTACH DATABASE** to emulate PostgreSQL schemas:

### Why ATTACH DATABASE?

| Approach | Pros | Cons |
|----------|------|------|
| **Table Prefixes** (`schema_table`) | Simple, no file management | Breaks foreign keys, ugly queries, non-standard |
| **ATTACH DATABASE** | Native SQLite syntax, proper isolation, FK support | File management, atomic transactions limited |
| **Single DB with views** | Single file | Complex view management, performance overhead |

**ATTACH DATABASE** provides the best PostgreSQL compatibility:
- `schema.table` syntax works natively
- Foreign keys work within each schema/database
- Clear separation of schema contents
- Can DETACH/ATTACH dynamically

### Implementation Details

1. **Schema Storage**: Each PostgreSQL schema (except `public`) maps to a separate SQLite file:
   - Main database file: `database.db` → `public` schema
   - Schema `inventory`: `database_inventory.db`
   - Schema `analytics`: `database_analytics.db`

2. **Naming Convention**: `{main_db_basename}_{schema_name}.db`

3. **Automatic ATTACH**: When a schema is created or referenced, automatically ATTACH the corresponding file

## Feature Matrix

### Phase 1: Core Schema Support (This Implementation)

| Feature | Priority | Description |
|---------|----------|-------------|
| CREATE SCHEMA | High | Create new schema with optional AUTHORIZATION |
| DROP SCHEMA | High | Drop schema with CASCADE/RESTRICT |
| Schema-qualified tables | High | CREATE TABLE schema.table, SELECT FROM schema.table |
| search_path | High | SET/SHOW search_path, object resolution |
| pg_namespace | High | Dynamic catalog view of schemas |
| current_schema() | Medium | Return current schema name |
| current_schemas() | Medium | Return list of schemas in search path |
| Schema privileges | Medium | GRANT/REVOKE USAGE, CREATE ON SCHEMA |

### Phase 2: Future Enhancements

| Feature | Priority | Description |
|---------|----------|-------------|
| ALTER SCHEMA | Low | Rename schema (requires file rename) |
| CREATE SCHEMA with elements | Low | CREATE SCHEMA ... CREATE TABLE ... |
| information_schema | Low | SQL-standard schema views |

## Implementation Plan

### Step 1: Schema Catalog Management

**File**: `src/schema.rs` (new file)

```rust
// Schema metadata storage
pub struct SchemaMetadata {
    pub oid: i64,
    pub nspname: String,      // schema name
    pub nspowner: i64,        // owner role OID
    pub nspacl: Option<String>, // ACL
}

// Schema management functions
pub fn init_schema_catalog(conn: &Connection) -> Result<()>;
pub fn create_schema(conn: &Connection, name: &str, owner: Option<&str>) -> Result<()>;
pub fn drop_schema(conn: &Connection, name: &str, cascade: bool) -> Result<()>;
pub fn schema_exists(conn: &Connection, name: &str) -> Result<bool>;
pub fn get_schema_owner(conn: &Connection, name: &str) -> Result<Option<i64>>;
pub fn list_schemas(conn: &Connection) -> Result<Vec<SchemaMetadata>>;
```

**Database Table**: `__pg_namespace__`
```sql
CREATE TABLE __pg_namespace__ (
    oid INTEGER PRIMARY KEY AUTOINCREMENT,
    nspname TEXT UNIQUE NOT NULL,
    nspowner INTEGER NOT NULL DEFAULT 10,
    nspacl TEXT
);
```

### Step 2: ATTACH DATABASE Management

**File**: `src/schema.rs`

```rust
pub struct SchemaManager {
    main_db_path: PathBuf,
    attached_schemas: DashMap<String, bool>,
}

impl SchemaManager {
    // Attach a schema's database file
    pub fn attach_schema(conn: &Connection, schema_name: &str, db_path: &Path) -> Result<()>;
    
    // Detach a schema's database file
    pub fn detach_schema(conn: &Connection, schema_name: &str) -> Result<()>;
    
    // Get the file path for a schema
    pub fn schema_db_path(&self, schema_name: &str) -> PathBuf;
    
    // Check if schema is attached
    pub fn is_attached(&self, schema_name: &str) -> bool;
}
```

### Step 3: Search Path Support

**File**: `src/schema.rs`

```rust
// Per-session search path
pub struct SearchPath {
    schemas: Vec<String>,
}

impl SearchPath {
    pub fn default() -> Self;
    pub fn parse(path: &str) -> Result<Self>;
    pub fn to_string(&self) -> String;
    pub fn resolve_table(&self, table_name: &str, conn: &Connection) -> Result<Option<String>>;
}
```

**Session State**: Store in `SessionContext` in `main.rs`:
```rust
struct SessionContext {
    authenticated_user: String,
    current_user: String,
    search_path: SearchPath,  // NEW
}
```

### Step 4: Transpiler Modifications

**File**: `src/transpiler.rs`

#### 4.1 Schema-Qualified Table References

Modify `reconstruct_range_var`:
```rust
fn reconstruct_range_var(range_var: &RangeVar, ctx: &mut TranspileContext) -> String {
    let table_name = range_var.relname.to_lowercase();
    let schema_name = range_var.schemaname.to_lowercase();
    
    // Handle explicit schema qualification
    if !schema_name.is_empty() && schema_name != "public" {
        // For non-public schemas, use schema.table (ATTACH syntax)
        return format!("{}.{}", schema_name, table_name);
    }
    
    // For public or no schema, just use table name
    table_name
}
```

#### 4.2 CREATE SCHEMA Transpilation

Add handling in `reconstruct_sql_with_metadata`:
```rust
NodeEnum::CreateSchemaStmt(stmt) => {
    // Handled in main.rs, return empty to avoid execution
    TranspileResult {
        sql: String::new(),
        create_table_metadata: None,
        referenced_tables: Vec::new(),
        operation_type: OperationType::DDL,
    }
}
```

#### 4.3 DROP SCHEMA Transpilation

Similar handling for `DropStmt` with `ObjectType::Schema`.

### Step 5: Statement Processing in main.rs

#### 5.1 CREATE SCHEMA

```rust
async fn handle_create_schema(&self, stmt: &CreateSchemaStmt) -> Result<Vec<Response>> {
    let schema_name = extract_schema_name(stmt);
    let owner = extract_owner(stmt);
    
    // 1. Create schema metadata
    catalog::create_schema(&conn, &schema_name, owner)?;
    
    // 2. Create and attach the database file
    let schema_db_path = self.schema_manager.schema_db_path(&schema_name);
    self.schema_manager.attach_schema(&conn, &schema_name, &schema_db_path)?;
    
    // 3. Return success
    Ok(vec![Response::Execution(Tag::new("CREATE SCHEMA"))])
}
```

#### 5.2 DROP SCHEMA

```rust
async fn handle_drop_schema(&self, stmt: &DropStmt) -> Result<Vec<Response>> {
    let schema_name = extract_schema_name(stmt);
    let cascade = stmt.behavior == DropBehavior::Cascade;
    
    // 1. Check if schema exists
    if !catalog::schema_exists(&conn, &schema_name)? {
        return Err(anyhow!("schema \"{schema_name}\" does not exist"));
    }
    
    // 2. If not CASCADE, check for objects
    if !cascade && !schema_is_empty(&conn, &schema_name)? {
        return Err(anyhow!("schema \"{schema_name}\" cannot be dropped without CASCADE"));
    }
    
    // 3. Drop all objects in schema (if CASCADE)
    if cascade {
        drop_schema_objects(&conn, &schema_name)?;
    }
    
    // 4. Detach and delete database file
    self.schema_manager.detach_schema(&conn, &schema_name)?;
    let db_path = self.schema_manager.schema_db_path(&schema_name);
    std::fs::remove_file(&db_path)?;
    
    // 5. Remove schema metadata
    catalog::drop_schema(&conn, &schema_name)?;
    
    Ok(vec![Response::Execution(Tag::new("DROP SCHEMA"))])
}
```

#### 5.3 SET search_path

```rust
async fn handle_set_search_path(&self, path: &str, session: &mut SessionContext) -> Result<Vec<Response>> {
    session.search_path = SearchPath::parse(path)?;
    Ok(vec![Response::Execution(Tag::new("SET"))])
}
```

#### 5.4 SHOW search_path

```rust
async fn handle_show_search_path(&self, session: &SessionContext) -> Result<Vec<Response>> {
    let path = session.search_path.to_string();
    Ok(vec![Response::Query(QueryResponse::new(
        vec![FieldInfo::new("search_path".to_string(), None, None, Type::TEXT)],
        vec![vec![Some(path)]],
    ))])
}
```

### Step 6: Catalog Views Updates

**File**: `src/catalog.rs`

Update `init_system_views` to use `__pg_namespace__`:

```rust
conn.execute(
    "CREATE VIEW IF NOT EXISTS pg_namespace AS
     SELECT oid, nspname, nspowner, nspacl
     FROM __pg_namespace__",
    [],
)?;

// Update pg_class to reference dynamic namespace
conn.execute(
    "CREATE VIEW IF NOT EXISTS pg_class AS
     SELECT
        sm.rowid as oid,
        sm.name as relname,
        CASE 
            WHEN sm.name IN (SELECT name FROM sqlite_master WHERE name NOT LIKE '__pg_%') 
            THEN (SELECT oid FROM __pg_namespace__ WHERE nspname = 'public')
            ELSE 2200
        END as relnamespace,
        ...",
    [],
)?;
```

### Step 7: Schema Privileges

**File**: `src/catalog.rs`

Extend `__pg_acl__` for schema privileges:

```sql
-- Schema privileges use object_type = 'schema'
-- object_id = schema OID
-- Privileges: USAGE, CREATE, ALL
```

**Functions**:
```rust
pub fn grant_schema_privilege(conn: &Connection, schema: &str, privilege: &str, grantee: &str) -> Result<()>;
pub fn revoke_schema_privilege(conn: &Connection, schema: &str, privilege: &str, grantee: &str) -> Result<()>;
pub fn check_schema_privilege(conn: &Connection, schema: &str, privilege: &str, user: &str) -> Result<bool>;
```

### Step 8: Helper Functions

Add to `main.rs`:

```rust
// current_schema() - returns first schema in search_path
conn.create_scalar_function("current_schema", 0, ..., |ctx| {
    // Get from session context
    Ok(session.search_path.first().unwrap_or("public").to_string())
})?;

// current_schemas(include_implicit) - returns array of schema names
conn.create_scalar_function("current_schemas", 1, ..., |ctx| {
    let include_implicit: bool = ctx.get(0)?;
    // Return schemas in search_path, optionally including pg_catalog
    Ok(format!("{{{}}}", schemas.join(",")))
})?;
```

## Test Plan

### Unit Tests

**File**: `tests/schema_tests.rs`

```rust
#[test]
fn test_create_schema() {
    let result = transpile("CREATE SCHEMA inventory");
    // Should be handled by main.rs, not transpiler
}

#[test]
fn test_drop_schema() {
    let result = transpile("DROP SCHEMA inventory");
    // Should be handled by main.rs, not transpiler
}

#[test]
fn test_schema_qualified_table() {
    let result = transpile("SELECT * FROM inventory.products");
    assert_eq!(result, "select * from inventory.products");
}

#[test]
fn test_public_schema_stripped() {
    let result = transpile("SELECT * FROM public.users");
    assert_eq!(result, "select * from users");
}

#[test]
fn test_create_table_in_schema() {
    let result = transpile("CREATE TABLE inventory.products (id SERIAL, name TEXT)");
    assert!(result.contains("inventory.products"));
}
```

### Integration Tests

**File**: `tests/schema_integration_tests.rs`

```rust
#[test]
fn test_schema_lifecycle() {
    // CREATE SCHEMA
    // CREATE TABLE schema.table
    // INSERT INTO schema.table
    // SELECT FROM schema.table
    // DROP SCHEMA CASCADE
}

#[test]
fn test_search_path() {
    // SET search_path TO schema1, public
    // SELECT FROM unqualified_table (should find schema1.table first)
}

#[test]
fn test_cross_schema_join() {
    // SELECT * FROM schema1.table1 JOIN schema2.table2 ON ...
}
```

### E2E Tests

**File**: `tests/schema_e2e_test.py`

```python
def test_schema_management():
    """Test full schema lifecycle via PostgreSQL client"""
    # Connect
    conn = psycopg.connect("host=127.0.0.1 port=5432 user=postgres")
    
    # Create schema
    conn.execute("CREATE SCHEMA test_schema")
    
    # Create table in schema
    conn.execute("CREATE TABLE test_schema.users (id SERIAL, name TEXT)")
    
    # Insert data
    conn.execute("INSERT INTO test_schema.users (name) VALUES ('Alice')")
    
    # Query data
    result = conn.execute("SELECT * FROM test_schema.users").fetchall()
    assert result == [(1, 'Alice')]
    
    # Test search_path
    conn.execute("SET search_path TO test_schema, public")
    result = conn.execute("SELECT * FROM users").fetchall()
    assert result == [(1, 'Alice')]
    
    # Drop schema
    conn.execute("DROP SCHEMA test_schema CASCADE")
```

## File Changes Summary

| File | Changes |
|------|---------|
| `src/schema.rs` | NEW - Schema management, ATTACH/DETACH, search_path |
| `src/catalog.rs` | Add `__pg_namespace__` table, update pg_namespace view |
| `src/transpiler.rs` | Handle schema-qualified names, CREATE/DROP SCHEMA |
| `src/main.rs` | Session search_path, statement handlers, helper functions |
| `src/lib.rs` | Export schema module |
| `tests/schema_tests.rs` | NEW - Unit tests |
| `tests/schema_integration_tests.rs` | NEW - Integration tests |
| `tests/schema_e2e_test.py` | NEW - E2E tests |

## Implementation Order

1. **Core infrastructure** (schema.rs, catalog.rs changes)
2. **Transpiler updates** (schema-qualified names)
3. **Statement handlers** (CREATE/DROP SCHEMA, SET/SHOW search_path)
4. **Session management** (search_path in SessionContext)
5. **Catalog views** (dynamic pg_namespace)
6. **Schema privileges** (GRANT/REVOKE ON SCHEMA)
7. **Unit tests**
8. **Integration tests**
9. **E2E tests**
10. **Documentation updates**

## Backward Compatibility

- Existing behavior for `public` schema unchanged
- Tables without schema qualifier still go to `public` (main database)
- No breaking changes to existing queries

## Limitations to Document

1. **Cross-schema foreign keys**: Not supported (SQLite ATTACH limitation)
2. **Cross-schema triggers**: Not supported (SQLite ATTACH limitation)
3. **Atomic commits**: WAL mode transactions are per-file only
4. **Schema rename**: Not implemented in Phase 1 (ALTER SCHEMA RENAME)

## Success Criteria

1. ✅ CREATE SCHEMA creates a new schema with attached database
2. ✅ DROP SCHEMA removes schema and deletes database file
3. ✅ Schema-qualified table references work correctly
4. ✅ SET/SHOW search_path works
5. ✅ Unqualified table names resolve via search_path
6. ✅ pg_namespace shows all schemas
7. ✅ current_schema() and current_schemas() work
8. ✅ GRANT/REVOKE ON SCHEMA works
9. ✅ All tests pass
10. ✅ Documentation updated
