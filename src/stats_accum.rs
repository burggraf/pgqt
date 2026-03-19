//! Statistical aggregate support functions
//!
//! These are internal functions used by PostgreSQL's statistical aggregates
//! (variance, stddev, etc.) for accumulating and combining state.
//!
//! The accumulators are stored as JSON arrays in SQLite:
//! - float8_accum uses [n, sum, sum_sqr] for computing variance/stddev
//! - float8_regr_accum uses [n, sum_x, sum_x2, sum_y, sum_y2, sum_xy] for regression

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, Result};

/// float8_accum - accumulate a value for statistical computation
/// Input: accum array as JSON string [n, sum, sum_sqr], new value
/// Output: updated accum array as JSON string
pub fn register_float8_accum(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "float8_accum",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let accum_str: String = ctx.get(0)?;
            let val: f64 = ctx.get(1)?;

            let mut accum: Vec<f64> = serde_json::from_str(&accum_str)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;

            // Ensure accum has at least 3 elements [n, sum, sum_sqr]
            while accum.len() < 3 {
                accum.push(0.0);
            }

            // Update: [count, sum, sum_of_squares]
            accum[0] += 1.0;       // count (n)
            accum[1] += val;       // sum
            accum[2] += val * val; // sum of squares

            serde_json::to_string(&accum)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
        },
    )?;

    Ok(())
}

/// float8_combine - combine two accumulators
/// Input: two accum arrays as JSON strings
/// Output: combined accum array as JSON string
pub fn register_float8_combine(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "float8_combine",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let accum1_str: String = ctx.get(0)?;
            let accum2_str: String = ctx.get(1)?;

            let accum1: Vec<f64> = serde_json::from_str(&accum1_str)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            let accum2: Vec<f64> = serde_json::from_str(&accum2_str)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;

            // Combine accumulators element-wise
            let len = accum1.len().max(accum2.len());
            let mut result = Vec::with_capacity(len);

            for i in 0..len {
                let v1 = accum1.get(i).copied().unwrap_or(0.0);
                let v2 = accum2.get(i).copied().unwrap_or(0.0);
                result.push(v1 + v2);
            }

            serde_json::to_string(&result)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
        },
    )?;

    Ok(())
}

/// float8_regr_accum - accumulate for regression
/// Input: accum array as JSON string, y value, x value
/// Output: updated accum array as JSON string
/// Accumulator format: [n, sum_x, sum_x2, sum_y, sum_y2, sum_xy, 0, 0]
pub fn register_float8_regr_accum(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "float8_regr_accum",
        3,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let accum_str: String = ctx.get(0)?;
            let y: f64 = ctx.get(1)?;
            let x: f64 = ctx.get(2)?;

            let mut accum: Vec<f64> = serde_json::from_str(&accum_str)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;

            // Ensure accum has at least 8 elements for regression
            // [n, sum_x, sum_x2, sum_y, sum_y2, sum_xy, 0, 0]
            while accum.len() < 8 {
                accum.push(0.0);
            }

            // Update regression accumulators
            accum[0] += 1.0;       // n (count)
            accum[1] += x;         // sum_x
            accum[2] += x * x;     // sum_x2 (sum of x squared)
            accum[3] += y;         // sum_y
            accum[4] += y * y;     // sum_y2 (sum of y squared)
            accum[5] += x * y;     // sum_xy (sum of x*y)
            // accum[6] and accum[7] are reserved/spare (always 0)

            serde_json::to_string(&accum)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
        },
    )?;

    Ok(())
}

/// float8_regr_combine - combine two regression accumulators
/// Input: two accum arrays as JSON strings
/// Output: combined accum array as JSON string
pub fn register_float8_regr_combine(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "float8_regr_combine",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let accum1_str: String = ctx.get(0)?;
            let accum2_str: String = ctx.get(1)?;

            let accum1: Vec<f64> = serde_json::from_str(&accum1_str)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
            let accum2: Vec<f64> = serde_json::from_str(&accum2_str)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;

            // Combine element-wise
            let len = accum1.len().max(accum2.len());
            let mut result = Vec::with_capacity(len);

            for i in 0..len {
                let v1 = accum1.get(i).copied().unwrap_or(0.0);
                let v2 = accum2.get(i).copied().unwrap_or(0.0);
                result.push(v1 + v2);
            }

            serde_json::to_string(&result)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
        },
    )?;

    Ok(())
}

