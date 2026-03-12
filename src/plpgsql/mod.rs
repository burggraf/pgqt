//! PL/pgSQL procedural language support
//!
//! This module provides a complete implementation of PL/pgSQL for PGQT,
//! including parsing, transpilation to Lua, and execution via mlua.
//!
//! # Architecture
//!
//! ```text
//! PL/pgSQL Source → Parser (pg_parse) → AST → Transpiler → Lua → Runtime (mlua)
//! ```
//!
//! # Example
//!
//! ```rust
//! use pgqt::plpgsql::{parse_plpgsql_function, transpile_to_lua, PlPgSqlRuntime};
//!
//! // Parse PL/pgSQL
//! let sql = r#"
//!     CREATE FUNCTION add(a int, b int) RETURNS int AS $$
//!     BEGIN
//!         RETURN a + b;
//!     END;
//!     $$ LANGUAGE plpgsql;
//! "#;
//! let func = parse_plpgsql_function(sql).unwrap();
//!
//! // Transpile to Lua
//! let lua_code = transpile_to_lua(&func).unwrap();
//!
//! // Execute
//! let runtime = PlPgSqlRuntime::new().unwrap();
//! // ... execute with arguments
//! ```

pub mod ast;
pub mod parser;
pub mod runtime;
pub mod sqlstate;
pub mod transpiler;

// Re-export main types and functions
// pub use ast::PlpgsqlFunction;
pub use parser::parse_plpgsql_function; // , parse_plpgsql_batch};
pub use runtime::PlPgSqlRuntime;
pub use transpiler::transpile_to_lua;

use anyhow::Result;
use rusqlite::{Connection, types::Value};
use std::collections::HashMap;
use dashmap::DashMap;
use std::sync::Arc;
use crate::catalog::FunctionMetadata;

#[allow(unused_imports)]
use mlua::{Lua, Value as LuaValue};

/// Execute a PL/pgSQL function by name (looks up in catalog)
#[allow(dead_code)]
pub fn execute_plpgsql_function(
    conn: &Connection,
    function_name: &str,
    args: &[Value],
    functions_cache: &Arc<DashMap<String, FunctionMetadata>>,
) -> Result<Value> {
    let metadata = functions_cache.get(function_name)
        .ok_or_else(|| anyhow::anyhow!("Function {} not found", function_name))?;

    let runtime = PlPgSqlRuntime::new()?;
    
    // We need to reconstruct the CREATE FUNCTION statement from metadata
    // Or just parse the function body if we had a way to handle parameters
    // execute_plpgsql_function in src/functions.rs already does this correctly.
    // This function in mod.rs seems to be a placeholder or for direct calls.
    
    let result = runtime.execute_function(conn, &metadata.function_body, args)?;
    Ok(result)
}

/// Execute a PL/pgSQL trigger function
/// 
/// Trigger functions receive special variables:
/// - NEW: the new row (for INSERT/UPDATE)
/// - OLD: the old row (for UPDATE/DELETE)
/// - TG_NAME: name of the trigger
/// - TG_WHEN: BEFORE, AFTER, or INSTEAD OF
/// - TG_LEVEL: ROW or STATEMENT
/// - TG_OP: INSERT, UPDATE, DELETE, or TRUNCATE
/// - TG_TABLE_NAME: name of the table
/// - TG_TABLE_SCHEMA: schema of the table
/// - TG_NARGS: number of arguments
/// - TG_ARGV: array of arguments
///
/// For BEFORE triggers, the function can return NULL to abort the operation,
/// or return a row (possibly modified) to change the data.
/// 
/// NOTE: This is a simplified stub implementation. Full trigger execution
/// requires setting up the Lua environment with OLD/NEW rows and trigger
/// variables, then executing the trigger function and processing the result.
#[allow(dead_code)]
pub fn execute_plpgsql_trigger(
    _conn: &Connection,
    function_name: &str,
    _trigger_name: &str,
    _trigger_timing: &str,
    _trigger_event: &str,
    _table_name: &str,
    _table_schema: &str,
    _trigger_args: &[String],
    _old_row: Option<HashMap<String, Value>>,
    new_row: Option<HashMap<String, Value>>,
    functions_cache: &Arc<DashMap<String, FunctionMetadata>>,
) -> Result<Option<HashMap<String, Value>>> {
    // Verify the function exists in catalog
    let _metadata = functions_cache.get(function_name)
        .ok_or_else(|| anyhow::anyhow!("Trigger function {} not found", function_name))?;
    
    // For now, just return the new_row unchanged
    // Full implementation would:
    // 1. Transpile the PL/pgSQL function to Lua
    // 2. Set up trigger variables (TG_NAME, TG_OP, NEW, OLD, etc.)
    // 3. Execute the Lua function
    // 4. Process the return value (modified row or NULL)
    Ok(new_row)
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_end_to_end_simple_function() {
        let sql = r#"
            CREATE FUNCTION greet(name text) RETURNS text AS $$
            BEGIN
                RETURN 'Hello, ' || name;
            END;
            $$ LANGUAGE plpgsql;
        "#;

        // Parse
        let func = parse_plpgsql_function(sql).unwrap();
        assert_eq!(func.fn_name, Some("greet".to_string()));

        // Transpile
        let lua_code = transpile_to_lua(&func).unwrap();
        assert!(lua_code.contains("local function greet"));
        assert!(lua_code.contains("return greet"));

        // Runtime can be created
        let runtime = PlPgSqlRuntime::new();
        assert!(runtime.is_ok());
    }

    #[test]
    fn test_end_to_end_with_control_flow() {
        let sql = r#"
            CREATE FUNCTION max_val(a int, b int) RETURNS int AS $$
            BEGIN
                IF a > b THEN
                    RETURN a;
                ELSE
                    RETURN b;
                END IF;
            END;
            $$ LANGUAGE plpgsql;
        "#;

        let func = parse_plpgsql_function(sql).unwrap();
        let lua_code = transpile_to_lua(&func).unwrap();

        // Check that control flow is transpiled
        assert!(lua_code.contains("if"));
        assert!(lua_code.contains("then"));
        assert!(lua_code.contains("else"));
        assert!(lua_code.contains("end"));
    }

    #[test]
    fn test_end_to_end_with_loop() {
        let sql = r#"
            CREATE FUNCTION factorial(n int) RETURNS int AS $$
            DECLARE
                result int := 1;
                i int;
            BEGIN
                FOR i IN 1..n LOOP
                    result := result * i;
                END LOOP;
                RETURN result;
            END;
            $$ LANGUAGE plpgsql;
        "#;

        let func = parse_plpgsql_function(sql).unwrap();
        let lua_code = transpile_to_lua(&func).unwrap();

        // Check loop constructs
        assert!(lua_code.contains("for"));
        assert!(lua_code.contains("do"));
        assert!(lua_code.contains("end"));
    }
}
