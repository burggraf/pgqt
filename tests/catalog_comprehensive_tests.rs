//! Comprehensive tests for PostgreSQL system catalog (pg_catalog) implementation

use postgresqlite::catalog::{init_catalog, init_system_views, populate_pg_attribute, populate_pg_index, populate_pg_constraint};
use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    init_catalog(&conn).unwrap();
    init_system_views(&conn).unwrap();
    conn
}

/// Helper to create a table and populate catalog tables
fn create_test_table(conn: &Connection, sql: &str) {
    conn.execute(sql, []).unwrap();
    // Extract table name from CREATE TABLE statement
    let table_name = sql.split_whitespace()
        .skip(2)
        .next()
        .unwrap_or("unknown")
        .to_lowercase();
    populate_pg_attribute(conn, &table_name).ok();
    populate_pg_index(conn).ok();
    populate_pg_constraint(conn).ok();
}

#[test]
fn test_pg_class_has_all_columns() {
    let conn = setup_test_db();
    
    // Create a test table
    conn.execute("CREATE TABLE test_pg_class (id INTEGER PRIMARY KEY, name TEXT)", []).unwrap();
    
    // Query pg_class and verify columns exist
    let columns = vec![
        "oid", "relname", "relnamespace", "reltype", "reloftype",
        "relowner", "relam", "relfilenode", "reltablespace", "relpages",
        "reltuples", "relallvisible", "reltoastrelid", "relhasindex",
        "relisshared", "relpersistence", "relkind", "relnatts", "relchecks",
        "relhasrules", "relhastriggers", "relhassubclass", "relrowsecurity",
        "relforcerowsecurity", "relispopulated", "relreplident", "relispartition",
        "relrewrite", "relfrozenxid", "relminmxid", "relacl", "reloptions", "relpartbound"
    ];
    
    for col in &columns {
        let result: Result<i64, _> = conn.query_row(
            &format!("SELECT COUNT({}) FROM pg_class WHERE relname = 'test_pg_class'", col),
            [],
            |row| row.get(0)
        );
        assert!(result.is_ok(), "Column {} should exist in pg_class", col);
    }
    
    // Verify the table is in pg_class with correct relkind
    let (relname, relkind): (String, String) = conn.query_row(
        "SELECT relname, relkind FROM pg_class WHERE relname = 'test_pg_class'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?))
    ).unwrap();
    
    assert_eq!(relname, "test_pg_class");
    assert_eq!(relkind, "r"); // regular table
}

#[test]
fn test_pg_class_contains_views() {
    let conn = setup_test_db();
    
    // Create a test view
    conn.execute("CREATE VIEW test_pg_class_view AS SELECT 1 as col", []).unwrap();
    
    let relkind: String = conn.query_row(
        "SELECT relkind FROM pg_class WHERE relname = 'test_pg_class_view'",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert_eq!(relkind, "v"); // view
}

#[test]
fn test_pg_class_contains_indexes() {
    let conn = setup_test_db();
    
    // Create a test table with an index
    conn.execute("CREATE TABLE test_pg_class_idx (id INTEGER PRIMARY KEY, name TEXT)", []).unwrap();
    conn.execute("CREATE INDEX idx_test ON test_pg_class_idx(name)", []).unwrap();
    
    let relkind: String = conn.query_row(
        "SELECT relkind FROM pg_class WHERE relname = 'idx_test'",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert_eq!(relkind, "i"); // index
}

#[test]
fn test_pg_attribute_has_all_columns() {
    let conn = setup_test_db();
    
    // Create a test table
    create_test_table(&conn, "CREATE TABLE test_pg_attr (id INTEGER PRIMARY KEY, email TEXT NOT NULL)");
    
    let columns = vec![
        "attrelid", "attname", "atttypid", "attstattarget", "attlen",
        "attnum", "attndims", "attcacheoff", "atttypmod", "attbyval",
        "attstorage", "attalign", "attnotnull", "atthasdef", "atthasmissing",
        "attidentity", "attgenerated", "attisdropped", "attislocal",
        "attinhcount", "attcollation", "attacl", "attoptions", "attfdwoptions", "attmissingval"
    ];
    
    for col in &columns {
        let result: Result<i64, _> = conn.query_row(
            &format!("SELECT COUNT({}) FROM pg_attribute WHERE attname = 'id'", col),
            [],
            |row| row.get(0)
        );
        assert!(result.is_ok(), "Column {} should exist in pg_attribute", col);
    }
}

#[test]
fn test_pg_attribute_column_metadata() {
    let conn = setup_test_db();
    
    // Create a test table with various column types
    create_test_table(&conn,
        "CREATE TABLE test_pg_attr_meta (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            age INTEGER,
            active BOOLEAN DEFAULT 1
        )"
    );
    
    // Query for columns
    let mut stmt = conn.prepare(
        "SELECT a.attname, t.typname, a.attnotnull, a.atthasdef
         FROM pg_attribute a
         JOIN pg_type t ON a.atttypid = t.oid
         JOIN pg_class c ON a.attrelid = c.oid
         WHERE c.relname = 'test_pg_attr_meta'
         ORDER BY a.attnum"
    ).unwrap();
    
    let rows: Vec<(String, String, bool, bool)> = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, bool>(2)?,
            row.get::<_, bool>(3)?,
        ))
    }).unwrap().filter_map(|r| r.ok()).collect();
    
    assert!(!rows.is_empty(), "Should have columns");
    
    // Check that id column exists
    let id_col = rows.iter().find(|(name, _, _, _)| name == "id");
    assert!(id_col.is_some(), "id column should exist");
    
    // Check that name column has not null
    let name_col = rows.iter().find(|(name, _, _, _)| name == "name");
    if let Some((_, _, notnull, _)) = name_col {
        assert!(*notnull, "name column should be NOT NULL");
    }
}

