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

    Ok(())
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
}
