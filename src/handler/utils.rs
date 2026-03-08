//! Handler utility functions
//!
//! This module contains helper methods for the SqliteHandler including:
//! - Permission checking
//! - RLS (Row-Level Security) application
//! - Schema management (CREATE SCHEMA, DROP SCHEMA)
//! - Search path handling
//! - User-defined function management

use std::sync::{Arc, Mutex};
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use dashmap::DashMap;
use futures::stream;

use crate::catalog::FunctionMetadata;
use crate::schema::{SchemaManager, SearchPath};
use crate::handler::SessionContext;
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::Type;

/// Trait for utility methods that need access to SqliteHandler fields
pub trait HandlerUtils {
    fn conn(&self) -> &Arc<Mutex<Connection>>;
    fn sessions(&self) -> &Arc<DashMap<u32, SessionContext>>;
    fn schema_manager(&self) -> &SchemaManager;
    fn functions(&self) -> &Arc<DashMap<String, FunctionMetadata>>;

    /// Check if the current user has permission to execute the query
    fn check_permissions(&self, referenced_tables: &[String], operation_type: crate::transpiler::OperationType, sql: &str) -> Result<bool> {
        // Get current user from session
        let session = self.sessions().get(&0).unwrap_or_else(|| {
            self.sessions().insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
            });
            self.sessions().get(&0).unwrap()
        });
        let current_user = session.current_user.clone();

        // Get the connection to query RBAC tables
        let conn = self.conn().lock().unwrap();

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

        // --- Step 1: Check for schema operations (RBAC completion plan Task 9) ---
        if operation_type == crate::transpiler::OperationType::DDL {
            let upper_sql = sql.trim().to_uppercase();
            if upper_sql.starts_with("CREATE SCHEMA") || upper_sql.starts_with("CREATE TABLE") {
                // Check if user has CREATE privilege on the schema
                // For now, we only check for CREATE SCHEMA or CREATE TABLE (implied)
                // A more complete implementation would check for specific schema
                let has_schema_create: bool = conn.query_row(
                    "SELECT EXISTS (
                        SELECT 1 FROM __pg_acl__ a
                        JOIN pg_namespace n ON n.oid = a.object_id
                        WHERE a.privilege = 'CREATE'
                        AND (
                            a.grantee_id IN (SELECT oid FROM __pg_authid__ WHERE rolname = ?1)
                            OR a.grantee_id = 0
                        )
                    )",
                    &[&current_user],
                    |row| row.get(0),
                ).unwrap_or(false);

                if !has_schema_create {
                    return Ok(false);
                }
            }
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

        // If no tables are referenced, skip RLS injection
        // This avoids issues with queries like "SELECT 1" that have no FROM clause
        if tables.is_empty() {
            return sql;
        }

        // Get current user from session
        let session = self.sessions().get(&0);
        let current_user = session.map(|s| s.current_user.clone()).unwrap_or_else(|| "postgres".to_string());

        let conn = self.conn().lock().unwrap();

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
            .ok_or_else(|| anyhow!("invalid CREATE SCHEMA syntax"))?;

        // Check for reserved names
        if schema_name.starts_with("pg_") {
            return Err(anyhow!(
                "unacceptable schema name \"{}\": system schemas must start with pg_",
                schema_name
            ));
        }

        let conn = self.conn().lock().unwrap();

        // Check if schema already exists
        if crate::schema::schema_exists(&conn, &schema_name)? {
            if if_not_exists {
                return Ok(vec![Response::Execution(Tag::new("CREATE SCHEMA"))]);
            }
            return Err(anyhow!("schema \"{}\" already exists", schema_name));
        }

        // Create schema in catalog
        crate::schema::create_schema(&conn, &schema_name, None)?;

        // Attach the schema database
        self.schema_manager().attach_schema(&conn, &schema_name)?;

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
            .ok_or_else(|| anyhow!("invalid DROP SCHEMA syntax"))?;

        // Cannot drop system schemas
        if schema_name == "public" {
            return Err(anyhow!("cannot drop schema \"public\""));
        }
        if schema_name == "pg_catalog" || schema_name == "information_schema" {
            return Err(anyhow!("cannot drop system schema \"{}\"", schema_name));
        }

        let conn = self.conn().lock().unwrap();

        // Check if schema exists
        if !crate::schema::schema_exists(&conn, &schema_name)? {
            if if_exists {
                return Ok(vec![Response::Execution(Tag::new("DROP SCHEMA"))]);
            }
            return Err(anyhow!("schema \"{}\" does not exist", schema_name));
        }

        // Check if schema is empty (unless CASCADE)
        if !cascade && !crate::schema::schema_is_empty(&conn, &schema_name, self.schema_manager())? {
            return Err(anyhow!(
                "schema \"{}\" cannot be dropped without CASCADE because it contains objects",
                schema_name
            ));
        }

        // Drop all objects in the schema (if CASCADE)
        if cascade {
            crate::schema::drop_schema_objects(&conn, &schema_name, self.schema_manager())?;
        }

        // Remove schema from catalog first
        crate::schema::drop_schema(&conn, &schema_name)?;

        // Try to detach the schema database - this may fail if it's locked
        // In that case, we just mark it for cleanup later
        match self.schema_manager().detach_schema(&conn, &schema_name) {
            Ok(_) => {}
            Err(e) => {
                // Log the error but continue - the schema is already removed from catalog
                eprintln!("Warning: Could not detach schema {}: {}", schema_name, e);
            }
        }

        // Delete the schema database file
        self.schema_manager().delete_schema_db(&schema_name)?;

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
        let mut session = self.sessions().get_mut(&0).unwrap_or_else(|| {
            self.sessions().insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
            });
            self.sessions().get_mut(&0).unwrap()
        });
        session.search_path = search_path;

        Ok(vec![Response::Execution(Tag::new("SET"))])
    }

    /// Handle SET ROLE statement
    fn handle_set_role(&self, sql: &str) -> Result<Vec<Response>> {
        let upper_sql = sql.trim().to_uppercase();
        let role_name = if upper_sql == "RESET ROLE" || upper_sql == "SET ROLE NONE" {
            "NONE".to_string()
        } else {
            sql.trim()
                .trim_start_matches("SET ROLE")
                .trim_start_matches("set role")
                .trim()
                .trim_end_matches(';')
                .to_string()
        };

        let mut session = self.sessions().get_mut(&0).unwrap_or_else(|| {
            self.sessions().insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(),
                transaction_status: crate::handler::TransactionStatus::Idle,
                savepoints: Vec::new(),
            });
            self.sessions().get_mut(&0).unwrap()
        });

        if role_name.to_uppercase() == "NONE" {
            session.current_user = session.authenticated_user.clone();
        } else {
            // Verify role membership
            let conn = self.conn().lock().unwrap();
            
            // Check if role exists
            let role_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM __pg_authid__ WHERE rolname = ?1)",
                &[&role_name],
                |row| row.get(0),
            ).unwrap_or(false);
            
            if !role_exists {
                return Err(anyhow!("role \"{}\" does not exist", role_name));
            }
            
            // Check if authenticated user is a member of the role
            let is_member: bool = conn.query_row(
                "WITH RECURSIVE effective_roles AS (
                    SELECT oid FROM __pg_authid__ WHERE rolname = ?1
                    UNION
                    SELECT m.member FROM __pg_auth_members__ m
                    JOIN effective_roles er ON er.oid = m.roleid
                 )
                 SELECT EXISTS(SELECT 1 FROM effective_roles er JOIN __pg_authid__ a ON a.oid = er.oid WHERE a.rolname = ?2)",
                &[&role_name, &session.authenticated_user],
                |row| row.get(0),
            ).unwrap_or(false);
            
            let is_superuser: bool = conn.query_row(
                "SELECT rolsuper FROM __pg_authid__ WHERE rolname = ?1",
                &[&session.authenticated_user],
                |row| row.get(0),
            ).unwrap_or(false);
            
            if !is_member && !is_superuser {
                return Err(anyhow!("permission denied to set role \"{}\"", role_name));
            }
            
            session.current_user = role_name;
        }

        Ok(vec![Response::Execution(Tag::new("SET"))])
    }

    /// Handle SHOW search_path statement
    fn handle_show_search_path(&self) -> Result<Vec<Response>> {
        let session = self.sessions().get(&0).unwrap_or_else(|| {
            self.sessions().insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
            });
            self.sessions().get(&0).unwrap()
        });

        let path = session.search_path.to_string();

        let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
            "search_path".to_string(),
            None,
            None,
            Type::TEXT,
            pgwire::api::results::FieldFormat::Text,
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

    /// Handle SHOW ALL statement
    fn handle_show_all(&self) -> Result<Vec<Response>> {
        // SHOW ALL displays all configuration parameters
        // For PGQT, we show a subset of supported parameters
        let params = vec![
            ("search_path", self.get_search_path_value()),
            ("server_version", "16.1".to_string()),
            ("server_encoding", "UTF8".to_string()),
            ("client_encoding", "UTF8".to_string()),
            ("application_name", "".to_string()),
            ("DateStyle", "ISO, MDY".to_string()),
            ("TimeZone", "UTC".to_string()),
            ("transaction_isolation", "read committed".to_string()),
            ("transaction_read_only", "off".to_string()),
            ("default_transaction_isolation", "read committed".to_string()),
            ("default_transaction_read_only", "off".to_string()),
            ("statement_timeout", "0".to_string()),
            ("lock_timeout", "0".to_string()),
            ("idle_in_transaction_session_timeout", "0".to_string()),
            ("max_connections", "100".to_string()),
            ("shared_buffers", "128MB".to_string()),
            ("effective_cache_size", "4GB".to_string()),
            ("work_mem", "4MB".to_string()),
            ("maintenance_work_mem", "64MB".to_string()),
            ("checkpoint_completion_target", "0.5".to_string()),
            ("wal_buffers", "16MB".to_string()),
            ("default_statistics_target", "100".to_string()),
            ("random_page_cost", "4.0".to_string()),
            ("effective_io_concurrency", "1".to_string()),
            ("work_mem", "4MB".to_string()),
            ("min_wal_size", "80MB".to_string()),
            ("max_wal_size", "1GB".to_string()),
        ];

        let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![
            FieldInfo::new("name".to_string(), None, None, Type::TEXT, FieldFormat::Text),
            FieldInfo::new("setting".to_string(), None, None, Type::TEXT, FieldFormat::Text),
            FieldInfo::new("description".to_string(), None, None, Type::TEXT, FieldFormat::Text),
        ]);

        let mut data_rows = Vec::new();
        for (name, setting) in params {
            let mut encoder = DataRowEncoder::new(fields.clone());
            encoder.encode_field(&Some(name.to_string()))?;
            encoder.encode_field(&Some(setting.clone()))?;
            encoder.encode_field(&Some("PGQT configuration parameter".to_string()))?;
            data_rows.push(Ok(encoder.take_row()));
        }

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(
            fields,
            row_stream,
        ))])
    }

    /// Handle SHOW <config_param> statement
    fn handle_show_config(&self, sql: &str) -> Result<Vec<Response>> {
        // Extract the parameter name from "SHOW parameter"
        let param_name = sql
            .trim()
            .trim_start_matches("SHOW")
            .trim()
            .trim_start_matches("SHOW ")
            .trim()
            .to_lowercase();

        let value = self.get_config_value(&param_name)?;

        let fields: Arc<Vec<FieldInfo>> = Arc::new(vec![FieldInfo::new(
            param_name.clone(),
            None,
            None,
            Type::TEXT,
            FieldFormat::Text,
        )]);

        let mut encoder = DataRowEncoder::new(fields.clone());
        encoder.encode_field(&Some(value))?;
        let data_rows = vec![Ok(encoder.take_row())];

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(
            fields,
            row_stream,
        ))])
    }

    /// Get the current search_path value
    fn get_search_path_value(&self) -> String {
        let session = self.sessions().get(&0).unwrap_or_else(|| {
            self.sessions().insert(0, SessionContext {
                authenticated_user: "postgres".to_string(),
                current_user: "postgres".to_string(),
                search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
            });
            self.sessions().get(&0).unwrap()
        });

        session.search_path.to_string()
    }

    /// Get the value of a configuration parameter
    fn get_config_value(&self, param: &str) -> Result<String> {
        let param_lower = param.to_lowercase();

        Ok(match param_lower.as_str() {
            "search_path" => self.get_search_path_value(),
            "server_version" | "server_version_num" => "16.1".to_string(),
            "server_encoding" => "UTF8".to_string(),
            "client_encoding" => "UTF8".to_string(),
            "application_name" => "".to_string(),
            "datestyle" => "ISO, MDY".to_string(),
            "timezone" | "time zone" => "UTC".to_string(),
            "transaction_isolation" | "transaction_isolation_level" => "read committed".to_string(),
            "transaction_read_only" | "default_transaction_read_only" => "off".to_string(),
            "default_transaction_isolation" => "read committed".to_string(),
            "statement_timeout" => "0".to_string(),
            "lock_timeout" => "0".to_string(),
            "idle_in_transaction_session_timeout" => "0".to_string(),
            "max_connections" => "100".to_string(),
            "shared_buffers" => "128MB".to_string(),
            "effective_cache_size" => "4GB".to_string(),
            "work_mem" => "4MB".to_string(),
            "maintenance_work_mem" => "64MB".to_string(),
            "checkpoint_completion_target" => "0.5".to_string(),
            "wal_buffers" => "16MB".to_string(),
            "default_statistics_target" => "100".to_string(),
            "random_page_cost" => "4.0".to_string(),
            "effective_io_concurrency" => "1".to_string(),
            "min_wal_size" => "80MB".to_string(),
            "max_wal_size" => "1GB".to_string(),
            // Client connection parameters
            "port" => "5432".to_string(),
            "host" => "127.0.0.1".to_string(),
            "database" | "current_database" => "postgres".to_string(),
            "user" | "current_user" | "session_user" => {
                let session = self.sessions().get(&0).unwrap_or_else(|| {
                    self.sessions().insert(0, SessionContext {
                        authenticated_user: "postgres".to_string(),
                        current_user: "postgres".to_string(),
                        search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
                    });
                    self.sessions().get(&0).unwrap()
                });
                session.current_user.clone()
            }
            _ => {
                // Return a sensible default for unknown parameters
                // This matches PostgreSQL behavior of showing a default
                format!("PGQT parameter: {}", param_lower)
            }
        })
    }

    /// Handle CREATE FUNCTION statement
    fn handle_create_function(&self, sql: &str) -> Result<Vec<Response>> {
        // Parse CREATE FUNCTION
        let metadata = crate::transpiler::parse_create_function(sql)?;

        // Store in catalog
        let conn = self.conn().lock().unwrap();
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
        self.functions().insert(metadata.name.clone(), metadata.clone());

        // Create references for the closure
        let func_name = metadata.name.clone();
        let func_name_for_closure = func_name.clone(); // Clone for the closure
        let arg_count = num_params;
        let is_strict = metadata.strict;
        let return_type_kind = metadata.return_type_kind.clone();
        let functions_cache = self.functions().clone();

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
            .ok_or_else(|| anyhow!("Could not extract function name"))?;


        // Get connection and look up function
        let conn = self.conn().lock().unwrap();
        let metadata = crate::catalog::get_function(&conn, &func_name, None)?
            .ok_or_else(|| anyhow!("Function {} not found", func_name))?;


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
        let conn = self.conn().lock().unwrap();
        crate::catalog::drop_function(&conn, name, None)?;

        Ok(vec![Response::Execution(Tag::new("DROP FUNCTION"))])
    }

    /// Handle EXPLAIN statement - strip PostgreSQL-specific options
    fn handle_explain(&self, sql: &str) -> Result<Vec<Response>> {
        // EXPLAIN (costs off) SELECT ... -> EXPLAIN SELECT ...
        // Extract the query after EXPLAIN and any options
        let sql_stripped = sql.trim();

        // Find the actual query (SELECT, INSERT, UPDATE, DELETE)
        let query = if let Some(pos) = sql_stripped.find("SELECT") {
            &sql_stripped[pos..]
        } else if let Some(pos) = sql_stripped.find("UPDATE") {
            &sql_stripped[pos..]
        } else if let Some(pos) = sql_stripped.find("INSERT") {
            &sql_stripped[pos..]
        } else if let Some(pos) = sql_stripped.find("DELETE") {
            &sql_stripped[pos..]
        } else {
            return Err(anyhow::anyhow!("EXPLAIN requires SELECT, INSERT, UPDATE, or DELETE query"));
        };

        // Strip PostgreSQL EXPLAIN options like (costs off)
        // and construct SQLite-compatible EXPLAIN
        let explain_sql = format!("EXPLAIN {}", query);

        // Execute the EXPLAIN query directly on SQLite
        let conn = self.conn().lock().unwrap();

        let mut stmt = conn.prepare(&explain_sql)?;
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let fields: Arc<Vec<FieldInfo>> = Arc::new(
            column_names
                .iter()
                .map(|name| FieldInfo::new(name.clone(), None, None, Type::TEXT, FieldFormat::Text))
                .collect(),
        );

        let mut data_rows = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let mut encoder = DataRowEncoder::new(fields.clone());
            for i in 0..column_names.len() {
                let value: Option<String> = row.get(i)?;
                encoder.encode_field(&value)?;
            }
            data_rows.push(Ok(encoder.take_row()));
        }

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(fields, row_stream))])
    }
}
