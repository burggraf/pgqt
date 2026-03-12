//! Trigger execution module
//!
//! This module provides trigger execution functionality for PGQT,
//! enabling PL/pgSQL trigger functions to fire on INSERT/UPDATE/DELETE operations.
//!
//! ## Architecture
//!
//! ```text
//! DML Statement → Parse Table/Operation → Lookup Triggers → Execute BEFORE →
//! Execute DML → Execute AFTER → Return Results
//! ```
//!
//! ## Key Components
//!
//! - [`TriggerExecutor`] - Main trigger execution coordinator
//! - [`build_old_row`] - Build OLD row data from SQLite for UPDATE/DELETE
//! - [`build_new_row`] - Build NEW row data from INSERT/UPDATE values
//! - [`execute_plpgsql_trigger`] - Execute PL/pgSQL trigger function

use anyhow::{anyhow, Result};
use rusqlite::{Connection, types::Value};
use std::collections::HashMap;
use dashmap::DashMap;
use std::sync::Arc;

use crate::catalog::{TriggerMetadata, TriggerTiming, TriggerEvent, FunctionMetadata};
use crate::catalog::trigger::{get_triggers_for_table, calc_table_oid};

pub mod rows;

pub use rows::{build_old_row, build_new_row};

/// Operation type for trigger execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Insert,
    Update,
    Delete,
}

impl OperationType {
    /// Convert to TriggerEvent for looking up applicable triggers
    pub fn to_trigger_event(&self) -> TriggerEvent {
        match self {
            OperationType::Insert => TriggerEvent::Insert,
            OperationType::Update => TriggerEvent::Update,
            OperationType::Delete => TriggerEvent::Delete,
        }
    }
}

/// Result of executing a BEFORE trigger
#[derive(Debug, Clone)]
pub enum BeforeTriggerResult {
    /// Continue with the operation using the (possibly modified) row
    Continue(Option<HashMap<String, Value>>),
    /// Abort the operation (trigger returned NULL)
    Abort,
}

/// Trigger execution coordinator
pub struct TriggerExecutor {
    functions_cache: Arc<DashMap<String, FunctionMetadata>>,
}

impl TriggerExecutor {
    /// Create a new trigger executor
    pub fn new(functions_cache: Arc<DashMap<String, FunctionMetadata>>) -> Self {
        Self { functions_cache }
    }

    /// Execute BEFORE triggers for a table and operation
    ///
    /// Returns the (possibly modified) NEW row, or None if the operation should be aborted
    pub fn execute_before_triggers(
        &self,
        conn: &Connection,
        table_name: &str,
        operation: OperationType,
        old_row: Option<HashMap<String, Value>>,
        new_row: Option<HashMap<String, Value>>,
    ) -> Result<BeforeTriggerResult> {
        let table_oid = calc_table_oid(table_name);
        
        let triggers = get_triggers_for_table(
            conn,
            table_oid,
            Some(TriggerTiming::Before),
            Some(operation.to_trigger_event()),
        )?;

        if triggers.is_empty() {
            return Ok(BeforeTriggerResult::Continue(new_row));
        }

        let mut current_new_row = new_row;

        // Execute triggers in order (by OID)
        for trigger in triggers {
            let result = execute_plpgsql_trigger(
                conn,
                &trigger.function_name,
                &trigger.name,
                "BEFORE",
                &operation_to_string(operation),
                table_name,
                "public",
                &trigger.args,
                old_row.clone(),
                current_new_row.clone(),
                &self.functions_cache,
            )?;

            match result {
                None => {
                    // Trigger returned NULL - abort operation
                    return Ok(BeforeTriggerResult::Abort);
                }
                Some(modified_row) => {
                    // Use the modified row for subsequent triggers and the DML
                    current_new_row = Some(modified_row);
                }
            }
        }

        Ok(BeforeTriggerResult::Continue(current_new_row))
    }

    /// Execute AFTER triggers for a table and operation
    ///
    /// AFTER triggers cannot modify data, so they only return success/failure
    pub fn execute_after_triggers(
        &self,
        conn: &Connection,
        table_name: &str,
        operation: OperationType,
        old_row: Option<HashMap<String, Value>>,
        new_row: Option<HashMap<String, Value>>,
    ) -> Result<()> {
        let table_oid = calc_table_oid(table_name);
        let triggers = get_triggers_for_table(
            conn,
            table_oid,
            Some(TriggerTiming::After),
            Some(operation.to_trigger_event()),
        )?;

        for trigger in triggers {
            // AFTER triggers don't modify rows, so we ignore the return value
            let _ = execute_plpgsql_trigger(
                conn,
                &trigger.function_name,
                &trigger.name,
                "AFTER",
                &operation_to_string(operation),
                table_name,
                "public",
                &trigger.args,
                old_row.clone(),
                new_row.clone(),
                &self.functions_cache,
            )?;
        }

        Ok(())
    }
}

