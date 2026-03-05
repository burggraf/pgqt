//! Table and column metadata storage and retrieval
//!
//! This module provides functions for storing and retrieving table/column metadata
//! in the `__pg_meta__` shadow table, as well as populating the PostgreSQL-compatible
//! system catalog views (`pg_attribute`, `pg_index`, `pg_constraint`).
//!
//! ## Key Functions
//! - [`store_column_metadata`] — Store a single column's type metadata
//! - [`store_table_metadata`] — Store metadata for all columns in a table
//! - [`get_table_metadata`] / [`get_column_metadata`] — Retrieve stored metadata
//! - [`populate_pg_attribute`] — Sync table schema to `pg_attribute` view
//! - [`populate_pg_index`] / [`populate_pg_constraint`] — Populate index/constraint views

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::ColumnMetadata;

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

/// Store relation ownership metadata
pub fn store_relation_metadata(
    conn: &Connection,
    table_name: &str,
    owner_oid: i64,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_relation_meta__ (relname, relowner) VALUES (?1, ?2)",
        (table_name, owner_oid),
    )
    .context("Failed to store relation metadata")?;
    Ok(())
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
/// Delete all metadata for a table (e.g., when table is dropped)
pub fn delete_table_metadata(conn: &Connection, table_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM __pg_meta__ WHERE table_name = ?1",
        [table_name],
    )
    .context("Failed to delete table metadata")?;

    Ok(())
}

/// Get column metadata including default expressions for a table
/// 
/// Returns column information in the order they appear in the table,
/// including default expressions from the catalog.
pub fn get_table_columns_with_defaults(conn: &Connection, table_name: &str) -> Result<Vec<ColumnMetadata>> {
    let mut stmt = conn.prepare(
        "SELECT table_name, column_name, original_type, constraints
         FROM __pg_meta__
         WHERE table_name = ?1
         ORDER BY rowid"
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
    
    // If no metadata in catalog, fall back to pragma_table_info
    if result.is_empty() {
        let mut pragma_stmt = conn.prepare(
            "SELECT name, type, cid, dflt_value FROM pragma_table_info(?1) ORDER BY cid"
        )?;
        
        let pragma_rows = pragma_stmt.query_map([table_name], |row| {
            let col_name: String = row.get(0)?;
            let col_type: String = row.get(1)?;
            let dflt_value: Option<String> = row.get(3)?;
            
            Ok(ColumnMetadata {
                table_name: table_name.to_string(),
                column_name: col_name,
                original_type: col_type,
                constraints: dflt_value.map(|d| format!("DEFAULT {}", d)),
            })
        })?;
        
        for row in pragma_rows {
            result.push(row?);
        }
    }

    Ok(result)
}

/// Extract default expression from constraints string
/// 
/// Parses a constraints string like "NOT NULL DEFAULT 5" and extracts "5"
pub fn extract_default_from_constraints(constraints: &str) -> Option<String> {
    let upper = constraints.to_uppercase();
    if let Some(idx) = upper.find("DEFAULT") {
        let after_default = &constraints[idx + 7..].trim();
        // Take everything until the next constraint keyword
        let end_idx = after_default
            .find([',', '(', ')'])
            .unwrap_or(after_default.len());
        let default_expr = after_default[..end_idx].trim();
        if !default_expr.is_empty() {
            return Some(default_expr.to_string());
        }
    }
    None
}

/// Populate __pg_attribute__ for a given table from sqlite metadata
pub fn populate_pg_attribute(conn: &Connection, table_name: &str) -> Result<()> {
    
    let oid_result: Result<i64, _> = conn.query_row(
        "SELECT oid FROM pg_class WHERE relname = ?1",
        [table_name],
        |row| row.get(0)
    );
    
    let oid = match oid_result {
        Ok(o) => o,
        Err(_) => return Ok(()), 
    };
    
    
    conn.execute(
        "DELETE FROM __pg_attribute__ WHERE attrelid = ?1",
        [oid],
    )?;
    
    
    let mut stmt = conn.prepare(
        "SELECT name, type, cid, \"notnull\", dflt_value 
         FROM pragma_table_info(?1)"
    )?;
    
    let rows = stmt.query_map([table_name], |row| {
        Ok((
            row.get::<_, String>(0)?,  
            row.get::<_, String>(1)?,  
            row.get::<_, i64>(2)?,     
            row.get::<_, bool>(3)?,    
            row.get::<_, Option<String>>(4)?,  
        ))
    })?;
    
    for row in rows {
        let (col_name, col_type, cid, notnull, dflt) = row?;
        
        
        let typid = match col_type.to_lowercase().as_str() {
            t if t.contains("int") => 23,      
            t if t.contains("real") => 700,    
            t if t.contains("float") => 701,   
            t if t.contains("bool") => 16,     
            t if t.contains("blob") => 17,     
            _ => 25,                           
        };
        
        conn.execute(
            "INSERT INTO __pg_attribute__ 
             (attrelid, attname, atttypid, attnum, attnotnull, atthasdef)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (oid, col_name, typid, cid + 1, notnull, dflt.is_some()),
        )?;
    }
    
    Ok(())
}

/// Populate __pg_index__ from sqlite_master
pub fn populate_pg_index(conn: &Connection) -> Result<()> {
    
    conn.execute("DELETE FROM __pg_index__", [])?;
    
    let mut stmt = conn.prepare(
        "SELECT sm.rowid, sm.name, sm.sql, sm.tbl_name 
         FROM sqlite_master sm 
         WHERE sm.type = 'index' 
         AND sm.name NOT LIKE 'sqlite_%' 
         AND sm.name NOT LIKE '__pg_%'"
    )?;
    
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,  
            row.get::<_, String>(1)?,  
            row.get::<_, Option<String>>(2)?,  
            row.get::<_, String>(3)?,  
        ))
    })?;
    
    for row in rows {
        let (indexrelid, _name, _sql, tbl_name) = row?;
        
        
        let table_oid: Option<i64> = conn.query_row(
            "SELECT oid FROM pg_class WHERE relname = ?1",
            [&tbl_name],
            |row| row.get(0)
        ).ok();
        
        if let Some(indrelid) = table_oid {
            
            let is_unique = _sql.as_ref().map(|s| s.to_uppercase().contains("UNIQUE")).unwrap_or(false);
            let is_primary = _name.starts_with("sqlite_autoindex") || 
                            _sql.as_ref().map(|s| s.to_uppercase().contains("PRIMARY")).unwrap_or(false);
            
            conn.execute(
                "INSERT INTO __pg_index__ 
                 (indexrelid, indrelid, indisunique, indisprimary)
                 VALUES (?1, ?2, ?3, ?4)",
                (indexrelid, indrelid, is_unique, is_primary),
            )?;
        }
    }
    
    Ok(())
}

