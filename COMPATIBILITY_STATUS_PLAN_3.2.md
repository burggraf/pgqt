# Implementation Plan: 3.2 System Catalog Completeness

## Overview
Complete the `pg_catalog` system views to support PostgreSQL introspection tools like `psql`, pgAdmin, and other database management tools that query system catalogs.

## Background
PostgreSQL clients and tools rely heavily on `pg_catalog` views for introspection. When these views are incomplete or have missing columns, tools may fail to display schema information, list tables, or provide autocomplete functionality.

## Problem Statement
Introspection tools like `psql` fail because `pg_catalog` views are missing standard columns that these tools expect. This affects:
- `\d` commands in psql
- Schema browsing in pgAdmin
- ORM introspection (Django, SQLAlchemy, etc.)
- Database migration tools

## Target Views to Audit and Complete

### Critical Views (Most Used by Tools)

1. **`pg_class`** - Table, index, sequence information
   - Missing columns: `reltoastrelid`, `reltoastidxid`, `reldeltarelid`, `reldeltaidx`, `relcudescrelid`, `relcudescidx`, `relhasindex`, `relisshared`, `relpersistence`, `relkind`, `relnatts`, `relchecks`, `relhasrules`, `relhastriggers`, `relhassubclass`, `relcmprs`, `relhasclusterkey`, `relrowmovement`, `parttype`, `relfrozenxid`, `relacl`, `reloptions`, `relreplident`

2. **`pg_attribute`** - Column information
   - Missing columns: `attrelid`, `attname`, `atttypid`, `attstattarget`, `attlen`, `attnum`, `attndims`, `attcacheoff`, `atttypmod`, `attbyval`, `attstorage`, `attalign`, `attnotnull`, `atthasdef`, `attisdropped`, `attislocal`, `attcmprmode`, `attinhcount`, `attcollation`, `attacl`, `attoptions`, `attfdwoptions`, `attinitdefval`

3. **`pg_type`** - Data type information
   - May need updates for completeness

4. **`pg_namespace`** - Schema information
   - Usually simpler, verify completeness

5. **`pg_index`** - Index information
   - Missing columns: `indexrelid`, `indrelid`, `indnatts`, `indisunique`, `indisprimary`, `indisexclusion`, `indimmediate`, `indisclustered`, `indisusable`, `indisvalid`, `indcheckxmin`, `indisready`, `indkey`, `indcollation`, `indclass`, `indoption`, `indexprs`, `indpred`

6. **`pg_constraint`** - Constraint information
   - Verify completeness for primary keys, foreign keys, check constraints

7. **`pg_attrdef`** - Default values
   - May need creation if not present

8. **`pg_proc`** - Function/procedure information
   - Needed for function introspection

9. **`pg_trigger`** - Trigger information
   - Needed for trigger listing

## Implementation Strategy

### Step 1: Audit Current Implementation

Read `src/catalog/system_views.rs` and document:
- [ ] Which views currently exist
- [ ] Which columns are present
- [ ] Which columns are missing compared to PostgreSQL

### Step 2: Research Column Meanings

For each missing column, determine:
- Data type
- Default value (if not applicable to SQLite)
- How to derive from SQLite catalog tables

### Step 3: Implement Missing Columns

Update view definitions in `src/catalog/system_views.rs` to include missing columns with appropriate:
- Data types
- Default values
- Derived values from SQLite schema

### Step 4: Add Helper Functions

Some columns may require helper functions to compute values:
- Type OID lookups
- Constraint type mappings
- Index property extraction

## Detailed Column Implementation

### pg_class View Enhancements

```sql
-- Example enhanced pg_class view
CREATE VIEW pg_catalog.pg_class AS
SELECT 
    t.oid as oid,
    t.name as relname,
    n.oid as relnamespace,
    0::oid as reltype,  -- No row types in SQLite
    0::oid as reloftype,
    0::oid as relowner,
    0::oid as relam,
    0::oid as reltablespace,
    0::oid as reltoastrelid,  -- No TOAST in SQLite
    0::oid as reltoastidxid,
    0::oid as reldeltarelid,
    0::oid as reldeltaidx,
    0::oid as relcudescrelid,
    0::oid as relcudescidx,
    CASE WHEN i.name IS NOT NULL THEN 1 ELSE 0 END as relhasindex,
    0 as relisshared,
    'p'::char as relpersistence,  -- 'p' = permanent
    CASE 
        WHEN t.type = 'table' THEN 'r'::char
        WHEN t.type = 'view' THEN 'v'::char
        WHEN t.type = 'index' THEN 'i'::char
        ELSE 'r'::char
    END as relkind,
    (SELECT COUNT(*) FROM pragma_table_info(t.name)) as relnatts,
    0 as relchecks,
    0 as relhasrules,
    0 as relhastriggers,
    0 as relhassubclass,
    0 as relcmprs,
    0 as relhasclusterkey,
    0 as relrowmovement,
    'n'::char as parttype,  -- 'n' = not partitioned
    0 as relfrozenxid,
    NULL as relacl,
    NULL as reloptions,
    'd'::char as relreplident  -- 'd' = default
FROM __pg_catalog__.pg_class_tables t
JOIN __pg_catalog__.pg_namespace n ON t.schema = n.nspname
LEFT JOIN sqlite_master i ON i.tbl_name = t.name AND i.type = 'index';
```

