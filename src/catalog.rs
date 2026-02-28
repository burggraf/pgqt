//! Shadow Catalog (`__pg_meta__`) for storing PostgreSQL metadata in SQLite
//!
//! This module manages a hidden system table that stores the original PostgreSQL
//! type information, allowing for reversible migrations back to PostgreSQL.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Represents column metadata for a table
#[derive(Debug, Clone)]
pub struct ColumnMetadata {
    pub table_name: String,
    pub column_name: String,
    pub original_type: String,
    pub constraints: Option<String>,
}

/// Ensures the `__pg_meta__` shadow catalog table exists
pub fn init_catalog(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_meta__ (
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            original_type TEXT NOT NULL,
            constraints TEXT,
            PRIMARY KEY (table_name, column_name)
        )",
        [],
    )
    .context("Failed to create __pg_meta__ table")?;

    // Create an index for faster lookups by table name
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_pg_meta_table ON __pg_meta__(table_name)",
        [],
    )
    .context("Failed to create index on __pg_meta__")?;

    Ok(())
}

/// Store column metadata in the shadow catalog
pub fn store_column_metadata(conn: &Connection, metadata: &ColumnMetadata) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_meta__ (table_name, column_name, original_type, constraints)
         VALUES (?1, ?2, ?3, ?4)",
        (
            &metadata.table_name,
            &metadata.column_name,
            &metadata.original_type,
            &metadata.constraints,
        ),
    )
    .context("Failed to store column metadata")?;

    Ok(())
}

/// Store multiple column metadata entries for a table
pub fn store_table_metadata(
    conn: &Connection,
    table_name: &str,
    columns: &[(String, String, Option<String>)],
) -> Result<()> {
    for (col_name, orig_type, constraints) in columns {
        let metadata = ColumnMetadata {
            table_name: table_name.to_string(),
            column_name: col_name.clone(),
            original_type: orig_type.clone(),
            constraints: constraints.clone(),
        };
        store_column_metadata(conn, &metadata)?;
    }
    Ok(())
}

/// Retrieve all column metadata for a specific table
pub fn get_table_metadata(conn: &Connection, table_name: &str) -> Result<Vec<ColumnMetadata>> {
    let mut stmt = conn.prepare(
        "SELECT table_name, column_name, original_type, constraints
         FROM __pg_meta__
         WHERE table_name = ?1
         ORDER BY column_name"
    )?;

    let rows = stmt.query_map([table_name], |row| {
        Ok(ColumnMetadata {
            table_name: row.get(0)?,
            column_name: row.get(1)?,
            original_type: row.get(2)?,
            constraints: row.get(3)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }

    Ok(result)
}

/// Retrieve metadata for a specific column
pub fn get_column_metadata(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<Option<ColumnMetadata>> {
    let result = conn.query_row(
        "SELECT table_name, column_name, original_type, constraints
         FROM __pg_meta__
         WHERE table_name = ?1 AND column_name = ?2",
        [table_name, column_name],
        |row| {
            Ok(ColumnMetadata {
                table_name: row.get(0)?,
                column_name: row.get(1)?,
                original_type: row.get(2)?,
                constraints: row.get(3)?,
            })
        },
    );

    match result {
        Ok(metadata) => Ok(Some(metadata)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete all metadata for a table (e.g., when table is dropped)
pub fn delete_table_metadata(conn: &Connection, table_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM __pg_meta__ WHERE table_name = ?1",
        [table_name],
    )
    .context("Failed to delete table metadata")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_catalog(&conn).unwrap();
        conn
    }

    #[test]
    fn test_init_catalog_creates_table() {
        let conn = setup_test_db();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = '__pg_meta__'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_store_and_retrieve_column_metadata() {
        let conn = setup_test_db();

        let metadata = ColumnMetadata {
            table_name: "test_table".to_string(),
            column_name: "name".to_string(),
            original_type: "VARCHAR(10)".to_string(),
            constraints: Some("NOT NULL".to_string()),
        };

        store_column_metadata(&conn, &metadata).unwrap();

        let retrieved =
            get_column_metadata(&conn, "test_table", "name")
                .unwrap()
                .expect("Should find metadata");

        assert_eq!(retrieved.table_name, "test_table");
        assert_eq!(retrieved.column_name, "name");
        assert_eq!(retrieved.original_type, "VARCHAR(10)");
        assert_eq!(retrieved.constraints, Some("NOT NULL".to_string()));
    }

    #[test]
    fn test_store_table_metadata() {
        let conn = setup_test_db();

        let columns = vec![
            ("id".to_string(), "SERIAL".to_string(), None),
            (
                "name".to_string(),
                "VARCHAR(10)".to_string(),
                Some("NOT NULL".to_string()),
            ),
            (
                "created_at".to_string(),
                "TIMESTAMP WITH TIME ZONE".to_string(),
                None,
            ),
        ];

        store_table_metadata(&conn, "test_table", &columns).unwrap();

        let metadata = get_table_metadata(&conn, "test_table").unwrap();
        assert_eq!(metadata.len(), 3);

        let types: Vec<String> = metadata
            .iter()
            .map(|m| m.original_type.clone())
            .collect();
        assert!(types.contains(&"SERIAL".to_string()));
        assert!(types.contains(&"VARCHAR(10)".to_string()));
        assert!(types.contains(&"TIMESTAMP WITH TIME ZONE".to_string()));
    }

    #[test]
    fn test_get_nonexistent_column() {
        let conn = setup_test_db();
        let result = get_column_metadata(&conn, "nonexistent", "col").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_table_metadata() {
        let conn = setup_test_db();

        let metadata = ColumnMetadata {
            table_name: "test_table".to_string(),
            column_name: "name".to_string(),
            original_type: "VARCHAR(10)".to_string(),
            constraints: None,
        };

        store_column_metadata(&conn, &metadata).unwrap();
        delete_table_metadata(&conn, "test_table").unwrap();

        let result = get_table_metadata(&conn, "test_table").unwrap();
        assert!(result.is_empty());
    }
}
