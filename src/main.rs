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
mod transpiler;

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

        let sqlite_sql = &transpile_result.sql;

        let conn = self.conn.lock().unwrap();

        let is_select = sqlite_sql.trim().to_lowercase().starts_with("select");
        let is_create_table = sqlite_sql.trim().to_uppercase().starts_with("CREATE TABLE");

        if is_create_table {
            // For CREATE TABLE, we need to execute the DDL first, then store metadata
            // This avoids the "cannot start a transaction within a transaction" error
            // because SQLite starts an implicit transaction for CREATE TABLE
            let result = self.execute_statement(&conn, sqlite_sql)?;

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
            self.execute_select(&conn, sqlite_sql)
        } else {
            self.execute_statement(&conn, sqlite_sql)
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
                let value: Option<String> = row.get(i).ok();
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
    async fn do_query<'a, 'b: 'a, C>(&'b self, _client: &mut C, query: &'a str) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
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
