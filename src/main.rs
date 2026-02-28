use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use dashmap::DashMap;
use futures::stream;
use pgwire::api::auth::noop::NoopStartupHandler;
use pgwire::api::query::{PlaceholderExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::{ClientInfo, Type};
use pgwire::error::{ErrorInfo, PgWireResult};
use pgwire::tokio::process_socket;
use rusqlite::Connection;
use tokio::net::TcpListener;

mod catalog;
mod rls;
mod rls_inject;
mod transpiler;
mod fts;

/// PostgreSQL-to-SQLite proxy server
#[derive(Parser, Debug)]
#[command(name = "postgresqlite")]
#[command(about = "A PostgreSQL wire protocol proxy for SQLite")]
#[command(version)]
struct Cli {
    /// Host address to listen on
    #[arg(short = 'H', long, env = "PG_LITE_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(short, long, env = "PG_LITE_PORT", default_value = "5432")]
    port: u16,

    /// Path to the SQLite database file
    #[arg(short, long, env = "PG_LITE_DB", default_value = "test.db")]
    database: String,
}

use catalog::{init_catalog, init_system_views, store_table_metadata, store_relation_metadata};
use transpiler::transpile_with_metadata;

/// Session context for each client connection
#[derive(Debug, Clone)]
struct SessionContext {
    #[allow(dead_code)]
    authenticated_user: String,
    current_user: String,
}

/// PostgreSQL-to-SQLite proxy handler
struct SqliteHandler {
    conn: Arc<Mutex<Connection>>,
    sessions: Arc<DashMap<u32, SessionContext>>,
}

impl SqliteHandler {
    fn new(db_path: &str) -> Result<Self> {
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
            Ok("PostgreSQL 15.0 (PostgresLite)".to_string())
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

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            sessions: Arc::new(DashMap::new()),
        })
    }

    /// Check if the current user has permission to execute the query
    fn check_permissions(&self, referenced_tables: &[String], operation_type: transpiler::OperationType) -> Result<bool> {
        // Get current user from session
        let session = self.sessions.get(&0).unwrap_or_else(|| {
            self.sessions.insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
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
            transpiler::OperationType::SELECT => "SELECT",
            transpiler::OperationType::INSERT => "INSERT",
            transpiler::OperationType::UPDATE => "UPDATE",
            transpiler::OperationType::DELETE => "DELETE",
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
    fn apply_rls_to_query(&self, sql: String, operation_type: transpiler::OperationType, tables: &[String]) -> String {
        use rls_inject::{inject_rls_into_select_sql, inject_rls_into_update_sql, inject_rls_into_delete_sql};
        
        // Get current user from session
        let session = self.sessions.get(&0);
        let current_user = session.map(|s| s.current_user.clone()).unwrap_or_else(|| "postgres".to_string());
        
        let conn = self.conn.lock().unwrap();
        
        // Check each table for RLS
        for table_name in tables {
            // Check if RLS is enabled for this table
            if let Ok(true) = catalog::is_rls_enabled(&conn, table_name) {
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
                    transpiler::OperationType::SELECT => "SELECT",
                    transpiler::OperationType::INSERT => "INSERT",
                    transpiler::OperationType::UPDATE => "UPDATE",
                    transpiler::OperationType::DELETE => "DELETE",
                    _ => continue,
                };
                
                if let Ok(policies) = catalog::get_applicable_policies(&conn, table_name, command, &["PUBLIC".to_string(), current_user.clone()]) {
                    if !policies.is_empty() {
                        // Build RLS expression from policies
                        let rls_expr = rls::build_rls_expression(&policies, true);
                        if let Some(expr) = rls_expr {
                            // Rewrite current_user in expression
                            let rewritten_expr = rls_inject::rewrite_rls_expression(&expr, &current_user, &current_user);
                            
                            // Inject RLS into the query based on operation type
                            return match operation_type {
                                transpiler::OperationType::SELECT => inject_rls_into_select_sql(&sql, &rewritten_expr),
                                transpiler::OperationType::UPDATE => inject_rls_into_update_sql(&sql, &rewritten_expr),
                                transpiler::OperationType::DELETE => inject_rls_into_delete_sql(&sql, &rewritten_expr),
                                _ => sql,
                            };
                        }
                    }
                }
            }
        }
        
        sql
    }

    /// Execute a SQL query and return the results
    fn execute_query(&self, sql: &str) -> Result<Vec<Response<'_>>> {
        let transpile_result = transpile_with_metadata(sql);
        
        // Handle SET ROLE specially
        if transpile_result.sql.starts_with("-- SET ROLE") {
            let role_name = transpile_result.sql.trim_start_matches("-- SET ROLE ").trim();
            if role_name != "NONE" {
                // Update session context
                let mut session = self.sessions.get_mut(&0).unwrap_or_else(|| {
                    self.sessions.insert(0, SessionContext {
                        authenticated_user: "postgres".to_string(),
                        current_user: "postgres".to_string(),
                    });
                    self.sessions.get_mut(&0).unwrap()
                });
                session.current_user = role_name.to_string();
            }
            // Return success without executing
            return Ok(vec![Response::Execution(Tag::new("SET"))]);
        }
        
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
            }

            Ok(result)
        } else if is_select {
            self.execute_select(&conn, &sqlite_sql)
        } else {
            self.execute_statement(&conn, &sqlite_sql)
        }
    }

    fn execute_select(&self, conn: &Connection, sql: &str) -> Result<Vec<Response<'_>>> {
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
                    .or_else(|| row.get::<_, Option<String>>(i).ok())
                    .flatten();
                match value {
                    Some(v) => encoder.encode_field(&Some(v))?,
                    None => encoder.encode_field(&None::<String>)?,
                }
            }

            data_rows.push(encoder.finish());
        }

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(
            fields,
            row_stream,
        ))])
    }

    fn execute_statement(&self, conn: &Connection, sql: &str) -> Result<Vec<Response<'_>>> {
        println!("Executing statement: {}", sql);
        let changes = conn.execute(sql, [])?;

        let upper_sql = sql.trim().to_uppercase();
        let tag = if upper_sql.starts_with("CREATE TABLE") {
            Tag::new("CREATE TABLE")
        } else if upper_sql.starts_with("INSERT") {
            // PostgreSQL format: INSERT oid rows
            // oid is 0 for tables without OIDs (most modern tables)
            Tag::new("INSERT 0").with_rows(changes)
        } else if upper_sql.starts_with("UPDATE") {
            Tag::new("UPDATE").with_rows(changes)
        } else if upper_sql.starts_with("DELETE") {
            Tag::new("DELETE").with_rows(changes)
        } else {
            Tag::new("OK")
        };

        Ok(vec![Response::Execution(tag)])
    }
}

