use std::sync::Arc;
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use futures::stream;

use crate::debug;
use crate::catalog::{store_table_metadata, store_relation_metadata};
use crate::handler::SessionContext;
use crate::handler::utils::HandlerUtils;
#[allow(unused_imports)]
use crate::transpiler::metadata::MetadataProvider;
use crate::copy;
use crate::trigger::{TriggerExecutor, BeforeTriggerResult, extract_table_and_operation};
use crate::trigger::rows::{fetch_inserted_row};
use pgwire::api::results::{DataRowEncoder, FieldInfo, QueryResponse, Response, Tag};

/// Helper to robustly split multiple SQL statements
fn robust_split(sql: &str) -> Vec<String> {
    // Attempt to use pg_query scanner first (most robust)
    match pg_query::split_with_scanner(sql) {
        Ok(statements) => {
            // Always return the scanner's result if it succeeded
            return statements.into_iter().map(|s| s.to_string()).collect();
        }
        Err(_) => {
            // Fallback: manual split by semicolon, respecting single and double quotes
            let mut statements = Vec::new();
            let mut current = String::new();
            let mut in_quote = false;
            let mut in_double_quote = false;
            let mut chars = sql.chars().peekable();

            while let Some(c) = chars.next() {
                match c {
                    '\'' if !in_double_quote => {
                        // Handle escaped quotes ''
                        if in_quote && chars.peek() == Some(&'\'') {
                            current.push(c);
                            current.push(chars.next().unwrap());
                        } else {
                            in_quote = !in_quote;
                            current.push(c);
                        }
                    }
                    '"' if !in_quote => {
                        in_double_quote = !in_double_quote;
                        current.push(c);
                    }
                    ';' if !in_quote && !in_double_quote => {
                        let trimmed = current.trim();
                        if !trimmed.is_empty() {
                            statements.push(trimmed.to_string());
                        }
                        current.clear();
                    }
                    _ => {
                        current.push(c);
                    }
                }
            }
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(trimmed.to_string());
            }
            
            if statements.is_empty() {
                vec![sql.to_string()]
            } else {
                statements
            }
        }
    }
}

/// Trait for query execution methods
pub trait QueryExecution: HandlerUtils + Clone {
    fn as_metadata_provider(&self) -> Arc<dyn crate::transpiler::metadata::MetadataProvider>;

    /// Execute a SQL query with optional parameters and return the results
    fn execute_query_params(&self, client_id: u32, sql: &str, params: &[Option<String>]) -> Result<Vec<Response>> {
        let statements = robust_split(sql);
        if statements.len() > 1 {
            let mut all_responses = Vec::new();
            for stmt_sql in statements {
                let responses = self.execute_single_query_params(client_id, &stmt_sql, params)?;
                all_responses.extend(responses);
            }
            return Ok(all_responses);
        }
        self.execute_single_query_params(client_id, sql, params)
    }

