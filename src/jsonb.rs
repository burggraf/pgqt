//! JSONB support for PGQT
//!
//! This module provides JSONB-specific functions that extend SQLite's JSON1
//! extension to support PostgreSQL JSONB operations.

use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;
use serde_json::Value as JsonValue;

/// Register all JSONB functions with the SQLite connection
pub fn register_jsonb_functions(conn: &Connection) -> rusqlite::Result<()> {
    // jsonb_contains - Check if JSONB contains another JSONB value
    conn.create_scalar_function(
        "jsonb_contains",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let container = ctx.get::<String>(0)?;
            let contained = ctx.get::<String>(1)?;
            
            let container_val: JsonValue = serde_json::from_str(&container)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            let contained_val: JsonValue = serde_json::from_str(&contained)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            
            Ok(json_contains(&container_val, &contained_val))
        },
    )?;

    // jsonb_contained - Check if JSONB is contained by another JSONB value
    conn.create_scalar_function(
        "jsonb_contained",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let contained = ctx.get::<String>(0)?;
            let container = ctx.get::<String>(1)?;
            
            let contained_val: JsonValue = serde_json::from_str(&contained)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            let container_val: JsonValue = serde_json::from_str(&container)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            
            Ok(json_contains(&container_val, &contained_val))
        },
    )?;

    // jsonb_exists - Check if a key exists in a JSONB object
    conn.create_scalar_function(
        "jsonb_exists",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let json = ctx.get::<String>(0)?;
            let key = ctx.get::<String>(1)?;
            
            let val: JsonValue = serde_json::from_str(&json)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            
            if let JsonValue::Object(map) = val {
                Ok(map.contains_key(&key))
            } else {
                Ok(false)
            }
        },
    )?;

    // jsonb_exists_any - Check if any of the keys exist in a JSONB object
    conn.create_scalar_function(
        "jsonb_exists_any",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let json = ctx.get::<String>(0)?;
            let keys_json = ctx.get::<String>(1)?;
            
            let val: JsonValue = serde_json::from_str(&json)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            let keys: Vec<String> = serde_json::from_str(&keys_json)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            
            if let JsonValue::Object(map) = val {
                Ok(keys.iter().any(|k| map.contains_key(k)))
            } else {
                Ok(false)
            }
        },
    )?;

    // jsonb_exists_all - Check if all keys exist in a JSONB object
    conn.create_scalar_function(
        "jsonb_exists_all",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let json = ctx.get::<String>(0)?;
            let keys_json = ctx.get::<String>(1)?;
            
            let val: JsonValue = serde_json::from_str(&json)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            let keys: Vec<String> = serde_json::from_str(&keys_json)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            
            if let JsonValue::Object(map) = val {
                Ok(keys.iter().all(|k| map.contains_key(k)))
            } else {
                Ok(false)
            }
        },
    )?;

    // jsonb_array_length - Get the length of a JSONB array
    conn.create_scalar_function(
        "jsonb_array_length",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let json = ctx.get::<String>(0)?;
            
            let val: JsonValue = serde_json::from_str(&json)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            
            if let JsonValue::Array(arr) = val {
                Ok(arr.len() as i64)
            } else {
                Ok(0i64)
            }
        },
    )?;

    // to_jsonb - Convert a value to JSONB
    conn.create_scalar_function(
        "to_jsonb",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let val = ctx.get_raw(0);
            let json_val = match val {
                rusqlite::types::ValueRef::Null => JsonValue::Null,
                rusqlite::types::ValueRef::Integer(i) => JsonValue::Number(i.into()),
                rusqlite::types::ValueRef::Real(f) => {
                    serde_json::Number::from_f64(f)
                        .map(JsonValue::Number)
                        .unwrap_or(JsonValue::Null)
                }
                rusqlite::types::ValueRef::Text(t) => {
                    // Try to parse as JSON first, otherwise treat as string
                    if let Ok(j) = serde_json::from_slice::<JsonValue>(t) {
                        j
                    } else {
                        JsonValue::String(String::from_utf8_lossy(t).to_string())
                    }
                }
                rusqlite::types::ValueRef::Blob(b) => {
                    JsonValue::String(format!("\\x{}", hex::encode(b)))
                }
            };
            
            Ok(json_val.to_string())
        },
    )?;




    register_json_constructor_functions(conn)?;

    Ok(())
}

