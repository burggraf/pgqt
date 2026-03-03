//! Handler module for PGQT proxy server.
//!
//! This module contains the `SqliteHandler` struct which implements the PostgreSQL wire protocol
//! handler for translating PostgreSQL queries to SQLite.

use std::sync::{Arc, Mutex};
use anyhow::Result;
use rusqlite::Connection;
use dashmap::DashMap;

use crate::catalog::{init_catalog, init_system_views};
use crate::schema::{SchemaManager, SearchPath};
use crate::copy;

// Submodules
pub mod query;
pub mod transaction;
pub mod utils;

// Re-export commonly used items
pub use query::QueryExecution;
pub use utils::HandlerUtils;

/// Session context for each client connection
#[derive(Debug, Clone)]
pub struct SessionContext {
    #[allow(dead_code)]
    pub authenticated_user: String,
    pub current_user: String,
    pub search_path: SearchPath,
}

/// PostgreSQL-to-SQLite proxy handler
#[derive(Clone)]
pub struct SqliteHandler {
    pub conn: Arc<Mutex<Connection>>,
    pub sessions: Arc<DashMap<u32, SessionContext>>,
    pub schema_manager: SchemaManager,
    pub copy_handler: copy::CopyHandler,
    pub functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>,
}

impl SqliteHandler {
    /// Create a new SqliteHandler with the given database path
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        init_catalog(&conn)?;
        init_system_views(&conn)?;

        // Register PostgreSQL-compatible functions
        Self::register_builtin_functions(&conn)?;

        let conn_arc = Arc::new(Mutex::new(conn));
        let copy_handler = crate::copy::CopyHandler::new(conn_arc.clone());

