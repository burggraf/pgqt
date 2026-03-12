//! Query execution module
//!
//! This module contains methods for executing SQL queries including:
//! - Main query execution dispatch
//! - SELECT statement execution
//! - DML statement execution (INSERT, UPDATE, DELETE)
//! - COPY statement handling

use std::sync::Arc;
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use futures::stream;

use crate::debug;
use crate::catalog::{store_table_metadata, store_relation_metadata};
use crate::schema::SearchPath;
use crate::handler::SessionContext;
use crate::handler::utils::HandlerUtils;
#[allow(unused_imports)]
use crate::transpiler::metadata::MetadataProvider;
use crate::copy;
use pgwire::api::results::{DataRowEncoder, FieldInfo, QueryResponse, Response, Tag};
use rusqlite::Statement;

/// Trait for query execution methods
pub trait QueryExecution: HandlerUtils + Clone {
    fn as_metadata_provider(&self) -> Arc<dyn crate::transpiler::metadata::MetadataProvider>;

    /// Execute a SQL query with optional parameters and return the results
    /// Uses per-session connection identified by client_id
    fn execute_query_params(&self, client_id: u32, sql: &str, params: &[Option<String>]) -> Result<Vec<Response>> {
        // Set the current user from the session for current_user() function
        if let Some(session) = self.sessions().get(&client_id) {
            crate::handler::set_current_user(&session.current_user);
        }

        // Check transaction error state before executing
        {
            let session = self.sessions().get(&client_id).unwrap_or_else(|| {
                self.sessions().insert(client_id, SessionContext {
                    authenticated_user: "postgres".to_string(),
                    current_user: "postgres".to_string(),
                    search_path: SearchPath::default(),
                    transaction_status: crate::handler::TransactionStatus::Idle,
                    savepoints: Vec::new(),
                });
                self.sessions().get(&client_id).unwrap()
            });

            if session.transaction_status == crate::handler::TransactionStatus::InError {
                let upper_sql = sql.trim().to_uppercase();
                if !upper_sql.starts_with("ROLLBACK") {
                    let pg_err = crate::handler::errors::PgError::new(
                        crate::handler::errors::PgErrorCode::InFailedSqlTransaction,
                        "current transaction is aborted, commands ignored until end of transaction block",
                    );
                    return Err(anyhow::anyhow!(pg_err));
                }
            }
        }

        let result = crate::transpiler::transpile_with_metadata(sql);
        if !result.errors.is_empty() {
            return Err(anyhow::anyhow!(result.errors.join("\n")));
        }
        let transpiled = result.sql;
        debug!("Original: {}", sql);
        debug!("Transpiled: {}", transpiled);
        let _upper_sql = transpiled.trim().to_uppercase();

        // Transaction Control and other special commands usually don't have parameters in extended query
        // but we should check if we need to handle them here.
        // For now, assume they are handled by execute_query or similar.

        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
        ctx.set_metadata_provider(self.as_metadata_provider());
        let transpile_result = crate::transpiler::transpile_with_context(sql, &mut ctx);

        if !transpile_result.errors.is_empty() {
            return Err(anyhow!("{}", transpile_result.errors.join("; ")));
        }

        // Check permissions before executing
        if !self.check_permissions(&transpile_result.referenced_tables, transpile_result.operation_type, sql)? {
            return Err(anyhow!("permission denied"));
        }

        // Apply RLS
        let sqlite_sql = self.apply_rls_to_query(transpile_result.sql, transpile_result.operation_type, &transpile_result.referenced_tables);

        // Use per-session connection instead of shared connection
        let conn = self.get_session_connection(client_id)?;
        let conn_guard = conn.lock().unwrap();
        let trimmed_lower = sqlite_sql.trim().to_lowercase();
        let is_select = trimmed_lower.starts_with("select") || trimmed_lower.starts_with("with ");

        if is_select {
            self.execute_select_with_params(&conn_guard, &sqlite_sql, params, &transpile_result.referenced_tables, &transpile_result.column_aliases, &transpile_result.column_types)
        } else {
            self.execute_statement_with_params(&conn_guard, &sqlite_sql, params)
        }
    }

