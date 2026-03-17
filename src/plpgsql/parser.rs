use anyhow::Result;
use crate::plpgsql::ast::PlpgsqlFunction;

/// Function metadata extracted from CREATE FUNCTION statement
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FunctionMetadata {
    pub name: String,
    pub arg_names: Vec<String>,
    pub arg_types: Vec<String>,
    pub return_type: String,
}

/// Parse a single PL/pgSQL function and return its AST
#[cfg(feature = "plpgsql")]
pub fn parse_plpgsql_function(sql: &str) -> Result<PlpgsqlFunction> {
    use anyhow::Context;
    
    let metadata = extract_function_metadata(sql)?;
    
    
    let json = pg_parse::parse_plpgsql(sql)
        .map_err(|e| anyhow::anyhow!("Failed to parse PL/pgSQL: {:?}", e))?;
    
    
    let func_array = json.as_array()
        .ok_or_else(|| anyhow::anyhow!("Expected array from pg_parse, got: {:?}", json))?;
    
    if func_array.is_empty() {
        anyhow::bail!("No functions found in PL/pgSQL source");
    }
    
    
    let func_json = func_array[0].get("PLpgSQL_function")
        .ok_or_else(|| anyhow::anyhow!("Expected PLpgSQL_function in AST: {:?}", func_array[0]))?;
    
    
    let mut function: PlpgsqlFunction = serde_json::from_value(func_json.clone())
        .context("Failed to deserialize PL/pgSQL AST")?;
    
    
    function.fn_name = Some(metadata.name);
    
    Ok(function)
}

/// Stub implementation when plpgsql feature is disabled (e.g., on Windows)
#[cfg(not(feature = "plpgsql"))]
pub fn parse_plpgsql_function(_sql: &str) -> Result<PlpgsqlFunction> {
    anyhow::bail!("PL/pgSQL support is not available on this platform. Enable the 'plpgsql' feature to use this functionality.")
}

/// Extract function metadata from CREATE FUNCTION statement using pg_query
#[cfg(feature = "plpgsql")]
fn extract_function_metadata(sql: &str) -> Result<FunctionMetadata> {
    use pg_query::protobuf::node::Node as NodeEnum;
    
    let result = pg_query::parse(sql)
        .map_err(|e| anyhow::anyhow!("Failed to parse CREATE FUNCTION: {}", e))?;
    
    
    let stmt = result.protobuf.stmts.first()
        .and_then(|s| s.stmt.as_ref())
        .ok_or_else(|| anyhow::anyhow!("No statement found in SQL"))?;
    
    
    if let Some(NodeEnum::CreateFunctionStmt(func_stmt)) = &stmt.node {
        
        let name = func_stmt.funcname.first()
            .and_then(|n| n.node.as_ref())
            .and_then(|n| match n {
                NodeEnum::String(s) => Some(s.sval.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "anonymous".to_string());
        
        
        
        return Ok(FunctionMetadata {
            name,
            arg_names: Vec::new(),
            arg_types: Vec::new(),
            return_type: "void".to_string(),
        });
    }
    
    Err(anyhow::anyhow!("Expected CREATE FUNCTION statement"))
}

/// Parse multiple functions (e.g., from CREATE OR REPLACE FUNCTION batch)
#[cfg(feature = "plpgsql")]
#[allow(dead_code)]
pub fn parse_plpgsql_batch(sql: &str) -> Result<Vec<PlpgsqlFunction>> {
    use anyhow::Context;
    
    let json = pg_parse::parse_plpgsql(sql)
        .map_err(|e| anyhow::anyhow!("Failed to parse PL/pgSQL batch: {:?}", e))?;
    
    
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

/// Stub implementation when plpgsql feature is disabled
#[cfg(not(feature = "plpgsql"))]
#[allow(dead_code)]
pub fn parse_plpgsql_batch(_sql: &str) -> Result<Vec<PlpgsqlFunction>> {
    anyhow::bail!("PL/pgSQL support is not available on this platform. Enable the 'plpgsql' feature to use this functionality.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let sql = r#"
            CREATE OR REPLACE FUNCTION test_func()
            RETURNS INTEGER AS $$
            BEGIN
                RETURN 42;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let result = parse_plpgsql_function(sql);
        
        #[cfg(feature = "plpgsql")]
        {
            assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
            let func = result.unwrap();
            assert_eq!(func.fn_name, Some("test_func".to_string()));
        }
        
        #[cfg(not(feature = "plpgsql"))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_parse_function_with_args() {
        let sql = r#"
            CREATE OR REPLACE FUNCTION add(a INTEGER, b INTEGER)
            RETURNS INTEGER AS $$
            BEGIN
                RETURN a + b;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let result = parse_plpgsql_function(sql);
        
        #[cfg(feature = "plpgsql")]
        {
            assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
            let func = result.unwrap();
            assert_eq!(func.fn_name, Some("add".to_string()));
        }
        
        #[cfg(not(feature = "plpgsql"))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_parse_function_with_if() {
        let sql = r#"
            CREATE OR REPLACE FUNCTION max_val(a INTEGER, b INTEGER)
            RETURNS INTEGER AS $$
            BEGIN
                IF a > b THEN
                    RETURN a;
                ELSE
                    RETURN b;
                END IF;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let result = parse_plpgsql_function(sql);
        
        #[cfg(feature = "plpgsql")]
        {
            assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
            let func = result.unwrap();
            assert_eq!(func.fn_name, Some("max_val".to_string()));
        }
        
        #[cfg(not(feature = "plpgsql"))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_parse_function_with_loop() {
        let sql = r#"
            CREATE OR REPLACE FUNCTION sum_n(n INTEGER)
            RETURNS INTEGER AS $$
            DECLARE
                i INTEGER := 0;
                total INTEGER := 0;
            BEGIN
                WHILE i < n LOOP
                    total := total + i;
                    i := i + 1;
                END LOOP;
                RETURN total;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let result = parse_plpgsql_function(sql);
        
        #[cfg(feature = "plpgsql")]
        {
            assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
            let func = result.unwrap();
            assert_eq!(func.fn_name, Some("sum_n".to_string()));
        }
        
        #[cfg(not(feature = "plpgsql"))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_parse_function_with_raise() {
        let sql = r#"
            CREATE OR REPLACE FUNCTION log_message(msg TEXT)
            RETURNS VOID AS $$
            BEGIN
                RAISE NOTICE 'Message: %', msg;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let result = parse_plpgsql_function(sql);
        
        #[cfg(feature = "plpgsql")]
        {
            assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
            let func = result.unwrap();
            assert_eq!(func.fn_name, Some("log_message".to_string()));
        }
        
        #[cfg(not(feature = "plpgsql"))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_parse_invalid_plpgsql() {
        let sql = "This is not valid PL/pgSQL";
        let result = parse_plpgsql_function(sql);
        assert!(result.is_err());
    }
}