        Ok(Self {
            conn: conn_arc,
            sessions: Arc::new(DashMap::new()),
            schema_manager: SchemaManager::new(std::path::Path::new(db_path)),
            copy_handler,
            functions: Arc::new(DashMap::new()),
        })
    }

    /// Register built-in PostgreSQL-compatible functions with SQLite
    fn register_builtin_functions(conn: &Connection) -> Result<()> {
        use rusqlite::functions::FunctionFlags;
        
        // pg_get_userbyid - returns username for OID
        conn.create_scalar_function("pg_get_userbyid", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok("postgres".to_string())
        })?;

        // pg_table_is_visible - checks if table is visible in search path
        conn.create_scalar_function("pg_table_is_visible", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        // pg_type_is_visible - checks if type is visible
        conn.create_scalar_function("pg_type_is_visible", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        // pg_function_is_visible - checks if function is visible
        conn.create_scalar_function("pg_function_is_visible", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        // format_type - formats type OID to type name
        conn.create_scalar_function("format_type", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let type_oid: i64 = ctx.get(0)?;
            let _typemod: Option<i64> = ctx.get(1)?;

            let type_name = match type_oid {
                16 => "boolean",
                20 => "bigint",
                21 => "smallint",
                23 => "integer",
                25 => "text",
                700 => "real",
                701 => "double precision",
                1043 => "character varying",
                1114 => "timestamp without time zone",
                1184 => "timestamp with time zone",
                _ => "text",
            };
            Ok(type_name.to_string())
        })?;

        // version - returns PostgreSQL version string
        conn.create_scalar_function("version", 0, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("PostgreSQL 15.0 (pgqt)".to_string())
        })?;

        // current_schema - returns current schema name
        conn.create_scalar_function("current_schema", 0, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("public".to_string())
        })?;

        // current_schemas - returns array of schema names
        conn.create_scalar_function("current_schemas", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("{public}".to_string())
        })?;

        // current_setting - returns setting value
        conn.create_scalar_function("current_setting", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let name: String = ctx.get(0)?;
            match name.as_str() {
                "server_version_num" => Ok("150000".to_string()),
                "server_version" => Ok("15.0".to_string()),
                "standard_conforming_strings" => Ok("on".to_string()),
                "client_encoding" => Ok("UTF8".to_string()),
                _ => Ok("".to_string()),
            }
        })?;

        // pg_encoding_to_char - returns encoding name
        conn.create_scalar_function("pg_encoding_to_char", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let encoding: i64 = ctx.get(0)?;
            match encoding {
                0 => Ok("SQL_ASCII".to_string()),
                6 => Ok("UTF8".to_string()),
                _ => Ok("UTF8".to_string()),
            }
        })?;

        // array_to_string - converts array to string with delimiter
        conn.create_scalar_function("array_to_string", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: Option<String> = ctx.get(0)?;
            let sep: String = ctx.get(1)?;
            match arr {
                Some(s) => {
                    let cleaned = s.replace('{', "").replace('}', "").trim().to_string();
                    Ok(Some(cleaned.replace(',', &sep)))
                }
                None => Ok(None),
            }
        })?;

        // array_length - returns array length
        conn.create_scalar_function("array_length", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: Option<String> = ctx.get(0)?;
            let _dim: i64 = ctx.get(1)?;
            match arr {
                Some(s) => {
                    let cleaned = s.trim_matches(|c| c == '{' || c == '}');
                    let elements: Vec<&str> = cleaned.split(',').filter(|s| !s.is_empty()).collect();
                    Ok(Some(elements.len() as i64))
                }
                None => Ok(None),
            }
        })?;

        // cardinality - returns total number of elements
        conn.create_scalar_function("cardinality", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: Option<String> = ctx.get(0)?;
            match arr {
                Some(s) => {
                    let cleaned = s.trim_matches(|c| c == '{' || c == '}');
                    let elements: Vec<&str> = cleaned.split(',').filter(|s| !s.is_empty()).collect();
                    Ok(Some(elements.len() as i64))
                }
                None => Ok(None),
            }
        })?;

        // to_tsvector - creates full-text search vector
        conn.create_scalar_function("to_tsvector", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            Ok(crate::fts::to_tsvector_impl(&config, &text))
        })?;

        // to_tsquery - creates full-text search query
        conn.create_scalar_function("to_tsquery", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let query: String = ctx.get(1)?;
            Ok(crate::fts::to_tsquery_impl(&config, &query))
        })?;

        // ts_rank - ranks document against query
        conn.create_scalar_function("ts_rank", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::ts_rank_impl(&tsvector, &tsquery))
        })?;

        // tsmatch - full-text search match operator
        conn.create_scalar_function("tsmatch", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::tsvector_matches_tsquery(&tsvector, &tsquery))
        })?;

        // l2_distance - L2/Euclidean distance between two vectors
        conn.create_scalar_function("l2_distance", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l2_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // inner_product - inner/dot product of two vectors
        conn.create_scalar_function("inner_product", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::inner_product(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // cosine_distance - cosine distance between two vectors
        conn.create_scalar_function("cosine_distance", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::cosine_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Array functions
        conn.create_scalar_function("array_overlap", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::operators::array_overlap(&left, &right)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_contains", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::operators::array_contains(&left, &right)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_contained", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::operators::array_contained(&left, &right)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_append", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_append(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_prepend", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let elem: String = ctx.get(0)?;
            let arr: String = ctx.get(1)?;
            crate::array::functions::array_prepend(&elem, &arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_cat", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::functions::array_cat(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_remove", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_remove(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_replace", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let old: String = ctx.get(1)?;
            let new: String = ctx.get(2)?;
            crate::array::functions::array_replace(&arr, &old, &new)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_position", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_position_fn(&arr, &elem, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_positions", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_positions_fn(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_ndims", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::functions::array_ndims_fn(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_dims", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::functions::array_dims_fn(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("trim_array", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let n: i32 = ctx.get(1)?;
            crate::array::functions::trim_array_fn(&arr, n)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_fill", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: String = ctx.get(0)?;
            let dimensions: String = ctx.get(1)?;
            crate::array::functions::array_fill_fn(&value, &dimensions, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Geo functions
        conn.create_scalar_function("geo_distance", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let p1: String = ctx.get(0)?;
            let p2: String = ctx.get(1)?;
            crate::geo::point_distance(&p1, &p2)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_overlaps", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_overlaps(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_contains", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_contains(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_contained", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_contains(&b2, &b1)  // reverse order for contained
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_left", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_left(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_right", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_right(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_below", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_below(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_above", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_above(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Range functions
        use crate::range::RangeType;

        conn.create_scalar_function("range_contains", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let range: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::range::range_contains_elem(&range, &elem, RangeType::Int4)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("int4range", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let lower: String = ctx.get(0)?;
            let upper: String = ctx.get(1)?;
            Ok(format!("[{},{})", lower, upper))
        })?;

        conn.create_scalar_function("daterange", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let lower: String = ctx.get(0)?;
            let upper: String = ctx.get(1)?;
            let bounds: String = ctx.get(2)?;
            Ok(format!("{}{},{}{}", 
                if bounds.starts_with('[') { '[' } else { '(' },
                lower, upper,
                if bounds.ends_with(']') { ']' } else { ')' }))
        })?;

        conn.create_scalar_function("isempty", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let range: String = ctx.get(0)?;
            crate::range::isempty(&range, RangeType::Int4)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("lower", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let range: String = ctx.get(0)?;
            crate::range::lower(&range, RangeType::Int4)
                .map(|opt| opt.unwrap_or_default())
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("upper", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let range: String = ctx.get(0)?;
            crate::range::upper(&range, RangeType::Int4)
                .map(|opt| opt.unwrap_or_default())
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        Ok(())
    }
}

// Implement HandlerUtils trait for SqliteHandler
impl HandlerUtils for SqliteHandler {
    fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    fn sessions(&self) -> &Arc<DashMap<u32, SessionContext>> {
        &self.sessions
    }

    fn schema_manager(&self) -> &SchemaManager {
        &self.schema_manager
    }

    fn functions(&self) -> &Arc<DashMap<String, crate::catalog::FunctionMetadata>> {
        &self.functions
    }
}

// Implement QueryExecution trait for SqliteHandler
impl QueryExecution for SqliteHandler {
    fn copy_handler(&self) -> &copy::CopyHandler {
        &self.copy_handler
    }
}