#[test]
fn test_pg_type_has_standard_types() {
    let conn = setup_test_db();
    
    let expected_types = vec![
        ("bool", "b"),
        ("bytea", "b"),
        ("char", "b"),
        ("int8", "b"),
        ("int2", "b"),
        ("int4", "b"),
        ("text", "b"),
        ("oid", "b"),
        ("json", "b"),
        ("point", "b"),
        ("lseg", "b"),
        ("path", "b"),
        ("box", "b"),
        ("polygon", "b"),
        ("line", "b"),
        ("float4", "b"),
        ("float8", "b"),
        ("circle", "b"),
        ("money", "b"),
        ("macaddr", "b"),
        ("inet", "b"),
        ("cidr", "b"),
        ("bpchar", "b"),
        ("varchar", "b"),
        ("date", "b"),
        ("time", "b"),
        ("timestamp", "b"),
        ("timestamptz", "b"),
        ("interval", "b"),
        ("timetz", "b"),
        ("bit", "b"),
        ("varbit", "b"),
        ("numeric", "b"),
        ("uuid", "b"),
        ("pg_lsn", "b"),
        ("tsvector", "b"),
        ("tsquery", "b"),
        ("jsonb", "b"),
    ];
    
    for (typname, expected_typtype) in &expected_types {
        let result: Result<(String, String), _> = conn.query_row(
            "SELECT typname, typtype FROM pg_type WHERE typname = ?1",
            [typname],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        );
        
        assert!(result.is_ok(), "Type {} should exist in pg_type", typname);
        let (_, typtype) = result.unwrap();
        assert_eq!(&typtype, *expected_typtype, "Type {} should have correct typtype", typname);
    }
}

#[test]
fn test_pg_namespace_exists() {
    let conn = setup_test_db();
    
    let schemas: Vec<String> = conn.prepare("SELECT nspname FROM pg_namespace ORDER BY nspname")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    
    assert!(schemas.contains(&"public".to_string()), "public schema should exist");
    assert!(schemas.contains(&"pg_catalog".to_string()), "pg_catalog schema should exist");
    assert!(schemas.contains(&"information_schema".to_string()), "information_schema should exist");
}

#[test]
fn test_pg_roles_exists() {
    let conn = setup_test_db();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_roles WHERE rolname = 'postgres'",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert!(count >= 1, "postgres role should exist");
}

#[test]
fn test_pg_database_exists() {
    let conn = setup_test_db();
    
    let datname: String = conn.query_row(
        "SELECT datname FROM pg_database LIMIT 1",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert_eq!(datname, "postgres");
}

#[test]
fn test_pg_settings_exists() {
    let conn = setup_test_db();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_settings",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert!(count > 0, "pg_settings should have entries");
}

#[test]
fn test_pg_tables_view() {
    let conn = setup_test_db();
    
    // Create a test table
    conn.execute("CREATE TABLE test_pg_tables (id INTEGER PRIMARY KEY)", []).unwrap();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_tables WHERE tablename = 'test_pg_tables'",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert_eq!(count, 1, "pg_tables should contain test_pg_tables");
}

#[test]
fn test_pg_views_view() {
    let conn = setup_test_db();
    
    // Create a test view
    conn.execute("CREATE VIEW test_pg_views AS SELECT 1 as col", []).unwrap();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_views WHERE viewname = 'test_pg_views'",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert_eq!(count, 1, "pg_views should contain test_pg_views");
}

#[test]
fn test_pg_indexes_view() {
    let conn = setup_test_db();
    
    // Create a test table with an index
    create_test_table(&conn, "CREATE TABLE test_pg_indexes (id INTEGER PRIMARY KEY, name TEXT)");
    conn.execute("CREATE INDEX idx_test_pg_indexes ON test_pg_indexes(name)", []).unwrap();
    populate_pg_index(&conn).ok();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_indexes WHERE tablename = 'test_pg_indexes'",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert!(count >= 1, "pg_indexes should contain test_pg_indexes");
}

#[test]
fn test_pg_proc_exists() {
    let conn = setup_test_db();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_proc WHERE proname IN ('now', 'current_timestamp')",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert!(count >= 1, "pg_proc should have common functions");
}

#[test]
fn test_pg_am_exists() {
    let conn = setup_test_db();
    
    let methods: Vec<String> = conn.prepare("SELECT amname FROM pg_am")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    
    assert!(methods.contains(&"btree".to_string()), "btree should exist");
    assert!(methods.contains(&"hash".to_string()), "hash should exist");
}

#[test]
fn test_pg_description_exists() {
    let conn = setup_test_db();
    
    // pg_description should exist even if empty
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM pg_description",
        [],
        |row| row.get(0)
    );
    
    assert!(result.is_ok(), "pg_description should be queryable");
}

