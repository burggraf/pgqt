//! Tests for the validation framework

use pgqt::validation::{validate_varchar, validate_char, ValidationError};

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