/// Execute a PL/pgSQL trigger function
///
/// Trigger functions receive special variables:
/// - TG_NAME: name of the trigger
/// - TG_WHEN: BEFORE, AFTER, or INSTEAD OF
/// - TG_LEVEL: ROW or STATEMENT
/// - TG_OP: INSERT, UPDATE, DELETE, or TRUNCATE
/// - TG_TABLE_NAME: name of the table
/// - TG_TABLE_SCHEMA: schema of the table
/// - TG_NARGS: number of arguments
/// - TG_ARGV: array of arguments
/// - NEW: the new row (for INSERT/UPDATE)
/// - OLD: the old row (for UPDATE/DELETE)
///
/// For BEFORE triggers, the function can return NULL to abort the operation,
/// or return a row (possibly modified) to change the data.
pub fn execute_plpgsql_trigger(
    conn: &Connection,
    function_name: &str,
    trigger_name: &str,
    trigger_timing: &str,
    trigger_event: &str,
    table_name: &str,
    table_schema: &str,
    trigger_args: &[String],
    old_row: Option<HashMap<String, Value>>,
    new_row: Option<HashMap<String, Value>>,
    functions_cache: &Arc<DashMap<String, FunctionMetadata>>,
) -> Result<Option<HashMap<String, Value>>> {
    // Look up function in catalog
    let metadata = functions_cache.get(function_name)
        .ok_or_else(|| anyhow!("Trigger function {} not found", function_name))?;

    // Create Lua runtime
    let runtime = crate::plpgsql::PlPgSqlRuntime::new()
        .map_err(|e| anyhow!("Failed to create Lua runtime: {}", e))?;

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
        format!("CREATE FUNCTION {}({}) RETURNS TRIGGER AS $${}$$ LANGUAGE plpgsql;", 
            function_name, args_signature, metadata.function_body)
    } else {
        format!("CREATE FUNCTION {}({}) RETURNS TRIGGER AS $$BEGIN {} END;$$ LANGUAGE plpgsql;", 
            function_name, args_signature, metadata.function_body)
    };

    // Parse and transpile to Lua
    let parsed_func = crate::plpgsql::parse_plpgsql_function(&create_sql)
        .map_err(|e| anyhow!("Failed to parse PL/pgSQL: {}", e))?;
    
    let lua_code = crate::plpgsql::transpile_to_lua(&parsed_func)
        .map_err(|e| anyhow!("Failed to transpile to Lua: {}", e))?;

    // Create Lua runtime with trigger variables
    let lua = mlua::Lua::new_with(
        mlua::StdLib::TABLE | mlua::StdLib::STRING | mlua::StdLib::MATH,
        mlua::LuaOptions::new(),
    )?;

    // Set up trigger variables as globals
    let globals = lua.globals();
    globals.set("TG_NAME", trigger_name)?;
    globals.set("TG_WHEN", trigger_timing)?;
    globals.set("TG_LEVEL", "ROW")?;
    globals.set("TG_OP", trigger_event)?;
    globals.set("TG_TABLE_NAME", table_name)?;
    globals.set("TG_TABLE_SCHEMA", table_schema)?;
    globals.set("TG_NARGS", trigger_args.len() as i64)?;
    
    // Set up TG_ARGV as a table
    let argv = lua.create_table()?;
    for (i, arg) in trigger_args.iter().enumerate() {
        argv.set((i + 1) as i64, arg.as_str())?;
    }
    globals.set("TG_ARGV", argv)?;

    // Set up NEW and OLD rows as tables
    // Clone new_row for later use in return value
    let new_row_clone = new_row.clone();
    if let Some(new) = new_row {
        let new_table = lua.create_table()?;
        for (key, value) in new {
            let lua_value = sqlite_to_lua(&lua, value)?;
            new_table.set(key, lua_value)?;
        }
        globals.set("NEW", new_table)?;
    } else {
        globals.set("NEW", mlua::Value::Nil)?;
    }

    if let Some(old) = old_row {
        let old_table = lua.create_table()?;
        for (key, value) in old {
            let lua_value = sqlite_to_lua(&lua, value)?;
            old_table.set(key, lua_value)?;
        }
        globals.set("OLD", old_table)?;
    } else {
        globals.set("OLD", mlua::Value::Nil)?;
    }

    // Load and execute the function
    let func: mlua::Function = lua.load(&lua_code).eval()?;
    
    // Create a minimal API table for the trigger function
    let api = lua.create_table()?;
    
    // Add NOW() function
    let now_fn = lua.create_function(|_lua, ()| {
        Ok(chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.f").to_string())
    })?;
    api.set("now", now_fn)?;
    
    // Call the function
    let result: mlua::Value = func.call(api)?;

    // Process the result
    match result {
        mlua::Value::Nil => {
            // Trigger returned NULL - abort operation
            Ok(None)
        }
        mlua::Value::Table(t) => {
            // Check if this is a row return (has column values)
            // Convert back to HashMap
            let mut row = HashMap::new();
            for pair in t.pairs() {
                let (key, value): (String, mlua::Value) = pair?;
                // Skip internal Lua keys
                if key.starts_with("_") {
                    continue;
                }
                let sqlite_value = lua_to_sqlite(value)?;
                row.insert(key, sqlite_value);
            }
            Ok(Some(row))
        }
        _ => {
            // Other return types - return original new_row
            Ok(new_row_clone)
        }
    }
}

