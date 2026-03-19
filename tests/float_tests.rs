use rusqlite::Connection;
use pgqt::float_special::{register_float_special_functions, register_float_validation, validate_float_input};

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    register_float_special_functions(&conn).unwrap();
    register_float_validation(&conn).unwrap();
    conn
}

#[test]
fn test_nan_function() {
    let conn = setup_test_db();
    
    let result: f64 = conn
        .query_row("SELECT nan()", [], |row| row.get(0))
        .unwrap();
    
    assert!(result.is_nan(), "nan() should return NaN");
}

#[test]
fn test_infinity_function() {
    let conn = setup_test_db();
    
    let result: f64 = conn
        .query_row("SELECT infinity()", [], |row| row.get(0))
        .unwrap();
    
    assert!(result.is_infinite(), "infinity() should return infinity");
    assert!(result > 0.0, "infinity() should return positive infinity");
}

#[test]
fn test_neg_infinity_function() {
    let conn = setup_test_db();
    
    let result: f64 = conn
        .query_row("SELECT neg_infinity()", [], |row| row.get(0))
        .unwrap();
    
    assert!(result.is_infinite(), "neg_infinity() should return infinity");
    assert!(result < 0.0, "neg_infinity() should return negative infinity");
}

#[test]
fn test_infinity_arithmetic() {
    let conn = setup_test_db();
    
    
    let result: f64 = conn
        .query_row("SELECT infinity() + 100.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_infinite() && result > 0.0);
    
    
    let result: f64 = conn
        .query_row("SELECT infinity() + neg_infinity()", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_nan());
    
    
    let result: f64 = conn
        .query_row("SELECT infinity() * 2.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_infinite() && result > 0.0);
}

#[test]
fn test_nan_arithmetic() {
    let conn = setup_test_db();
    
    
    let result: f64 = conn
        .query_row("SELECT nan() + 100.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_nan());
    
    
    let result: f64 = conn
        .query_row("SELECT nan() * 0.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_nan());
}

#[test]
fn test_division_by_zero() {
    let conn = setup_test_db();
    
    
    let result: f64 = conn
        .query_row("SELECT 1.0 / 0.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_infinite() && result > 0.0);
    
    
    let result: f64 = conn
        .query_row("SELECT -1.0 / 0.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_infinite() && result < 0.0);
    
    
    let result: f64 = conn
        .query_row("SELECT 0.0 / 0.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_nan());
}

#[test]
fn test_infinity_division() {
    let conn = setup_test_db();
    
    
    let result: f64 = conn
        .query_row("SELECT infinity() / infinity()", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_nan());
    
    
    let result: f64 = conn
        .query_row("SELECT infinity() / 2.0", [], |row| row.get(0))
        .unwrap();
    assert!(result.is_infinite() && result > 0.0);
    
    
    let result: f64 = conn
        .query_row("SELECT 100.0 / infinity()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(result, 0.0);
}

#[test]
fn test_float8_nan_alias() {
    let conn = setup_test_db();
    
    let result: f64 = conn
        .query_row("SELECT float8_nan()", [], |row| row.get(0))
        .unwrap();
    
    assert!(result.is_nan(), "float8_nan() should return NaN");
}

#[test]
fn test_float8_infinity_alias() {
    let conn = setup_test_db();
    
    let result: f64 = conn
        .query_row("SELECT float8_infinity()", [], |row| row.get(0))
        .unwrap();
    
    assert!(result.is_infinite(), "float8_infinity() should return infinity");
    assert!(result > 0.0);
}

// Float Input Validation Tests (Phase 6.2)

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
    // Leading/trailing spaces are OK - they get trimmed
    assert_eq!(validate_float_input("  3.14  ").unwrap(), 3.14);
}

#[test]
fn test_validate_float_input_invalid_xyz() {
    // Invalid text like 'xyz'::float4 should error
    let result = validate_float_input("xyz");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("invalid input syntax"));
    assert!(err.contains("xyz"));
}

#[test]
fn test_validate_float_input_invalid_multiple_decimals() {
    // Multiple decimal points like '5.0.0'::float4 should error
    let result = validate_float_input("5.0.0");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("invalid input syntax"));
    assert!(err.contains("5.0.0"));
}

#[test]
fn test_validate_float_input_invalid_spaces() {
    // Spaces in number like '5 . 0'::float4 should error
    let result = validate_float_input("5 . 0");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("invalid input syntax"));
}

#[test]
fn test_validate_float_input_invalid_spaces_in_negative() {
    // Spaces in negative number like '     - 3.0'::float4 should error
    let result = validate_float_input("     - 3.0");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("invalid input syntax"));
}

#[test]
fn test_validate_float_input_invalid_empty() {
    // Empty string ''::float4 should error
    let result = validate_float_input("");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("invalid input syntax"));
}

#[test]
fn test_validate_float_input_invalid_whitespace_only() {
    // Whitespace only '       '::float4 should error
    let result = validate_float_input("       ");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("invalid input syntax"));
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
    let conn = setup_test_db();
    register_float_validation(&conn).unwrap();

    // Valid inputs should succeed
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
    assert!(result.is_err(), "'xyz' should be rejected");

    let result: Result<f64, _> = conn
        .query_row("SELECT validate_float('5.0.0')", [], |row| row.get(0));
    assert!(result.is_err(), "'5.0.0' should be rejected");

    let result: Result<f64, _> = conn
        .query_row("SELECT validate_float('5 . 0')", [], |row| row.get(0));
    assert!(result.is_err(), "'5 . 0' should be rejected");

    let result: Result<f64, _> = conn
        .query_row        ("SELECT validate_float('     - 3.0')", [], |row| row.get(0));
    assert!(result.is_err(), "'     - 3.0' should be rejected");

    let result: Result<f64, _> = conn
        .query_row("SELECT validate_float('')", [], |row| row.get(0));
    assert!(result.is_err(), "empty string should be rejected");

    let result: Result<f64, _> = conn
        .query_row("SELECT validate_float('       ')", [], |row| row.get(0));
    assert!(result.is_err(), "whitespace-only should be rejected");
}
