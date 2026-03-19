//! Boolean aggregate functions: bool_and, bool_or, every
//!
//! These aggregates compute the logical AND/OR of all non-null boolean values.
//! - bool_and: Returns true if all values are true, false if any are false.
//!             Returns true for empty result sets.
//! - bool_or: Returns true if any value is true, false if all are false.
//!            Returns false for empty result sets.
//! - every: Alias for bool_and (SQL standard)

use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::{Connection, Result};

/// State for bool_and aggregate
#[derive(Debug, Clone)]
pub struct BoolAndState {
    result: Option<bool>,
}

/// bool_and aggregate implementation
/// AND of all non-null values. Returns true for empty result sets.
pub struct BoolAnd;

impl Aggregate<BoolAndState, Option<bool>> for BoolAnd {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<BoolAndState> {
        Ok(BoolAndState { result: None })
    }

    fn step(&self, ctx: &mut Context<'_>, state: &mut BoolAndState) -> Result<()> {
        let val: Option<bool> = ctx.get(0)?;

        if let Some(v) = val {
            state.result = Some(state.result.unwrap_or(true) && v);
        }

        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, state: Option<BoolAndState>) -> Result<Option<bool>> {
        match state {
            Some(s) => Ok(Some(s.result.unwrap_or(true))),
            None => Ok(Some(true)), // Empty result set returns true for bool_and
        }
    }
}

/// State for bool_or aggregate
#[derive(Debug, Clone)]
pub struct BoolOrState {
    result: Option<bool>,
}

/// bool_or aggregate implementation
/// OR of all non-null values. Returns false for empty result sets.
pub struct BoolOr;

impl Aggregate<BoolOrState, Option<bool>> for BoolOr {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<BoolOrState> {
        Ok(BoolOrState { result: None })
    }

    fn step(&self, ctx: &mut Context<'_>, state: &mut BoolOrState) -> Result<()> {
        let val: Option<bool> = ctx.get(0)?;

        if let Some(v) = val {
            state.result = Some(state.result.unwrap_or(false) || v);
        }

        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, state: Option<BoolOrState>) -> Result<Option<bool>> {
        match state {
            Some(s) => Ok(Some(s.result.unwrap_or(false))),
            None => Ok(Some(false)), // Empty result set returns false for bool_or
        }
    }
}

/// Register boolean aggregate functions
pub fn register_bool_aggregates(conn: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8;

    conn.create_aggregate_function("bool_and", 1, flags, BoolAnd)?;
    conn.create_aggregate_function("bool_or", 1, flags, BoolOr)?;
    // every is an alias for bool_and
    conn.create_aggregate_function("every", 1, flags, BoolAnd)?;

    Ok(())
}

/// Register state transition functions for boolean aggregates
/// These are scalar functions that take (accumulator, value) and return new accumulator
pub fn register_bool_statefuncs(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "booland_statefunc",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let acc: Option<bool> = ctx.get(0)?;
            let val: Option<bool> = ctx.get(1)?;

            match (acc, val) {
                (Some(a), Some(v)) => Ok(Some(a && v)),
                (Some(a), None) => Ok(Some(a)),
                (None, Some(v)) => Ok(Some(v)),
                (None, None) => Ok(None),
            }
        },
    )?;

    conn.create_scalar_function(
        "boolor_statefunc",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let acc: Option<bool> = ctx.get(0)?;
            let val: Option<bool> = ctx.get(1)?;

            match (acc, val) {
                (Some(a), Some(v)) => Ok(Some(a || v)),
                (Some(a), None) => Ok(Some(a)),
                (None, Some(v)) => Ok(Some(v)),
                (None, None) => Ok(None),
            }
        },
    )?;

    Ok(())
}

// ============================================================================
// Bitwise Aggregate Functions
// ============================================================================

/// State for bit_and aggregate
#[derive(Debug, Clone)]
pub struct BitAndState {
    result: Option<i64>,
}

/// bit_and aggregate implementation
/// Bitwise AND of all non-null values. Returns NULL for empty result sets.
pub struct BitAnd;

impl Aggregate<BitAndState, Option<i64>> for BitAnd {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<BitAndState> {
        Ok(BitAndState { result: None })
    }

    fn step(&self, ctx: &mut Context<'_>, state: &mut BitAndState) -> Result<()> {
        let val: Option<i64> = ctx.get(0)?;

        if let Some(v) = val {
            state.result = Some(state.result.map_or(v, |r| r & v));
        }

        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, state: Option<BitAndState>) -> Result<Option<i64>> {
        match state {
            Some(s) => Ok(s.result),
            None => Ok(None),
        }
    }
}

