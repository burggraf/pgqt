//! Schema (Namespace) Support for PostgreSQL Compatibility
//!
//! This module implements PostgreSQL schema/namespace support using SQLite's
//! ATTACH DATABASE feature. Each PostgreSQL schema (except 'public') maps to
//! a separate SQLite database file.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Schema metadata stored in __pg_namespace__
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SchemaMetadata {
    pub oid: i64,
    pub nspname: String,
    pub nspowner: i64,
    pub nspacl: Option<String>,
}

/// Manages schema-to-database-file mappings and ATTACH state
#[derive(Debug, Clone)]
pub struct SchemaManager {
    /// Path to the main database file
    main_db_path: PathBuf,
    /// Set of currently attached schemas (schema_name)
    attached_schemas: Arc<Mutex<HashSet<String>>>,
}

/// Search path for schema resolution
#[derive(Debug, Clone)]
pub struct SearchPath {
    pub schemas: Vec<String>,
}

impl Default for SearchPath {
    fn default() -> Self {
        SearchPath {
            schemas: vec!["$user".to_string(), "public".to_string()],
        }
    }
}

impl SearchPath {
    /// Parse a search_path string (e.g., "schema1, public, $user")
    pub fn parse(path: &str) -> Result<Self> {
        let schemas: Vec<String> = path
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        if schemas.is_empty() {
            return Ok(Self::default());
        }

        Ok(SearchPath { schemas })
    }