    /// Execute a single query with parameters
    fn execute_single_query_params(&self, client_id: u32, sql: &str, params: &[Option<String>]) -> Result<Vec<Response>> {
        crate::handler::set_current_client_id(client_id);
        if let Some(session) = self.sessions().get(&client_id) {
            crate::handler::set_current_user(&session.current_user);
        }

        {
            let session = self.sessions().get(&client_id).unwrap_or_else(|| {
                self.sessions().insert(client_id, SessionContext::new("postgres".to_string()));
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

        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
        ctx.set_metadata_provider(self.as_metadata_provider());
        let transpile_result = crate::transpiler::transpile_with_context(sql, &mut ctx);

        if !transpile_result.errors.is_empty() {
            let error_msg = transpile_result.errors.join("; ");
            // Retry if multiple statements are found during parsing
            if error_msg.contains("Multiple statements provided") {
                let statements = robust_split(sql);
                if statements.len() > 1 {
                    let mut all_responses = Vec::new();
                    for stmt in statements {
                        let responses = self.execute_single_query_params(client_id, &stmt, params)?;
                        all_responses.extend(responses);
                    }
                    return Ok(all_responses);
                }
            }
            return Err(anyhow!("{}", error_msg));
        }

        let transpiled_out = transpile_result.sql.clone();
        
        let trans_statements = robust_split(&transpiled_out);
        if trans_statements.len() > 1 {
            let mut all_responses = Vec::new();
            for t_stmt in trans_statements {
                let responses = self.execute_transpiled_stmt_params(client_id, &t_stmt, sql, params, &transpile_result)?;
                all_responses.extend(responses);
            }
            return Ok(all_responses);
        }

        self.execute_transpiled_stmt_params(client_id, &transpiled_out, sql, params, &transpile_result)
    }

    /// Execute a SQL query and return the results
    fn execute_query(&self, client_id: u32, sql: &str) -> Result<Vec<Response>> {
        let statements = robust_split(sql);

        if statements.len() > 1 {
            let mut all_responses = Vec::new();
            for stmt_sql in statements {
                let responses = self.execute_single_query(client_id, &stmt_sql)?;
                all_responses.extend(responses);
            }
            return Ok(all_responses);
        }
        
        self.execute_single_query(client_id, sql)
    }

    /// Execute a single SQL query and return the results
    fn execute_single_query(&self, client_id: u32, sql: &str) -> Result<Vec<Response>> {
        crate::handler::set_current_client_id(client_id);
        if let Some(session) = self.sessions().get(&client_id) {
            crate::handler::set_current_user(&session.current_user);
        }

        let original_upper = sql.trim().to_uppercase();

        if original_upper == "SHOW ALL" {
            return self.handle_show_all();
        }
        if original_upper.starts_with("SHOW ") && !original_upper.starts_with("SHOW ALL") {
            return self.handle_show_config(sql);
        }
        if original_upper == "SHOW SEARCH_PATH" {
            return self.handle_show_search_path();
        }

        if original_upper.starts_with("DROP FUNCTION") {
            return self.handle_drop_function(sql);
        }

        if original_upper.starts_with("SET SEARCH_PATH") {
            return self.handle_set_search_path(sql);
        }

        if original_upper.starts_with("SET ROLE") || original_upper.starts_with("RESET ROLE") {
            return self.handle_set_role(sql);
        }

        if original_upper.starts_with("CREATE TRIGGER") {
            return self.handle_create_trigger(sql);
        }

        if original_upper.starts_with("DROP TRIGGER") {
            return self.handle_drop_trigger(sql);
        }

        let result = crate::transpiler::transpile_with_metadata(sql);
        if !result.errors.is_empty() {
            let error_msg = result.errors.join("; ");
            if error_msg.contains("Multiple statements provided") {
                let statements = robust_split(sql);
                if statements.len() > 1 {
                    let mut all_responses = Vec::new();
                    for stmt in statements {
                        let responses = self.execute_single_query(client_id, &stmt)?;
                        all_responses.extend(responses);
                    }
                    return Ok(all_responses);
                }
            }
            return Err(anyhow::anyhow!(error_msg));
        }
        
        let transpiled = result.sql.clone();
        debug!("Original: {}", sql);
        debug!("Transpiled: {}", transpiled);

        let trans_statements = robust_split(&transpiled);
        
        if trans_statements.len() > 1 {
            let mut all_responses = Vec::new();
            for t_stmt in trans_statements {
                let responses = self.execute_transpiled_stmt(client_id, &t_stmt, sql, &result)?;
                all_responses.extend(responses);
            }
            return Ok(all_responses);
        }

        self.execute_transpiled_stmt(client_id, &transpiled, sql, &result)
    }

    /// Execute a single transpiled statement
    fn execute_transpiled_stmt(&self, client_id: u32, transpiled: &str, original_sql: &str, transpile_result: &crate::transpiler::TranspileResult) -> Result<Vec<Response>> {
        self.execute_transpiled_stmt_params(client_id, transpiled, original_sql, &[], transpile_result)
    }

    /// Execute a single transpiled statement with parameters
    fn execute_transpiled_stmt_params(&self, client_id: u32, transpiled: &str, original_sql: &str, params: &[Option<String>], transpile_result: &crate::transpiler::TranspileResult) -> Result<Vec<Response>> {
        let upper_sql = transpiled.trim().to_uppercase();

        if upper_sql.starts_with("SELECT __PG_CREATE_ENUM") {
             let conn = self.get_session_connection(client_id)?;
             let conn_guard = conn.lock().unwrap();
             if let Some(start) = transpiled.find('(') {
                 if let Some(end) = transpiled.rfind(')') {
                     let args_str = &transpiled[start+1..end];
                     let args: Vec<String> = args_str.split(',')
                         .map(|s| s.trim().trim_matches('\'').replace("''", "'").to_string())
                         .collect();
                     if args.len() == 3 {
                         let type_name = &args[0];
                         let label = &args[1];
                         let sort_order: f64 = args[2].parse().unwrap_or(0.0);
                         crate::catalog::store_enum_value(&conn_guard, type_name, label, sort_order)?;
                         return Ok(vec![Response::Execution(Tag::new("OK"))]);
                     }
                 }
             }
        }

        if upper_sql.starts_with("SELECT __PG_COMMENT_ON") {
             let conn = self.get_session_connection(client_id)?;
             let conn_guard = conn.lock().unwrap();
             if let Some(start) = transpiled.find('(') {
                 if let Some(end) = transpiled.rfind(')') {
                     let args_str = &transpiled[start+1..end];
                     let args: Vec<String> = args_str.split(',')
                         .map(|s| s.trim().trim_matches('\'').replace("''", "'").to_string())
                         .collect();
                     if args.len() == 3 {
                         let obj_type = &args[0];
                         let obj_name = &args[1];
                         let comment = &args[2];
                         crate::catalog::store_comment(&conn_guard, obj_type, obj_name, comment)?;
                         return Ok(vec![Response::Execution(Tag::new("OK"))]);
                     }
                 }
             }
        }

        if crate::handler::transaction::is_transaction_control(original_sql) {
            let mut session_clone = {
                let session_ref = self.sessions().get(&client_id).unwrap_or_else(|| {
                    self.sessions().insert(client_id, SessionContext::new("postgres".to_string()));
                    self.sessions().get(&client_id).unwrap()
                });
                session_ref.clone()
            };

            if let Some(cmd) = crate::handler::transaction::parse_transaction_command(original_sql) {
                let result = {
                    let conn_guard = self.conn().lock().unwrap();
                    crate::handler::transaction::execute_transaction_command(
                        cmd,
                        &mut session_clone,
                        &conn_guard,
                    )
                };

                self.sessions().insert(client_id, session_clone);
                return result;
            }
        }

        {
            let session = self.sessions().get(&client_id).unwrap_or_else(|| {
                self.sessions().insert(client_id, SessionContext::new("postgres".to_string()));
                self.sessions().get(&client_id).unwrap()
            });

            if session.transaction_status == crate::handler::TransactionStatus::InError {
                let upper_orig = original_sql.trim().to_uppercase();
                if !upper_orig.starts_with("ROLLBACK") {
                    let pg_err = crate::handler::errors::PgError::new(
                        crate::handler::errors::PgErrorCode::InFailedSqlTransaction,
                        "current transaction is aborted, commands ignored until end of transaction block",
                    );
                    return Err(anyhow::anyhow!(pg_err));
                }
            }
        }

        let execute_result = (|| -> Result<Vec<Response>> {
        let upper_orig = original_sql.trim().to_uppercase();
        if upper_orig.starts_with("CREATE SCHEMA") {
            return self.handle_create_schema(original_sql);
        }

        if upper_orig.starts_with("DROP SCHEMA") {
            return self.handle_drop_schema(original_sql);
        }

        if upper_orig.starts_with("EXPLAIN") {
            return self.handle_explain(original_sql);
        }

        if upper_orig.starts_with("CREATE FUNCTION") || upper_orig.starts_with("CREATE OR REPLACE FUNCTION") {
            return self.handle_create_function(original_sql);
        }

        match self.try_execute_simple_function_call(original_sql) {
            Ok(result) => {
                return Ok(result);
            }
            Err(_) => {
            }
        }

        if let Some(copy_stmt) = transpile_result.copy_metadata.clone() {
            return self.handle_copy_statement(copy_stmt);
        }

        if !self.check_permissions(&transpile_result.referenced_tables, transpile_result.operation_type, original_sql)? {
            return Err(anyhow!("permission denied for table(s)"));
        }

        let sqlite_sql = self.apply_rls_to_query(transpiled.to_string(), transpile_result.operation_type, &transpile_result.referenced_tables);

        let conn = self.get_session_connection(client_id)?;
        let conn_guard = conn.lock().unwrap();

        let trimmed_lower = sqlite_sql.trim().to_lowercase();
        let is_select = trimmed_lower.starts_with("select") || trimmed_lower.starts_with("with ");
        let is_create_table = sqlite_sql.trim().to_uppercase().starts_with("CREATE TABLE");

        if is_create_table {
            let result = self.execute_statement_with_params(&conn_guard, &sqlite_sql, params)?;

            if let Some(metadata) = transpile_result.create_table_metadata.clone() {
                let columns: Vec<(String, String, Option<String>)> = metadata
                    .columns
                    .into_iter()
                    .map(|c| (c.column_name, c.original_type, c.constraints))
                    .collect();

                store_table_metadata(&conn_guard, &metadata.table_name, &columns)?;

                let session = self.sessions().get(&client_id).unwrap_or_else(|| {
                    self.sessions().insert(client_id, SessionContext::new("postgres".to_string()));
                    self.sessions().get(&client_id).unwrap()
                });
                let owner_oid: i64 = conn_guard.query_row(
                    "SELECT oid FROM __pg_authid__ WHERE rolname = ?1",
                    &[&session.current_user],
                    |row| row.get(0),
                ).unwrap_or(10); 

                store_relation_metadata(&conn_guard, &metadata.table_name, owner_oid)?;

                crate::catalog::populate_pg_attribute(&conn_guard, &metadata.table_name)?;
                crate::catalog::populate_pg_index(&conn_guard)?;
                crate::catalog::populate_pg_constraint(&conn_guard)?;
            }

            Ok(result)
        } else if is_select {
            self.execute_select_with_params(&conn_guard, &sqlite_sql, params, &transpile_result.referenced_tables, &transpile_result.column_aliases, &transpile_result.column_types)
        } else {
            // Check if it's a comment or no-op before attempting to execute
            let trimmed = sqlite_sql.trim();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                return Ok(vec![Response::Execution(Tag::new("OK"))]);
            }

            match transpile_result.operation_type {
                crate::transpiler::OperationType::INSERT |
                crate::transpiler::OperationType::UPDATE |
                crate::transpiler::OperationType::DELETE => {
                    self.execute_dml_with_triggers(&conn_guard, original_sql, &sqlite_sql, transpile_result.operation_type)
                }
                _ => {
                    self.execute_statement_with_params(&conn_guard, &sqlite_sql, params)
                }
            }
        }
        })();

        if execute_result.is_err() {
            let mut session_clone = self.sessions().get(&client_id).unwrap().clone();
            if session_clone.transaction_status == crate::handler::TransactionStatus::InTransaction {
                session_clone.transaction_status = crate::handler::TransactionStatus::InError;
                self.sessions().insert(client_id, session_clone);
            }
        }

        execute_result
    }

    fn handle_copy_statement(&self,
        copy_stmt: crate::copy::CopyStatement,
    ) -> Result<Vec<Response>> {
        use crate::copy::{CopyDirection};

        match copy_stmt.direction {
            CopyDirection::From => {
                let table_name = copy_stmt.table_name.ok_or_else(|| anyhow!("COPY FROM requires table name"))?;
                let options = copy_stmt.options;

                let response = self.copy_handler().start_copy_from(
                    table_name,
                    copy_stmt.columns,
                    options,
                )?;
                Ok(vec![response])
            }
            CopyDirection::To => {
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

    fn execute_select_with_tables(&self, conn: &Connection, sql: &str, referenced_tables: &[String]) -> Result<Vec<Response>> {
        self.execute_select_with_params(conn, sql, &[], referenced_tables, &[], &[])
    }

    fn execute_select_with_params(&self, conn: &Connection, sql: &str, params: &[Option<String>], referenced_tables: &[String], column_aliases: &[String], column_types: &[Option<String>]) -> Result<Vec<Response>> {
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();

        let fields: Arc<Vec<FieldInfo>> = Arc::new(self.build_field_info(&stmt, referenced_tables, conn, column_aliases, column_types)?);

        let mut data_rows = Vec::new();

        let rusqlite_params: Vec<rusqlite::types::Value> = params.iter().map(|p| {
            match p {
                Some(s) => rusqlite::types::Value::Text(s.clone()),
                None => rusqlite::types::Value::Null,
            }
        }).collect();

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
                    .or_else(|| row.get::<_, Option<f64>>(i).ok().map(|v| v.map(|x| x.to_string())))
                    .or_else(|| row.get::<_, Option<String>>(i).ok().flatten().map(Some))
                    .unwrap_or(None);

                encoder.encode_field(&value)?;
            }

            data_rows.push(Ok(encoder.take_row()));
        }

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(fields, row_stream))])
    }

