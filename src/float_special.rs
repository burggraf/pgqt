//! Special float value handling for NaN, Infinity, and -Infinity
//!
//! This module provides support for PostgreSQL's special float values:
//! - NaN (Not a Number)
//! - Infinity (positive infinity)
//! - -Infinity (negative infinity)
//!
//! SQLite uses IEEE 754 floats which natively support these values.

use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::{Connection, Result};

/// Register special float value functions
pub fn register_float_special_functions(conn: &Connection) -> Result<()> {
    // Register nan() function
    conn.create_scalar_function(
        "nan",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |_ctx| Ok(f64::NAN),
    )?;

    // Register infinity() function
    conn.create_scalar_function(
        "infinity",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |_ctx| Ok(f64::INFINITY),
    )?;

    // Register neg_infinity() function
    conn.create_scalar_function(
        "neg_infinity",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |_ctx| Ok(f64::NEG_INFINITY),
    )?;

    // Register alternative names (PostgreSQL compatibility)
    conn.create_scalar_function(
        "float8_nan",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |_ctx| Ok(f64::NAN),
    )?;

    conn.create_scalar_function(
        "float8_infinity",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |_ctx| Ok(f64::INFINITY),
    )?;

    Ok(())
}

/// Validate float input according to PostgreSQL rules
/// 
/// PostgreSQL float validation rules:
/// - Empty string or whitespace-only is invalid
/// - Multiple decimal points are invalid
/// - Spaces in the middle of numbers are invalid (except sign prefix)
/// - Must be a valid floating point representation
/// - Numeric overflow to infinity is an error (unless explicitly "infinity")
/// 
/// Returns the parsed f64 on success, or an error message on failure
pub fn validate_float_input(s: &str) -> Result<f64, String> {
    let trimmed = s.trim();
    
    // Check for empty or whitespace-only input
    if trimmed.is_empty() {
        return Err(format!("invalid input syntax for type double precision: \"{}\"", s));
    }
    
    // Check for multiple decimal points
    if trimmed.matches('.').count() > 1 {
        return Err(format!("invalid input syntax for type double precision: \"{}\"", s));
    }
    
    // Check for spaces in the middle of the number
    // PostgreSQL rejects: '5 . 0', '     - 3.0', etc.
    // Only leading/trailing spaces are allowed, not internal spaces
    // Exception: sign followed by space (e.g., "- 3.0") is also invalid
    if trimmed.contains(' ') || trimmed.contains('\t') || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(format!("invalid input syntax for type double precision: \"{}\"", s));
    }
    
    // Check for special float values first (these are valid)
    if let Some(special) = parse_special_float(trimmed) {
        return Ok(special);
    }
    
    // Try to parse as f64
    match trimmed.parse::<f64>() {
        Ok(v) => {
            // Check for overflow to infinity that wasn't explicitly requested
            // PostgreSQL returns "numeric value out of range" for this case
            if v.is_infinite() {
                return Err(format!(
                    "\"{}\" is out of range for type double precision",
                    s
                ));
            }
            Ok(v)
        }
        Err(_) => Err(format!("invalid input syntax for type double precision: \"{}\"", s)),
    }
}