    /// Execute a SELECT statement with parameters
    fn execute_select_with_params(&self, conn: &Connection, sql: &str, params: &[Option<String>], referenced_tables: &[String], column_aliases: &[String], column_types: &[Option<String>]) -> Result<Vec<Response>> {
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();

        // Build field info
        let fields: Arc<Vec<FieldInfo>> = Arc::new(self.build_field_info(&stmt, referenced_tables, conn, column_aliases, column_types)?);

        let mut data_rows = Vec::new();
        
        // Convert params to rusqlite params
        let rusqlite_params: Vec<rusqlite::types::Value> = params.iter().map(|p| {
            match p {
                Some(s) => rusqlite::types::Value::Text(s.clone()),
                None => rusqlite::types::Value::Null,
            }
        }).collect();

        // We need to convert Vec<Value> to something rusqlite accepts
        // rusqlite::params_from_iter works
        let mut rows = stmt.query(rusqlite::params_from_iter(rusqlite_params))?;

        while let Some(row) = rows.next()? {
            let mut encoder = DataRowEncoder::new(fields.clone());

            for i in 0..col_count {
                let field_type = fields[i].datatype();
                let value: Option<String> = row.get::<_, Option<i64>>(i).ok()
                    .map(|v| v.map(|x| {
                        if *field_type == pgwire::api::Type::BOOL {
                            if x == 1 { "t".to_string() } else { "f".to_string() }
                        } else {
                            x.to_string()
                        }
                    }))
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

        let row_stream = futures::stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(
            fields,
            row_stream,
        ))])
    }

    /// Execute a non-SELECT statement with parameters
    fn execute_statement_with_params(&self, conn: &Connection, sql: &str, params: &[Option<String>]) -> Result<Vec<Response>> {
        debug!("Executing statement with params: {}", sql);

        // Skip comments and empty statements
        let trimmed = sql.trim();
        if trimmed.starts_with("--") || trimmed.starts_with("/*") || trimmed.is_empty() {
            return Ok(vec![Response::Execution(Tag::new("OK"))]);
        }

        let rusqlite_params: Vec<rusqlite::types::Value> = params.iter().map(|p| {
            match p {
                Some(s) => rusqlite::types::Value::Text(s.clone()),
                None => rusqlite::types::Value::Null,
            }
        }).collect();

        let mut stmt = conn.prepare(sql)?;
        let changes = stmt.execute(rusqlite::params_from_iter(rusqlite_params))?;

        let upper_sql = sql.trim().to_uppercase();
        let tag = if upper_sql.starts_with("INSERT") {
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

    /// Execute a SQL query and return the results
    fn execute_query(&self, client_id: u32, sql: &str) -> Result<Vec<Response>> {
        // Set the current user from the session for current_user() function
        if let Some(session) = self.sessions().get(&0) {
            crate::handler::set_current_user(&session.current_user);
        }

        // Check for commands BEFORE transpilation (transpiler may convert them)
        let original_upper = sql.trim().to_uppercase();
        
        // SHOW commands
        if original_upper == "SHOW ALL" {
            return self.handle_show_all();
        }
        if original_upper.starts_with("SHOW ") && !original_upper.starts_with("SHOW ALL") {
            return self.handle_show_config(sql);
        }
        if original_upper == "SHOW SEARCH_PATH" {
            return self.handle_show_search_path();
        }
        
        // DROP FUNCTION (transpiler doesn't handle this, it deparses to invalid SQL)
        if original_upper.starts_with("DROP FUNCTION") {
            return self.handle_drop_function(sql);
        }
        
        // Handle SET search_path (transpiler converts this to "select 1")
        if original_upper.starts_with("SET SEARCH_PATH") {
            return self.handle_set_search_path(sql);
        }

        // Handle SET ROLE and RESET ROLE
        if original_upper.starts_with("SET ROLE") || original_upper.starts_with("RESET ROLE") {
            return self.handle_set_role(sql);
        }

        let result = crate::transpiler::transpile_with_metadata(sql);
        if !result.errors.is_empty() {
            return Err(anyhow::anyhow!(result.errors.join("\n")));
        }
        let transpiled = result.sql;
        debug!("Original: {}", sql);
        debug!("Transpiled: {}", transpiled);
        let upper_sql = transpiled.trim().to_uppercase();

        // Handle transaction control statements
        if crate::handler::transaction::is_transaction_control(sql) {
            // Get or create session for client 0 (legacy single-client mode during transition)
            let mut session_clone = {
                let session_ref = self.sessions().get(&0).unwrap_or_else(|| {
                    self.sessions().insert(0, SessionContext {
                        authenticated_user: "postgres".to_string(),
                        current_user: "postgres".to_string(),
                        search_path: SearchPath::default(),
                        transaction_status: crate::handler::TransactionStatus::Idle,
                        savepoints: Vec::new(),
                    });
                    self.sessions().get(&0).unwrap()
                });
                session_ref.clone()
            };

            // Parse and execute transaction command
            if let Some(cmd) = crate::handler::transaction::parse_transaction_command(sql) {
                // For now, use the shared connection to maintain backward compatibility
                // TODO: Migrate to per-session connections once all tests are updated
                let result = {
                    let conn_guard = self.conn().lock().unwrap();
                    crate::handler::transaction::execute_transaction_command(
                        cmd,
                        &mut session_clone,
                        &conn_guard,
                    )
                };

                // Store updated session state
                self.sessions().insert(0, session_clone);
                return result;
            }
        }

        // Before executing anything else, check transaction error state
        {
            let session = self.sessions().get(&0).unwrap_or_else(|| {
                self.sessions().insert(0, SessionContext {
                    authenticated_user: "postgres".to_string(),
                    current_user: "postgres".to_string(),
                    search_path: SearchPath::default(),
                    transaction_status: crate::handler::TransactionStatus::Idle,
                    savepoints: Vec::new(),
                });
                self.sessions().get(&0).unwrap()
            });

            if session.transaction_status == crate::handler::TransactionStatus::InError {
                if !upper_sql.starts_with("ROLLBACK") {
                    let pg_err = crate::handler::errors::PgError::new(
                        crate::handler::errors::PgErrorCode::InFailedSqlTransaction,
                        "current transaction is aborted, commands ignored until end of transaction block",
                    );
                    return Err(anyhow::anyhow!(pg_err));
                }
            }
        }

        let execute_result = (|| -> Result<Vec<Response>> {
            // Handle CREATE SCHEMA
        if upper_sql.starts_with("CREATE SCHEMA") {
            return self.handle_create_schema(sql);
        }

        // Handle DROP SCHEMA
        if upper_sql.starts_with("DROP SCHEMA") {
            return self.handle_drop_schema(sql);
        }

        // Handle EXPLAIN with PostgreSQL-specific options (e.g., EXPLAIN (costs off) SELECT ...)
        if upper_sql.starts_with("EXPLAIN") {
            return self.handle_explain(sql);
        }

        // Handle CREATE FUNCTION
        if upper_sql.starts_with("CREATE FUNCTION") || upper_sql.starts_with("CREATE OR REPLACE FUNCTION") {
            return self.handle_create_function(sql);
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

        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
        ctx.set_metadata_provider(self.as_metadata_provider());
        let transpile_result = crate::transpiler::transpile_with_context(sql, &mut ctx);

        // Check for transpilation errors (e.g., unknown pseudo-type)
        if !transpile_result.errors.is_empty() {
            return Err(anyhow!("{}", transpile_result.errors.join("; ")));
        }

        // Handle COPY statements
        if let Some(copy_stmt) = transpile_result.copy_metadata {
            return self.handle_copy_statement(copy_stmt);
        }

        // Check permissions before executing
        if !self.check_permissions(&transpile_result.referenced_tables, transpile_result.operation_type, sql)? {
            return Err(anyhow!("permission denied for table(s)"));
        }

        // Apply RLS (Row-Level Security) to the query
        let sqlite_sql = self.apply_rls_to_query(transpile_result.sql, transpile_result.operation_type, &transpile_result.referenced_tables);

        let conn = self.conn().lock().unwrap();

        let trimmed_lower = sqlite_sql.trim().to_lowercase();
        let is_select = trimmed_lower.starts_with("select") || trimmed_lower.starts_with("with ");
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
                let session = self.sessions().get(&0).unwrap_or_else(|| {
                    self.sessions().insert(0, SessionContext {
                        authenticated_user: "postgres".to_string(),
                        current_user: "postgres".to_string(),
                        search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
                    });
                    self.sessions().get(&0).unwrap()
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
            self.execute_select_with_tables(&conn, &sqlite_sql, &transpile_result.referenced_tables)
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
        })();

        // Check for error and update transaction status
        if execute_result.is_err() {
            let mut session_clone = self.sessions().get(&0).unwrap().clone();
            if session_clone.transaction_status == crate::handler::TransactionStatus::InTransaction {
                session_clone.transaction_status = crate::handler::TransactionStatus::InError;
                self.sessions().insert(0, session_clone);
            }
        }

        execute_result
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
                let response = self.copy_handler().start_copy_from(
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

                self.copy_handler().start_copy_to(
                    query,
                    copy_stmt.options,
                ).map(|r| vec![r])
            }
        }
    }

    /// Execute a SELECT statement with known referenced tables for type inference
    fn execute_select_with_tables(&self, conn: &Connection, sql: &str, referenced_tables: &[String]) -> Result<Vec<Response>> {
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();

        // Build field info using the already-locked connection
        let fields: Arc<Vec<FieldInfo>> = Arc::new(self.build_field_info(&stmt, referenced_tables, conn, &[], &[])?);

        let mut data_rows = Vec::new();
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let mut encoder = DataRowEncoder::new(fields.clone());

            for i in 0..col_count {
                let field_type = fields[i].datatype();
                
                // Try to get value as different types and convert to string
                let value: Option<String> = row.get::<_, Option<i64>>(i).ok()
                    .map(|v| v.map(|x| {
                        // For boolean columns, convert 1/0 to PostgreSQL's 't'/'f' format
                        if *field_type == pgwire::api::Type::BOOL {
                            if x == 1 { "t".to_string() } else { "f".to_string() }
                        } else {
                            x.to_string()
                        }
                    }))
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

    /// Build field info for a SQLite statement using the rewriter's type mapping
    fn build_field_info(
        &self,
        sqlite_stmt: &Statement,
        referenced_tables: &[String],
        conn: &Connection,
        column_aliases: &[String],
        _column_types: &[Option<String>],
    ) -> Result<Vec<FieldInfo>> {
        use crate::handler::rewriter::{map_original_type_to_pg_type, ColumnFieldInfo};
        use pgwire::api::results::{FieldFormat, FieldInfo};
        use pgwire::api::Type;
        use std::collections::HashMap;

        let col_count = sqlite_stmt.column_count();
        let mut fields = Vec::with_capacity(col_count);

        // Build a map of table -> columns from the catalog using the already-locked connection
        let mut table_columns: HashMap<String, Vec<ColumnFieldInfo>> = HashMap::new();
        
        for table_name in referenced_tables {
            if let Ok(columns) = crate::catalog::get_table_columns_with_defaults(conn, table_name) {
                let field_infos: Vec<ColumnFieldInfo> = columns
                    .iter()
                    .map(|col| {
                        let pg_type = map_original_type_to_pg_type(&col.original_type);
                        ColumnFieldInfo {
                            name: col.column_name.clone(),
                            pg_type,
                        }
                    })
                    .collect();
                table_columns.insert(table_name.clone(), field_infos);
            }
        }

        for i in 0..col_count {
            let col_name = sqlite_stmt.column_name(i)?.to_string();
            
            // Check if this is a known column from one of the referenced tables
            let mut found = false;
            for table_name in referenced_tables {
                if let Some(columns) = table_columns.get(table_name) {
                    for col in columns {
                        if col.name == col_name {
                            fields.push(FieldInfo::new(
                                col_name.clone(),
                                None,
                                None,
                                col.pg_type.clone(),
                                FieldFormat::Text,
                            ));
                            found = true;
                            break;
                        }
                    }
                    if found { break; }
                }
            }

            if found {
                continue;
            }

            // For expressions, use PostgreSQL's ?column? convention
            let lower_name = col_name.to_lowercase();
            let is_expression = col_name.contains('(') || col_name.contains(')') ||
                               col_name.contains('+') || col_name.contains('-') ||
                               col_name.contains('*') || col_name.contains('/') ||
                               col_name.contains(' ') ||
                               col_name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);

            let pg_type = if lower_name.starts_with("count(") {
                Type::INT8
            } else if lower_name.starts_with("sum(") || lower_name.starts_with("avg(") {
                Type::NUMERIC
            } else {
                Type::TEXT
            };

            // Use column alias from original query if available, otherwise fall back to SQLite's column name
            let name = if i < column_aliases.len() && !column_aliases[i].is_empty() {
                column_aliases[i].clone()
            } else if col_name == "?column?" || (is_expression && !col_name.contains(" as ")) {
                "?column?".to_string()
            } else {
                col_name
            };

            fields.push(FieldInfo::new(name, None, None, pg_type, FieldFormat::Text));
        }

        Ok(fields)
    }

    /// Execute a non-SELECT statement (INSERT, UPDATE, DELETE, DDL)
    fn execute_statement(&self, conn: &Connection, sql: &str) -> Result<Vec<Response>> {
        debug!("Executing statement: {}", sql);

        // Split multiple statements and execute them sequentially
        let statements: Vec<&str> = sql.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        let mut total_changes = 0;

        for stmt in statements {
            // Skip comments and empty statements which can cause SQLITE_MISUSE in some versions/cases
            if stmt.starts_with("--") || stmt.starts_with("/*") {
                continue;
            }
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

    /// Reference to the copy handler
    fn copy_handler(&self) -> &copy::CopyHandler;
}
