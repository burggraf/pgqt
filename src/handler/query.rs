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

use crate::catalog::{store_table_metadata, store_relation_metadata};
use crate::schema::SearchPath;
use crate::handler::SessionContext;
use crate::handler::utils::HandlerUtils;
use crate::transpiler::metadata::MetadataProvider;
use crate::copy;
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::Type;

/// Trait for query execution methods
pub trait QueryExecution: HandlerUtils + Clone {
    fn as_metadata_provider(&self) -> Arc<dyn crate::transpiler::metadata::MetadataProvider>;
    
    /// Execute a SQL query and return the results
    fn execute_query(&self, sql: &str) -> Result<Vec<Response>> {
        let upper_sql = sql.trim().to_uppercase();

        // Ignore transaction control statements - SQLite handles transactions automatically
        // PostgreSQL transaction control:
        // - BEGIN / START TRANSACTION: start a transaction
        // - COMMIT / END: commit the transaction
        // - ROLLBACK / ABORT: roll back the transaction
        if upper_sql == "BEGIN" || upper_sql == "COMMIT" || upper_sql == "ROLLBACK" || upper_sql == "END"
            || upper_sql == "ABORT" || upper_sql.starts_with("START TRANSACTION") {
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

        // Handle EXPLAIN with PostgreSQL-specific options (e.g., EXPLAIN (costs off) SELECT ...)
        if upper_sql.starts_with("EXPLAIN") {
            return self.handle_explain(sql);
        }

        // Handle SHOW search_path
        if upper_sql == "SHOW SEARCH_PATH" {
            return self.handle_show_search_path();
        }

        // Handle SHOW ALL
        if upper_sql == "SHOW ALL" {
            return self.handle_show_all();
        }

        // Handle SHOW <config_param>
        if upper_sql.starts_with("SHOW ") && !upper_sql.starts_with("SHOW ALL") {
            return self.handle_show_config(sql);
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

        // Handle SET ROLE specially
        if transpile_result.sql.starts_with("-- SET ROLE") {
            let role_name = transpile_result.sql.trim_start_matches("-- SET ROLE ").trim();
            if role_name != "NONE" {
                // Update session context
                let mut session = self.sessions().get_mut(&0).unwrap_or_else(|| {
                    self.sessions().insert(0, SessionContext {
                        authenticated_user: "postgres".to_string(),
                        current_user: "postgres".to_string(),
                        search_path: SearchPath::default(),
                    });
                    self.sessions().get_mut(&0).unwrap()
                });
                session.current_user = role_name.to_string();
            }
            // Return success without executing
            return Ok(vec![Response::Execution(Tag::new("SET"))]);
        }

        // Check permissions before executing
        if !self.check_permissions(&transpile_result.referenced_tables, transpile_result.operation_type)? {
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
                        search_path: SearchPath::default(),
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

    /// Execute a SELECT statement and return results
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

    /// Execute a non-SELECT statement (INSERT, UPDATE, DELETE, DDL)
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

    /// Reference to the copy handler
    fn copy_handler(&self) -> &copy::CopyHandler;
}