/// Convert SQLite value to Lua value
fn sqlite_to_lua(lua: &mlua::Lua, value: Value) -> Result<mlua::Value> {
    match value {
        Value::Null => Ok(mlua::Value::Nil),
        Value::Integer(i) => Ok(mlua::Value::Integer(i as i32)),
        Value::Real(f) => Ok(mlua::Value::Number(f)),
        Value::Text(s) => Ok(mlua::Value::String(lua.create_string(&s)?)),
        Value::Blob(b) => Ok(mlua::Value::String(lua.create_string(&b)?)),
    }
}

/// Convert Lua value to SQLite value
fn lua_to_sqlite(value: mlua::Value) -> Result<Value> {
    match value {
        mlua::Value::Nil => Ok(Value::Null),
        mlua::Value::Boolean(b) => Ok(Value::Integer(if b { 1 } else { 0 })),
        mlua::Value::Integer(i) => Ok(Value::Integer(i as i64)),
        mlua::Value::Number(n) => Ok(Value::Real(n)),
        mlua::Value::String(s) => Ok(Value::Text(s.to_str()?.to_string())),
        _ => Err(anyhow!("Cannot convert Lua value to SQLite: {:?}", value)),
    }
}

/// Convert operation type to string
fn operation_to_string(op: OperationType) -> &'static str {
    match op {
        OperationType::Insert => "INSERT",
        OperationType::Update => "UPDATE",
        OperationType::Delete => "DELETE",
    }
}

/// Extract table name from an INSERT statement
pub fn extract_table_from_insert(sql: &str) -> Option<String> {
    // Parse the SQL to extract table name
    if let Ok(result) = pg_query::parse(sql) {
        if let Some(raw_stmt) = result.protobuf.stmts.first() {
            if let Some(ref stmt_node) = raw_stmt.stmt {
                if let Some(pg_query::protobuf::node::Node::InsertStmt(stmt)) = &stmt_node.node {
                    return stmt.relation.as_ref().map(|r| r.relname.clone());
                }
            }
        }
    }
    None
}

/// Extract table name from an UPDATE statement
pub fn extract_table_from_update(sql: &str) -> Option<String> {
    if let Ok(result) = pg_query::parse(sql) {
        if let Some(raw_stmt) = result.protobuf.stmts.first() {
            if let Some(ref stmt_node) = raw_stmt.stmt {
                if let Some(pg_query::protobuf::node::Node::UpdateStmt(stmt)) = &stmt_node.node {
                    return stmt.relation.as_ref().map(|r| r.relname.clone());
                }
            }
        }
    }
    None
}

/// Extract table name from a DELETE statement
pub fn extract_table_from_delete(sql: &str) -> Option<String> {
    if let Ok(result) = pg_query::parse(sql) {
        if let Some(raw_stmt) = result.protobuf.stmts.first() {
            if let Some(ref stmt_node) = raw_stmt.stmt {
                if let Some(pg_query::protobuf::node::Node::DeleteStmt(stmt)) = &stmt_node.node {
                    return stmt.relation.as_ref().map(|r| r.relname.clone());
                }
            }
        }
    }
    None
}

/// Extract table name and operation type from a DML statement
pub fn extract_table_and_operation(sql: &str) -> Option<(String, OperationType)> {
    let upper = sql.trim().to_uppercase();
    
    if upper.starts_with("INSERT") {
        extract_table_from_insert(sql).map(|t| (t, OperationType::Insert))
    } else if upper.starts_with("UPDATE") {
        extract_table_from_update(sql).map(|t| (t, OperationType::Update))
    } else if upper.starts_with("DELETE") {
        extract_table_from_delete(sql).map(|t| (t, OperationType::Delete))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::catalog::init_catalog;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_catalog(&conn).unwrap();
        conn
    }

    #[test]
    fn test_extract_table_from_insert() {
        let sql = "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')";
        let table = extract_table_from_insert(sql);
        assert_eq!(table, Some("users".to_string()));
    }

    #[test]
    fn test_extract_table_from_update() {
        let sql = "UPDATE users SET name = 'Bob' WHERE id = 1";
        let table = extract_table_from_update(sql);
        assert_eq!(table, Some("users".to_string()));
    }

    #[test]
    fn test_extract_table_from_delete() {
        let sql = "DELETE FROM users WHERE id = 1";
        let table = extract_table_from_delete(sql);
        assert_eq!(table, Some("users".to_string()));
    }

    #[test]
    fn test_extract_table_and_operation() {
        let insert_sql = "INSERT INTO orders (total) VALUES (100)";
        let result = extract_table_and_operation(insert_sql);
        assert_eq!(result, Some(("orders".to_string(), OperationType::Insert)));

        let update_sql = "UPDATE orders SET total = 200 WHERE id = 1";
        let result = extract_table_and_operation(update_sql);
        assert_eq!(result, Some(("orders".to_string(), OperationType::Update)));

        let delete_sql = "DELETE FROM orders WHERE id = 1";
        let result = extract_table_and_operation(delete_sql);
        assert_eq!(result, Some(("orders".to_string(), OperationType::Delete)));
    }
}
