//! PL/pgSQL Lua runtime
//!
//! Provides a sandboxed Lua execution environment for running
//! PL/pgSQL functions that have been transpiled to Lua.

use mlua::{Lua, Table, Value as LuaValue, Function as LuaFunction};
use rusqlite::{Connection, types::Value as SqliteValue, OptionalExtension};
use anyhow::Result;
use std::sync::{Arc, Mutex};
// use std::collections::HashMap;

/// Runtime environment for executing PL/pgSQL (transpiled to Lua)
pub struct PlPgSqlRuntime {
    lua: Lua,
}

/// Execution context passed to Lua functions
pub struct ExecutionContext {
    conn: Arc<Mutex<Connection>>,
    /// Current SQLSTATE
    pub sqlstate: Option<String>,
    /// Current error message
    pub sqlerrm: Option<String>,
    /// Row count from last operation
    pub row_count: i64,
    /// Result set for RETURN NEXT
    pub result_set: Vec<Vec<SqliteValue>>,
    /// Return OID from last INSERT (if applicable)
    pub return_oid: Option<i64>,
    /// PG_CONTEXT string (stack trace)
    pub pg_context: Option<String>,
}

impl PlPgSqlRuntime {
    /// Create a new PL/pgSQL runtime with Luau sandbox
    pub fn new() -> Result<Self> {
        // Create Luau VM with limited stdlib
        let lua = Lua::new_with(
            mlua::StdLib::TABLE | mlua::StdLib::STRING | mlua::StdLib::MATH,
            mlua::LuaOptions::new(),
        )?;
        
        let runtime = Self { lua };
        
        // Register built-in functions
        runtime.register_builtins()?;
        
        Ok(runtime)
    }
    
    fn register_builtins(&self) -> Result<()> {
        let globals = self.lua.globals();
        
        // select(n, ...) - Access function arguments from _args table
        let select_fn = self.lua.create_function(|lua, n: i64| {
            let args: Table = lua.globals().get("_args")?;
            let value: LuaValue = args.get(n)?;
            Ok(value)
        })?;
        globals.set("select", select_fn)?;
        
        Ok(())
    }
    
    /// Execute a PL/pgSQL function
    pub fn execute_function(
        &self,
        _conn: &Connection,
        lua_code: &str,
        args: &[SqliteValue],
    ) -> Result<SqliteValue> {
        // Compile the Lua code to get the function
        let func: LuaFunction = self.lua.load(lua_code).eval()?;
        
        // Create execution context with a new connection
        let ctx = ExecutionContext {
            conn: Arc::new(Mutex::new(Connection::open_in_memory()?)),
            sqlstate: None,
            sqlerrm: None,
            row_count: 0,
            result_set: Vec::new(),
            return_oid: None,
            pg_context: None,
        };
        
        // Create API table
        let api = ctx.create_api_table(&self.lua)?;
        
        // Convert args to Lua values and store in a table
        let args_table = self.lua.create_table()?;
        for (i, arg) in args.iter().enumerate() {
            args_table.set((i + 1) as i64, sqlite_to_lua(&self.lua, arg.clone())?)?;
        }
        self.lua.globals().set("_args", args_table)?;
        
        // Call function with api as first argument
        let result: LuaValue = func.call(api)?;
        
        // Check if result is a result set (table with return_next values)
        if let LuaValue::Table(ref t) = result {
            // Check if this is a result set by looking for _is_result_set marker
            let is_result_set: bool = t.get("_is_result_set").unwrap_or(false);
            if is_result_set {
                // Return the result set as a Vec of Values
                // For now, we return a special marker
                // In a full implementation, we'd return this as a SETOF result
                return Ok(SqliteValue::Text("SETOF result".to_string()));
            }
        }
        
        // Convert result back to SQLite
        lua_to_sqlite(result).map_err(|e| anyhow::anyhow!("Lua error: {}", e))
    }
}

impl ExecutionContext {
    /// Create a Lua table with the PGQT API
    fn create_api_table(&self, lua: &Lua) -> Result<Table> {
        let api = lua.create_table()?;
        let conn = self.conn.clone();
        
        // _ctx.scalar(query, params) -> value
        let scalar_fn = lua.create_function(move |lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            // Convert to references for rusqlite
            let param_refs: Vec<&dyn rusqlite::ToSql> = sqlite_params.iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect();
            
            let result: Option<SqliteValue> = conn.query_row(&query, &*param_refs, |row| {
                row.get(0)
            }).optional().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            Ok(sqlite_to_lua(lua, result.unwrap_or(SqliteValue::Null))?)
        })?;
        api.set("scalar", scalar_fn)?;
        
