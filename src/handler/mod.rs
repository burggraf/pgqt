//! Handler module for PGQT proxy server.
//!
//! This module contains the `SqliteHandler` struct which implements the PostgreSQL wire protocol
//! handler for translating PostgreSQL queries to SQLite.

//! PostgreSQL wire protocol handler for SQLite
//!
//! This module implements the `SqliteHandler` struct which bridges PostgreSQL wire
//! protocol messages to SQLite operations. It handles:
//!
//! - **Simple queries** — `SimpleQueryHandler` trait implementation
//! - **Extended queries** — `ExtendedQueryHandler` trait implementation (prepared statements)
//! - **COPY commands** — `CopyHandler` for COPY FROM/TO STDIN/STDOUT
//! - **Session management** — Per-connection context (current_user, search_path)
//! - **Custom SQL functions** — PostgreSQL-compatible functions registered with SQLite
//!
//! ## Key Components
//!
//! - `SqliteHandler` — Main handler struct implementing pgwire traits
//! - `SessionContext` — Per-connection session state
//!
//! ## Custom Functions
//!
//! The handler registers numerous PostgreSQL-compatible scalar functions with SQLite:
//! - System info: `version()`, `current_schema()`, `current_setting()`
//! - Type introspection: `format_type()`, `pg_type_is_visible()`
//! - Table introspection: `pg_table_is_visible()`, `pg_get_userbyid()`
//! - FTS: `to_tsvector()`, `to_tsquery()`, `ts_rank()`, `ts_headline()`
//! - Arrays: `array_append()`, `array_length()`, `cardinality()`
//! - Vectors: `l2_distance()`, `cosine_distance()`, `inner_product()`
//! - Full-text search operators: `tsmatch`, `tsquery_and`, `tsquery_or`

use std::sync::{Arc, Mutex};
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use dashmap::DashMap;
use futures::stream;

use crate::catalog::{init_catalog, init_system_views, store_table_metadata, store_relation_metadata};
use crate::schema::{SchemaManager, SearchPath};
use crate::copy;
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::Type;

/// Session context for each client connection
#[derive(Debug, Clone)]
pub(crate) struct SessionContext {
    #[allow(dead_code)]
    pub(crate) authenticated_user: String,
    pub(crate) current_user: String,
    pub(crate) search_path: SearchPath,
}

/// PostgreSQL-to-SQLite proxy handler
pub struct SqliteHandler {
    pub(crate) conn: Arc<Mutex<Connection>>,
    pub(crate) sessions: Arc<DashMap<u32, SessionContext>>,
    pub(crate) schema_manager: SchemaManager,
    pub(crate) copy_handler: copy::CopyHandler,
    
    pub(crate) functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>,
}

impl SqliteHandler {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        init_catalog(&conn)?;
        init_system_views(&conn)?;