    fn execute_statement_with_params(&self, conn: &Connection, sql: &str, params: &[Option<String>]) -> Result<Vec<Response>> {
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
            Tag::new("INSERT").with_oid(0).with_rows(changes)
        } else if upper_sql.starts_with("UPDATE") {
            Tag::new("UPDATE").with_rows(changes)
        } else if upper_sql.starts_with("DELETE") {
            Tag::new("DELETE").with_rows(changes)
        } else if upper_sql.starts_with("CREATE") {
            Tag::new("CREATE")
        } else if upper_sql.starts_with("DROP") {
            Tag::new("DROP")
        } else if upper_sql.starts_with("ALTER") {
            Tag::new("ALTER")
        } else {
            Tag::new("OK")
        };

        Ok(vec![Response::Execution(tag)])
    }

    fn execute_statement(&self, conn: &Connection, sql: &str) -> Result<Vec<Response>> {
        self.execute_statement_with_params(conn, sql, &[])
    }

    fn build_field_info(
        &self,
        sqlite_stmt: &rusqlite::Statement,
        referenced_tables: &[String],
        conn: &Connection,
        column_aliases: &[String],
        column_types: &[Option<String>],
    ) -> Result<Vec<FieldInfo>> {
        use crate::handler::rewriter::{map_original_type_to_pg_type, ColumnFieldInfo};
        use pgwire::api::results::{FieldFormat, FieldInfo};
        use pgwire::api::Type;
        use std::collections::HashMap;

        let col_count = sqlite_stmt.column_count();
        let mut fields = Vec::with_capacity(col_count);

        let mut table_columns: HashMap<String, Vec<ColumnFieldInfo>> = HashMap::new();
        for table in referenced_tables {
            if let Ok(columns) = crate::catalog::get_table_metadata(conn, table) {
                let field_infos: Vec<ColumnFieldInfo> = columns
                    .into_iter()
                    .map(|col| {
                        let pg_type = map_original_type_to_pg_type(&col.original_type);
                        ColumnFieldInfo {
                            name: col.column_name,
                            pg_type,
                        }
                    })
                    .collect();
                table_columns.insert(table.to_lowercase(), field_infos);
            }
        }

        for i in 0..col_count {
            let col_name = sqlite_stmt.column_name(i)?.to_string();
            
            if i < column_aliases.len() && !column_aliases[i].is_empty() && column_aliases[i] == col_name {
                if i < column_types.len() {
                    if let Some(ref type_name) = column_types[i] {
                        let pg_type = map_original_type_to_pg_type(type_name);
                        fields.push(FieldInfo::new(col_name.clone(), None, None, pg_type, FieldFormat::Text));
                        continue;
                    }
                }
            }

            let mut found = false;
            let lower_name = col_name.to_lowercase();
            for (_table, columns) in &table_columns {
                if let Some(info) = columns.iter().find(|c| c.name.to_lowercase() == lower_name) {
                    fields.push(FieldInfo::new(col_name.clone(), None, None, info.pg_type.clone(), FieldFormat::Text));
                    found = true;
                    break;
                }
            }

            if !found {
                fields.push(FieldInfo::new(col_name, None, None, Type::TEXT, FieldFormat::Text));
            }
        }

        Ok(fields)
    }

    fn execute_dml_with_triggers(
        &self,
        conn: &Connection,
        original_sql: &str,
        sqlite_sql: &str,
        operation: crate::transpiler::OperationType,
    ) -> Result<Vec<Response>> {
        use crate::trigger::{TriggerExecutor, OperationType};

        let (table_name, trigger_op) = match operation {
            crate::transpiler::OperationType::INSERT => {
                let table = extract_table_and_operation(original_sql).unwrap().0;
                (table, OperationType::Insert)
            }
            crate::transpiler::OperationType::UPDATE => {
                let table = extract_table_and_operation(original_sql).unwrap().0;
                (table, OperationType::Update)
            }
            crate::transpiler::OperationType::DELETE => {
                let table = extract_table_and_operation(original_sql).unwrap().0;
                (table, OperationType::Delete)
            }
            _ => return self.execute_statement(conn, sqlite_sql),
        };

        if table_name.is_empty() {
            return self.execute_statement(conn, sqlite_sql);
        }

        let trigger_executor = TriggerExecutor::new(self.functions().clone());

        if trigger_op == OperationType::Update || trigger_op == OperationType::Delete {
            return self.execute_multi_row_dml_with_triggers(
                conn,
                &table_name,
                original_sql,
                sqlite_sql,
                trigger_op,
            );
        }

        let old_row = None;
        let new_row = crate::trigger::rows::extract_inserted_row(original_sql);
        let new_row_clone = new_row.clone();

        match trigger_executor.execute_before_triggers(
            conn,
            &table_name,
            trigger_op,
            old_row.clone(),
            new_row,
        )? {
            BeforeTriggerResult::Abort => {
                Ok(vec![Response::Execution(Tag::new("OK"))])
            }
            BeforeTriggerResult::Continue(modified_new_row) => {
                let result = self.execute_statement(conn, sqlite_sql)?;

                if let Some(modified) = &modified_new_row {
                    let columns_to_update: Vec<(String, rusqlite::types::Value)> = if let Some(ref original) = new_row_clone {
                        modified
                            .iter()
                            .filter(|(col, val)| {
                                original.get(*col) != Some(val.clone())
                            })
                            .map(|(col, val)| (col.clone(), val.clone()))
                            .collect()
                    } else {
                        modified.iter().map(|(col, val)| (col.clone(), val.clone())).collect()
                    };
                    
                    if !columns_to_update.is_empty() {
                        let rowid: i64 = conn.last_insert_rowid();
                        let updates: Vec<String> = columns_to_update
                            .iter()
                            .map(|(col, val)| format!("{} = {}", col, value_to_sql(val)))
                            .collect();
                        
                        let update_sql = format!(
                            "UPDATE {} SET {} WHERE rowid = {}",
                            table_name,
                            updates.join(", "),
                            rowid
                        );
                        let _ = conn.execute(&update_sql, []);
                    }
                }

                let after_new_row = fetch_inserted_row(conn, &table_name).ok();
                trigger_executor.execute_after_triggers(
                    conn,
                    &table_name,
                    trigger_op,
                    old_row.clone(), 
                    after_new_row,
                )?;

                Ok(result)
            }
        }
    }

    fn execute_multi_row_dml_with_triggers(
        &self,
        conn: &Connection,
        table_name: &str,
        original_sql: &str,
        sqlite_sql: &str,
        operation: crate::trigger::OperationType,
    ) -> Result<Vec<Response>> {
        use crate::trigger::rows::{extract_update_expressions, row_to_map};

        let table_oid = crate::catalog::trigger::calc_table_oid(table_name);
        let triggers = crate::catalog::get_triggers_for_table(
            conn,
            table_oid,
            None,
            Some(operation.to_trigger_event()),
        )?;

        if triggers.is_empty() {
            return self.execute_statement(conn, sqlite_sql);
        }

        let lower_sql = sqlite_sql.to_lowercase();
        let where_sql = if let Some(where_pos) = lower_sql.find(" where ") {
            &sqlite_sql[where_pos + 7..]
        } else {
            "1=1"
        };

        let trigger_executor = TriggerExecutor::new(self.functions().clone());
        let mut affected_rows = 0;

        if operation == crate::trigger::OperationType::Update {
            let updates = extract_update_expressions(original_sql)?;
            let select_sql = format!("SELECT *, rowid FROM {} WHERE {}", table_name, where_sql);
            
            let rows_data = {
                let mut stmt = conn.prepare(&select_sql)?;
                let mut rows = stmt.query([])?;
                let mut results = Vec::new();
                while let Some(row) = rows.next()? {
                    results.push((row_to_map(row)?, row.get::<_, i64>("rowid")?));
                }
                results
            };

            for (old_row_map, rowid) in rows_data {
                let mut new_row_map = old_row_map.clone();
                for (col, expr) in &updates {
                    new_row_map.insert(col.clone(), rusqlite::types::Value::Text(expr.clone()));
                }

                match trigger_executor.execute_before_triggers(conn, table_name, operation, Some(old_row_map.clone()), Some(new_row_map))? {
                    BeforeTriggerResult::Abort => continue,
                    BeforeTriggerResult::Continue(modified_row) => {
                        let final_row = modified_row.unwrap_or_else(|| {
                            let mut m = old_row_map.clone();
                            for (col, expr) in &updates {
                                m.insert(col.clone(), rusqlite::types::Value::Text(expr.clone()));
                            }
                            m
                        });

                        let set_clauses: Vec<String> = final_row.iter()
                            .filter(|(k, _)| *k != "rowid")
                            .map(|(k, v)| format!("{} = {}", k, value_to_sql(v)))
                            .collect();
                        
                        let update_stmt = format!("UPDATE {} SET {} WHERE rowid = {}", table_name, set_clauses.join(", "), rowid);
                        affected_rows += conn.execute(&update_stmt, [])?;

                        let after_row = fetch_inserted_row(conn, table_name).ok();
                        trigger_executor.execute_after_triggers(conn, table_name, operation, Some(old_row_map), after_row)?;
                    }
                }
            }
        } else {
            let select_sql = format!("SELECT *, rowid FROM {} WHERE {}", table_name, where_sql);
            
            let rows_data = {
                let mut stmt = conn.prepare(&select_sql)?;
                let mut rows = stmt.query([])?;
                let mut results = Vec::new();
                while let Some(row) = rows.next()? {
                    results.push((row_to_map(row)?, row.get::<_, i64>("rowid")?));
                }
                results
            };

            for (old_row_map, rowid) in rows_data {
                match trigger_executor.execute_before_triggers(conn, table_name, operation, Some(old_row_map.clone()), None)? {
                    BeforeTriggerResult::Abort => continue,
                    BeforeTriggerResult::Continue(_) => {
                        let delete_stmt = format!("DELETE FROM {} WHERE rowid = {}", table_name, rowid);
                        affected_rows += conn.execute(&delete_stmt, [])?;
                        trigger_executor.execute_after_triggers(conn, table_name, operation, Some(old_row_map), None)?;
                    }
                }
            }
        }

        let tag = if operation == crate::trigger::OperationType::Update {
            Tag::new("UPDATE").with_rows(affected_rows)
        } else {
            Tag::new("DELETE").with_rows(affected_rows)
        };

        Ok(vec![Response::Execution(tag)])
    }

    fn copy_handler(&self) -> &copy::CopyHandler;
}

fn value_to_sql(val: &rusqlite::types::Value) -> String {
    match val {
        rusqlite::types::Value::Null => "NULL".to_string(),
        rusqlite::types::Value::Integer(i) => i.to_string(),
        rusqlite::types::Value::Real(f) => f.to_string(),
        rusqlite::types::Value::Text(s) => format!("'{}'", s.replace('\'', "''")),
        rusqlite::types::Value::Blob(b) => format!("X'{}'", hex::encode(b)),
    }
}
