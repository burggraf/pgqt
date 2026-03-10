//! Handler module for PGQT proxy server.
//!
//! This module contains the `SqliteHandler` struct which implements the PostgreSQL wire protocol
//! handler for translating PostgreSQL queries to SQLite.

use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use anyhow::Result;
use rusqlite::Connection;
use dashmap::DashMap;
use pgwire::api::portal::{Portal, Format};
use pgwire::api::stmt::{StoredStatement, QueryParser};
use pgwire::api::query::ExtendedQueryHandler;
use pgwire::api::results::{DescribePortalResponse, DescribeStatementResponse, FieldInfo, Response, DescribeResponse};
use pgwire::api::{ClientInfo, ClientPortalStore};
use pgwire::error::PgWireResult;
use pgwire::messages::PgWireBackendMessage;
use pgwire::messages::data::RowDescription;
use futures::Sink;
use async_trait::async_trait;

use crate::debug;
use crate::catalog::{init_catalog, init_system_views};
use crate::schema::{SchemaManager, SearchPath};
use crate::copy;

// Thread-local storage for the current user during query execution
thread_local! {
    static CURRENT_USER: RefCell<String> = RefCell::new("postgres".to_string());
}

/// Set the current user for the current thread
pub fn set_current_user(user: &str) {
    CURRENT_USER.with(|u| *u.borrow_mut() = user.to_string());
}

/// Get the current user for the current thread
pub fn get_current_user() -> String {
    CURRENT_USER.with(|u| u.borrow().clone())
}

// Submodules
pub mod errors;
pub mod query;
pub mod rewriter;
pub mod transaction;
pub mod utils;

// Re-export commonly used items
pub use query::QueryExecution;
pub use utils::HandlerUtils;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Idle,           // Not in a transaction
    InTransaction,  // BEGIN called, no errors
    InError,        // Command failed inside transaction, must ROLLBACK
}

/// Session context for each client connection
#[derive(Debug, Clone)]
pub struct SessionContext {
    #[allow(dead_code)]
    pub authenticated_user: String,
    pub current_user: String,
    pub search_path: SearchPath,
    pub transaction_status: TransactionStatus,
    pub savepoints: Vec<String>,
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

        let conn_arc = Arc::new(Mutex::new(conn));
        let copy_handler = crate::copy::CopyHandler::new(conn_arc.clone());

        let handler = Self {
            conn: conn_arc,
            sessions: Arc::new(DashMap::new()),
            schema_manager: SchemaManager::new(std::path::Path::new(db_path)),
            copy_handler,
            functions: Arc::new(DashMap::new()),
        };

        // Register PostgreSQL-compatible functions
        Self::register_builtin_functions(&handler.conn.lock().unwrap())?;

        // Register PL/pgSQL call wrappers
        handler.register_plpgsql_wrappers(&handler.conn.lock().unwrap())?;