    /// Convert search_path to string representation
    pub fn to_string(&self) -> String {
        self.schemas
            .iter()
            .map(|s| {
                if s.contains('$') || s.contains('-') {
                    format!("\"{}\"", s)
                } else {
                    s.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Get the first schema in the path (the "current" schema)
    #[allow(dead_code)]
    pub fn first(&self) -> Option<&str> {
        // Skip $user if it doesn't resolve to an actual schema
        for schema in &self.schemas {
            if schema != "$user" {
                return Some(schema);
            }
        }
        Some("public")
    }
}

impl SchemaManager {
    /// Create a new SchemaManager for the given main database path
    pub fn new(main_db_path: &Path) -> Self {
        SchemaManager {
            main_db_path: main_db_path.to_path_buf(),
            attached_schemas: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Get the database file path for a schema
    pub fn schema_db_path(&self, schema_name: &str) -> PathBuf {
        if schema_name == "public" || schema_name.is_empty() {
            return self.main_db_path.clone();
        }

        let parent = self.main_db_path.parent().unwrap_or(Path::new("."));
        let stem = self
            .main_db_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("database");

        parent.join(format!("{}_{}.db", stem, schema_name))
    }

    /// Check if a schema is currently attached
    pub fn is_attached(&self, schema_name: &str) -> bool {
        let attached = self.attached_schemas.lock().unwrap();
        attached.contains(schema_name)
    }

    /// Attach a schema's database file
    pub fn attach_schema(&self, conn: &Connection, schema_name: &str) -> Result<()> {
        if schema_name == "public" || schema_name.is_empty() {
            return Ok(()); // public is always the main database
        }

        // Check if already attached
        {
            let attached = self.attached_schemas.lock().unwrap();
            if attached.contains(schema_name) {
                return Ok(());
            }
        }

        let db_path = self.schema_db_path(schema_name);

        // Create the database file if it doesn't exist
        if !db_path.exists() {
            // Create by opening and closing
            let _ = Connection::open(&db_path)
                .with_context(|| format!("Failed to create schema database: {:?}", db_path))?;
        }

        // Attach the database
        let sql = format!(
            "ATTACH DATABASE '{}' AS {}",
            db_path.display(),
            schema_name
        );
        conn.execute(&sql, [])
            .with_context(|| format!("Failed to attach schema: {}", schema_name))?;

        // Mark as attached
        {
            let mut attached = self.attached_schemas.lock().unwrap();
            attached.insert(schema_name.to_lowercase());
        }

        Ok(())
    }

    /// Detach a schema's database file
    pub fn detach_schema(&self, conn: &Connection, schema_name: &str) -> Result<()> {
        if schema_name == "public" || schema_name.is_empty() {
            return Ok(()); // Can't detach public
        }

        let sql = format!("DETACH DATABASE {}", schema_name);
        conn.execute(&sql, [])
            .with_context(|| format!("Failed to detach schema: {}", schema_name))?;

        // Mark as detached
        {
            let mut attached = self.attached_schemas.lock().unwrap();
            attached.remove(schema_name);
        }

        Ok(())
    }

    /// Delete a schema's database file
    pub fn delete_schema_db(&self, schema_name: &str) -> Result<()> {
        if schema_name == "public" || schema_name.is_empty() {
            return Err(anyhow::anyhow!("Cannot delete the public schema database"));
        }

        let db_path = self.schema_db_path(schema_name);
        if db_path.exists() {
            std::fs::remove_file(&db_path)
                .with_context(|| format!("Failed to delete schema database: {:?}", db_path))?;
        }

        Ok(())
    }

    /// Attach all existing schemas (called on startup)
    #[allow(dead_code)]
    pub fn attach_all_schemas(&self, conn: &Connection) -> Result<()> {
        let schemas = list_schemas(conn)?;
        for schema in schemas {
            if schema.nspname != "public"
                && schema.nspname != "pg_catalog"
                && schema.nspname != "information_schema"
            {
                self.attach_schema(conn, &schema.nspname)?;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Schema Catalog Functions
// ============================================================================

/// Initialize the schema catalog table
#[allow(dead_code)]
pub fn init_schema_catalog(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_namespace__ (
            oid INTEGER PRIMARY KEY AUTOINCREMENT,
            nspname TEXT UNIQUE NOT NULL,
            nspowner INTEGER NOT NULL DEFAULT 10,
            nspacl TEXT
        )",
        [],
    )
    .context("Failed to create __pg_namespace__ table")?;

    // Insert default schemas if they don't exist
    let default_schemas = [
        ("public", 10),
        ("pg_catalog", 10),
        ("information_schema", 10),
    ];

    for (name, owner) in default_schemas {
        conn.execute(
            "INSERT OR IGNORE INTO __pg_namespace__ (nspname, nspowner) VALUES (?, ?)",
            [name, &owner.to_string()],
        )
        .ok();
    }

    Ok(())
}

/// Create a new schema
pub fn create_schema(conn: &Connection, name: &str, owner_oid: Option<i64>) -> Result<i64> {
    let owner = owner_oid.unwrap_or(10); // Default to postgres (oid 10)

    // Check if schema already exists
    if schema_exists(conn, name)? {
        return Err(anyhow::anyhow!("schema \"{}\" already exists", name));
    }

    // Validate schema name
    if name.starts_with("pg_") {
        return Err(anyhow::anyhow!(
            "unacceptable schema name \"{}\": system schemas must start with pg_",
            name
        ));
    }

    conn.execute(
        "INSERT INTO __pg_namespace__ (nspname, nspowner) VALUES (?, ?)",
        [name.to_lowercase(), owner.to_string()],
    )
    .context("Failed to create schema")?;

    // Get the OID of the new schema
    let oid: i64 = conn.query_row(
        "SELECT oid FROM __pg_namespace__ WHERE nspname = ?",
        [name.to_lowercase()],
        |row| row.get(0),
    )?;

    Ok(oid)
}

/// Drop a schema
pub fn drop_schema(conn: &Connection, name: &str) -> Result<()> {
    if name == "public" {
        return Err(anyhow::anyhow!("cannot drop schema \"public\""));
    }
    if name == "pg_catalog" || name == "information_schema" {
        return Err(anyhow::anyhow!("cannot drop system schema \"{}\"", name));
    }

    let rows_affected = conn
        .execute(
            "DELETE FROM __pg_namespace__ WHERE nspname = ?",
            [name.to_lowercase()],
        )
        .context("Failed to drop schema")?;

    if rows_affected == 0 {
        return Err(anyhow::anyhow!("schema \"{}\" does not exist", name));
    }

    Ok(())
}

/// Check if a schema exists
pub fn schema_exists(conn: &Connection, name: &str) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM __pg_namespace__ WHERE nspname = ?)",
        [name.to_lowercase()],
        |row| row.get(0),
    )?;
    Ok(exists)
}

/// Get schema OID
#[allow(dead_code)]
pub fn get_schema_oid(conn: &Connection, name: &str) -> Result<Option<i64>> {
    let result: Result<i64, _> = conn.query_row(
        "SELECT oid FROM __pg_namespace__ WHERE nspname = ?",
        [name.to_lowercase()],
        |row| row.get(0),
    );
    Ok(result.ok())
}

/// Get schema owner OID
#[allow(dead_code)]
pub fn get_schema_owner(conn: &Connection, name: &str) -> Result<Option<i64>> {
    let result: Result<i64, _> = conn.query_row(
        "SELECT nspowner FROM __pg_namespace__ WHERE nspname = ?",
        [name.to_lowercase()],
        |row| row.get(0),
    );
    Ok(result.ok())
}

/// List all schemas
#[allow(dead_code)]
pub fn list_schemas(conn: &Connection) -> Result<Vec<SchemaMetadata>> {
    let mut stmt = conn.prepare(
        "SELECT oid, nspname, nspowner, nspacl FROM __pg_namespace__ ORDER BY oid",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(SchemaMetadata {
            oid: row.get(0)?,
            nspname: row.get(1)?,
            nspowner: row.get(2)?,
            nspacl: row.get(3)?,
        })
    })?;

    let mut schemas = Vec::new();
    for row in rows {
        schemas.push(row?);
    }

    Ok(schemas)
}

/// Check if a schema contains any objects
pub fn schema_is_empty(conn: &Connection, schema_name: &str, schema_manager: &SchemaManager) -> Result<bool> {
    // For public schema, check sqlite_master
    if schema_name == "public" {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'view', 'index', 'trigger') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '__pg_%'",
            [],
            |row| row.get(0),
        )?;
        return Ok(count == 0);
    }

    // For other schemas, check the attached database
    // First ensure the schema is attached
    if !schema_manager.is_attached(schema_name) {
        schema_manager.attach_schema(conn, schema_name)?;
    }

    let sql = format!(
        "SELECT COUNT(*) FROM {}.sqlite_master WHERE type IN ('table', 'view', 'index', 'trigger') AND name NOT LIKE 'sqlite_%'",
        schema_name
    );
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count == 0)
}

/// Drop all objects in a schema
pub fn drop_schema_objects(conn: &Connection, schema_name: &str, schema_manager: &SchemaManager) -> Result<()> {
    // Ensure schema is attached
    if !schema_manager.is_attached(schema_name) {
        schema_manager.attach_schema(conn, schema_name)?;
    }

    // Get list of tables
    let sql = format!(
        "SELECT name FROM {}.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        schema_name
    );
    let mut stmt = conn.prepare(&sql)?;
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Drop each table (CASCADE is automatic in SQLite)
    for table in tables {
        let drop_sql = format!("DROP TABLE IF EXISTS {}.{}", schema_name, table);
        conn.execute(&drop_sql, [])?;
    }

    // Get and drop views
    let sql = format!(
        "SELECT name FROM {}.sqlite_master WHERE type = 'view' AND name NOT LIKE 'sqlite_%'",
        schema_name
    );
    let mut stmt = conn.prepare(&sql)?;
    let views: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for view in views {
        let drop_sql = format!("DROP VIEW IF EXISTS {}.{}", schema_name, view);
        conn.execute(&drop_sql, [])?;
    }

    // Get and drop indexes
    let sql = format!(
        "SELECT name FROM {}.sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
        schema_name
    );
    let mut stmt = conn.prepare(&sql)?;
    let indexes: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for index in indexes {
        let drop_sql = format!("DROP INDEX IF EXISTS {}.{}", schema_name, index);
        conn.execute(&drop_sql, [])?;
    }

    // Get and drop triggers
    let sql = format!(
        "SELECT name FROM {}.sqlite_master WHERE type = 'trigger' AND name NOT LIKE 'sqlite_%'",
        schema_name
    );
    let mut stmt = conn.prepare(&sql)?;
    let triggers: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for trigger in triggers {
        let drop_sql = format!("DROP TRIGGER IF EXISTS {}.{}", schema_name, trigger);
        conn.execute(&drop_sql, [])?;
    }

    Ok(())
}

// ============================================================================
// Schema Privileges
// ============================================================================

/// Grant a privilege on a schema
#[allow(dead_code)]
pub fn grant_schema_privilege(
    conn: &Connection,
    schema_name: &str,
    privilege: &str,
    grantee_oid: i64,
) -> Result<()> {
    let schema_oid = get_schema_oid(conn, schema_name)?
        .ok_or_else(|| anyhow::anyhow!("schema \"{}\" does not exist", schema_name))?;

    conn.execute(
        "INSERT OR REPLACE INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantable, grantor_id)
         VALUES (?, 'schema', ?, ?, 0, 10)",
        rusqlite::params![schema_oid, grantee_oid, privilege.to_uppercase()],
    )?;

    Ok(())
}

/// Revoke a privilege on a schema
#[allow(dead_code)]
pub fn revoke_schema_privilege(
    conn: &Connection,
    schema_name: &str,
    privilege: &str,
    grantee_oid: i64,
) -> Result<()> {
    let schema_oid = get_schema_oid(conn, schema_name)?
        .ok_or_else(|| anyhow::anyhow!("schema \"{}\" does not exist", schema_name))?;

    conn.execute(
        "DELETE FROM __pg_acl__ WHERE object_id = ? AND object_type = 'schema' AND grantee_id = ? AND privilege = ?",
        rusqlite::params![schema_oid, grantee_oid, privilege.to_uppercase()],
    )?;

    Ok(())
}

/// Check if a user has a privilege on a schema
#[allow(dead_code)]
pub fn check_schema_privilege(
    conn: &Connection,
    schema_name: &str,
    privilege: &str,
    user_oid: i64,
) -> Result<bool> {
    // Superusers have all privileges
    let is_superuser: bool = conn.query_row(
        "SELECT rolsuper FROM __pg_authid__ WHERE oid = ?",
        [user_oid],
        |row| row.get(0),
    )?;
    if is_superuser {
        return Ok(true);
    }

    // Schema owner has all privileges
    let schema_owner = get_schema_owner(conn, schema_name)?;
    if schema_owner == Some(user_oid) {
        return Ok(true);
    }

    let schema_oid = match get_schema_oid(conn, schema_name)? {
        Some(oid) => oid,
        None => return Ok(false),
    };

    // Check explicit grant
    let has_privilege: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM __pg_acl__
            WHERE object_id = ? AND object_type = 'schema'
            AND grantee_id IN (?, 0)  -- 0 = PUBLIC
            AND (privilege = ? OR privilege = 'ALL')
        )",
        rusqlite::params![schema_oid, user_oid, privilege.to_uppercase()],
        |row| row.get(0),
    )?;