        // _ctx.query(query, params) -> table of rows
        let conn2 = self.conn.clone();
        let query_fn = lua.create_function(move |lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn2.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            let mut stmt = conn.prepare(&query).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            let column_count = stmt.column_count();
            
            // Collect column names before iterating
            let mut col_names = Vec::new();
            for i in 0..column_count {
                let col_name = stmt.column_name(i).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                col_names.push(col_name.to_string());
            }
            
            let rows_table = lua.create_table()?;
            let mut row_idx = 1;
            
            // Convert to references for rusqlite
            let param_refs: Vec<&dyn rusqlite::ToSql> = sqlite_params.iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect();
            
            let mut rows = stmt.query(&*param_refs).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            while let Some(row) = rows.next().map_err(|e| mlua::Error::RuntimeError(e.to_string()))? {
                let row_table = lua.create_table()?;
                for (i, col_name) in col_names.iter().enumerate() {
                    let value: SqliteValue = row.get(i).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                    row_table.set(col_name.as_str(), sqlite_to_lua(lua, value)?)?;
                }
                rows_table.set(row_idx, row_table)?;
                row_idx += 1;
            }
            
            Ok(rows_table)
        })?;
        api.set("query", query_fn)?;
        
        // _ctx.query_iter(query, params) -> iterator function
        // Simplified version - returns a function that returns rows one by one
        let conn3 = self.conn.clone();
        let query_iter_fn = lua.create_function(move |lua, (_query, params): (String, Vec<LuaValue>)| {
            let _sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            // Store query state in a closure
            let _conn = conn3.clone();
            let iter = lua.create_function_mut(move |_lua, (): ()| {
                // This is a simplified implementation
                // A full implementation would maintain cursor state
                Ok(LuaValue::Nil)
            })?;
            Ok(iter)
        })?;
        api.set("query_iter", query_iter_fn)?;
        
        // _ctx.exec(query, params) -> row_count
        let conn4 = self.conn.clone();
        let exec_fn = lua.create_function(move |_lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn4.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            // Convert to references for rusqlite
            let param_refs: Vec<&dyn rusqlite::ToSql> = sqlite_params.iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect();
            
            let rows_affected = conn.execute(&query, &*param_refs)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            Ok(rows_affected as i64)
        })?;
        api.set("exec", exec_fn)?;
        
        // _ctx.perform(query, params) -> nil
        let conn5 = self.conn.clone();
        let perform_fn = lua.create_function(move |_lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn5.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            // Convert to references for rusqlite
            let param_refs: Vec<&dyn rusqlite::ToSql> = sqlite_params.iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect();
            
            conn.execute(&query, &*param_refs)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            Ok(())
        })?;
        api.set("perform", perform_fn)?;
        
        // _ctx.execute(query, params) -> nil (dynamic SQL)
        let conn6 = self.conn.clone();
        let execute_fn = lua.create_function(move |_lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn6.lock().map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            // Convert to references for rusqlite
            let param_refs: Vec<&dyn rusqlite::ToSql> = sqlite_params.iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect();
            
            conn.execute(&query, &*param_refs)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            
            Ok(())
        })?;
        api.set("execute", execute_fn)?;
        
        // _ctx.raise(level, message, params)
        let raise_fn = lua.create_function(|_lua, (level, message, params): (String, String, Option<Vec<LuaValue>>)| {
            let formatted = match params {
                Some(args) if !args.is_empty() => {
                    // Simple %s substitution - convert LuaValue to string
                    let mut result = message;
                    for arg in args {
                        let arg_str = match arg {
                            LuaValue::String(s) => s.to_string_lossy().to_string(),
                            LuaValue::Integer(i) => i.to_string(),
                            LuaValue::Number(n) => n.to_string(),
                            LuaValue::Boolean(b) => b.to_string(),
                            LuaValue::Nil => "NULL".to_string(),
                            _ => format!("{:?}", arg),
                        };
                        result = result.replacen("%s", &arg_str, 1);
                    }
                    result
                }
                _ => message,
            };
            
            match level.as_str() {
                "DEBUG" => println!("DEBUG: {}", formatted),
                "INFO" => println!("INFO: {}", formatted),
                "NOTICE" => println!("NOTICE: {}", formatted),
                "WARNING" => eprintln!("WARNING: {}", formatted),
                _ => println!("{}: {}", level, formatted),
            }
            
            Ok(())
        })?;
        api.set("raise", raise_fn)?;
        
        // _ctx.raise_exception(message, options)
        let raise_ex_fn = lua.create_function(|_lua, (message, options): (String, Option<Table>)| -> Result<(), mlua::Error> {
            let errcode: String = match options {
                Some(t) => match t.get("errcode") {
                    Ok(v) => v,
                    Err(_) => "P0001".to_string(),
                },
                None => "P0001".to_string(),
            };
            
            Err(mlua::Error::RuntimeError(format!(
                "{{\"sqlstate\": \"{}\", \"message\": \"{}\"}}",
                errcode, message
            )))
        })?;
        api.set("raise_exception", raise_ex_fn)?;
        
        // _ctx.quote_ident(ident) -> quoted identifier
        let quote_ident_fn = lua.create_function(|_lua, ident: String| {
            // Simple implementation - production would handle reserved words
            if ident.chars().all(|c| c.is_alphanumeric() || c == '_') 
                && !ident.chars().next().unwrap_or('_').is_ascii_digit() {
                Ok(ident)
            } else {
                Ok(format!("\"{}\"", ident.replace('"', "\"\"")))
            }
        })?;
        api.set("quote_ident", quote_ident_fn)?;
        
        // Special variables for GET DIAGNOSTICS
        if let Some(sqlstate) = &self.sqlstate {
            api.set("SQLSTATE", sqlstate.clone())?;
        }
        if let Some(sqlerrm) = &self.sqlerrm {
            api.set("SQLERRM", sqlerrm.clone())?;
        }
        
        // ROW_COUNT - rows affected by last command
        api.set("ROW_COUNT", self.row_count)?;
        
        // RESULT_OID - OID from last INSERT (if applicable)
        if let Some(oid) = self.return_oid {
            api.set("RESULT_OID", oid)?;
        }
        
        // PG_CONTEXT - stack trace (simplified)
        if let Some(context) = &self.pg_context {
            api.set("PG_CONTEXT", context.clone())?;
        }
        
        Ok(api)
    }
}