        Ok(handler)
    }

    /// Register PL/pgSQL call wrappers
    pub fn register_plpgsql_wrappers(&self, conn: &Connection) -> Result<()> {
        use rusqlite::functions::FunctionFlags;

        // pgqt_plpgsql_call_scalar - Execute PL/pgSQL function and return scalar
        let functions_cache = self.functions().clone();
        conn.create_scalar_function("pgqt_plpgsql_call_scalar", -1, FunctionFlags::SQLITE_UTF8, move |ctx| {
            let func_name: String = ctx.get(0)?;
            let mut args = Vec::new();
            for i in 1..ctx.len() {
                args.push(ctx.get::<rusqlite::types::Value>(i)?);
            }

            // Look up function metadata from the cache
            let metadata = functions_cache.get(&func_name)
                .ok_or_else(|| rusqlite::Error::UserFunctionError(format!("Function {} not found", func_name).into()))?;

            // Create a new Lua runtime for this call
            let runtime = crate::plpgsql::PlPgSqlRuntime::new()
                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

            let temp_conn = Connection::open_in_memory()?; 
            
            // Reconstruct the CREATE FUNCTION statement to parse the PL/pgSQL
            let mut arg_defs = Vec::new();
            for (i, typ) in metadata.arg_types.iter().enumerate() {
                let name = if i < metadata.arg_names.len() && !metadata.arg_names[i].is_empty() {
                    metadata.arg_names[i].clone()
                } else {
                    format!("arg{}", i + 1)
                };
                arg_defs.push(format!("{} {}", name, typ));
            }
            let args_signature = arg_defs.join(", ");

            let create_sql = if metadata.function_body.to_uppercase().contains("BEGIN") {
                format!("CREATE FUNCTION {}({}) RETURNS {} AS $${}$$ LANGUAGE plpgsql;", 
                    func_name, args_signature, metadata.return_type, metadata.function_body)
            } else {
                format!("CREATE FUNCTION {}({}) RETURNS {} AS $$BEGIN {} END;$$ LANGUAGE plpgsql;", 
                    func_name, args_signature, metadata.return_type, metadata.function_body)
            };
            
            let parsed_func = crate::plpgsql::parse_plpgsql_function(&create_sql)
                .map_err(|e| {
                    eprintln!("Failed to parse PL/pgSQL: {}. SQL: {}", e, create_sql);
                    rusqlite::Error::UserFunctionError(e.into())
                })?;
            let lua_code = crate::plpgsql::transpile_to_lua(&parsed_func)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

            let result = runtime.execute_function(&temp_conn, &lua_code, &args)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

            Ok(result)
        })?;

        Ok(())
    }

    /// Register built-in PostgreSQL-compatible functions with SQLite
    pub fn register_builtin_functions(conn: &Connection) -> Result<()> {
        use rusqlite::functions::FunctionFlags;
        // Register current_user function that returns the session user
        conn.create_scalar_function("pgqt_current_user", 0, FunctionFlags::SQLITE_UTF8, |_ctx| {
            Ok(get_current_user())
        })?;


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

        
        // pg_get_function_result - returns return type based on OID
        conn.create_scalar_function("pg_get_function_result", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let oid: i64 = ctx.get(0)?;
            // Built-in function OIDs
            let result = match oid {
                10001 => "timestamp with time zone", // now
                10002 => "timestamp with time zone", // current_timestamp
                10003 => "date",                      // current_date
                10004 => "time with time zone",       // current_time
                _ => "integer",                       // default for user functions
            };
            Ok(result.to_string())
        })?;

        // pg_get_function_identity_arguments - returns argument signature
        conn.create_scalar_function("pg_get_function_identity_arguments", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("".to_string())
        })?;

        // pg_get_function_arguments - returns formatted argument list
        conn.create_scalar_function("pg_get_function_arguments", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok("".to_string())
        })?;
        // repeat(text, int) - repeats text N times
        conn.create_scalar_function("repeat", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let s: String = ctx.get(0)?;
            let n: i64 = ctx.get(1)?;
            if n <= 0 {
                Ok("".to_string())
            } else {
                Ok(s.repeat(n as usize))
            }
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

        // to_char - format a value according to a format string
        // Basic implementation supporting common numeric formats
        conn.create_scalar_function("to_char", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: f64 = ctx.get(0)?;
            let format: String = ctx.get(1)?;
            
            // Map PostgreSQL format patterns to Rust format strings
            // FM9999.99 -> removes leading/trailing spaces
            // 9999.99 -> standard numeric format
            let format = format.trim_start_matches("FM");
            
            if format.contains('.') {
                // Determine decimal places from format
                let decimal_places = format.split('.').nth(1).map(|s| s.len()).unwrap_or(2);
                let format_str = format!("%.{}f", decimal_places);
                Ok(format_str.replace("{}", &format!("{:.1$}", value, decimal_places)))
            } else {
                // Integer format
                Ok(format!("{:.0}", value))
            }
        })?;

        // version - returns PostgreSQL version string
        conn.create_scalar_function("version", 0, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("PostgreSQL 15.0 (pgqt)".to_string())
        })?;

        // set_config(name, value, is_local) - returns new value
        conn.create_scalar_function("set_config", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _name: String = ctx.get(0)?;
            let value: String = ctx.get(1)?;
            let _is_local: bool = ctx.get(2)?;
            Ok(value)
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
            let arr: String = ctx.get(0)?;
            let sep: String = ctx.get(1)?;
            crate::array::array_to_string_fn(&arr, &sep, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_to_string", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let sep: String = ctx.get(1)?;
            let null_str: String = ctx.get(2)?;
            crate::array::array_to_string_fn(&arr, &sep, Some(&null_str))
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // array_length - returns array length
        conn.create_scalar_function("array_length", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let dim: i32 = ctx.get(1)?;
            crate::array::array_length_fn(&arr, dim)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // cardinality - returns total number of elements
        conn.create_scalar_function("cardinality", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::array_cardinality(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
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

        // string_to_array - splits string into array
        conn.create_scalar_function("string_to_array", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let text: String = ctx.get(0)?;
            let delimiter: String = ctx.get(1)?;
            crate::array::functions::string_to_array_fn(&text, &delimiter, None)
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

        // int4range with 2 args (default bounds [))
        conn.create_scalar_function("int4range", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low = ctx.get_raw(0).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(0).unwrap().to_string());
            let high = ctx.get_raw(1).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(1).unwrap().to_string());
            let rv = crate::range::parse_range(&format!("[{},{})", low, high), RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        // int4range with 3 args (custom bounds)
        conn.create_scalar_function("int4range", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low = ctx.get_raw(0).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(0).unwrap().to_string());
            let high = ctx.get_raw(1).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(1).unwrap().to_string());
            let bounds: String = ctx.get(2)?;
            let rv = crate::range::parse_range(&format!("{}{},{}{}", &bounds[0..1], low, high, &bounds[1..2]), RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        // daterange with 2 args (default bounds [))
        conn.create_scalar_function("daterange", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low: String = ctx.get(0)?;
            let high: String = ctx.get(1)?;
            let rv = crate::range::parse_range(&format!("[{},{})", low, high), RangeType::Date)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        // daterange with 3 args (custom bounds)
        conn.create_scalar_function("daterange", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low: String = ctx.get(0)?;
            let high: String = ctx.get(1)?;
            let bounds: String = ctx.get(2)?;
            let rv = crate::range::parse_range(&format!("{}{},{}{}", &bounds[0..1], low, high, &bounds[1..2]), RangeType::Date)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        conn.create_scalar_function("isempty", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let range: String = ctx.get(0)?;
            crate::range::isempty(&range, RangeType::Int4)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("lower", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let val: String = ctx.get(0)?;
            match crate::range::lower(&val, RangeType::Int4) {
                Ok(opt) => Ok(opt.unwrap_or_default()),
                Err(_) => Ok(val.to_lowercase()),
            }
        })?;

        conn.create_scalar_function("upper", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let val: String = ctx.get(0)?;
            match crate::range::upper(&val, RangeType::Int4) {
                Ok(opt) => Ok(opt.unwrap_or_default()),
                Err(_) => Ok(val.to_uppercase()),
            }
        })?;

        // regexp - pattern matching function (used by ~ operator)
        conn.create_scalar_function("regexp", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            let regex = regex::Regex::new(&pattern)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(if regex.is_match(&text) { 1i64 } else { 0i64 })
        })?;

        // regexpi - case-insensitive pattern matching (used by ~* operator)        
        conn.create_scalar_function("regexpi", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            let regex = regex::Regex::new(&format!("(?i){}", pattern))
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(if regex.is_match(&text) { 1i64 } else { 0i64 })
        })?;

        // Register statistical aggregate functions
        crate::stats::register_statistical_functions(conn)?;

        // random - returns float between 0 and 1
        conn.create_scalar_function("random", 0, FunctionFlags::SQLITE_UTF8, |_ctx| {
            let r = unsafe { libc::rand() } as f64 / libc::RAND_MAX as f64;
            Ok(r)
        })?;

        // pg_typeof - returns type name of the value
        conn.create_scalar_function("pg_typeof", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let val = ctx.get_raw(0);
            match val {
                rusqlite::types::ValueRef::Null => Ok("unknown".to_string()),
                rusqlite::types::ValueRef::Integer(_) => Ok("integer".to_string()),
                rusqlite::types::ValueRef::Real(_) => Ok("double precision".to_string()),
                rusqlite::types::ValueRef::Text(_) => Ok("text".to_string()),
                rusqlite::types::ValueRef::Blob(_) => Ok("bytea".to_string()),
            }
        })?;

        // pg_get_viewdef - stub for view definition
        conn.create_scalar_function("pg_get_viewdef", -1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("SELECT 1".to_string())
        })?;

        Ok(())
    }
}

use crate::transpiler::metadata::{ColumnInfo, MetadataProvider};

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

impl MetadataProvider for SqliteHandler {
    fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>> {
        let conn = self.conn.lock().unwrap();
        
        match crate::catalog::get_table_columns_with_defaults(&conn, table_name) {
            Ok(metadata) => {
                let columns: Vec<ColumnInfo> = metadata
                    .into_iter()
                    .map(|m| {
                        let default_expr = m.constraints.as_ref()
                            .and_then(|c| crate::catalog::extract_default_from_constraints(c));
                        
                        ColumnInfo {
                            name: m.column_name,
                            original_type: m.original_type,
                            default_expr,
                            is_nullable: m.constraints.as_ref()
                                .map(|c| !c.to_uppercase().contains("NOT NULL"))
                                .unwrap_or(true),
                            type_oid: None,
                        }
                    })
                    .collect();
                
                if columns.is_empty() {
                    None
                } else {
                    Some(columns)
                }
            }
            Err(_) => None,
        }
    }
    
    fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        
        match crate::catalog::get_column_metadata(&conn, table_name, column_name) {
            Ok(Some(metadata)) => {
                metadata.constraints
                    .and_then(|c| crate::catalog::extract_default_from_constraints(&c))
            }
            _ => None,
        }
    }
}