/// Register JSON constructor functions (to_json, json_build_object, json_build_array, etc.)
fn register_json_constructor_functions(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    
    // Register to_json
    conn.create_scalar_function("to_json", 1, flags, |ctx| {
        let json_val = value_to_json(ctx.get_raw(0))?;
        Ok(json_val.to_string())
    })?;
    
    // Register to_jsonb
    conn.create_scalar_function("to_jsonb", 1, flags, |ctx| {
        let json_val = value_to_json(ctx.get_raw(0))?;
        Ok(json_val.to_string())
    })?;
    
    // Register json_build_object with multiple arities (0-10 args, even numbers only)
    for arity in [0, 2, 4, 6, 8, 10] {
        conn.create_scalar_function("json_build_object", arity, flags, move |ctx| {
            let mut map = serde_json::Map::new();
            for i in (0..arity).step_by(2) {
                let key: String = ctx.get(i as usize)?;
                let val = value_to_json(ctx.get_raw(i as usize + 1))?;
                map.insert(key, val);
            }
            Ok(JsonValue::Object(map).to_string())
        })?;
    }
    
    // Register jsonb_build_object
    for arity in [0, 2, 4, 6, 8, 10] {
        conn.create_scalar_function("jsonb_build_object", arity, flags, move |ctx| {
            let mut map = serde_json::Map::new();
            for i in (0..arity).step_by(2) {
                let key: String = ctx.get(i as usize)?;
                let val = value_to_json(ctx.get_raw(i as usize + 1))?;
                map.insert(key, val);
            }
            Ok(JsonValue::Object(map).to_string())
        })?;
    }
    
    // Register json_build_array with multiple arities (0-10 args)
    for arity in 0..=10 {
        conn.create_scalar_function("json_build_array", arity, flags, move |ctx| {
            let mut arr = Vec::new();
            for i in 0..arity {
                arr.push(value_to_json(ctx.get_raw(i as usize))?);
            }
            Ok(JsonValue::Array(arr).to_string())
        })?;
    }
    
    // Register jsonb_build_array
    for arity in 0..=10 {
        conn.create_scalar_function("jsonb_build_array", arity, flags, move |ctx| {
            let mut arr = Vec::new();
            for i in 0..arity {
                arr.push(value_to_json(ctx.get_raw(i as usize))?);
            }
            Ok(JsonValue::Array(arr).to_string())
        })?;
    }
    
    // Register array_to_json
    conn.create_scalar_function("array_to_json", 1, flags, |ctx| {
        let json_val = value_to_json(ctx.get_raw(0))?;
        Ok(json_val.to_string())
    })?;
    


    Ok(())
}

/// Convert a SQLite value to a JSON value
fn value_to_json(val: rusqlite::types::ValueRef) -> rusqlite::Result<JsonValue> {
    match val {
        rusqlite::types::ValueRef::Null => Ok(JsonValue::Null),
        rusqlite::types::ValueRef::Integer(i) => Ok(JsonValue::Number(i.into())),
        rusqlite::types::ValueRef::Real(f) => {
            if let Some(num) = serde_json::Number::from_f64(f) {
                Ok(JsonValue::Number(num))
            } else {
                Ok(JsonValue::Null)
            }
        }
        rusqlite::types::ValueRef::Text(t) => {
            if let Ok(json_val) = serde_json::from_str(std::str::from_utf8(t).unwrap_or("")) {
                Ok(json_val)
            } else {
                Ok(JsonValue::String(std::str::from_utf8(t).unwrap_or("").to_string()))
            }
        }
        rusqlite::types::ValueRef::Blob(b) => {
            Ok(JsonValue::String(format!("\\x{}", hex::encode(b))))
        }
    }
}

