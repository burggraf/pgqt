//! Tests for the validation framework

use pgqt::validation::{validate_varchar, validate_char, validate_value, ValidationError};
use pgqt::validation::types::{parse_type_modifier, extract_base_type, validate_date, validate_float4, validate_float8, validate_int2, validate_int4, validate_int8};

#[test]
fn test_varchar_length_validation_rejects_too_long() {
    // Test that validate_varchar rejects values that are too long
    let result = validate_varchar("hello", 3);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, "22001");
    assert!(err.message.contains("value too long for type character varying(3)"));
}

#[test]
fn test_varchar_length_validation_accepts_valid() {
    // Test that valid values pass
    let result = validate_varchar("hi", 3);
    assert!(result.is_ok());
}

#[test]
fn test_varchar_exact_length() {
    // Test that exact length values pass
    let result = validate_varchar("abc", 3);
    assert!(result.is_ok());
}

#[test]
fn test_varchar_empty_string() {
    // Test that empty string passes
    let result = validate_varchar("", 3);
    assert!(result.is_ok());
}

#[test]
fn test_char_length_validation_rejects_too_long() {
    // Test that validate_char rejects values that are too long
    let result = validate_char("hello", 3);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, "22001");
    assert!(err.message.contains("value too long for type character(3)"));
}

#[test]
fn test_char_length_validation_accepts_valid() {
    // Test that valid values pass
    let result = validate_char("hi", 3);
    assert!(result.is_ok());
}

#[test]
fn test_char_exact_length() {
    // Test that exact length values pass
    let result = validate_char("abc", 3);
    assert!(result.is_ok());
}

#[test]
fn test_char_empty_string() {
    // Test that empty string passes
    let result = validate_char("", 3);
    assert!(result.is_ok());
}

#[test]
fn test_validation_error_debug() {
    let err = ValidationError {
        code: "22001".to_string(),
        message: "test error".to_string(),
        position: Some(10),
    };
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("22001"));
    assert!(debug_str.contains("test error"));
}

#[test]
fn test_validation_error_clone() {
    let err = ValidationError {
        code: "22001".to_string(),
        message: "test error".to_string(),
        position: Some(10),
    };
    let cloned = err.clone();
    assert_eq!(err.code, cloned.code);
    assert_eq!(err.message, cloned.message);
    assert_eq!(err.position, cloned.position);
}

// Type modifier parsing tests
#[test]
fn test_parse_varchar_modifier() {
    assert_eq!(parse_type_modifier("VARCHAR(10)"), Some(10));
    assert_eq!(parse_type_modifier("varchar(255)"), Some(255));
    assert_eq!(parse_type_modifier("VARCHAR(1)"), Some(1));
}

#[test]
fn test_parse_char_modifier() {
    assert_eq!(parse_type_modifier("CHAR(5)"), Some(5));
    assert_eq!(parse_type_modifier("char(1)"), Some(1));
    assert_eq!(parse_type_modifier("CHARACTER(100)"), Some(100));
}

#[test]
fn test_parse_no_modifier() {
    assert_eq!(parse_type_modifier("TEXT"), None);
    assert_eq!(parse_type_modifier("INTEGER"), None);
    assert_eq!(parse_type_modifier("VARCHAR"), None);
}

#[test]
fn test_extract_base_type() {
    assert_eq!(extract_base_type("VARCHAR(10)"), "VARCHAR");
    assert_eq!(extract_base_type("char(5)"), "CHAR");
    assert_eq!(extract_base_type("TEXT"), "TEXT");
    assert_eq!(extract_base_type("integer"), "INTEGER");
}

// Date validation tests
#[test]
fn test_valid_date() {
    assert!(validate_date("'2024-03-15'").is_ok());
    assert!(validate_date("2024-03-15").is_ok());
    assert!(validate_date("'2024-12-31'").is_ok());
}

#[test]
fn test_valid_leap_year_date() {
    // 2024 is a leap year
    assert!(validate_date("'2024-02-29'").is_ok());
}

#[test]
fn test_invalid_date_non_leap_year() {
    // 2023 is not a leap year
    let result = validate_date("'2023-02-29'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22008");
}

#[test]
fn test_invalid_date_month() {
    let result = validate_date("'2024-13-01'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22008");
}

#[test]
fn test_invalid_date_day() {
    let result = validate_date("'2024-04-31'"); // April has 30 days
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22008");
}

// Float4 validation tests
#[test]
fn test_valid_float4() {
    assert!(validate_float4("'3.14'").is_ok());
    assert!(validate_float4("'1.5e10'").is_ok());
    assert!(validate_float4("'-2.5'").is_ok());
}

#[test]
fn test_float4_overflow() {
    // 10e70 is way beyond f32 range
    let result = validate_float4("'10e70'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22003");
}

#[test]
fn test_float4_large_exponent() {
    let result = validate_float4("'1e100'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22003");
}

// Float8 validation tests
#[test]
fn test_valid_float8() {
    assert!(validate_float8("'3.14159265359'").is_ok());
    assert!(validate_float8("'1.5e100'").is_ok());
}

#[test]
fn test_float8_overflow() {
    // 1e400 is beyond f64 range
    let result = validate_float8("'1e400'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22003");
}

// Integer validation tests
#[test]
fn test_valid_int2() {
    assert!(validate_int2("'32767'").is_ok());
    assert!(validate_int2("'-32768'").is_ok());
    assert!(validate_int2("'100'").is_ok());
}

#[test]
fn test_int2_overflow() {
    let result = validate_int2("'32768'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22003");
}

#[test]
fn test_valid_int4() {
    assert!(validate_int4("'2147483647'").is_ok());
    assert!(validate_int4("'-2147483648'").is_ok());
}

#[test]
fn test_int4_overflow() {
    let result = validate_int4("'2147483648'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22003");
}

#[test]
fn test_valid_int8() {
    assert!(validate_int8("'9223372036854775807'").is_ok());
    assert!(validate_int8("'-9223372036854775808'").is_ok());
}

#[test]
fn test_int8_overflow() {
    let result = validate_int8("'9223372036854775808'");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "22003");
}

// validate_value integration tests
#[test]
fn test_validate_value_varchar() {
    assert!(validate_value("hello", "VARCHAR(10)").is_ok());
    assert!(validate_value("hello world", "VARCHAR(5)").is_err());
}

#[test]
fn test_validate_value_char() {
    assert!(validate_value("ab", "CHAR(5)").is_ok());
    assert!(validate_value("abcdef", "CHAR(5)").is_err());
}

#[test]
fn test_validate_value_date() {
    assert!(validate_value("2024-03-15", "DATE").is_ok());
    assert!(validate_value("2023-02-29", "DATE").is_err());
}

#[test]
fn test_validate_value_float4() {
    assert!(validate_value("3.14", "REAL").is_ok());
    assert!(validate_value("10e70", "REAL").is_err());
}

#[test]
fn test_validate_value_integer() {
    assert!(validate_value("100", "INTEGER").is_ok());
    assert!(validate_value("2147483648", "INTEGER").is_err());
}