// Implement QueryExecution trait for SqliteHandler
impl QueryExecution for SqliteHandler {
    fn copy_handler(&self) -> &copy::CopyHandler {
        &self.copy_handler
    }
    
    fn as_metadata_provider(&self) -> Arc<dyn crate::transpiler::metadata::MetadataProvider> {
        Arc::new(self.clone())
    }
}

pub struct SqliteQueryParser;

#[async_trait]
impl QueryParser for SqliteQueryParser {
    type Statement = String;

    async fn parse_sql<C>(&self, _client: &C, sql: &str, _types: &[Option<pgwire::api::Type>]) -> PgWireResult<Self::Statement>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        Ok(sql.to_string())
    }

    fn get_parameter_types(&self, _stmt: &Self::Statement) -> PgWireResult<Vec<pgwire::api::Type>> {
        Ok(vec![])
    }

    fn get_result_schema(&self, _stmt: &Self::Statement, _format: Option<&Format>) -> PgWireResult<Vec<FieldInfo>> {
        Ok(vec![])
    }
}

#[async_trait]
impl ExtendedQueryHandler for SqliteHandler {
    type Statement = String;
    type QueryParser = SqliteQueryParser;

    fn query_parser(&self) -> Arc<Self::QueryParser> {
        Arc::new(SqliteQueryParser)
    }