#[async_trait]
impl SimpleQueryHandler for SqliteHandler {
    async fn do_query<'a, 'b: 'a, C>(&'b self, client: &mut C, query: &'a str) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        // Initialize session from client metadata if not already set
        if self.sessions.is_empty() {
            let metadata = client.metadata();
            let user = metadata.get("user").map(|s| s.to_string()).unwrap_or_else(|| "postgres".to_string());
            self.sessions.insert(0, SessionContext {
                authenticated_user: user.clone(),
                current_user: user,
            });
        }
        
        println!("Received query: {}", query);
        match self.execute_query(query) {
            Ok(responses) => Ok(responses),
            Err(e) => {
                eprintln!("Error executing query: {}", e);
                Ok(vec![Response::Error(Box::new(ErrorInfo::new(
                    "ERROR".to_owned(),
                    "XX000".to_owned(),
                    e.to_string(),
                )))])
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let addr = format!("{}:{}", cli.host, cli.port);

    let listener = TcpListener::bind(&addr).await?;
    println!("Server listening on {}", addr);
    println!("Using database: {}", cli.database);

    let handler = Arc::new(SqliteHandler::new(&cli.database)?);
    let startup_handler = Arc::new(NoopStartupHandler);
    let extended_handler = Arc::new(PlaceholderExtendedQueryHandler);

    loop {
        let (incoming_socket, client_addr) = listener.accept().await?;
        println!("New connection from {}", client_addr);

        let handler = handler.clone();
        let startup_handler = startup_handler.clone();
        let extended_handler = extended_handler.clone();

        tokio::spawn(async move {
            let _ = process_socket(
                incoming_socket,
                None,
                startup_handler,
                handler,
                extended_handler,
            )
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db_path(name: &str) -> String {
        let temp_dir = std::env::temp_dir();
        temp_dir.join(name).to_str().unwrap().to_string()
    }

    fn cleanup_db(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_handler_initializes_catalog() {
        let db_path = temp_db_path("test_pg_lite.db");
        cleanup_db(&db_path);

        let handler = SqliteHandler::new(&db_path).unwrap();

        let conn = handler.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='__pg_meta__'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
        cleanup_db(&db_path);
    }

    #[test]
    fn test_create_table_stores_metadata() {
        let db_path = temp_db_path("test_pg_lite_meta.db");
        cleanup_db(&db_path);

        let handler = SqliteHandler::new(&db_path).unwrap();

        let _ = handler.execute_query("CREATE TABLE test_table (id SERIAL, name VARCHAR(10), created_at TIMESTAMP WITH TIME ZONE)");

        let conn = handler.conn.lock().unwrap();
        let metadata = catalog::get_table_metadata(&conn, "test_table").unwrap();

        assert_eq!(metadata.len(), 3);

        let types: Vec<String> = metadata.iter().map(|m| m.original_type.clone()).collect();
        assert!(types.contains(&"SERIAL".to_string()));
        assert!(types.contains(&"VARCHAR(10)".to_string()));
        assert!(types.contains(&"TIMESTAMP WITH TIME ZONE".to_string()));

        cleanup_db(&db_path);
    }
}