/// State for bit_or aggregate
#[derive(Debug, Clone)]
pub struct BitOrState {
    result: Option<i64>,
}

/// bit_or aggregate implementation
/// Bitwise OR of all non-null values. Returns NULL for empty result sets.
pub struct BitOr;

impl Aggregate<BitOrState, Option<i64>> for BitOr {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<BitOrState> {
        Ok(BitOrState { result: None })
    }

    fn step(&self, ctx: &mut Context<'_>, state: &mut BitOrState) -> Result<()> {
        let val: Option<i64> = ctx.get(0)?;

        if let Some(v) = val {
            state.result = Some(state.result.map_or(v, |r| r | v));
        }

        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, state: Option<BitOrState>) -> Result<Option<i64>> {
        match state {
            Some(s) => Ok(s.result),
            None => Ok(None),
        }
    }
}

/// State for bit_xor aggregate
#[derive(Debug, Clone)]
pub struct BitXorState {
    result: Option<i64>,
}

/// bit_xor aggregate implementation
/// Bitwise XOR of all non-null values. Returns NULL for empty result sets.
pub struct BitXor;

impl Aggregate<BitXorState, Option<i64>> for BitXor {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<BitXorState> {
        Ok(BitXorState { result: None })
    }

    fn step(&self, ctx: &mut Context<'_>, state: &mut BitXorState) -> Result<()> {
        let val: Option<i64> = ctx.get(0)?;

        if let Some(v) = val {
            state.result = Some(state.result.map_or(v, |r| r ^ v));
        }

        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, state: Option<BitXorState>) -> Result<Option<i64>> {
        match state {
            Some(s) => Ok(s.result),
            None => Ok(None),
        }
    }
}