/// Convert Lua value to SQLite value
fn lua_to_sqlite(value: LuaValue) -> Result<SqliteValue, mlua::Error> {
    match value {
        LuaValue::Nil => Ok(SqliteValue::Null),
        LuaValue::Boolean(b) => Ok(SqliteValue::Integer(if b { 1 } else { 0 })),
        LuaValue::Integer(i) => Ok(SqliteValue::Integer(i as i64)),
        LuaValue::Number(n) => Ok(SqliteValue::Real(n)),
        LuaValue::String(s) => Ok(SqliteValue::Text(s.to_str()?.to_string())),
        _ => Err(mlua::Error::RuntimeError(
            format!("Cannot convert Lua value to SQLite: {:?}", value)
        )),
    }
}

/// Convert SQLite value to Lua value
fn sqlite_to_lua(lua: &Lua, value: SqliteValue) -> Result<LuaValue, mlua::Error> {
    match value {
        SqliteValue::Null => Ok(LuaValue::Nil),
        SqliteValue::Integer(i) => Ok(LuaValue::Integer(i as i32)),
        SqliteValue::Real(f) => Ok(LuaValue::Number(f)),
        SqliteValue::Text(s) => Ok(LuaValue::String(lua.create_string(&s)?)),
        SqliteValue::Blob(b) => Ok(LuaValue::String(lua.create_string(&b)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = PlPgSqlRuntime::new();
        assert!(runtime.is_ok());
    }

    #[test]
    fn test_execute_simple_function() {
        let runtime = PlPgSqlRuntime::new().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        
        // Simple Lua function that returns the first argument
        let lua_code = r#"
            local function test(_ctx)
                local a = select(1)
                local b = select(2)
                return a + b
            end
            return test
        "#;
        
        let result = runtime.execute_function(
            &conn, lua_code, &[
                SqliteValue::Integer(5),
                SqliteValue::Integer(3),
            ]
        );
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SqliteValue::Integer(8));
    }

    #[test]
    fn test_scalar_api() {
        let runtime = PlPgSqlRuntime::new().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        
        let lua_code = r#"
            local function test(_ctx)
                return _ctx.scalar("SELECT 1 + 1", {})
            end
            return test
        "#;
        
        let result = runtime.execute_function(&conn, lua_code, &[]).unwrap();
        assert_eq!(result, SqliteValue::Integer(2));
    }

    #[test]
    fn test_raise_api() {
        let runtime = PlPgSqlRuntime::new().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        
        let lua_code = r#"
            local function test(_ctx)
                _ctx.raise("NOTICE", "Test message: %s", {"hello"})
                return 1
            end
            return test
        "#;
        
        let result = runtime.execute_function(&conn, lua_code, &[]).unwrap();
        assert_eq!(result, SqliteValue::Integer(1));
    }
}