        // Register PostgreSQL compatibility functions
        conn.create_scalar_function("pg_get_userbyid", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok("postgres".to_string())
        })?;

        conn.create_scalar_function("pg_table_is_visible", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        conn.create_scalar_function("pg_type_is_visible", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        conn.create_scalar_function("pg_function_is_visible", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        conn.create_scalar_function("format_type", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let type_oid: i64 = ctx.get(0)?;
            let _typemod: Option<i64> = ctx.get(1)?;

            // Map common OIDs back to type names
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

        conn.create_scalar_function("version", 0, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("PostgreSQL 15.0 (pgqt)".to_string())
        })?;

        conn.create_scalar_function("current_schema", 0, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("public".to_string())
        })?;

        conn.create_scalar_function("current_schemas", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("{public}".to_string())
        })?;

        conn.create_scalar_function("current_setting", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let name: String = ctx.get(0)?;
            match name.as_str() {
                "server_version_num" => Ok("150000".to_string()),
                "server_version" => Ok("15.0".to_string()),
                "standard_conforming_strings" => Ok("on".to_string()),
                "client_encoding" => Ok("UTF8".to_string()),
                _ => Ok("".to_string()),
            }
        })?;

        conn.create_scalar_function("pg_get_expr", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let expr: String = ctx.get(0)?;
            Ok(expr)
        })?;

        conn.create_scalar_function("pg_get_indexdef", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok("".to_string())
        })?;

        conn.create_scalar_function("obj_description", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(None::<String>)
        })?;

        conn.create_scalar_function("pg_encoding_to_char", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let encoding: i64 = ctx.get(0)?;
            match encoding {
                6 => Ok("UTF8".to_string()),
                _ => Ok("UTF8".to_string()),
            }
        })?;

        conn.create_scalar_function("array_to_string", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: Option<String> = ctx.get(0)?;
            let sep: String = ctx.get(1)?;
            match arr {
                Some(s) => Ok(s.replace('{', "").replace('}', "").replace(',', &sep)),
                None => Ok("".to_string()),
            }
        })?;

        conn.create_scalar_function("array_length", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: Option<String> = ctx.get(0)?;
            let _dim: i64 = ctx.get(1)?;
            match arr {
                Some(s) => {
                    let cleaned = s.replace('{', "").replace('}', "").trim().to_string();
                    if cleaned.is_empty() {
                        Ok(0i64)
                    } else {
                        Ok(cleaned.split(',').count() as i64)
                    }
                }
                None => Ok(0i64),
            }
        })?;

        conn.create_scalar_function("pg_table_size", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(0i64)
        })?;

        conn.create_scalar_function("pg_total_relation_size", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(0i64)
        })?;

        conn.create_scalar_function("pg_size_pretty", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let size: i64 = ctx.get(0)?;
            Ok(format!("{} bytes", size))
        })?;

        conn.create_scalar_function("current_database", 0, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("postgres".to_string())
        })?;

        conn.create_scalar_function("pg_my_temp_schema", 0, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(0i64)
        })?;

        // PL/pgSQL function execution wrappers
        // These are called by the transpiler when a PL/pgSQL function is used in SQL
        conn.create_scalar_function("pgqt_plpgsql_call_scalar", -1, 
            rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            // First arg is function name, rest are arguments
            let argc = ctx.len();
            if argc < 1 {
                return Err(rusqlite::Error::UserFunctionError("Invalid function parameter".into()));
            }
            
            let func_name: String = ctx.get(0)?;
            
            // Collect remaining args
            let mut args = Vec::new();
            for i in 1..argc {
                let value = ctx.get_raw(i).into();
                args.push(value);
            }
            
            use crate::functions::{execute_function, FunctionResult};
            use crate::catalog::get_function;
            
            // Get function metadata from catalog (by name only for now)
            let conn = unsafe { ctx.get_connection()? };
            let metadata = match get_function(&conn, &func_name, None) {
                Ok(Some(m)) => m,
                _ => return Err(rusqlite::Error::UserFunctionError("Function not found".into())),
            };
            
            // Check if it's a PL/pgSQL function
            if metadata.language.to_lowercase() != "plpgsql" {
                return Err(rusqlite::Error::UserFunctionError("Not a PL/pgSQL function".into()));
            }
            
            // Execute the function
            match execute_function(&conn, &metadata, &args) {
                Ok(FunctionResult::Scalar(Some(val))) => Ok(val),
                Ok(FunctionResult::Scalar(None)) => Ok(rusqlite::types::Value::Null),
                Ok(_) => Err(rusqlite::Error::UserFunctionError("Invalid function result".into())),
                Err(e) => Err(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                    Some(e.to_string())
                )),
            }
        })?;

        conn.create_scalar_function("pgqt_plpgsql_call_void", -1, 
            rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            // First arg is function name, rest are arguments
            let argc = ctx.len();
            if argc < 1 {
                return Err(rusqlite::Error::UserFunctionError("Invalid function parameter".into()));
            }
            
            let func_name: String = ctx.get(0)?;
            
            // Collect remaining args
            let mut args = Vec::new();
            for i in 1..argc {
                let value = ctx.get_raw(i).into();
                args.push(value);
            }
            
            use crate::functions::execute_function;
            use crate::catalog::get_function;
            
            let conn = unsafe { ctx.get_connection()? };
            let metadata = match get_function(&conn, &func_name, None) {
                Ok(Some(m)) => m,
                _ => return Err(rusqlite::Error::UserFunctionError("Function not found".into())),
            };
            
            if metadata.language.to_lowercase() != "plpgsql" {
                return Err(rusqlite::Error::UserFunctionError("Not a PL/pgSQL function".into()));
            }
            
            // Execute and ignore result
            match execute_function(&conn, &metadata, &args) {
                Ok(_) => Ok(rusqlite::types::Value::Null),
                Err(e) => Err(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                    Some(e.to_string())
                )),
            }
        })?;

        // RLS-related functions: current_user() and session_user()
        // Note: These return a default value; actual RLS uses session context directly
        conn.create_scalar_function("current_user", 0, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |_ctx| {
            // This is a fallback; the actual user is handled via session context
            Ok("postgres".to_string())
        })?;

        conn.create_scalar_function("session_user", 0, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |_ctx| {
            Ok("postgres".to_string())
        })?;

        // Privilege checks
        conn.create_scalar_function("has_table_privilege", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        conn.create_scalar_function("has_table_privilege", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        conn.create_scalar_function("has_database_privilege", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        conn.create_scalar_function("has_database_privilege", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        conn.create_scalar_function("has_schema_privilege", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        conn.create_scalar_function("has_schema_privilege", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        conn.create_scalar_function("pg_has_role", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        conn.create_scalar_function("pg_has_role", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok(true)
        })?;

        // REGEXP support (needed for \dt)
        conn.create_scalar_function("regexp", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;

            let re = regex::Regex::new(&pattern).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
            Ok(re.is_match(&text))
        })?;

        // REGEXPI support (case-insensitive regex)
        conn.create_scalar_function("regexpi", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;

            let re = regex::RegexBuilder::new(&pattern)
                .case_insensitive(true)
                .build()
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
            Ok(re.is_match(&text))
        })?;

        // ========== Full-Text Search Functions ==========

        // to_tsvector([config,] text) - converts text to tsvector
        conn.create_scalar_function("to_tsvector", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let text: String = ctx.get(0)?;
            Ok(crate::fts::to_tsvector_impl("english", &text))
        })?;

        conn.create_scalar_function("to_tsvector", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            Ok(crate::fts::to_tsvector_impl(&config, &text))
        })?;

        // to_tsquery([config,] text) - converts text to tsquery
        conn.create_scalar_function("to_tsquery", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let query: String = ctx.get(0)?;
            Ok(crate::fts::to_tsquery_impl("english", &query))
        })?;

        conn.create_scalar_function("to_tsquery", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let query: String = ctx.get(1)?;
            Ok(crate::fts::to_tsquery_impl(&config, &query))
        })?;

        // plainto_tsquery([config,] text) - converts plain text to tsquery
        conn.create_scalar_function("plainto_tsquery", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let query: String = ctx.get(0)?;
            Ok(crate::fts::plainto_tsquery_impl("english", &query))
        })?;

        conn.create_scalar_function("plainto_tsquery", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let query: String = ctx.get(1)?;
            Ok(crate::fts::plainto_tsquery_impl(&config, &query))
        })?;

        // phraseto_tsquery([config,] text) - converts phrase to tsquery
        conn.create_scalar_function("phraseto_tsquery", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let query: String = ctx.get(0)?;
            Ok(crate::fts::phraseto_tsquery_impl("english", &query))
        })?;

        conn.create_scalar_function("phraseto_tsquery", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let query: String = ctx.get(1)?;
            Ok(crate::fts::phraseto_tsquery_impl(&config, &query))
        })?;

        // websearch_to_tsquery([config,] text) - converts web search query to tsquery
        conn.create_scalar_function("websearch_to_tsquery", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let query: String = ctx.get(0)?;
            Ok(crate::fts::websearch_to_tsquery_impl("english", &query))
        })?;

        conn.create_scalar_function("websearch_to_tsquery", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let query: String = ctx.get(1)?;
            Ok(crate::fts::websearch_to_tsquery_impl(&config, &query))
        })?;

        // ts_rank(tsvector, tsquery) - returns rank of match
        conn.create_scalar_function("ts_rank", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::ts_rank_impl(&tsvector, &tsquery))
        })?;

        // ts_rank_cd(tsvector, tsquery) - cover density ranking (same as ts_rank for now)
        conn.create_scalar_function("ts_rank_cd", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::ts_rank_impl(&tsvector, &tsquery))
        })?;

        // ts_headline([config,] text, tsquery [, options]) - returns highlighted text
        conn.create_scalar_function("ts_headline", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let text: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::ts_headline_impl("english", &text, &tsquery, None))
        })?;

        conn.create_scalar_function("ts_headline", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            let tsquery: String = ctx.get(2)?;
            Ok(crate::fts::ts_headline_impl(&config, &text, &tsquery, None))
        })?;

        conn.create_scalar_function("ts_headline", 4, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            let tsquery: String = ctx.get(2)?;
            let options: String = ctx.get(3)?;
            Ok(crate::fts::ts_headline_impl(&config, &text, &tsquery, Some(&options)))
        })?;

        // setweight(tsvector, char) - sets weight on tsvector
        conn.create_scalar_function("setweight", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let weight: String = ctx.get(1)?;
            let weight_char = weight.chars().next().unwrap_or('D');
            Ok(crate::fts::setweight_impl(&tsvector, weight_char))
        })?;

        // strip(tsvector) - removes positions and weights from tsvector
        conn.create_scalar_function("strip", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            Ok(crate::fts::strip_impl(&tsvector))
        })?;

        // numnode(tsquery) - returns number of nodes in tsquery
        conn.create_scalar_function("numnode", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsquery: String = ctx.get(0)?;
            Ok(crate::fts::numnode_impl(&tsquery))
        })?;

        // querytree(tsquery) - returns query tree representation
        conn.create_scalar_function("querytree", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsquery: String = ctx.get(0)?;
            Ok(crate::fts::querytree_impl(&tsquery))
        })?;

        // tsvector_concat(tsvector, tsvector) - concatenates two tsvectors
        conn.create_scalar_function("tsvector_concat", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            Ok(crate::fts::tsvector_concat(&left, &right))
        })?;

        // fts_match(tsvector, tsquery) - implements @@ operator
        conn.create_scalar_function("fts_match", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::tsvector_matches_tsquery(&tsvector, &tsquery))
        })?;

        // fts_contains(tsquery, tsquery) - implements @> operator
        conn.create_scalar_function("fts_contains", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            // Simplified: check if all terms in right are in left
            let left_terms = crate::fts::extract_tsquery_terms(&left);
            let right_terms = crate::fts::extract_tsquery_terms(&right);
            let all_contained = right_terms.positive.iter().all(|t| left_terms.positive.contains(t));
            Ok(all_contained)
        })?;

        // fts_contained(tsquery, tsquery) - implements <@ operator
        conn.create_scalar_function("fts_contained", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            // Simplified: check if all terms in left are in right
            let left_terms = crate::fts::extract_tsquery_terms(&left);
            let right_terms = crate::fts::extract_tsquery_terms(&right);
            let all_contained = left_terms.positive.iter().all(|t| right_terms.positive.contains(t));
            Ok(all_contained)
        })?;

        // array_to_tsvector(text[]) - converts array to tsvector
        conn.create_scalar_function("array_to_tsvector", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: Option<String> = ctx.get(0)?;
            match arr {
                Some(arr_str) => {
                    // Parse array format {elem1,elem2,...}
                    let cleaned = arr_str.trim_matches(|c| c == '{' || c == '}');
                    let elements: Vec<&str> = cleaned.split(',').filter(|s| !s.is_empty()).collect();
                    let mut entries: Vec<String> = Vec::new();
                    for (pos, elem) in elements.iter().enumerate() {
                        entries.push(format!("'{}':{}", elem.trim().trim_matches('"'), pos + 1));
                    }
                    entries.sort();
                    Ok(entries.join(" "))
                }
                None => Ok(String::new()),
            }
        })?;

        // =====================================================
        // Vector Search Functions (pgvector compatibility)
        // =====================================================

        // vector_l2_distance(vector, vector) - L2 (Euclidean) distance
        conn.create_scalar_function("vector_l2_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l2_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // vector_cosine_distance(vector, vector) - Cosine distance
        conn.create_scalar_function("vector_cosine_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::cosine_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // vector_inner_product(vector, vector) - Inner product (dot product)
        conn.create_scalar_function("vector_inner_product", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::inner_product(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // vector_l1_distance(vector, vector) - L1 (Manhattan) distance
        conn.create_scalar_function("vector_l1_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l1_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // l2_distance - alias for vector_l2_distance (pgvector function name)
        conn.create_scalar_function("l2_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l2_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // cosine_distance - alias for vector_cosine_distance (pgvector function name)
        conn.create_scalar_function("cosine_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::cosine_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // inner_product - alias for vector_inner_product (pgvector function name)
        conn.create_scalar_function("inner_product", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::inner_product(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // l1_distance - alias for vector_l1_distance (pgvector function name)
        conn.create_scalar_function("l1_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l1_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // vector_dims(vector) - returns number of dimensions
        conn.create_scalar_function("vector_dims", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let v: String = ctx.get(0)?;
            crate::vector::vector_dims(&v)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // l2_norm(vector) - returns L2 norm (magnitude)
        conn.create_scalar_function("l2_norm", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let v: String = ctx.get(0)?;
            crate::vector::l2_norm(&v)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // l2_normalize(vector) - returns normalized vector
        conn.create_scalar_function("l2_normalize", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let v: String = ctx.get(0)?;
            crate::vector::l2_normalize(&v)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // subvector(vector, start, length) - extracts subvector
        conn.create_scalar_function("subvector", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let v: String = ctx.get(0)?;
            let start: i32 = ctx.get(1)?;
            let length: i32 = ctx.get(2)?;
            crate::vector::subvector(&v, start, length)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // vector_add(vector, vector) - adds two vectors
        conn.create_scalar_function("vector_add", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::vector_add(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // vector_sub(vector, vector) - subtracts two vectors
        conn.create_scalar_function("vector_sub", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::vector_sub(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // ========================================
        // PostgreSQL Array Functions
        // ========================================

        // Array Operators
        conn.create_scalar_function("array_overlap", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_overlap(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_contains", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_contains(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_contained", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_contained(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_concat", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_concat(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Array Manipulation Functions
        conn.create_scalar_function("array_append", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            crate::array::array_append(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // ========================================
        // PostgreSQL Geometric Functions
        // ========================================

        conn.create_scalar_function("geo_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            // If it's a vector, don't try to parse it as a point
            if a.trim().starts_with('[') || b.trim().starts_with('[') {
                return crate::vector::l2_distance(&a, &b)
                        .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))));
            }
            match crate::geo::point_distance(&a, &b) {
                Ok(d) => Ok(d),
                Err(e) => Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            }
        })?;

        conn.create_scalar_function("geo_overlaps", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::geo::box_overlaps(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_contains", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::geo::box_contains(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_contained", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::geo::box_contains(&b, &a) // Contained is reverse of contains for boxes
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_left", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::geo::box_left(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_right", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::geo::box_right(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_below", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::geo::box_below(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_above", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::geo::box_above(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_prepend", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let elem = match ctx.get_raw(0) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            let arr: String = ctx.get(1)?;
            crate::array::array_prepend(&elem, &arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_cat", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left = match ctx.get_raw(0) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            let right = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            crate::array::array_cat(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_remove", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            crate::array::array_remove(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_replace", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let old = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            let new = match ctx.get_raw(2) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            crate::array::array_replace(&arr, &old, &new)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Array Information Functions
        conn.create_scalar_function("array_length", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let dim = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i as i32,
                _ => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Invalid parameter type")))),
            };
            match crate::array::array_length_fn(&arr, dim) {
                Ok(Some(len)) => Ok(len),
                Ok(None) => Ok(-1i64), // Return -1 for NULL to indicate no value
                Err(e) => Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            }
        })?;

        conn.create_scalar_function("array_lower", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let dim = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i as i32,
                _ => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Invalid parameter type")))),
            };
            match crate::array::array_lower_fn(&arr, dim) {
                Ok(Some(val)) => Ok(val),
                Ok(None) => Ok(-1i32), // Return -1 for NULL
                Err(e) => Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            }
        })?;

        conn.create_scalar_function("array_upper", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let dim = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i as i32,
                _ => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Invalid parameter type")))),
            };
            match crate::array::array_upper_fn(&arr, dim) {
                Ok(Some(val)) => Ok(val),
                Ok(None) => Ok(-1i32), // Return -1 for NULL
                Err(e) => Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            }
        })?;

        conn.create_scalar_function("array_ndims", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::array_ndims_fn(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_dims", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::array_dims_fn(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("cardinality", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::array_cardinality(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Array Search Functions
        conn.create_scalar_function("array_position", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            match crate::array::array_position_fn(&arr, &elem, None) {
                Ok(Some(pos)) => Ok(pos),
                Ok(None) => Ok(-1i32), // Return -1 for not found
                Err(e) => Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            }
        })?;

        conn.create_scalar_function("array_position", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            let start = match ctx.get_raw(2) {
                rusqlite::types::ValueRef::Integer(i) => i as i32,
                _ => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Invalid parameter type")))),
            };
            match crate::array::array_position_fn(&arr, &elem, Some(start)) {
                Ok(Some(pos)) => Ok(pos),
                Ok(None) => Ok(-1i32), // Return -1 for not found
                Err(e) => Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            }
        })?;

        conn.create_scalar_function("array_positions", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            crate::array::array_positions_fn(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Array Conversion Functions
        conn.create_scalar_function("array_to_string", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let delimiter: String = ctx.get(1)?;
            crate::array::array_to_string_fn(&arr, &delimiter, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_to_string", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let delimiter: String = ctx.get(1)?;
            let null_string: String = ctx.get(2)?;
            crate::array::array_to_string_fn(&arr, &delimiter, Some(&null_string))
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("string_to_array", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let text: String = ctx.get(0)?;
            let delimiter: String = ctx.get(1)?;
            crate::array::string_to_array_fn(&text, &delimiter, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("string_to_array", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let text: String = ctx.get(0)?;
            let delimiter: String = ctx.get(1)?;
            let null_string: String = ctx.get(2)?;
            crate::array::string_to_array_fn(&text, &delimiter, Some(&null_string))
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_fill", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value = match ctx.get_raw(0) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            let dimensions: String = ctx.get(1)?;
            crate::array::array_fill_fn(&value, &dimensions, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_fill", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value = match ctx.get_raw(0) {
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                rusqlite::types::ValueRef::Text(s) => std::str::from_utf8(s).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Blob(b) => std::str::from_utf8(b).unwrap_or("").to_string(),
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
            };
            let dimensions: String = ctx.get(1)?;
            let lower_bounds: String = ctx.get(2)?;
            crate::array::array_fill_fn(&value, &dimensions, Some(&lower_bounds))
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("trim_array", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let n = match ctx.get_raw(1) {
                rusqlite::types::ValueRef::Integer(i) => i as i32,
                _ => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Invalid parameter type")))),
            };
            crate::array::trim_array_fn(&arr, n)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Array Comparison Functions
        conn.create_scalar_function("array_eq", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_eq(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_ne", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_ne(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_lt", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_lt(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_gt", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_gt(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_le", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_le(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_ge", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::array_ge(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // ANY/ALL support
        conn.create_scalar_function("array_any_eq", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: String = ctx.get(0)?;
            let arr: String = ctx.get(1)?;
            crate::array::array_any_eq(&value, &arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_all_eq", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: String = ctx.get(0)?;
            let arr: String = ctx.get(1)?;
            crate::array::array_all_eq(&value, &arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Range Types Operators
        conn.create_scalar_function("range_contains", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;

            // Handle different parameter types for index 1
            let right = if let Ok(s) = ctx.get::<String>(1) {
                s
            } else {
                // Try integer/real without using .get() which is strict
                let val = ctx.get_raw(1);
                match val {
                    rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                    rusqlite::types::ValueRef::Real(f) => f.to_string(),
                    _ => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Invalid parameter type")))),
                }
            };

            // Try as range @> element first, then range @> range
            match crate::range::range_contains_elem(&left, &right, crate::range::RangeType::Int4) {
                Ok(res) => Ok(res),
                Err(_) => crate::range::range_contains(&left, &right, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            }
        })?;

        conn.create_scalar_function("range_contained", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::range::range_contained(&left, &right, crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("range_overlaps", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::range::range_overlaps(&left, &right, crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("range_left", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::range::range_left(&left, &right, crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("range_right", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::range::range_right(&left, &right, crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("range_adjacent", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::range::range_adjacent(&left, &right, crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Range Metadata Functions        
        conn.create_scalar_function("lower", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Only apply range logic if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if is_range_format {
                crate::range::lower(&r, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            } else {
                // Not a range, use standard string lowercase
                Ok(Some(r.to_lowercase()))
            }
        })?;

        conn.create_scalar_function("upper", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Only apply range logic if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if is_range_format {
                crate::range::upper(&r, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            } else {
                // Not a range, use standard string uppercase
                Ok(Some(r.to_uppercase()))
            }
        })?;

        conn.create_scalar_function("lower_inc", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Only apply range logic if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if is_range_format {
                crate::range::lower_inc(&r, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            } else {
                // Not a range, return false
                Ok(false)
            }
        })?;

        conn.create_scalar_function("upper_inc", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Only apply range logic if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if is_range_format {
                crate::range::upper_inc(&r, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            } else {
                // Not a range, return false
                Ok(false)
            }
        })?;

        conn.create_scalar_function("lower_inf", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Only apply range logic if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if is_range_format {
                crate::range::lower_inf(&r, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            } else {
                // Not a range, return false
                Ok(false)
            }
        })?;

        conn.create_scalar_function("upper_inf", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Only apply range logic if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if is_range_format {
                crate::range::upper_inf(&r, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            } else {
                // Not a range, return false
                Ok(false)
            }
        })?;

        conn.create_scalar_function("isempty", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Only apply range logic if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if is_range_format {
                crate::range::isempty(&r, crate::range::RangeType::Int4)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
            } else {
                // Not a range, return false
                Ok(false)
            }
        })?;

        conn.create_scalar_function("range_canonicalize", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let r: String = ctx.get(0)?;
            // Default to Int4 for now, as strings don't carry type info
            
            // Only canonicalize if input looks like a PostgreSQL range format
            let trimmed = r.trim();
            let is_range_format = (trimmed.starts_with('[') || trimmed.starts_with('(')) 
                && (trimmed.ends_with(']') || trimmed.ends_with(')'));
            
            if !is_range_format {
                return Ok(r);
            }
            
            let rv = crate::range::parse_range(&r, crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        // Range Constructor Functions
        conn.create_scalar_function("int4range", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low = ctx.get_raw(0).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(0).unwrap().to_string());
            let high = ctx.get_raw(1).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(1).unwrap().to_string());
            let rv = crate::range::parse_range(&format!("[{},{})", low, high), crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        conn.create_scalar_function("int4range", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low = ctx.get_raw(0).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(0).unwrap().to_string());
            let high = ctx.get_raw(1).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(1).unwrap().to_string());
            let bounds: String = ctx.get(2)?;
            let rv = crate::range::parse_range(&format!("{}{},{}{}", &bounds[0..1], low, high, &bounds[1..2]), crate::range::RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        conn.create_scalar_function("daterange", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low: String = ctx.get(0)?;
            let high: String = ctx.get(1)?;
            let rv = crate::range::parse_range(&format!("[{},{})", low, high), crate::range::RangeType::Date)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        conn.create_scalar_function("daterange", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low: String = ctx.get(0)?;
            let high: String = ctx.get(1)?;
            let bounds: String = ctx.get(2)?;
            let rv = crate::range::parse_range(&format!("{}{},{}{}", &bounds[0..1], low, high, &bounds[1..2]), crate::range::RangeType::Date)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

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

    /// Check if the current user has permission to execute the query
    fn check_permissions(&self, referenced_tables: &[String], operation_type: crate::transpiler::OperationType) -> Result<bool> {
        // Get current user from session
        let session = self.sessions.get(&0).unwrap_or_else(|| {
            self.sessions.insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(),
            });
            self.sessions.get(&0).unwrap()
        });
        let current_user = session.current_user.clone();

        // Get the connection to query RBAC tables
        let conn = self.conn.lock().unwrap();

        // Check if user is superuser
        let is_superuser: bool = conn.query_row(
            "SELECT rolsuper FROM __pg_authid__ WHERE rolname = ?1",
            &[&current_user],
            |row| row.get(0),
        ).unwrap_or(false);

        if is_superuser {
            return Ok(true);
        }

        // If user doesn't exist in __pg_authid__, create them as a superuser
        // This allows any connecting user to have full access (development mode)
        let user_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM __pg_authid__ WHERE rolname = ?1)",
            &[&current_user],
            |row| row.get(0),
        ).unwrap_or(false);

        if !user_exists {
            // Auto-create unknown users as superusers
            conn.execute(
                "INSERT INTO __pg_authid__ (rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin) VALUES (?1, 1, 1, 1, 1, 1)",
                &[&current_user],
            )?;
            return Ok(true);
        }

        // Get effective roles (including inherited) using prepare and query_map
        let mut stmt = conn.prepare("
            WITH RECURSIVE effective_roles AS (
                SELECT oid FROM __pg_authid__ WHERE rolname = ?1
                UNION
                SELECT m.roleid FROM __pg_auth_members__ m
                JOIN effective_roles er ON er.oid = m.member
             )
             SELECT oid FROM effective_roles
        ").unwrap();
        let effective_roles: Vec<i64> = stmt.query_map([&current_user], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        if effective_roles.is_empty() {
            return Ok(false);
        }

        // Map operation to privilege
        let required_privilege = match operation_type {
            crate::transpiler::OperationType::SELECT => "SELECT",
            crate::transpiler::OperationType::INSERT => "INSERT",
            crate::transpiler::OperationType::UPDATE => "UPDATE",
            crate::transpiler::OperationType::DELETE => "DELETE",
            _ => return Ok(true), // DDL and other operations are allowed for now
        };

        // Check permissions for each table
        for table_name in referenced_tables {
            let has_privilege: bool = conn.query_row(
                "SELECT EXISTS (
                    SELECT 1 FROM __pg_acl__ a
                    JOIN pg_class c ON c.oid = a.object_id AND c.relname = ?1
                    WHERE a.privilege = ?2
                    AND (
                        a.grantee_id IN (SELECT oid FROM __pg_authid__)
                        OR a.grantee_id = 0
                    )
                )",
                &[table_name, required_privilege],
                |row| row.get(0),
            ).unwrap_or(false);

            if !has_privilege {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Apply RLS (Row-Level Security) to a transpiled query
    fn apply_rls_to_query(&self, sql: String, operation_type: crate::transpiler::OperationType, tables: &[String]) -> String {
        use crate::rls_inject::{inject_rls_into_select_sql, inject_rls_into_update_sql, inject_rls_into_delete_sql};

        // Get current user from session
        let session = self.sessions.get(&0);
        let current_user = session.map(|s| s.current_user.clone()).unwrap_or_else(|| "postgres".to_string());

        let conn = self.conn.lock().unwrap();

        // Check each table for RLS
        for table_name in tables {
            // Check if RLS is enabled for this table
            if let Ok(true) = crate::catalog::is_rls_enabled(&conn, table_name) {
                // Check if user can bypass RLS
                let can_bypass = conn.query_row(
                    "SELECT rls_forced FROM __pg_rls_enabled__ WHERE relname = ?1",
                    [table_name],
                    |row| {
                        let forced: bool = row.get(0)?;
                        if forced {
                            return Ok(false); // Cannot bypass if forced
                        }
                        // Check if user is table owner
                        let owner_oid: Result<i64, _> = conn.query_row(
                            "SELECT relowner FROM __pg_relation_meta__ WHERE relname = ?1",
                            [table_name],
                            |row| row.get(0),
                        );
                        if let Ok(owner) = owner_oid {
                            let user_oid: Result<i64, _> = conn.query_row(
                                "SELECT oid FROM __pg_authid__ WHERE rolname = ?1",
                                [&current_user],
                                |row| row.get(0),
                            );
                            if let Ok(user) = user_oid {
                                if owner == user {
                                    return Ok(true); // Owner can bypass
                                }
                            }
                        }
                        // Check if user is superuser
                        let is_superuser: bool = conn.query_row(
                            "SELECT rolsuper FROM __pg_authid__ WHERE rolname = ?1",
                            [&current_user],
                            |row| row.get(0),
                        ).unwrap_or(false);
                        Ok(is_superuser)
                    },
                );

                if can_bypass.unwrap_or(false) {
                    continue; // Skip RLS for this table
                }

                // Get applicable RLS policies for this table and operation
                let command = match operation_type {
                    crate::transpiler::OperationType::SELECT => "SELECT",
                    crate::transpiler::OperationType::INSERT => "INSERT",
                    crate::transpiler::OperationType::UPDATE => "UPDATE",
                    crate::transpiler::OperationType::DELETE => "DELETE",
                    _ => continue,
                };

                if let Ok(policies) = crate::catalog::get_applicable_policies(&conn, table_name, command, &["PUBLIC".to_string(), current_user.clone()]) {
                    if !policies.is_empty() {
                        // Build RLS expression from policies
                        let rls_expr = crate::rls::build_rls_expression(&policies, true);
                        if let Some(expr) = rls_expr {
                            // Rewrite current_user in expression
                            let rewritten_expr = crate::rls_inject::rewrite_rls_expression(&expr, &current_user, &current_user);

                            // Inject RLS into the query based on operation type
                            return match operation_type {
                                crate::transpiler::OperationType::SELECT => inject_rls_into_select_sql(&sql, &rewritten_expr),
                                crate::transpiler::OperationType::UPDATE => inject_rls_into_update_sql(&sql, &rewritten_expr),
                                crate::transpiler::OperationType::DELETE => inject_rls_into_delete_sql(&sql, &rewritten_expr),
                                _ => sql,
                            };
                        }
                    }
                }
            }
        }

        sql
    }

    /// Handle CREATE SCHEMA statement
    fn handle_create_schema(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse schema name from CREATE SCHEMA [IF NOT EXISTS] name [AUTHORIZATION user]
        let upper_sql = sql.to_uppercase();
        let if_not_exists = upper_sql.contains("IF NOT EXISTS");

        // Extract schema name (simplified parsing)
        let schema_name = sql
            .to_lowercase()
            .split_whitespace()
            .skip_while(|w| *w == "create" || *w == "schema" || *w == "if" || *w == "not" || *w == "exists")
            .next()
            .map(|s| {
                let s = s.trim_matches('"');
                let s = s.trim_end_matches(|c| c == ';' || c == '"');
                s.to_string()
            })
            .ok_or_else(|| anyhow::anyhow!("invalid CREATE SCHEMA syntax"))?;

        // Check for reserved names
        if schema_name.starts_with("pg_") {
            return Err(anyhow::anyhow!(
                "unacceptable schema name \"{}\": system schemas must start with pg_",
                schema_name
            ));
        }

        let conn = self.conn.lock().unwrap();

        // Check if schema already exists
        if crate::schema::schema_exists(&conn, &schema_name)? {
            if if_not_exists {
                return Ok(vec![Response::Execution(Tag::new("CREATE SCHEMA"))]);
            }
            return Err(anyhow::anyhow!("schema \"{}\" already exists", schema_name));
        }

        // Create schema in catalog
        crate::schema::create_schema(&conn, &schema_name, None)?;

        // Attach the schema database
        self.schema_manager.attach_schema(&conn, &schema_name)?;

        Ok(vec![Response::Execution(Tag::new("CREATE SCHEMA"))])
    }

    /// Handle DROP SCHEMA statement
    fn handle_drop_schema(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse schema name from DROP SCHEMA [IF EXISTS] name [CASCADE | RESTRICT]
        let upper_sql = sql.to_uppercase();
        let if_exists = upper_sql.contains("IF EXISTS");
        let cascade = upper_sql.ends_with("CASCADE") || (!upper_sql.ends_with("RESTRICT") && upper_sql.contains(" CASCADE"));

        // Extract schema name
        let schema_name = sql
            .to_lowercase()
            .split_whitespace()
            .skip_while(|w| *w == "drop" || *w == "schema" || *w == "if" || *w == "exists")
            .next()
            .map(|s| {
                let s = s.trim_matches('"');
                let s = s.trim_end_matches(|c| c == ';' || c == '"');
                s.to_string()
            })
            .ok_or_else(|| anyhow::anyhow!("invalid DROP SCHEMA syntax"))?;

        // Cannot drop system schemas
        if schema_name == "public" {
            return Err(anyhow::anyhow!("cannot drop schema \"public\""));
        }
        if schema_name == "pg_catalog" || schema_name == "information_schema" {
            return Err(anyhow::anyhow!("cannot drop system schema \"{}\"", schema_name));
        }

        let conn = self.conn.lock().unwrap();

        // Check if schema exists
        if !crate::schema::schema_exists(&conn, &schema_name)? {
            if if_exists {
                return Ok(vec![Response::Execution(Tag::new("DROP SCHEMA"))]);
            }
            return Err(anyhow::anyhow!("schema \"{}\" does not exist", schema_name));
        }

        // Check if schema is empty (unless CASCADE)
        if !cascade && !crate::schema::schema_is_empty(&conn, &schema_name, &self.schema_manager)? {
            return Err(anyhow::anyhow!(
                "schema \"{}\" cannot be dropped without CASCADE because it contains objects",
                schema_name
            ));
        }

        // Drop all objects in the schema (if CASCADE)
        if cascade {
            crate::schema::drop_schema_objects(&conn, &schema_name, &self.schema_manager)?;
        }

        // Detach the schema database
        self.schema_manager.detach_schema(&conn, &schema_name)?;

        // Delete the schema database file
        self.schema_manager.delete_schema_db(&schema_name)?;

        // Remove schema from catalog
        crate::schema::drop_schema(&conn, &schema_name)?;

        Ok(vec![Response::Execution(Tag::new("DROP SCHEMA"))])
    }

    /// Handle SET search_path statement
    fn handle_set_search_path(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse search_path from SET search_path TO value or SET search_path = value
        let path_str = sql
            .to_lowercase()
            .split_whitespace()
            .skip_while(|w| *w != "to" && *w != "=")
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        let search_path = crate::schema::SearchPath::parse(&path_str)?;

        // Update session context
        let mut session = self.sessions.get_mut(&0).unwrap_or_else(|| {
            self.sessions.insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(),
            });
            self.sessions.get_mut(&0).unwrap()
        });
        session.search_path = search_path;

        Ok(vec![Response::Execution(Tag::new("SET"))])
    }

    /// Handle SHOW search_path statement
    fn handle_show_search_path(&self) -> Result<Vec<Response>> {
        let session = self.sessions.get(&0).unwrap_or_else(|| {
            self.sessions.insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(),
            });
            self.sessions.get(&0).unwrap()
        });

        let path = session.search_path.to_string();

        let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
            "search_path".to_string(),
            None,
            None,
            Type::TEXT,
            FieldFormat::Text,
        )]);

        let mut encoder = DataRowEncoder::new(fields.clone());
        encoder.encode_field(&Some(path))?;
        let data_rows = vec![Ok(encoder.take_row())];

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(
            fields,
            row_stream,
        ))])
    }

    /// Handle CREATE FUNCTION statement
    fn handle_create_function(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse CREATE FUNCTION
        let metadata = crate::transpiler::parse_create_function(sql)?;

        // Store in catalog
        let conn = self.conn.lock().unwrap();
        crate::catalog::store_function(&conn, &metadata)?;

        // Also register as a SQLite custom function for runtime interception
        self.register_sqlite_function(&conn, &metadata)?;

        Ok(vec![Response::Execution(Tag::new("CREATE FUNCTION"))])
    }

    /// Register a user-defined function as a SQLite custom function
    fn register_sqlite_function(&self, conn: &Connection, metadata: &crate::catalog::FunctionMetadata) -> Result<()> {
        use rusqlite::functions::FunctionFlags;
        use crate::catalog::ReturnTypeKind;
        use rusqlite::types::Value;

        // Determine the number of parameters (excluding OUT params for now)
        let num_params = metadata.arg_types.len();

        // Store metadata in the in-memory cache for fast lookup
        self.functions.insert(metadata.name.clone(), metadata.clone());

        // Create references for the closure
        let func_name = metadata.name.clone();
        let func_name_for_closure = func_name.clone(); // Clone for the closure
        let arg_count = num_params;
        let is_strict = metadata.strict;
        let return_type_kind = metadata.return_type_kind.clone();
        let functions_cache = self.functions.clone();

        // Register as a scalar function
        conn.create_scalar_function(
            func_name.as_str(),
            arg_count as i32,
            FunctionFlags::SQLITE_UTF8,
            move |ctx| {
                // Collect arguments
                let mut args = Vec::new();
                for i in 0..arg_count {
                    let arg = ctx.get::<Value>(i)?;
                    args.push(arg);
                }

                // If STRICT and any NULL args, return NULL
                if is_strict && args.iter().any(|v| matches!(v, Value::Null)) {
                    return Ok(Value::Null);
                }

                // Look up function metadata from cache
                let _func_metadata = match functions_cache.get(&func_name_for_closure) {
                    Some(metadata) => metadata.clone(),
                    None => return Ok(Value::Null), // Function not found
                };

                // Only support scalar functions for now
                if return_type_kind != ReturnTypeKind::Scalar {
                    // For non-scalar functions, return NULL (not yet supported)
                    return Ok(Value::Null);
                }

                // For now, return NULL to indicate not fully implemented
                // The AST-based interception will handle simple cases
                Ok(Value::Null)
            }
        )?;

        Ok(())
    }

    /// Try to execute a simple function call like SELECT func(arg1, arg2)
    /// Returns Ok(response) if it was a simple function call, Err if not
    fn try_execute_simple_function_call(&self, sql: &str) -> Result<Vec<Response>> {
        use pg_query::protobuf::node::Node as NodeEnum;


        // Parse the SQL
        let result = pg_query::parse(sql)?;

        // Check if this is a simple SELECT with a function call
        if let Some(raw_stmt) = result.protobuf.stmts.first() {
            if let Some(ref stmt_node) = raw_stmt.stmt {
                if let Some(NodeEnum::SelectStmt(ref select_stmt)) = &stmt_node.node {
                    // Check if the SELECT list has exactly one item and it's a function call
                    if select_stmt.target_list.len() == 1 {
                        if let Some(NodeEnum::ResTarget(ref target)) = &select_stmt.target_list[0].node {
                            if let Some(ref val_node) = target.val {
                                if let Some(NodeEnum::FuncCall(ref func_call)) = &val_node.node {
                                    // This is a simple function call! Execute it.
                                    return self.execute_function_call(func_call, sql);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Not a simple function call
        anyhow::bail!("Not a simple function call")
    }

    /// Execute a function call
    fn execute_function_call(&self, func_call: &pg_query::protobuf::FuncCall, _original_sql: &str) -> Result<Vec<Response>> {
        use pg_query::protobuf::node::Node as NodeEnum;
        use rusqlite::types::Value;


        // Extract function name
        let func_name = func_call
            .funcname
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::String(s) = inner {
                        return Some(s.sval.to_lowercase());
                    }
                }
                None
            })
            .last()
            .ok_or_else(|| anyhow::anyhow!("Could not extract function name"))?;


        // Get connection and look up function
        let conn = self.conn.lock().unwrap();
        let metadata = crate::catalog::get_function(&conn, &func_name, None)?
            .ok_or_else(|| anyhow::anyhow!("Function {} not found", func_name))?;


        // Extract arguments (only handle simple literals for now)
        let mut args = Vec::new();
        for (_i, arg_node) in func_call.args.iter().enumerate() {
            if let Some(ref inner) = arg_node.node {
                match inner {
                    NodeEnum::AConst(ref aconst) => {
                        // Handle literal values
                        if let Some(ref val) = aconst.val {
                            match val {
                                pg_query::protobuf::a_const::Val::Ival(iv) => {
                                    args.push(Value::Integer(iv.ival as i64));
                                }
                                pg_query::protobuf::a_const::Val::Fval(fv) => {
                                    // fval is a string representation of the float
                                    let parsed = fv.fval.parse::<f64>().unwrap_or(0.0);
                                    args.push(Value::Real(parsed));
                                }
                                pg_query::protobuf::a_const::Val::Sval(sv) => {
                                    args.push(Value::Text(sv.sval.clone()));
                                }
                                _ => {
                                    // Unsupported argument type
                                    anyhow::bail!("Unsupported argument type in function call");
                                }
                            }
                        }
                    }
                    _ => {
                        // Non-literal argument (column ref, etc.) - not supported yet
                        anyhow::bail!("Only literal arguments supported in function calls");
                    }
                }
            }
        }


        // Execute the function
        let result = crate::functions::execute_sql_function(&conn, &metadata, &args)?;


        // Convert result to Response
        self.convert_function_result_to_response(result)
    }

    /// Convert function execution result to pgwire Response
    fn convert_function_result_to_response(&self, result: crate::functions::FunctionResult) -> Result<Vec<Response>> {
        use crate::functions::FunctionResult;
        use pgwire::api::results::{DataRowEncoder, FieldInfo, QueryResponse, Response, Tag};
        use std::sync::Arc;
        use rusqlite::types::Value;

        match result {
            FunctionResult::Scalar(Some(value)) => {
                // Return single value
                let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
                    "result".to_string(),
                    None,
                    None,
                    pgwire::api::Type::UNKNOWN,
                    pgwire::api::results::FieldFormat::Text,
                )]);

                let mut encoder = DataRowEncoder::new(fields.clone());

                // Convert Value to string properly
                let value_str = match value {
                    Value::Null => None,
                    Value::Integer(i) => Some(i.to_string()),
                    Value::Real(f) => Some(f.to_string()),
                    Value::Text(s) => Some(s),
                    Value::Blob(b) => Some(String::from_utf8_lossy(&b).to_string()),
                };

                encoder.encode_field(&value_str)?;
                let data_rows = vec![Ok(encoder.take_row())];

                Ok(vec![Response::Query(QueryResponse::new(
                    fields,
                    futures::stream::iter(data_rows),
                ))])
            }
            FunctionResult::Scalar(None) | FunctionResult::Null | FunctionResult::Void => {
                // Return NULL for scalar None/Null or Void
                let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
                    "result".to_string(),
                    None,
                    None,
                    pgwire::api::Type::UNKNOWN,
                    pgwire::api::results::FieldFormat::Text,
                )]);

                let mut encoder = DataRowEncoder::new(fields.clone());
                encoder.encode_field(&None::<String>)?;
                let data_rows = vec![Ok(encoder.take_row())];

                Ok(vec![Response::Query(QueryResponse::new(
                    fields,
                    futures::stream::iter(data_rows),
                ))])
            }
            FunctionResult::SetOf(values) => {
                // Return multiple rows, each with one column
                let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
                    "result".to_string(),
                    None,
                    None,
                    pgwire::api::Type::UNKNOWN,
                    pgwire::api::results::FieldFormat::Text,
                )]);

                let mut data_rows = Vec::new();
                for value in values {
                    let mut encoder = DataRowEncoder::new(fields.clone());
                    let value_str = match value {
                        Value::Null => None,
                        Value::Integer(i) => Some(i.to_string()),
                        Value::Real(f) => Some(f.to_string()),
                        Value::Text(s) => Some(s),
                        Value::Blob(b) => Some(String::from_utf8_lossy(&b).to_string()),
                    };
                    encoder.encode_field(&value_str)?;
                    data_rows.push(Ok(encoder.take_row()));
                }

                Ok(vec![Response::Query(QueryResponse::new(
                    fields,
                    futures::stream::iter(data_rows),
                ))])
            }
            FunctionResult::Table(rows) => {
                // Return multiple rows with multiple columns
                if rows.is_empty() {
                    return Ok(vec![Response::Execution(Tag::new("SELECT 0"))]);
                }

                let column_count = rows[0].len();
                let mut fields_vec = Vec::new();
                for i in 0..column_count {
                    fields_vec.push(FieldInfo::new(
                        format!("col_{}", i),
                        None,
                        None,
                        pgwire::api::Type::UNKNOWN,
                        pgwire::api::results::FieldFormat::Text,
                    ));
                }
                let fields = Arc::new(fields_vec);

                let mut data_rows = Vec::new();
                for row in rows {
                    let mut encoder = DataRowEncoder::new(fields.clone());
                    for value in row {
                        let value_str = match value {
                            Value::Null => None,
                            Value::Integer(i) => Some(i.to_string()),
                            Value::Real(f) => Some(f.to_string()),
                            Value::Text(s) => Some(s),
                            Value::Blob(b) => Some(String::from_utf8_lossy(&b).to_string()),
                        };
                        encoder.encode_field(&value_str)?;
                    }
                    data_rows.push(Ok(encoder.take_row()));
                }

                Ok(vec![Response::Query(QueryResponse::new(
                    fields,
                    futures::stream::iter(data_rows),
                ))])
            }
        }
    }

    /// Handle DROP FUNCTION statement
    fn handle_drop_function(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse function name from DROP FUNCTION
        // For now, simple parsing - extract name between "DROP FUNCTION" and "(" or end
        let upper_sql = sql.trim().to_uppercase();
        let name_part = upper_sql.trim_start_matches("DROP FUNCTION").trim();
        let name = name_part.split_whitespace().next().unwrap_or("");
        let name = name.trim_start_matches("IF EXISTS").trim();
        let name = name.split('(').next().unwrap_or(name).trim();

        // Remove from catalog
        let conn = self.conn.lock().unwrap();
        crate::catalog::drop_function(&conn, name, None)?;

        Ok(vec![Response::Execution(Tag::new("DROP FUNCTION"))])
    }

    /// Handle COPY statement
    fn handle_copy_statement(&self,
        copy_stmt: crate::copy::CopyStatement,
    ) -> Result<Vec<Response>> {
        use crate::copy::{CopyDirection};

        match copy_stmt.direction {
            CopyDirection::From => {
                // COPY FROM STDIN - start the COPY operation
                let table_name = copy_stmt.table_name.ok_or_else(|| anyhow!("COPY FROM requires table name"))?;
                let options = copy_stmt.options;

                // Return CopyInResponse to start the COPY protocol
                let response = self.copy_handler.start_copy_from(
                    table_name,
                    copy_stmt.columns,
                    options,
                )?;
                Ok(vec![response])
            }
            CopyDirection::To => {
                // COPY TO STDOUT
                let query = if let Some(q) = copy_stmt.query {
                    q
                } else if let Some(t) = copy_stmt.table_name {
                    format!("SELECT * FROM {}", t)
                } else {
                    return Err(anyhow!("COPY TO requires table name or query"));
                };

                self.copy_handler.start_copy_to(
                    query,
                    copy_stmt.options,
                ).map(|r| vec![r])
            }
        }
    }

    /// Execute a SQL query and return the results
    pub fn execute_query(&self, sql: &str) -> Result<Vec<Response>> {
        let upper_sql = sql.trim().to_uppercase();

        // Ignore transaction control statements - SQLite handles transactions automatically
        if upper_sql == "BEGIN" || upper_sql == "COMMIT" || upper_sql == "ROLLBACK" {
            return Ok(vec![Response::Execution(Tag::new("OK"))]);
        }

        // Handle CREATE SCHEMA
        if upper_sql.starts_with("CREATE SCHEMA") {
            return self.handle_create_schema(sql);
        }

        // Handle DROP SCHEMA
        if upper_sql.starts_with("DROP SCHEMA") {
            return self.handle_drop_schema(sql);
        }

        // Handle SET search_path
        if upper_sql.starts_with("SET SEARCH_PATH") {
            return self.handle_set_search_path(sql);
        }

        // Handle SHOW search_path
        if upper_sql == "SHOW SEARCH_PATH" {
            return self.handle_show_search_path();
        }

        // Handle CREATE FUNCTION
        if upper_sql.starts_with("CREATE FUNCTION") || upper_sql.starts_with("CREATE OR REPLACE FUNCTION") {
            return self.handle_create_function(sql);
        }

        // Handle DROP FUNCTION
        if upper_sql.starts_with("DROP FUNCTION") {
            return self.handle_drop_function(sql);
        }

        // Try to handle simple function calls like SELECT func(arg1, arg2)
        // This intercepts user-defined function calls before normal execution
        match self.try_execute_simple_function_call(sql) {
            Ok(result) => {
                return Ok(result);
            }
            Err(_) => {
                // Fall through to normal transpilation
            }
        }

        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions.clone());
        let transpile_result = crate::transpiler::transpile_with_context(sql, &mut ctx);

        // Handle COPY statements
        if let Some(copy_stmt) = transpile_result.copy_metadata {
            return self.handle_copy_statement(copy_stmt);
        }

        // Handle SET ROLE specially
        if transpile_result.sql.starts_with("-- SET ROLE") {
            let role_name = transpile_result.sql.trim_start_matches("-- SET ROLE ").trim();
            if role_name != "NONE" {
                // Update session context
                let mut session = self.sessions.get_mut(&0).unwrap_or_else(|| {
                    self.sessions.insert(0, SessionContext {
                        authenticated_user: "postgres".to_string(),
                        current_user: "postgres".to_string(),
                        search_path: SearchPath::default(),
                    });
                    self.sessions.get_mut(&0).unwrap()
                });
                session.current_user = role_name.to_string();
            }
            // Return success without executing
            return Ok(vec![Response::Execution(Tag::new("SET"))]);
        }

        // GLOBAL PATCH FOR RANGES

        // Check permissions before executing
        if !self.check_permissions(&transpile_result.referenced_tables, transpile_result.operation_type)? {
            return Err(anyhow::anyhow!("permission denied for table(s)"));
        }

        // Apply RLS (Row-Level Security) to the query
        let sqlite_sql = self.apply_rls_to_query(transpile_result.sql, transpile_result.operation_type, &transpile_result.referenced_tables);

        let conn = self.conn.lock().unwrap();

        let is_select = sqlite_sql.trim().to_lowercase().starts_with("select");
        let is_create_table = sqlite_sql.trim().to_uppercase().starts_with("CREATE TABLE");

        if is_create_table {
            // For CREATE TABLE, we need to execute the DDL first, then store metadata
            // This avoids the "cannot start a transaction within a transaction" error
            // because SQLite starts an implicit transaction for CREATE TABLE
            let result = self.execute_statement(&conn, &sqlite_sql)?;

            // Store metadata after CREATE TABLE completes
            if let Some(metadata) = transpile_result.create_table_metadata {
                let columns: Vec<(String, String, Option<String>)> = metadata
                    .columns
                    .into_iter()
                    .map(|c| (c.column_name, c.original_type, c.constraints))
                    .collect();

                store_table_metadata(&conn, &metadata.table_name, &columns)?;

                // Store ownership (use current user as owner)
                let session = self.sessions.get(&0).unwrap_or_else(|| {
                    self.sessions.insert(0, SessionContext {
                        authenticated_user: "postgres".to_string(),
                        current_user: "postgres".to_string(),
                        search_path: SearchPath::default(),
                    });
                    self.sessions.get(&0).unwrap()
                });
                // Find owner OID from __pg_authid__
                let owner_oid: i64 = conn.query_row(
                    "SELECT oid FROM __pg_authid__ WHERE rolname = ?1",
                    &[&session.current_user],
                    |row| row.get(0),
                ).unwrap_or(10); // Default to postgres (OID 10)

                store_relation_metadata(&conn, &metadata.table_name, owner_oid)?;

                // Populate pg_catalog tables for ORM compatibility
                crate::catalog::populate_pg_attribute(&conn, &metadata.table_name)?;
                crate::catalog::populate_pg_index(&conn)?;
                crate::catalog::populate_pg_constraint(&conn)?;
            }

            Ok(result)
        } else if is_select {
            self.execute_select(&conn, &sqlite_sql)
        } else {
            // Handle multiple statements (e.g., from TRUNCATE or DROP with multiple tables)
            let statements: Vec<&str> = sqlite_sql.split("; ").collect();
            if statements.len() > 1 {
                let mut all_responses = Vec::new();
                for stmt in statements {
                    let stmt = stmt.trim();
                    if !stmt.is_empty() {
                        let responses = self.execute_statement(&conn, stmt)?;
                        all_responses.extend(responses);
                    }
                }
                Ok(all_responses)
            } else {
                self.execute_statement(&conn, &sqlite_sql)
            }
        }
    }

    fn execute_select(&self, conn: &Connection, sql: &str) -> Result<Vec<Response>> {
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();

        let fields: Arc<Vec<FieldInfo>> = Arc::new(
            (0..col_count)
                .map(|i| {
                    let col_name = stmt.column_name(i).unwrap_or("?column?").to_string();

                    FieldInfo::new(col_name, None, None, Type::TEXT, FieldFormat::Text)
                })
                .collect(),
        );

        let mut data_rows = Vec::new();
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let mut encoder = DataRowEncoder::new(fields.clone());

            for i in 0..col_count {
                // Try to get value as different types and convert to string
                let value: Option<String> = row.get::<_, Option<i64>>(i).ok()
                    .map(|v| v.map(|x| x.to_string()))
                    .or_else(|| row.get::<_, Option<f64>>(i).ok()
                        .map(|v| v.map(|x| x.to_string())))
                    .or_else(|| row.get::<_, Option<String>>(i).ok())
                    .flatten();
                match value {
                    Some(v) => encoder.encode_field(&Some(v))?,
                    None => encoder.encode_field(&None::<String>)?,
                }
            }

            data_rows.push(Ok(encoder.take_row()));
        }

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(
            fields,
            row_stream,
        ))])
    }

    fn execute_statement(&self, conn: &Connection, sql: &str) -> Result<Vec<Response>> {
        println!("Executing statement: {}", sql);

        // Split multiple statements and execute them sequentially
        let statements: Vec<&str> = sql.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        let mut total_changes = 0;

        for stmt in statements {
            total_changes += conn.execute(stmt, [])?;
        }

        let upper_sql = sql.trim().to_uppercase();
        let tag = if upper_sql.starts_with("CREATE TABLE") {
            Tag::new("CREATE TABLE")
        } else if upper_sql.starts_with("INSERT") {
            // PostgreSQL format: INSERT oid rows
            // oid is 0 for tables without OIDs (most modern tables)
            Tag::new("INSERT 0").with_rows(total_changes)
        } else if upper_sql.starts_with("UPDATE") {
            Tag::new("UPDATE").with_rows(total_changes)
        } else if upper_sql.starts_with("DELETE") {
            Tag::new("DELETE").with_rows(total_changes)
        } else {
            Tag::new("OK")
        };

        Ok(vec![Response::Execution(tag)])
    }
}