/// Validates a numeric value and checks for overflow
/// 
/// Similar to validate_float_input but with explicit overflow checking.
/// Returns PostgreSQL-compatible error code 22003 for numeric overflow.
/// 
/// # Arguments
/// * `s` - The string to validate
/// * `type_name` - The PostgreSQL type name for error messages (e.g., "real", "double precision")
/// 
/// # Returns
/// * `Ok(f64)` if valid
/// * `Err(String)` with PostgreSQL-compatible error message if invalid or overflow
#[allow(dead_code)]
pub fn validate_numeric_with_overflow_check(s: &str, type_name: &str) -> Result<f64, String> {
    let trimmed = s.trim();
    
    // Check for empty or whitespace-only input
    if trimmed.is_empty() {
        return Err(format!("invalid input syntax for type {}: \"{}\"", type_name, s));
    }
    
    // Check for special float values first (these are valid)
    let lower = trimmed.to_lowercase();
    if lower == "nan" || lower == "'nan'" {
        return Ok(f64::NAN);
    }
    if lower == "infinity" || lower == "'infinity'" || lower == "inf" || lower == "'inf'" {
        return Ok(f64::INFINITY);
    }
    if lower == "-infinity" || lower == "'-infinity'" || lower == "-inf" || lower == "'-inf'" {
        return Ok(f64::NEG_INFINITY);
    }
    
    // Try to parse as f64
    match trimmed.parse::<f64>() {
        Ok(v) => {
            // Check for overflow to infinity that wasn't explicitly requested
            if v.is_infinite() {
                return Err(format!(
                    "\"{}\" is out of range for type {}",
                    s, type_name
                ));
            }
            Ok(v)
        }
        Err(_) => Err(format!("invalid input syntax for type {}: \"{}\"", type_name, s)),
    }
}

/// SQLite wrapper function for validate_float_input
/// This is registered as a scalar function for use in SQL
pub fn validate_float_sqlite(ctx: &Context) -> Result<f64> {
    let s: String = ctx.get(0)?;
    validate_float_input(&s).map_err(|e| rusqlite::Error::UserFunctionError(e.into()))
}

/// Register float validation function
pub fn register_float_validation(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "validate_float",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        validate_float_sqlite,
    )?;
    
    Ok(())
}

/// Get the SQL function call for a special float value if applicable
/// This is used by the transpiler to convert 'NaN'::float8 to nan()
/// Returns Some(function_call) if it's a special value, None otherwise
pub fn get_special_float_function(s: &str) -> Option<&'static str> {
    let normalized = s.trim().to_lowercase();
    // Remove quotes if present
    let normalized = normalized.trim_matches('\'');
    
    match normalized {
        "nan" => Some("nan()"),
        "infinity" | "inf" => Some("infinity()"),
        "-infinity" | "-inf" => Some("neg_infinity()"),
        _ => None,
    }
}