### pg_attribute View Enhancements

```sql
-- Example enhanced pg_attribute view
CREATE VIEW pg_catalog.pg_attribute AS
SELECT 
    t.oid as attrelid,
    c.name as attname,
    COALESCE(pt.oid, 25) as atttypid,  -- 25 = text default
    -1 as attstattarget,
    -1 as attlen,
    c.cid as attnum,
    0 as attndims,
    -1 as attcacheoff,
    -1 as atttypmod,
    false as attbyval,
    'x'::char as attstorage,  -- 'x' = extended
    'i'::char as attalign,    -- 'i' = int alignment
    c.notnull as attnotnull,
    false as atthasdef,       -- Would need pg_attrdef join
    false as attisdropped,
    true as attislocal,
    0 as attcmprmode,
    0 as attinhcount,
    0 as attcollation,
    NULL as attacl,
    NULL as attoptions,
    NULL as attfdwoptions,
    NULL as attinitdefval
FROM __pg_catalog__.pg_class_tables t
JOIN pragma_table_info(t.name) c
LEFT JOIN __pg_catalog__.pg_types pt ON pt.typname = 
    CASE c.type
        WHEN 'INTEGER' THEN 'int4'
        WHEN 'REAL' THEN 'float8'
        WHEN 'TEXT' THEN 'text'
        WHEN 'BLOB' THEN 'bytea'
        ELSE 'text'
    END;
```

## Implementation Steps

### Phase A: Audit and Document
- [ ] Read current `src/catalog/system_views.rs`
- [ ] Create a mapping of existing vs required columns
- [ ] Document default values for SQLite-irrelevant columns

### Phase B: Core View Updates
- [ ] Update `pg_class` with all missing columns
- [ ] Update `pg_attribute` with all missing columns
- [ ] Update `pg_index` with all missing columns
- [ ] Update `pg_type` if needed
- [ ] Update `pg_namespace` if needed

### Phase C: Additional Views
- [ ] Create/update `pg_constraint` view
- [ ] Create/update `pg_attrdef` view
- [ ] Create/update `pg_proc` view
- [ ] Create/update `pg_trigger` view

### Phase D: Testing
- [ ] Add unit tests for view column counts
- [ ] Add integration tests querying views
- [ ] Test with actual psql client

## Testing Strategy

### Unit Tests

Add to `src/catalog/system_views.rs` or `tests/catalog_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pg_class_columns() {
        // Verify pg_class has expected columns
        let conn = setup_test_db();
        let mut stmt = conn.prepare(
            "SELECT column_name FROM information_schema.columns 
             WHERE table_name = 'pg_class' AND table_schema = 'pg_catalog'"
        ).unwrap();
        let columns: Vec<String> = stmt.query_map([], |row| {
            row.get(0)
        }).unwrap().map(|r| r.unwrap()).collect();
        
        assert!(columns.contains(&"reltoastrelid".to_string()));
        assert!(columns.contains(&"relhasindex".to_string()));
        assert!(columns.contains(&"relpersistence".to_string()));
        // ... etc
    }
    
    #[test]
    fn test_pg_attribute_columns() {
        // Verify pg_attribute has expected columns
        let conn = setup_test_db();
        let mut stmt = conn.prepare(
            "SELECT column_name FROM information_schema.columns 
             WHERE table_name = 'pg_attribute' AND table_schema = 'pg_catalog'"
        ).unwrap();
        let columns: Vec<String> = stmt.query_map([], |row| {
            row.get(0)
        }).unwrap().map(|r| r.unwrap()).collect();
        
        assert!(columns.contains(&"attstattarget".to_string()));
        assert!(columns.contains(&"attcacheoff".to_string()));
        assert!(columns.contains(&"attisdropped".to_string()));
        // ... etc
    }
}
```

### Integration Tests

Create `tests/system_catalog_tests.rs`:

```rust
use pgqt::catalog::init_catalog;

#[test]
fn test_pg_class_query() {
    let conn = setup_test_db();
    init_catalog(&conn).unwrap();
    
    // Query should not fail
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM pg_catalog.pg_class",
        [],
        |row| row.get(0)
    );
    assert!(result.is_ok());
}

#[test]
fn test_pg_attribute_query() {
    let conn = setup_test_db();
    init_catalog(&conn).unwrap();
    
    // Create a test table
    conn.execute("CREATE TABLE test (id INT, name TEXT)", []).unwrap();
    
    // Query should return columns
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_catalog.pg_attribute 
         WHERE attrelid = (SELECT oid FROM pg_catalog.pg_class WHERE relname = 'test')",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert_eq!(count, 2);  -- id and name columns
}
```

### E2E Test with psql

Create `tests/catalog_e2e_test.py`:

```python
#!/usr/bin/env python3
"""End-to-end tests for system catalog completeness."""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_catalog_e2e.db"

def start_proxy():
    env = os.environ.copy()
    env["RUST_LOG"] = "info"
    proc = subprocess.Popen(
        ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    time.sleep(1)
    return proc

def stop_proxy(proc):
    proc.send_signal(signal.SIGTERM)
    proc.wait()

def test_pg_class_columns():
    """Test pg_class has expected columns."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table
        cur.execute("CREATE TABLE test_table (id INT PRIMARY KEY, name TEXT)")
        conn.commit()
        
        # Query pg_class for expected columns
        cur.execute("""
            SELECT relname, relkind, relnatts, relhasindex 
            FROM pg_catalog.pg_class 
            WHERE relname = 'test_table'
        """)
        row = cur.fetchone()
        assert row is not None, "Table not found in pg_class"
        assert row[0] == 'test_table'
        assert row[1] == 'r'  -- regular table
        assert row[2] == 2    -- 2 columns
        
        # Query pg_attribute for columns
        cur.execute("""
            SELECT attname, atttypid, attnotnull, attnum
            FROM pg_catalog.pg_attribute a
            JOIN pg_catalog.pg_class c ON a.attrelid = c.oid
            WHERE c.relname = 'test_table'
            ORDER BY attnum
        """)
        rows = cur.fetchall()
        assert len(rows) == 2, f"Expected 2 columns, got {len(rows)}"
        
        cur.close()
        conn.close()
        print("test_pg_class_columns: PASSED")
    finally:
        stop_proxy(proc)
        if os.path.exists(DB_PATH):
            os.remove(DB_PATH)

def test_psql_compatibility():
    """Test that common psql introspection queries work."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Query similar to what psql uses for \d
        cur.execute("""
            SELECT n.nspname as "Schema",
                   c.relname as "Name",
                   CASE c.relkind 
                       WHEN 'r' THEN 'table'
                       WHEN 'v' THEN 'view'
                       WHEN 'i' THEN 'index'
                       ELSE c.relkind::text
                   END as "Type"
            FROM pg_catalog.pg_class c
            LEFT JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind IN ('r', 'v')
            ORDER BY 1, 2
        """)
        # Should not raise an error
        rows = cur.fetchall()
        
        cur.close()
        conn.close()
        print("test_psql_compatibility: PASSED")
    finally:
        stop_proxy(proc)
        if os.path.exists(DB_PATH):
            os.remove(DB_PATH)

if __name__ == "__main__":
    test_pg_class_columns()
    test_psql_compatibility()
```

## Verification Checklist

- [ ] `./run_tests.sh` passes all tests
- [ ] `cargo build --release` completes with no warnings
- [ ] postgres-compatibility-suite catalog-related tests pass
- [ ] psql `\d` command works without errors
- [ ] pgAdmin can browse schema
- [ ] All critical views have required columns
- [ ] COMPATIBILITY_STATUS_PLAN.md updated with completion status

## Progress Update Template

After completing this item, update COMPATIBILITY_STATUS_PLAN.md:

```markdown
### 3.2 System Catalog Completeness
- **Problem**: Introspection tools (like `psql`) fail due to missing columns in `pg_catalog` views.
- **Action**: Audit `src/catalog/system_views.rs` and add missing standard columns to `pg_attribute`, `pg_class`, etc.
- **Status**: Completed (Added all missing columns to pg_class, pg_attribute, pg_index, and other system views).
- **Metric**: psql \d commands work, pgAdmin can browse schema, introspection queries succeed.
```

## References

- PostgreSQL System Catalogs: https://www.postgresql.org/docs/current/catalogs.html
- pg_class: https://www.postgresql.org/docs/current/catalog-pg-class.html
- pg_attribute: https://www.postgresql.org/docs/current/catalog-pg-attribute.html
- pg_index: https://www.postgresql.org/docs/current/catalog-pg-index.html