    async fn do_query<C>(
        &self,
        _client: &mut C,
        portal: &Portal<Self::Statement>,
        _max_rows: usize,
    ) -> PgWireResult<Response>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        pgwire::error::PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let query = &portal.statement.statement;
        let params = &portal.parameters;
        
        debug!("Extended query: {}", query);
        
        // Convert params to Option<String> for execute_query_params
        let mut param_strings = Vec::new();
        for (i, param) in params.iter().enumerate() {
            if let Some(bytes) = param {
                if portal.parameter_format.is_binary(i) {
                    // Get type from statement if available
                    let pg_type = portal.statement.parameter_types.get(i)
                        .and_then(|t| t.as_ref())
                        .unwrap_or(&pgwire::api::Type::UNKNOWN);
                    
                    debug!("Parameter {} is binary, type: {:?}", i, pg_type);
                    
                    match *pg_type {
                        pgwire::api::Type::INT4 | pgwire::api::Type::OID | pgwire::api::Type::REGCLASS | pgwire::api::Type::INT2 => {
                            if bytes.len() == 4 {
                                let b: [u8; 4] = bytes.as_ref().try_into().unwrap_or([0; 4]);
                                let val = i32::from_be_bytes(b);
                                param_strings.push(Some(val.to_string()));
                                continue;
                            } else if bytes.len() == 2 {
                                let b: [u8; 2] = bytes.as_ref().try_into().unwrap_or([0; 2]);
                                let val = i16::from_be_bytes(b);
                                param_strings.push(Some(val.to_string()));
                                continue;
                            }
                        }
                        pgwire::api::Type::INT8 => {
                            if bytes.len() == 8 {
                                let b: [u8; 8] = bytes.as_ref().try_into().unwrap_or([0; 8]);
                                let val = i64::from_be_bytes(b);
                                param_strings.push(Some(val.to_string()));
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                param_strings.push(Some(String::from_utf8_lossy(bytes).to_string()));
            } else {
                param_strings.push(None);
            }
        }

        match self.execute_query_params(query, &param_strings) {
            Ok(mut responses) => {
                if let Some(resp) = responses.pop() {
                    // Force RowDescription for SELECTs if not already sent by client Describe
                    if let Response::Query(ref query_resp) = resp {
                        let fields = query_resp.row_schema();
                        let _row_desc = RowDescription::new(fields.iter().map(|f| {
                            pgwire::messages::data::FieldDescription::new(
                                f.name().to_string(),
                                f.table_id().unwrap_or(0),
                                f.column_id().unwrap_or(0),
                                f.datatype().oid(),
                                0,
                                0,
                                f.format().value(),
                            )
                        }).collect());
                        // println!("DEBUG: Sending forced RowDescription");
                        // client.send(PgWireBackendMessage::RowDescription(row_desc)).await?;
                    }
                    Ok(resp)
                } else {
                    Ok(Response::Execution(pgwire::api::results::Tag::new("OK")))
                }
            }
            Err(e) => {
                eprintln!("Error executing extended query: {}", e);
                let pg_err = crate::handler::errors::PgError::from_anyhow(e);
                Ok(Response::Error(Box::new(pg_err.into_error_info())))
            }
        }
    }

    async fn do_describe_statement<C>(
        &self,
        _client: &mut C,
        statement: &StoredStatement<Self::Statement>,
    ) -> PgWireResult<DescribeStatementResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        pgwire::error::PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let query = &statement.statement;
        debug!("Describe statement: {}", query);
        let _result = crate::transpiler::transpile_with_metadata(query);
        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
        ctx.set_metadata_provider(self.as_metadata_provider());
        let transpile_result = crate::transpiler::transpile_with_context(query, &mut ctx);
        
        let conn = self.conn().lock().unwrap();
        if let Ok(stmt) = conn.prepare(&transpile_result.sql) {
            let fields = self.build_field_info(&stmt, &transpile_result.referenced_tables, &conn, &transpile_result.column_aliases, &transpile_result.column_types)
                .unwrap_or_default();
            
            // For parameters, we don't know the types yet easily, so return UNKNOWN or derived from statement.parameter_types
            let param_types = statement.parameter_types.iter().map(|t| t.clone().unwrap_or(pgwire::api::Type::UNKNOWN)).collect();
            
            debug!("Returning {} fields for Describe statement", fields.len());
            return Ok(DescribeStatementResponse::new(param_types, fields));
        }
        
        Ok(DescribeStatementResponse::no_data())
    }

    async fn do_describe_portal<C>(
        &self,
        _client: &mut C,
        portal: &Portal<Self::Statement>,
    ) -> PgWireResult<DescribePortalResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        pgwire::error::PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let query = &portal.statement.statement;
        debug!("Describe portal: {}", query);
        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
        ctx.set_metadata_provider(self.as_metadata_provider());
        let transpile_result = crate::transpiler::transpile_with_context(query, &mut ctx);
        
        let conn = self.conn().lock().unwrap();
        if let Ok(stmt) = conn.prepare(&transpile_result.sql) {
            let fields = self.build_field_info(&stmt, &transpile_result.referenced_tables, &conn, &transpile_result.column_aliases, &transpile_result.column_types)
                .unwrap_or_default();
            debug!("Returning {} fields for Describe portal", fields.len());
            return Ok(DescribePortalResponse::new(fields));
        }
        
        Ok(DescribePortalResponse::new(vec![]))
    }
}