/// Register bitwise aggregate functions
pub fn register_bitwise_aggregates(conn: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8;

    conn.create_aggregate_function("bit_and", 1, flags, BitAnd)?;
    conn.create_aggregate_function("bit_or", 1, flags, BitOr)?;
    conn.create_aggregate_function("bit_xor", 1, flags, BitXor)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_bool_aggregates(&conn).unwrap();
        conn
    }

    #[test]
    fn test_bool_and_all_true() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute(
            "INSERT INTO test VALUES (1), (1), (1)", // 1 = true in SQLite
            [],
        )
        .unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_and(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_bool_and_all_false() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute(
            "INSERT INTO test VALUES (0), (0), (0)", // 0 = false in SQLite
            [],
        )
        .unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_and(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_bool_and_mixed() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (0), (1)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_and(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_bool_or_all_true() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (1), (1)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_or(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_bool_or_all_false() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (0), (0), (0)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_or(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_bool_or_mixed() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (0), (1), (0)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_or(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_bool_and_with_nulls() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (NULL), (1)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_and(b) FROM test", [], |r| r.get(0))
            .unwrap();
        // NULLs are skipped, so it's like bool_and(1, 1) = true
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_bool_and_all_nulls() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (NULL), (NULL)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_and(b) FROM test", [], |r| r.get(0))
            .unwrap();
        // All NULLs means no non-null values, so bool_and returns true (identity for AND)
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_bool_or_all_nulls() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (NULL), (NULL)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_or(b) FROM test", [], |r| r.get(0))
            .unwrap();
        // All NULLs means no non-null values, so bool_or returns false (identity for OR)
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_bool_and_empty_table() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_and(b) FROM test", [], |r| r.get(0))
            .unwrap();
        // Empty result set returns true for bool_and
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_bool_or_empty_table() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT bool_or(b) FROM test", [], |r| r.get(0))
            .unwrap();
        // Empty result set returns false for bool_or
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_every_alias() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (1), (1)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT every(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_every_returns_false_when_any_false() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (b BOOLEAN)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1), (0), (1)", []).unwrap();

        let result: Option<bool> = conn
            .query_row("SELECT every(b) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(false));
    }

    #[test]
    fn test_booland_statefunc() {
        let conn = Connection::open_in_memory().unwrap();
        register_bool_statefuncs(&conn).unwrap();

        // Both values present
        let result: Option<bool> = conn
            .query_row("SELECT booland_statefunc(1, 1)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));

        let result: Option<bool> = conn
            .query_row("SELECT booland_statefunc(1, 0)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(false));

        // First NULL
        let result: Option<bool> = conn
            .query_row("SELECT booland_statefunc(NULL, 1)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));

        // Second NULL
        let result: Option<bool> = conn
            .query_row("SELECT booland_statefunc(1, NULL)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));

        // Both NULL
        let result: Option<bool> = conn
            .query_row("SELECT booland_statefunc(NULL, NULL)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_boolor_statefunc() {
        let conn = Connection::open_in_memory().unwrap();
        register_bool_statefuncs(&conn).unwrap();

        // Both values present
        let result: Option<bool> = conn
            .query_row("SELECT boolor_statefunc(0, 0)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(false));

        let result: Option<bool> = conn
            .query_row("SELECT boolor_statefunc(0, 1)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));

        // First NULL
        let result: Option<bool> = conn
            .query_row("SELECT boolor_statefunc(NULL, 1)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));

        // Second NULL
        let result: Option<bool> = conn
            .query_row("SELECT boolor_statefunc(1, NULL)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(true));

        // Both NULL
        let result: Option<bool> = conn
            .query_row("SELECT boolor_statefunc(NULL, NULL)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_bool_with_group_by() {
        let conn = setup_db();
        conn.execute("CREATE TABLE test (category TEXT, b BOOLEAN)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO test VALUES ('A', 1), ('A', 1), ('B', 1), ('B', 0)",
            [],
        )
        .unwrap();

        let results: Vec<(String, Option<bool>)> = conn
            .prepare("SELECT category, bool_and(b) FROM test GROUP BY category ORDER BY category")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("A".to_string(), Some(true)));
        assert_eq!(results[1], ("B".to_string(), Some(false)));
    }

    // ============================================================================
    // Bitwise Aggregate Tests
    // ============================================================================

    fn setup_bitwise_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_bitwise_aggregates(&conn).unwrap();
        conn
    }

    #[test]
    fn test_bit_and_multiple_values() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        // 5 = 101, 3 = 011, 1 = 001
        // 5 & 3 & 1 = 001 = 1
        conn.execute("INSERT INTO test VALUES (5), (3), (1)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_and(i) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_bit_and_single_value() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (42)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_and(i) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_bit_and_with_nulls() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        // NULL values should be skipped
        conn.execute("INSERT INTO test VALUES (7), (NULL), (3)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_and(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // 7 = 111, 3 = 011, 7 & 3 = 011 = 3
        assert_eq!(result, Some(3));
    }

    #[test]
    fn test_bit_and_all_nulls() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (NULL), (NULL)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_and(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // All NULLs should return NULL
        assert_eq!(result, None);
    }

    #[test]
    fn test_bit_and_empty_table() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_and(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // Empty table should return NULL
        assert_eq!(result, None);
    }

    #[test]
    fn test_bit_or_multiple_values() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        // 1 = 001, 2 = 010, 4 = 100
        // 1 | 2 | 4 = 111 = 7
        conn.execute("INSERT INTO test VALUES (1), (2), (4)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_or(i) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(7));
    }

    #[test]
    fn test_bit_or_single_value() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (42)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_or(i) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_bit_or_with_nulls() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        // NULL values should be skipped
        conn.execute("INSERT INTO test VALUES (1), (NULL), (2)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_or(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // 1 | 2 = 3
        assert_eq!(result, Some(3));
    }

    #[test]
    fn test_bit_or_all_nulls() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (NULL), (NULL)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_or(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // All NULLs should return NULL
        assert_eq!(result, None);
    }

    #[test]
    fn test_bit_or_empty_table() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_or(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // Empty table should return NULL
        assert_eq!(result, None);
    }

    #[test]
    fn test_bit_xor_multiple_values() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        // 5 = 101, 3 = 011
        // 5 ^ 3 = 110 = 6
        conn.execute("INSERT INTO test VALUES (5), (3)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_xor(i) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(6));
    }

    #[test]
    fn test_bit_xor_three_values() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        // 1 = 001, 2 = 010, 3 = 011
        // 1 ^ 2 = 011, 011 ^ 3 = 000 = 0
        conn.execute("INSERT INTO test VALUES (1), (2), (3)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_xor(i) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_bit_xor_single_value() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (42)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_xor(i) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_bit_xor_with_nulls() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        // NULL values should be skipped
        conn.execute("INSERT INTO test VALUES (5), (NULL), (3)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_xor(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // 5 ^ 3 = 6
        assert_eq!(result, Some(6));
    }

    #[test]
    fn test_bit_xor_all_nulls() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (NULL), (NULL)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_xor(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // All NULLs should return NULL
        assert_eq!(result, None);
    }

    #[test]
    fn test_bit_xor_empty_table() {
        let conn = setup_bitwise_db();
        conn.execute("CREATE TABLE test (i INTEGER)", []).unwrap();

        let result: Option<i64> = conn
            .query_row("SELECT bit_xor(i) FROM test", [], |r| r.get(0))
            .unwrap();
        // Empty table should return NULL
        assert_eq!(result, None);
    }
}