/// Parse a string that might be a special float value
/// Returns Some(f64) if it's a special value, None otherwise
pub fn parse_special_float(s: &str) -> Option<f64> {
    let normalized = s.trim().to_lowercase();
    match normalized.as_str() {
        "nan" | "'nan'" => Some(f64::NAN),
        "infinity" | "'infinity'" | "inf" | "'inf'" => Some(f64::INFINITY),
        "-infinity" | "'-infinity'" | "-inf" | "'-inf'" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

/// Check if a float value is NaN
#[allow(dead_code)]
pub fn is_nan_sqlite(ctx: &Context) -> Result<bool> {
    let val: f64 = ctx.get(0)?;
    Ok(val.is_nan())
}

/// Check if a float value is infinite
#[allow(dead_code)]
pub fn is_infinite_sqlite(ctx: &Context) -> Result<bool> {
    let val: f64 = ctx.get(0)?;
    Ok(val.is_infinite())
}

/// Check if a float value is finite
#[allow(dead_code)]
pub fn is_finite_sqlite(ctx: &Context) -> Result<bool> {
    let val: f64 = ctx.get(0)?;
    Ok(val.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_float_special_functions(&conn).unwrap();
        conn
    }

    #[test]
    fn test_nan_function() {
        let conn = setup_db();
        let result: f64 = conn.query_row("SELECT nan()", [], |row| row.get(0)).unwrap();
        assert!(result.is_nan());
    }

    #[test]
    fn test_infinity_function() {
        let conn = setup_db();
        let result: f64 = conn.query_row("SELECT infinity()", [], |row| row.get(0)).unwrap();
        assert!(result.is_infinite());
        assert!(result > 0.0);
    }

    #[test]
    fn test_neg_infinity_function() {
        let conn = setup_db();
        let result: f64 = conn.query_row("SELECT neg_infinity()", [], |row| row.get(0)).unwrap();
        assert!(result.is_infinite());
        assert!(result < 0.0);
    }

    #[test]
    fn test_parse_special_float() {
        assert!(parse_special_float("NaN").unwrap().is_nan());
        assert!(parse_special_float("nan").unwrap().is_nan());
        assert!(parse_special_float("'NaN'").unwrap().is_nan());

        assert_eq!(parse_special_float("infinity"), Some(f64::INFINITY));
        assert_eq!(parse_special_float("Infinity"), Some(f64::INFINITY));
        assert_eq!(parse_special_float("'infinity'"), Some(f64::INFINITY));

        assert_eq!(parse_special_float("-infinity"), Some(f64::NEG_INFINITY));
        assert_eq!(parse_special_float("-Infinity"), Some(f64::NEG_INFINITY));
        assert_eq!(parse_special_float("'-infinity'"), Some(f64::NEG_INFINITY));

        assert_eq!(parse_special_float("123.45"), None);
        assert_eq!(parse_special_float("hello"), None);
    }

    #[test]
    fn test_arithmetic_with_infinity() {
        let conn = setup_db();

        // Infinity + 100 = Infinity
        let result: f64 = conn
            .query_row("SELECT infinity() + 100.0", [], |row| row.get(0))
            .unwrap();
        assert!(result.is_infinite() && result > 0.0);

        // Infinity + (-Infinity) = NaN
        let result: f64 = conn
            .query_row("SELECT infinity() + neg_infinity()", [], |row| row.get(0))
            .unwrap();
        assert!(result.is_nan());

        // Infinity / Infinity = NaN
        let result: f64 = conn
            .query_row("SELECT infinity() / infinity()", [], |row| row.get(0))
            .unwrap();
        assert!(result.is_nan());

        // 1.0 / 0.0 = Infinity (using division, not the function)
        let result: f64 = conn.query_row("SELECT 1.0 / 0.0", [], |row| row.get(0)).unwrap();
        assert!(result.is_infinite());
    }

    #[test]
    fn test_comparison_with_nan() {
        let conn = setup_db();

        // NaN comparisons - in SQL, NaN comparisons are always false
        // But we're just testing the values are returned correctly
        let result: f64 = conn.query_row("SELECT nan()", [], |row| row.get(0)).unwrap();
        assert!(result.is_nan());
    }

    #[test]
    fn test_validate_float_input_valid() {
        // Valid float inputs
        assert!(validate_float_input("5.0").is_ok());
        assert_eq!(validate_float_input("5.0").unwrap(), 5.0);
        assert_eq!(validate_float_input("-3.14").unwrap(), -3.14);
        assert_eq!(validate_float_input("123").unwrap(), 123.0);
        assert_eq!(validate_float_input("0.0").unwrap(), 0.0);
        assert_eq!(validate_float_input("1e10").unwrap(), 1e10);
        assert_eq!(validate_float_input("-1.5e-3").unwrap(), -1.5e-3);
        assert_eq!(validate_float_input("  3.14  ").unwrap(), 3.14); // Leading/trailing spaces are OK
    }

    #[test]
    fn test_validate_float_input_invalid_xyz() {
        // Invalid text
        let result = validate_float_input("xyz");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("invalid input syntax"));
        assert!(err.contains("xyz"));
    }

    #[test]
    fn test_validate_float_input_invalid_multiple_decimals() {
        // Multiple decimal points
        let result = validate_float_input("5.0.0");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid input syntax"));
    }

    #[test]
    fn test_validate_float_input_invalid_spaces() {
        // Spaces in number
        let result = validate_float_input("5 . 0");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid input syntax"));
    }

    #[test]
    fn test_validate_float_input_invalid_spaces_in_negative() {
        // Spaces in negative number
        let result = validate_float_input("     - 3.0");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid input syntax"));
    }

    #[test]
    fn test_validate_float_input_invalid_empty() {
        // Empty string
        let result = validate_float_input("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid input syntax"));
    }

    #[test]
    fn test_validate_float_input_invalid_whitespace_only() {
        // Whitespace only
        let result = validate_float_input("       ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid input syntax"));
    }

    #[test]
    fn test_validate_float_input_special_values() {
        // Special values should still be valid
        assert!(validate_float_input("NaN").unwrap().is_nan());
        assert_eq!(validate_float_input("infinity").unwrap(), f64::INFINITY);
        assert_eq!(validate_float_input("-infinity").unwrap(), f64::NEG_INFINITY);
        assert!(validate_float_input("  NaN  ").unwrap().is_nan());
    }

    #[test]
    fn test_validate_float_sqlite_function() {
        let conn = Connection::open_in_memory().unwrap();
        register_float_validation(&conn).unwrap();

        // Valid inputs
        let result: f64 = conn
            .query_row("SELECT validate_float('3.14')", [], |row| row.get(0))
            .unwrap();
        assert_eq!(result, 3.14);

        let result: f64 = conn
            .query_row("SELECT validate_float('-42.5')", [], |row| row.get(0))
            .unwrap();
        assert_eq!(result, -42.5);

        // Invalid inputs should error
        let result: Result<f64, _> = conn
            .query_row("SELECT validate_float('xyz')", [], |row| row.get(0));
        assert!(result.is_err());

        let result: Result<f64, _> = conn
            .query_row("SELECT validate_float('5.0.0')", [], |row| row.get(0));
        assert!(result.is_err());

        let result: Result<f64, _> = conn
            .query_row("SELECT validate_float('')", [], |row| row.get(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_numeric_overflow_validation() {
        // Test that values overflowing to infinity are rejected
        // unless explicitly specified as "infinity"
        
        // Very large numbers that overflow to infinity should error
        let huge_number = "1e309";  // This overflows to +inf in f64
        let result = validate_float_input(huge_number);
        assert!(result.is_err(), "Should reject overflow to infinity: {}", huge_number);
        let err = result.unwrap_err();
        assert!(err.contains("out of range"), "Error should mention 'out of range': {}", err);
        
        // Negative huge number
        let neg_huge = "-1e309";
        let result = validate_float_input(neg_huge);
        assert!(result.is_err(), "Should reject negative overflow: {}", neg_huge);
        let err = result.unwrap_err();
        assert!(err.contains("out of range"), "Error should mention 'out of range': {}", err);
        
        // Explicit "infinity" should still be valid
        assert!(validate_float_input("infinity").is_ok());
        assert!(validate_float_input("'infinity'").is_ok());
        assert!(validate_float_input("-infinity").is_ok());
        
        // Normal large numbers should be fine
        assert!(validate_float_input("1e300").is_ok());
        assert!(validate_float_input("-1e300").is_ok());
    }

    #[test]
    fn test_validate_numeric_with_overflow_check() {
        // Test the explicit overflow check function with type name
        
        // Overflow cases
        let result = validate_numeric_with_overflow_check("1e309", "real");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of range"));
        
        let result = validate_numeric_with_overflow_check("-1e309", "double precision");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of range"));
        
        // Explicit infinity should work
        assert!(validate_numeric_with_overflow_check("infinity", "real").is_ok());
        assert!(validate_numeric_with_overflow_check("-infinity", "double precision").is_ok());
        
        // Normal values should work
        assert!(validate_numeric_with_overflow_check("3.14", "real").is_ok());
        assert!(validate_numeric_with_overflow_check("-42.5", "double precision").is_ok());
        
        // Error message should include type name
        let result = validate_numeric_with_overflow_check("1e309", "real");
        let err = result.unwrap_err();
        assert!(err.contains("real"), "Error should include type name: {}", err);
    }
}