    Ok(has_privilege)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::path::Path;

    #[test]
    fn test_search_path_parse() {
        let path = SearchPath::parse("schema1, public").unwrap();
        assert_eq!(path.schemas, vec!["schema1", "public"]);

        let path = SearchPath::parse("\"$user\", public").unwrap();
        assert_eq!(path.schemas, vec!["$user", "public"]);

        let path = SearchPath::parse("").unwrap();
        assert_eq!(path.schemas, vec!["$user", "public"]); // default
    }

    #[test]
    fn test_search_path_first() {
        let path = SearchPath::parse("schema1, public").unwrap();
        assert_eq!(path.first(), Some("schema1"));

        let path = SearchPath::parse("$user, public").unwrap();
        assert_eq!(path.first(), Some("public")); // skips $user
    }

    #[test]
    fn test_schema_manager_path() {
        let manager = SchemaManager::new(Path::new("/data/myapp.db"));
        assert_eq!(manager.schema_db_path("public"), PathBuf::from("/data/myapp.db"));
        assert_eq!(manager.schema_db_path("inventory"), PathBuf::from("/data/myapp_inventory.db"));
    }

    #[test]
    fn test_create_and_drop_schema() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema_catalog(&conn).unwrap();

        // Create schema
        let oid = create_schema(&conn, "test_schema", Some(10)).unwrap();
        assert!(oid > 0);

        // Check exists
        assert!(schema_exists(&conn, "test_schema").unwrap());
        assert!(!schema_exists(&conn, "nonexistent").unwrap());

        // Cannot create duplicate
        assert!(create_schema(&conn, "test_schema", Some(10)).is_err());

        // Drop schema
        drop_schema(&conn, "test_schema").unwrap();
        assert!(!schema_exists(&conn, "test_schema").unwrap());
    }

    #[test]
    fn test_cannot_drop_system_schemas() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema_catalog(&conn).unwrap();

        assert!(drop_schema(&conn, "public").is_err());
        assert!(drop_schema(&conn, "pg_catalog").is_err());
        assert!(drop_schema(&conn, "information_schema").is_err());
    }

    #[test]
    fn test_list_schemas() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema_catalog(&conn).unwrap();

        create_schema(&conn, "alpha", Some(10)).unwrap();
        create_schema(&conn, "beta", Some(10)).unwrap();

        let schemas = list_schemas(&conn).unwrap();
        let names: Vec<&str> = schemas.iter().map(|s| s.nspname.as_str()).collect();

        assert!(names.contains(&"public"));
        assert!(names.contains(&"pg_catalog"));
        assert!(names.contains(&"information_schema"));
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }
}