/// Populate __pg_constraint__ from SQLite constraints
pub fn populate_pg_constraint(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM __pg_constraint__", [])?;
    
    
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '__pg_%'"
    )?;
    
    let tables: Vec<String> = stmt.query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    
    let mut oid_counter: i64 = 10000;
    
    for table in &tables {
        
        let table_oid: i64 = conn.query_row(
            "SELECT oid FROM pg_class WHERE relname = ?1",
            [table],
            |row| row.get(0)
        ).unwrap_or(0);
        
        if table_oid == 0 {
            continue;
        }
        
        
        let mut pk_stmt = conn.prepare(
            "SELECT name, cid FROM pragma_table_info(?1) WHERE pk > 0 ORDER BY pk"
        )?;
        
        let pk_cols: Vec<(String, i64)> = pk_stmt.query_map([table], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?.filter_map(|r| r.ok()).collect();
        
        if !pk_cols.is_empty() {
            let pk_name = format!("{}_pkey", table);
            let pk_key = pk_cols.iter().map(|(_, cid)| (cid + 1).to_string()).collect::<Vec<_>>().join(" ");
            
            conn.execute(
                "INSERT INTO __pg_constraint__ 
                 (oid, conname, contype, conrelid, conkey)
                 VALUES (?1, ?2, 'p', ?3, ?4)",
                (oid_counter, &pk_name, table_oid, pk_key),
            )?;
            oid_counter += 1;
        }
        
        
        let mut fk_stmt = conn.prepare("SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete, match FROM pragma_foreign_key_list(?1)")?;
        let fk_rows = fk_stmt.query_map([table], |row| {
            Ok((
                row.get::<_, i64>(0)?,  
                row.get::<_, String>(1)?,  
                row.get::<_, String>(2)?,  
                row.get::<_, String>(3)?,  
                row.get::<_, String>(4)?,  
                row.get::<_, String>(5)?,  
                row.get::<_, String>(6)?,  
                row.get::<_, String>(7)?,  
            ))
        })?;
        
        for fk in fk_rows.filter_map(|r| r.ok()) {
            let (_, _, ref fk_table, ref fk_from, _, _, _, _) = fk;
            let fk_name = format!("{}_{}_fkey", table, fk_from);
            
            
            let from_cid: i64 = conn.query_row(
                "SELECT cid FROM pragma_table_info(?1) WHERE name = ?2",
                [table.clone(), fk_from.clone()],
                |row| row.get(0)
            ).unwrap_or(0);
            
            conn.execute(
                "INSERT INTO __pg_constraint__ 
                 (oid, conname, contype, conrelid, confrelid, conkey, confkey)
                 VALUES (?1, ?2, 'f', ?3, 
                    (SELECT oid FROM pg_class WHERE relname = ?4), ?5, ?6)",
                (oid_counter, &fk_name, table_oid, fk_table.clone(), from_cid + 1, "1"),
            )?;
            oid_counter += 1;
        }
    }
    
    Ok(())
}
