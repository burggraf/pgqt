//! User-defined function (UDF) metadata storage and retrieval
//!
//! This module handles persistence of function definitions in the `__pg_functions__`
//! shadow table. Functions created via `CREATE FUNCTION` are serialized here so they
//! survive across connections and can be retrieved for execution.
//!
//! ## Key Functions
//! - [`store_function`] — Persist a function definition to the catalog
//! - [`get_function`] — Look up a function by name (with optional schema)
//! - [`drop_function`] — Remove a function from the catalog

use anyhow::Result;
use rusqlite::Connection;
use serde_json;

use super::{FunctionMetadata, ParamMode, ReturnTypeKind};

/// Store a function definition in the catalog
pub fn store_function(conn: &Connection, metadata: &FunctionMetadata) -> Result<i64> {
    let arg_types_json = serde_json::to_string(&metadata.arg_types)?;
    let arg_names_json = serde_json::to_string(&metadata.arg_names)?;
    let arg_modes_json = serde_json::to_string(&metadata.arg_modes)?;
    let return_table_cols_json = match &metadata.return_table_cols {
        Some(cols) => serde_json::to_string(cols)?,
        None => "null".to_string(),
    };

    conn.execute(
        "INSERT INTO __pg_functions__ 
         (funcname, schema_name, arg_types, arg_names, arg_modes, 
          return_type, return_type_kind, return_table_cols,
          function_body, language, volatility, strict, security_definer, parallel, owner_oid)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (
            &metadata.name,
            &metadata.schema,
            &arg_types_json,
            &arg_names_json,
            &arg_modes_json,
            &metadata.return_type,
            &format!("{:?}", metadata.return_type_kind),
            &return_table_cols_json,
            &metadata.function_body,
            &metadata.language,
            &metadata.volatility,
            metadata.strict,
            metadata.security_definer,
            &metadata.parallel,
            &metadata.owner_oid,
        ),
    )?;

    
    let oid: i64 = conn.query_row(
        "SELECT last_insert_rowid()",
        [],
        |row| row.get(0),
    )?;

    Ok(oid)
}

/// Retrieve function metadata by name
pub fn get_function(
    conn: &Connection,
    name: &str,
    arg_types: Option<&[String]>
) -> Result<Option<FunctionMetadata>> {
    let query = if arg_types.is_some() {
        "SELECT * FROM __pg_functions__ WHERE funcname = ? AND arg_types = ? ORDER BY oid DESC LIMIT 1"
    } else {
        "SELECT * FROM __pg_functions__ WHERE funcname = ? ORDER BY oid DESC LIMIT 1"
    };

    let arg_types_json = arg_types.map(|types| serde_json::to_string(types).unwrap());

    let mut stmt = conn.prepare(query)?;
    
    let row_result = if let Some(json) = &arg_types_json {
        stmt.query_row([name, json], |row| {
            Ok((
                row.get::<_, i64>(0)?,        
                row.get::<_, String>(1)?,     
                row.get::<_, String>(2)?,     
                row.get::<_, String>(3)?,     
                row.get::<_, String>(4)?,     
                row.get::<_, String>(5)?,     
                row.get::<_, String>(6)?,     
                row.get::<_, String>(7)?,     
                row.get::<_, Option<String>>(8)?, 
                row.get::<_, String>(9)?,     
                row.get::<_, String>(10)?,    
                row.get::<_, String>(11)?,    
                row.get::<_, bool>(12)?,      
                row.get::<_, bool>(13)?,      
                row.get::<_, String>(14)?,    
                row.get::<_, i64>(15)?,       
                row.get::<_, Option<String>>(16)?, 
            ))
        })
    } else {
        stmt.query_row([name], |row| {
            Ok((
                row.get::<_, i64>(0)?,        
                row.get::<_, String>(1)?,     
                row.get::<_, String>(2)?,     
                row.get::<_, String>(3)?,     
                row.get::<_, String>(4)?,     
                row.get::<_, String>(5)?,     
                row.get::<_, String>(6)?,     
                row.get::<_, String>(7)?,     
                row.get::<_, Option<String>>(8)?, 
                row.get::<_, String>(9)?,     
                row.get::<_, String>(10)?,    
                row.get::<_, String>(11)?,    
                row.get::<_, bool>(12)?,      
                row.get::<_, bool>(13)?,      
                row.get::<_, String>(14)?,    
                row.get::<_, i64>(15)?,       
                row.get::<_, Option<String>>(16)?, 
            ))
        })
    };

    match row_result {
        Ok((oid, name, schema, arg_types_json, arg_names_json, arg_modes_json, return_type, return_type_kind_str, 
            return_table_cols_json, function_body, language, volatility, strict, security_definer, 
            parallel, owner_oid, created_at)) => 
        {
            let arg_types: Vec<String> = serde_json::from_str(&arg_types_json)?;
            let arg_names: Vec<String> = serde_json::from_str(&arg_names_json)?;
            let arg_modes: Vec<ParamMode> = serde_json::from_str(&arg_modes_json)?;
            let return_type_kind: ReturnTypeKind = 
                match return_type_kind_str.as_str() {
                    "Scalar" => ReturnTypeKind::Scalar,
                    "SetOf" => ReturnTypeKind::SetOf,
                    "Table" => ReturnTypeKind::Table,
                    "Void" => ReturnTypeKind::Void,
                    _ => ReturnTypeKind::Scalar,
                };
            let return_table_cols: Option<Vec<(String, String)>> = 
                return_table_cols_json
                .and_then(|s| serde_json::from_str(&s).ok());

            Ok(Some(FunctionMetadata {
                oid,
                name,
                schema,
                arg_types,
                arg_names,
                arg_modes,
                return_type,
                return_type_kind,
                return_table_cols,
                function_body,
                language,
                volatility,
                strict,
                security_definer,
                parallel,
                owner_oid,
                created_at,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Drop a function from the catalog
pub fn drop_function(
    conn: &Connection,
    name: &str,
    arg_types: Option<&[String]>
) -> Result<bool> {
    let query = if arg_types.is_some() {
        "DELETE FROM __pg_functions__ WHERE funcname = ? AND arg_types = ?"
    } else {
        "DELETE FROM __pg_functions__ WHERE funcname = ?"
    };

    let arg_types_json = arg_types.map(|types| serde_json::to_string(types).unwrap());

    let mut stmt = conn.prepare(query)?;
    
    let changes = if let Some(json) = &arg_types_json {
        stmt.execute([name, json])?
    } else {
        stmt.execute([name])?
    };

    Ok(changes > 0)
}
