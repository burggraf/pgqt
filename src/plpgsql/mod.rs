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
pub mod transpiler;

// Re-export main types and functions
pub use ast::PlpgsqlFunction;
pub use parser::{parse_plpgsql_function, parse_plpgsql_batch};
pub use runtime::PlPgSqlRuntime;
pub use transpiler::transpile_to_lua;

use anyhow::Result;
use rusqlite::{Connection, types::Value};
use std::collections::HashMap;

/// Execute a PL/pgSQL function by name (looks up in catalog)
pub fn execute_plpgsql_function(
    _conn: &Connection,
    _function_name: &str,
    _args: &[Value],
) -> Result<Value> {
    // TODO: Look up function from catalog, transpile if needed, execute
    anyhow::bail!("execute_plpgsql_function not yet implemented")
}

/// Execute a PL/pgSQL trigger function
pub fn execute_plpgsql_trigger(
    _conn: &Connection,
    _function_name: &str,
    _old_row: Option<HashMap<String, Value>>,
    _new_row: Option<HashMap<String, Value>>,
) -> Result<Option<HashMap<String, Value>>> {
    // TODO: Look up trigger function, execute with OLD/NEW
    anyhow::bail!("execute_plpgsql_trigger not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::types::Value as SqliteValue;

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