#[test]
fn test_pg_extension_exists() {
    let conn = setup_test_db();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_extension",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert!(count > 0, "pg_extension should have entries");
}

#[test]
fn test_pg_enum_exists() {
    let conn = setup_test_db();
    
    // pg_enum should exist even if empty
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM pg_enum",
        [],
        |row| row.get(0)
    );
    
    assert!(result.is_ok(), "pg_enum should be queryable");
}

#[test]
fn test_pg_constraint_exists() {
    let conn = setup_test_db();
    
    // Create a test table with constraints
    conn.execute(
        "CREATE TABLE test_pg_constraint (
            id INTEGER PRIMARY KEY,
            email TEXT UNIQUE,
            ref_id INTEGER REFERENCES test_pg_constraint(id)
        )",
        []
    ).unwrap();
    
    // Query for constraints
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM pg_constraint WHERE conrelid = (
            SELECT oid FROM pg_class WHERE relname = 'test_pg_constraint'
        )",
        [],
        |row| row.get(0)
    );
    
    assert!(result.is_ok(), "pg_constraint should be queryable");
}

#[test]
fn test_pg_index_exists() {
    let conn = setup_test_db();
    
    // Create a test table with an index
    conn.execute("CREATE TABLE test_pg_index (id INTEGER PRIMARY KEY, name TEXT)", []).unwrap();
    conn.execute("CREATE INDEX idx_test_pg_index ON test_pg_index(name)", []).unwrap();
    
    // Query for indexes
    let result: Result<Vec<i64>, _> = conn.prepare(
        "SELECT indexrelid FROM pg_index WHERE indrelid = (
            SELECT oid FROM pg_class WHERE relname = 'test_pg_index'
        )"
    ).unwrap().query_map([], |row| row.get::<_, i64>(0)).map(|rows| rows.filter_map(|r| r.ok()).collect());
    
    assert!(result.is_ok(), "pg_index should be queryable");
}

#[test]
fn test_pg_attrdef_exists() {
    let conn = setup_test_db();
    
    // Create a test table with a default
    conn.execute(
        "CREATE TABLE test_pg_attrdef (id INTEGER PRIMARY KEY, created_at TEXT DEFAULT CURRENT_TIMESTAMP)",
        []
    ).unwrap();
    
    // Query for defaults
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM pg_attrdef WHERE adrelid = (
            SELECT oid FROM pg_class WHERE relname = 'test_pg_attrdef'
        )",
        [],
        |row| row.get(0)
    );
    
    assert!(result.is_ok(), "pg_attrdef should be queryable");
}

#[test]
fn test_orm_introspection_query() {
    let conn = setup_test_db();
    
    // Create test schema similar to what an ORM would use
    create_test_table(
        &conn,
        "CREATE TABLE test_orm_table (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT UNIQUE,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )"
    );
    
    // Query similar to what Prisma/TypeORM would do
    let mut stmt = conn.prepare(
        "SELECT 
            c.relname as table_name,
            a.attname as column_name,
            t.typname as data_type,
            a.attnotnull as is_nullable,
            a.attnum as ordinal_position
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
          AND a.attnum > 0
          AND NOT a.attisdropped
        ORDER BY c.relname, a.attnum"
    ).unwrap();
    
    let rows: Vec<(String, String, String, bool, i64)> = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, i64>(4)?,
        ))
    }).unwrap().filter_map(|r| r.ok()).collect();
    
    // Find our test table columns
    let test_cols: Vec<_> = rows.iter().filter(|(tbl, _, _, _, _)| tbl == "test_orm_table").collect();
    
    assert!(!test_cols.is_empty(), "Should find test_orm_table columns");
    
    let col_names: Vec<_> = test_cols.iter().map(|(_, name, _, _, _)| name.clone()).collect();
    assert!(col_names.contains(&"id".to_string()), "Should have id column");
    assert!(col_names.contains(&"name".to_string()), "Should have name column");
    assert!(col_names.contains(&"email".to_string()), "Should have email column");
    assert!(col_names.contains(&"created_at".to_string()), "Should have created_at column");
}

#[test]
fn test_pg_authid_exists() {
    let conn = setup_test_db();
    
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pg_authid WHERE rolname = 'postgres'",
        [],
        |row| row.get(0)
    ).unwrap();
    
    assert!(count >= 1, "pg_authid should have postgres role");
}

#[test]
fn test_pg_auth_members_exists() {
    let conn = setup_test_db();
    
    // pg_auth_members should exist
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM pg_auth_members",
        [],
        |row| row.get(0)
    );
    
    assert!(result.is_ok(), "pg_auth_members should be queryable");
}
