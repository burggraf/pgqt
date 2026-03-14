//! Array aggregate function for PostgreSQL compatibility
//!
//! This module implements the `array_agg` aggregate function which collects
//! values into a PostgreSQL array format.

use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::{Connection, Result};
use rusqlite::types::Value;

/// State for array_agg accumulation
#[derive(Debug, Clone, Default)]
struct ArrayAggState {
    values: Vec<Value>,
}

/// Aggregate function for array_agg
pub struct ArrayAgg;

impl Aggregate<ArrayAggState, Option<String>> for ArrayAgg {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<ArrayAggState> {
        Ok(ArrayAggState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut ArrayAggState) -> Result<()> {
        // Get the value (can be any type)
        let value: Value = ctx.get(0)?;
        
        // PostgreSQL includes NULL values in the array
        acc.values.push(value);
        
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<ArrayAggState>) -> Result<Option<String>> {
        match acc {
            Some(state) => {
                if state.values.is_empty() {
                    Ok(Some("{}".to_string())) // Empty array
                } else {
                    // Format as PostgreSQL array: {val1,val2,val3}
                    let formatted: Vec<String> = state.values.iter().map(|v| {
                        match v {
                            Value::Null => "NULL".to_string(),
                            Value::Integer(i) => i.to_string(),
                            Value::Real(f) => f.to_string(),
                            Value::Text(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
                            Value::Blob(b) => format!("\"\\\\x{}\"", hex::encode(b)),
                        }
                    }).collect();
                    
                    Ok(Some(format!("{{{}}}", formatted.join(","))))
                }
            }
            None => Ok(None),
        }
    }
}

/// Register the array_agg aggregate function
pub fn register_array_agg(conn: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8;
    
    conn.create_aggregate_function("array_agg", 1, flags, ArrayAgg)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_array_agg(&conn).unwrap();
        conn
    }

    #[test]
    fn test_array_agg_basic() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (x INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (2), (3)", []).unwrap();
        
        let result: String = conn
            .query_row("SELECT array_agg(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{1,2,3}");
    }

    #[test]
    fn test_array_agg_with_nulls() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (x TEXT)", []).unwrap();
        conn.execute("INSERT INTO test VALUES ('a'), (NULL), ('b')", []).unwrap();
        
        let result: String = conn
            .query_row("SELECT array_agg(x) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{\"a\",NULL,\"b\"}");
    }

    #[test]
    fn test_array_agg_empty() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (x INTEGER)", []).unwrap();
        
        let result: Option<String> = conn
            .query_row("SELECT array_agg(x) FROM test", [], |r| r.get(0))
            .unwrap();
        // Empty table returns NULL (not empty array)
        // PostgreSQL actually returns NULL for empty aggregate
        assert!(result.is_none());
    }

    #[test]
    fn test_array_agg_strings() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (name TEXT)", []).unwrap();
        conn.execute("INSERT INTO test VALUES ('alice'), ('bob')", []).unwrap();
        
        let result: String = conn
            .query_row("SELECT array_agg(name) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{\"alice\",\"bob\"}");
    }

    #[test]
    fn test_array_agg_with_special_chars() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (s TEXT)", []).unwrap();
        conn.execute("INSERT INTO test VALUES ('a\"b'), ('c,d')", []).unwrap();
        
        let result: String = conn
            .query_row("SELECT array_agg(s) FROM test", [], |r| r.get(0))
            .unwrap();
        // Special characters should be escaped
        assert_eq!(result, "{\"a\\\"b\",\"c,d\"}");
    }

    #[test]
    fn test_array_agg_with_group_by() {
        let conn = setup_db();
        conn.execute("CREATE TABLE sales (product TEXT, amount INTEGER)", []).unwrap();
        conn.execute("INSERT INTO sales VALUES ('A', 10), ('A', 20), ('B', 30)", []).unwrap();
        
        let result: Vec<(String, String)> = conn
            .prepare("SELECT product, array_agg(amount) FROM sales GROUP BY product ORDER BY product")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "A");
        assert_eq!(result[0].1, "{10,20}");
        assert_eq!(result[1].0, "B");
        assert_eq!(result[1].1, "{30}");
    }
}