/// Register all statistical accumulator functions
pub fn register_stats_accum_functions(conn: &Connection) -> Result<()> {
    register_float8_accum(conn)?;
    register_float8_combine(conn)?;
    register_float8_regr_accum(conn)?;
    register_float8_regr_combine(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_stats_accum_functions(&conn).unwrap();
        conn
    }

    #[test]
    fn test_float8_accum_empty_array() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT float8_accum('[]', 10.0)", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum.len(), 3);
        assert_eq!(accum[0], 1.0);   // count
        assert_eq!(accum[1], 10.0);  // sum
        assert_eq!(accum[2], 100.0); // sum of squares
    }

    #[test]
    fn test_float8_accum_multiple_values() {
        let conn = setup_db();

        // Start with empty array
        let result: String = conn
            .query_row("SELECT float8_accum('[]', 2.0)", [], |r| r.get(0))
            .unwrap();

        // Accumulate second value
        let result: String = conn
            .query_row("SELECT float8_accum(?1, 3.0)", [&result], |r| r.get(0))
            .unwrap();

        // Accumulate third value
        let result: String = conn
            .query_row("SELECT float8_accum(?1, 4.0)", [&result], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 3.0);         // count = 3
        assert_eq!(accum[1], 9.0);         // sum = 2+3+4 = 9
        assert_eq!(accum[2], 4.0 + 9.0 + 16.0); // sum of squares = 4+9+16 = 29
    }

    #[test]
    fn test_float8_accum_with_existing_accumulator() {
        let conn = setup_db();

        // Start with pre-populated accumulator [n=2, sum=5, sum_sqr=13]
        let result: String = conn
            .query_row("SELECT float8_accum('[2.0, 5.0, 13.0]', 5.0)", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 3.0);   // count = 2+1 = 3
        assert_eq!(accum[1], 10.0);  // sum = 5+5 = 10
        assert_eq!(accum[2], 38.0);  // sum_sqr = 13+25 = 38
    }

    #[test]
    fn test_float8_accum_negative_values() {
        let conn = setup_db();

        let result: String = conn
            .query_row("SELECT float8_accum('[]', -3.0)", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 1.0);
        assert_eq!(accum[1], -3.0);
        assert_eq!(accum[2], 9.0); // (-3)^2 = 9
    }

    #[test]
    fn test_float8_accum_decimal_values() {
        let conn = setup_db();

        let result: String = conn
            .query_row("SELECT float8_accum('[]', 2.5)", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 1.0);
        assert_eq!(accum[1], 2.5);
        assert_eq!(accum[2], 6.25); // 2.5^2 = 6.25
    }

    #[test]
    fn test_float8_combine() {
        let conn = setup_db();

        let result: String = conn
            .query_row("SELECT float8_combine('[1.0, 10.0, 100.0]', '[2.0, 20.0, 200.0]')", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 3.0);   // 1+2
        assert_eq!(accum[1], 30.0);  // 10+20
        assert_eq!(accum[2], 300.0); // 100+200
    }

    #[test]
    fn test_float8_combine_empty_arrays() {
        let conn = setup_db();

        let result: String = conn
            .query_row("SELECT float8_combine('[]', '[]')", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert!(accum.is_empty());
    }

    #[test]
    fn test_float8_combine_different_lengths() {
        let conn = setup_db();

        let result: String = conn
            .query_row("SELECT float8_combine('[1.0, 2.0]', '[3.0, 4.0, 5.0]')", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum.len(), 3);
        assert_eq!(accum[0], 4.0); // 1+3
        assert_eq!(accum[1], 6.0); // 2+4
        assert_eq!(accum[2], 5.0); // 0+5 (padding with 0)
    }

    #[test]
    fn test_float8_combine_three_way() {
        let conn = setup_db();

        // First combine
        let result: String = conn
            .query_row("SELECT float8_combine('[1.0, 10.0, 100.0]', '[2.0, 20.0, 200.0]')", [], |r| r.get(0))
            .unwrap();

        // Combine with third
        let result: String = conn
            .query_row("SELECT float8_combine(?1, '[3.0, 30.0, 300.0]')", [&result], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 6.0);    // 1+2+3
        assert_eq!(accum[1], 60.0);   // 10+20+30
        assert_eq!(accum[2], 600.0);  // 100+200+300
    }

    #[test]
    fn test_float8_regr_accum_empty_array() {
        let conn = setup_db();

        let result: String = conn
            .query_row("SELECT float8_regr_accum('[]', 5.0, 2.0)", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum.len(), 8);
        assert_eq!(accum[0], 1.0);   // n
        assert_eq!(accum[1], 2.0);   // sum_x
        assert_eq!(accum[2], 4.0);   // sum_x2 = 2^2
        assert_eq!(accum[3], 5.0);   // sum_y
        assert_eq!(accum[4], 25.0);  // sum_y2 = 5^2
        assert_eq!(accum[5], 10.0);  // sum_xy = 2*5
        assert_eq!(accum[6], 0.0);   // spare
        assert_eq!(accum[7], 0.0);   // spare
    }

    #[test]
    fn test_float8_regr_accum_multiple_values() {
        let conn = setup_db();

        // First point: (x=1, y=2)
        let result: String = conn
            .query_row("SELECT float8_regr_accum('[]', 2.0, 1.0)", [], |r| r.get(0))
            .unwrap();

        // Second point: (x=2, y=4)
        let result: String = conn
            .query_row("SELECT float8_regr_accum(?1, 4.0, 2.0)", [&result], |r| r.get(0))
            .unwrap();

        // Third point: (x=3, y=6)
        let result: String = conn
            .query_row("SELECT float8_regr_accum(?1, 6.0, 3.0)", [&result], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 3.0);   // n = 3
        assert_eq!(accum[1], 6.0);   // sum_x = 1+2+3 = 6
        assert_eq!(accum[2], 14.0);  // sum_x2 = 1+4+9 = 14
        assert_eq!(accum[3], 12.0);  // sum_y = 2+4+6 = 12
        assert_eq!(accum[4], 56.0);  // sum_y2 = 4+16+36 = 56
        assert_eq!(accum[5], 28.0);  // sum_xy = 1*2 + 2*4 + 3*6 = 2+8+18 = 28
    }

    #[test]
    fn test_float8_regr_combine() {
        let conn = setup_db();

        // Combine two regression accumulators
        let result: String = conn
            .query_row(
                "SELECT float8_regr_combine('[1.0, 2.0, 4.0, 5.0, 25.0, 10.0, 0.0, 0.0]', '[2.0, 6.0, 18.0, 12.0, 74.0, 38.0, 0.0, 0.0]')",
                [],
                |r| r.get(0)
            )
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 3.0);    // n = 1+2
        assert_eq!(accum[1], 8.0);    // sum_x = 2+6
        assert_eq!(accum[2], 22.0);   // sum_x2 = 4+18
        assert_eq!(accum[3], 17.0);   // sum_y = 5+12
        assert_eq!(accum[4], 99.0);   // sum_y2 = 25+74
        assert_eq!(accum[5], 48.0);   // sum_xy = 10+38
        assert_eq!(accum[6], 0.0);
        assert_eq!(accum[7], 0.0);
    }

    #[test]
    fn test_float8_regr_combine_empty_arrays() {
        let conn = setup_db();

        let result: String = conn
            .query_row("SELECT float8_regr_combine('[]', '[]')", [], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert!(accum.is_empty());
    }

    #[test]
    fn test_float8_regr_combine_partial() {
        let conn = setup_db();

        // Combine with one empty array
        let result: String = conn
            .query_row(
                "SELECT float8_regr_combine('[3.0, 6.0, 14.0, 12.0, 56.0, 28.0, 0.0, 0.0]', '[]')",
                [],
                |r| r.get(0)
            )
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
        assert_eq!(accum[0], 3.0);
        assert_eq!(accum[1], 6.0);
        assert_eq!(accum[2], 14.0);
        assert_eq!(accum[3], 12.0);
        assert_eq!(accum[4], 56.0);
        assert_eq!(accum[5], 28.0);
    }

    #[test]
    fn test_chained_operations() {
        let conn = setup_db();

        // Simulate a parallel aggregation scenario:
        // Worker 1 accumulates values 1, 2
        let worker1: String = conn
            .query_row("SELECT float8_accum('[]', 1.0)", [], |r| r.get(0))
            .unwrap();
        let worker1: String = conn
            .query_row("SELECT float8_accum(?1, 2.0)", [&worker1], |r| r.get(0))
            .unwrap();

        // Worker 2 accumulates values 3, 4        
        let worker2: String = conn
            .query_row("SELECT float8_accum('[]', 3.0)", [], |r| r.get(0))
            .unwrap();
        let worker2: String = conn
            .query_row("SELECT float8_accum(?1, 4.0)", [&worker2], |r| r.get(0))
            .unwrap();

        // Combine worker results
        let combined: String = conn
            .query_row("SELECT float8_combine(?1, ?2)", [&worker1, &worker2], |r| r.get(0))
            .unwrap();

        let accum: Vec<f64> = serde_json::from_str(&combined).unwrap();
        assert_eq!(accum[0], 4.0);   // n = 2+2 = 4
        assert_eq!(accum[1], 10.0);  // sum = (1+2) + (3+4) = 10
        assert_eq!(accum[2], 30.0);  // sum_sqr = (1+4) + (9+16) = 30
    }
}