/// Check if a JSON value contains another JSON value (PostgreSQL @> semantics)
fn json_contains(container: &JsonValue, contained: &JsonValue) -> bool {
    match (container, contained) {
        // Object containment: contained object must be a subset
        (JsonValue::Object(container_map), JsonValue::Object(contained_map)) => {
            contained_map.iter().all(|(key, val)| {
                container_map.get(key)
                    .map(|container_val| json_contains(container_val, val))
                    .unwrap_or(false)
            })
        }
        // Array containment: check if contained is in container
        (JsonValue::Array(container_arr), JsonValue::Array(contained_arr)) => {
            contained_arr.iter().all(|contained_item| {
                container_arr.iter().any(|container_item| {
                    json_contains(container_item, contained_item)
                })
            })
        }
        // Scalar equality
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_jsonb_contains() {
        let conn = Connection::open_in_memory().unwrap();
        register_jsonb_functions(&conn).unwrap();

        // Test object containment
        let sql = r#"SELECT jsonb_contains(json('{"a": 1, "b": 2}'), json('{"a": 1}'))"#;
        let result: bool = conn.query_row(sql, [], |row| row.get(0)).unwrap();
        assert!(result);

        // Test non-containment
        let sql = r#"SELECT jsonb_contains(json('{"a": 1}'), json('{"b": 2}'))"#;
        let result: bool = conn.query_row(sql, [], |row| row.get(0)).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_jsonb_exists() {
        let conn = Connection::open_in_memory().unwrap();
        register_jsonb_functions(&conn).unwrap();

        let sql = r#"SELECT jsonb_exists(json('{"a": 1, "b": 2}'), 'a')"#;
        let result: bool = conn.query_row(sql, [], |row| row.get(0)).unwrap();
        assert!(result);

        let sql = r#"SELECT jsonb_exists(json('{"a": 1}'), 'c')"#;
        let result: bool = conn.query_row(sql, [], |row| row.get(0)).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_jsonb_array_length() {
        let conn = Connection::open_in_memory().unwrap();
        register_jsonb_functions(&conn).unwrap();

        let result: i64 = conn.query_row(
            "SELECT jsonb_array_length('[1, 2, 3]')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_invalid_json_strict_validation() {
        // Test that invalid JSON strings return PostgreSQL-compatible error messages
        let invalid_cases = vec![
            "{invalid json}",
            "{\"key\": value}",           // unquoted value
            "{\"key\": }",                 // missing value
            "[1, 2,]",                     // trailing comma
            "{\"key\": \"value\"",        // unclosed object
            "\"unclosed string",           // unclosed string
            "",                             // empty string
            "   ",                         // whitespace only
            "{key: value}",                // unquoted keys (JavaScript style)
        ];

        for input in invalid_cases {
            let result = validate_json_strict(input);
            assert!(result.is_err(), "Should reject invalid JSON: {}", input);
            
            let err_msg = result.unwrap_err();
            assert!(
                err_msg.contains("invalid input syntax for type json"),
                "Error message should be PostgreSQL-compatible for '{}': got '{}'",
                input,
                err_msg
            );
        }
    }

    #[test]
    fn test_valid_json_strict_validation() {
        // Ensure valid JSON still passes
        let valid_cases = vec![
            "{}",
            "[]",
            "null",
            "true",
            "false",
            "42",
            "\"string\"",
            "{\"key\": \"value\"}",
            "[1, 2, 3]",
            "{\"nested\": {\"key\": [1, 2, 3]}}",
        ];

        for input in valid_cases {
            let result = validate_json_strict(input);
            assert!(result.is_ok(), "Should accept valid JSON '{}': {:?}", input, result);
        }
    }
}

/// Validates a JSON string with strict parsing and PostgreSQL-compatible error messages
/// 
/// # Arguments
/// * `json_str` - The string to validate as JSON
/// 
/// # Returns
/// * `Ok(())` if the string is valid JSON
/// * `Err(String)` with a PostgreSQL-compatible error message if invalid
/// 
/// # Examples
/// 
/// ```
/// use pgqt::jsonb::validate_json_strict;
/// 
/// assert!(validate_json_strict("{\"key\": \"value\"}").is_ok());
/// assert!(validate_json_strict("invalid").is_err());
/// ```
#[allow(dead_code)]
pub fn validate_json_strict(json_str: &str) -> Result<(), String> {
    let trimmed = json_str.trim();
    
    if trimmed.is_empty() {
        return Err(format!(
            "invalid input syntax for type json: \"{}\"",
            json_str
        ));
    }
    
    match serde_json::from_str::<JsonValue>(trimmed) {
        Ok(_) => Ok(()),
        Err(_) => Err(format!(
            "invalid input syntax for type json: \"{}\"",
            json_str
        )),
    }
}

use rusqlite::functions::{Aggregate, Context};

/// State for json_agg aggregate
#[derive(Debug, Clone)]
pub struct JsonAggState {
    values: Vec<serde_json::Value>,
}

impl Default for JsonAggState {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

/// json_agg aggregate implementation
pub struct JsonAgg;

impl Aggregate<JsonAggState, Option<String>> for JsonAgg {
    fn init(&self, _ctx: &mut Context<'_>) -> rusqlite::Result<JsonAggState> {
        Ok(JsonAggState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, state: &mut JsonAggState) -> rusqlite::Result<()> {
        // Get the value and convert to JSON
        if let Ok(val_str) = ctx.get::<String>(0) {
            // Try to parse as JSON first
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&val_str) {
                state.values.push(json_val);
            } else {
                // Treat as string
                state.values.push(serde_json::Value::String(val_str));
            }
        } else if let Ok(val_i64) = ctx.get::<i64>(0) {
            state.values.push(serde_json::Value::Number(val_i64.into()));
        } else if let Ok(val_f64) = ctx.get::<f64>(0) {
            if let Some(num) = serde_json::Number::from_f64(val_f64) {
                state.values.push(serde_json::Value::Number(num));
            } else {
                state.values.push(serde_json::Value::Null);
            }
        } else {
            state.values.push(serde_json::Value::Null);
        }



    Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, state: Option<JsonAggState>) -> rusqlite::Result<Option<String>> {
        match state {
            Some(s) => {
                let json_array = serde_json::Value::Array(s.values);
                Ok(Some(json_array.to_string()))
            }
            None => Ok(None),
        }
    }
}

/// State for json_object_agg aggregate
#[derive(Debug, Clone)]
pub struct JsonObjectAggState {
    pairs: Vec<(String, serde_json::Value)>,
}

impl Default for JsonObjectAggState {
    fn default() -> Self {
        Self { pairs: Vec::new() }
    }
}

/// json_object_agg aggregate implementation
pub struct JsonObjectAgg;

impl Aggregate<JsonObjectAggState, Option<String>> for JsonObjectAgg {
    fn init(&self, _ctx: &mut Context<'_>) -> rusqlite::Result<JsonObjectAggState> {
        Ok(JsonObjectAggState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, state: &mut JsonObjectAggState) -> rusqlite::Result<()> {
        let key: String = ctx.get(0)?;
        
        // Get the value
        let json_val = if let Ok(val_str) = ctx.get::<String>(1) {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&val_str) {
                json_val
            } else {
                serde_json::Value::String(val_str)
            }
        } else if let Ok(val_i64) = ctx.get::<i64>(1) {
            serde_json::Value::Number(val_i64.into())
        } else if let Ok(val_f64) = ctx.get::<f64>(1) {
            if let Some(num) = serde_json::Number::from_f64(val_f64) {
                serde_json::Value::Number(num)
            } else {
                serde_json::Value::Null
            }
        } else {
            serde_json::Value::Null
        };
        
        state.pairs.push((key, json_val));



    Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, state: Option<JsonObjectAggState>) -> rusqlite::Result<Option<String>> {
        match state {
            Some(s) => {
                let mut map = serde_json::Map::new();
                for (key, value) in s.pairs {
                    map.insert(key, value);
                }
                let json_obj = serde_json::Value::Object(map);
                Ok(Some(json_obj.to_string()))
            }
            None => Ok(None),
        }
    }
}

/// Register JSON aggregate functions
pub fn register_json_agg_functions(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8;
    
    // Register json_agg
    conn.create_aggregate_function("json_agg", 1, flags, JsonAgg)?;
    
    // Register jsonb_agg (same implementation)
    conn.create_aggregate_function("jsonb_agg", 1, flags, JsonAgg)?;
    
    // Register json_object_agg
    conn.create_aggregate_function("json_object_agg", 2, flags, JsonObjectAgg)?;
    
    // Register jsonb_object_agg (same implementation)
    conn.create_aggregate_function("jsonb_object_agg", 2, flags, JsonObjectAgg)?;
    



    Ok(())
}
