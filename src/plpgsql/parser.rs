//! PL/pgSQL parser using pg_parse
//!
//! This module parses PL/pgSQL function source code into our AST types
//! using the pg_parse library's parse_plpgsql() function.

use anyhow::{Result, Context};
use crate::plpgsql::ast::PlpgsqlFunction;

/// Parse a single PL/pgSQL function and return its AST
pub fn parse_plpgsql_function(sql: &str) -> Result<PlpgsqlFunction> {
    // Use pg_parse to get JSON AST
    let json = pg_parse::parse_plpgsql(sql)
        .map_err(|e| anyhow::anyhow!("Failed to parse PL/pgSQL: {:?}", e))?;
    
    // pg_parse returns an array of function objects
    let func_array = json.as_array()
        .ok_or_else(|| anyhow::anyhow!("Expected array from pg_parse, got: {:?}", json))?;
    
    if func_array.is_empty() {
        anyhow::bail!("No functions found in PL/pgSQL source");
    }
    
    // Extract first PLpgSQL_function object
    let func_json = func_array[0].get("PLpgSQL_function")
        .ok_or_else(|| anyhow::anyhow!("Expected PLpgSQL_function in AST: {:?}", func_array[0]))?;
    
    // Deserialize to our Rust types
    let function: PlpgsqlFunction = serde_json::from_value(func_json.clone())
        .context("Failed to deserialize PL/pgSQL AST")?;
    
    Ok(function)
}

/// Parse multiple functions (e.g., from CREATE OR REPLACE FUNCTION batch)
pub fn parse_plpgsql_batch(sql: &str) -> Result<Vec<PlpgsqlFunction>> {
    let json = pg_parse::parse_plpgsql(sql)
        .map_err(|e| anyhow::anyhow!("Failed to parse PL/pgSQL batch: {:?}", e))?;
    
    // pg_parse returns an array for multiple functions
    let func_array = json.as_array()
        .ok_or_else(|| anyhow::anyhow!("Expected array from pg_parse"))?;
    
    let functions: Vec<PlpgsqlFunction> = func_array
        .iter()
        .map(|v| {
            let func_json = v.get("PLpgSQL_function")
                .ok_or_else(|| anyhow::anyhow!("Expected PLpgSQL_function"))?;
            serde_json::from_value(func_json.clone())
                .context("Failed to deserialize function")
        })
        .collect::<Result<Vec<_>>>()?;
    
    Ok(functions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let sql = r#"
            CREATE FUNCTION add(a int, b int) RETURNS int AS $$
            BEGIN
                RETURN a + b;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let result = parse_plpgsql_function(sql);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
        
        let func = result.unwrap();
        // Note: pg_parse doesn't provide fn_name in the output
        // We may need to extract it separately from CREATE FUNCTION statement
        assert!(!func.fn_body().is_empty(), "Function body should not be empty");
    }

    #[test]
    fn test_parse_function_with_args() {
        let sql = r#"
            CREATE FUNCTION greet(name text) RETURNS text AS $$
            BEGIN
                RETURN 'Hello, ' || name;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        // Note: pg_parse doesn't provide fn_name in the output
        assert!(!func.fn_body().is_empty());
        
        // Check argument names - pg_parse provides these in datums
        // For now, just verify parsing succeeded
    }

    #[test]
    fn test_parse_function_with_if() {
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
        
        // Should have a block with an IF statement
        assert!(!func.fn_body().is_empty());
    }

    #[test]
    fn test_parse_function_with_loop() {
        let sql = r#"
            CREATE FUNCTION counter() RETURNS int AS $$
            DECLARE
                i int := 0;
                total int := 0;
            BEGIN
                WHILE i < 10 LOOP
                    total := total + i;
                    i := i + 1;
                END LOOP;
                RETURN total;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        assert!(!func.fn_body().is_empty());
    }

    #[test]
    fn test_parse_function_with_raise() {
        let sql = r#"
            CREATE FUNCTION log_message(msg text) RETURNS void AS $$
            BEGIN
                RAISE NOTICE 'Message: %', msg;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        assert!(!func.fn_body().is_empty());
    }

    #[test]
    fn test_parse_invalid_plpgsql() {
        let sql = "This is not valid PL/pgSQL";
        let result = parse_plpgsql_function(sql);
        assert!(result.is_err());
    }
}
