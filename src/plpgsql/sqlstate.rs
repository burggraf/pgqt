//! SQLSTATE error code mapping for PL/pgSQL exception handling
//!
//! This module provides mappings between SQLSTATE codes and PostgreSQL
//! condition names used in PL/pgSQL EXCEPTION blocks.

use std::collections::HashMap;
use lazy_static::lazy_static;

/// SQLSTATE error codes
#[allow(dead_code)]
pub const SQLSTATE_SUCCESSFUL_COMPLETION: &str = "00000";
#[allow(dead_code)]
pub const SQLSTATE_WARNING: &str = "01000";
#[allow(dead_code)]
pub const SQLSTATE_NO_DATA: &str = "02000";
#[allow(dead_code)]
pub const SQLSTATE_TOO_MANY_ROWS: &str = "P0003";
#[allow(dead_code)]
pub const SQLSTATE_DIVISION_BY_ZERO: &str = "22012";
#[allow(dead_code)]
pub const SQLSTATE_NUMERIC_VALUE_OUT_OF_RANGE: &str = "22003";
#[allow(dead_code)]
pub const SQLSTATE_INVALID_TEXT_REPRESENTATION: &str = "22P02";
#[allow(dead_code)]
pub const SQLSTATE_FOREIGN_KEY_VIOLATION: &str = "23503";
#[allow(dead_code)]
pub const SQLSTATE_UNIQUE_VIOLATION: &str = "23505";
#[allow(dead_code)]
pub const SQLSTATE_CHECK_VIOLATION: &str = "23514";
#[allow(dead_code)]
pub const SQLSTATE_NOT_NULL_VIOLATION: &str = "23502";
#[allow(dead_code)]
pub const SQLSTATE_EXCLUSION_VIOLATION: &str = "23P01";
#[allow(dead_code)]
pub const SQLSTATE_INVALID_PARAMETER_VALUE: &str = "22023";
#[allow(dead_code)]
pub const SQLSTATE_UNDEFINED_COLUMN: &str = "42703";
#[allow(dead_code)]
pub const SQLSTATE_UNDEFINED_TABLE: &str = "42P01";
#[allow(dead_code)]
pub const SQLSTATE_UNDEFINED_FUNCTION: &str = "42883";
#[allow(dead_code)]
pub const SQLSTATE_DUPLICATE_TABLE: &str = "42P07";
#[allow(dead_code)]
pub const SQLSTATE_DUPLICATE_COLUMN: &str = "42701";
#[allow(dead_code)]
pub const SQLSTATE_AMBIGUOUS_COLUMN: &str = "42702";
#[allow(dead_code)]
pub const SQLSTATE_SYNTAX_ERROR: &str = "42601";
#[allow(dead_code)]
pub const SQLSTATE_FEATURE_NOT_SUPPORTED: &str = "0A000";
#[allow(dead_code)]
pub const SQLSTATE_IN_FAILED_SQL_TRANSACTION: &str = "25P02";
#[allow(dead_code)]
pub const SQLSTATE_RAISE_EXCEPTION: &str = "P0001";
#[allow(dead_code)]
pub const SQLSTATE_NO_DATA_FOUND: &str = "P0002";
#[allow(dead_code)]
pub const SQLSTATE_ASSERT_FAILURE: &str = "P0004";

// Mapping from condition names to SQLSTATE codes
lazy_static! {
    pub static ref CONDITION_MAP: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("successful_completion", SQLSTATE_SUCCESSFUL_COMPLETION);
        m.insert("warning", SQLSTATE_WARNING);
        m.insert("no_data", SQLSTATE_NO_DATA);
        m.insert("too_many_rows", SQLSTATE_TOO_MANY_ROWS);
        m.insert("division_by_zero", SQLSTATE_DIVISION_BY_ZERO);
        m.insert("numeric_value_out_of_range", SQLSTATE_NUMERIC_VALUE_OUT_OF_RANGE);
        m.insert("invalid_text_representation", SQLSTATE_INVALID_TEXT_REPRESENTATION);
        m.insert("foreign_key_violation", SQLSTATE_FOREIGN_KEY_VIOLATION);
        m.insert("unique_violation", SQLSTATE_UNIQUE_VIOLATION);
        m.insert("check_violation", SQLSTATE_CHECK_VIOLATION);
        m.insert("not_null_violation", SQLSTATE_NOT_NULL_VIOLATION);
        m.insert("exclusion_violation", SQLSTATE_EXCLUSION_VIOLATION);
        m.insert("invalid_parameter_value", SQLSTATE_INVALID_PARAMETER_VALUE);
        m.insert("undefined_column", SQLSTATE_UNDEFINED_COLUMN);
        m.insert("undefined_table", SQLSTATE_UNDEFINED_TABLE);
        m.insert("undefined_function", SQLSTATE_UNDEFINED_FUNCTION);
        m.insert("duplicate_table", SQLSTATE_DUPLICATE_TABLE);
        m.insert("duplicate_column", SQLSTATE_DUPLICATE_COLUMN);
        m.insert("ambiguous_column", SQLSTATE_AMBIGUOUS_COLUMN);
        m.insert("syntax_error", SQLSTATE_SYNTAX_ERROR);
        m.insert("feature_not_supported", SQLSTATE_FEATURE_NOT_SUPPORTED);
        m.insert("in_failed_sql_transaction", SQLSTATE_IN_FAILED_SQL_TRANSACTION);
        m.insert("raise_exception", SQLSTATE_RAISE_EXCEPTION);
        m.insert("no_data_found", SQLSTATE_NO_DATA_FOUND);
        m.insert("assert_failure", SQLSTATE_ASSERT_FAILURE);
        m
    };
}

/// Get SQLSTATE code for a condition name
#[allow(dead_code)]
pub fn get_sqlstate(condition: &str) -> Option<&'static str> {
    CONDITION_MAP.get(condition).copied()
}

/// Check if a SQLSTATE matches a condition (handles class matching)
#[allow(dead_code)]
pub fn sqlstate_matches(sqlstate: &str, condition: &str) -> bool {
    if let Some(expected) = get_sqlstate(condition) {
        // Exact match
        if sqlstate == expected {
            return true;
        }
        
        // Class match (e.g., '22' matches all '22xxx' codes)
        if condition.len() == 2 && sqlstate.starts_with(condition) {
            return true;
        }
    }
    
    false
}

/// Get error class from SQLSTATE (first 2 characters)
#[allow(dead_code)]
pub fn get_error_class(sqlstate: &str) -> &str {
    if sqlstate.len() >= 2 {
        &sqlstate[0..2]
    } else {
        "XX"
    }
}

/// Get error category description
#[allow(dead_code)]
pub fn get_error_category(sqlstate: &str) -> &'static str {
    let class = get_error_class(sqlstate);
    match class {
        "00" => "Successful Completion",
        "01" => "Warning",
        "02" => "No Data",
        "03" => "SQL Statement Not Yet Complete",
        "08" => "Connection Exception",
        "09" => "Triggered Action Exception",
        "0A" => "Feature Not Supported",
        "0B" => "Invalid Transaction Initiation",
        "0F" => "Locator Exception",
        "0L" => "Invalid Grantor",
        "0P" => "Invalid Role Specification",
        "0Z" => "Diagnostics Exception",
        "20" => "Case Not Found",
        "21" => "Cardinality Violation",
        "22" => "Data Exception",
        "23" => "Integrity Constraint Violation",
        "24" => "Invalid Cursor State",
        "25" => "Invalid Transaction State",
        "26" => "Invalid SQL Statement Name",
        "27" => "Triggered Data Change Violation",
        "28" => "Invalid Authorization Specification",
        "2B" => "Dependent Privilege Descriptors Still Exist",
        "2D" => "Invalid Transaction Termination",
        "2F" => "SQL Routine Exception",
        "34" => "Invalid Cursor Name",
        "38" => "External Routine Exception",
        "39" => "External Routine Invocation Exception",
        "3B" => "Savepoint Exception",
        "3D" => "Invalid Catalog Name",
        "3F" => "Invalid Schema Name",
        "40" => "Transaction Rollback",
        "42" => "Syntax Error or Access Rule Violation",
        "44" => "WITH CHECK OPTION Violation",
        "53" => "Insufficient Resources",
        "54" => "Program Limit Exceeded",
        "55" => "Object Not In Prerequisite State",
        "57" => "Operator Intervention",
        "58" => "System Error",
        "72" => "Snapshot Failure",
        "F0" => "Configuration File Error",
        "HV" => "Foreign Data Wrapper Error",
        "P0" => "PL/pgSQL Error",
        "XX" => "Internal Error",
        _ => "Unknown Error Class",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sqlstate() {
        assert_eq!(get_sqlstate("division_by_zero"), Some(SQLSTATE_DIVISION_BY_ZERO));
        assert_eq!(get_sqlstate("unique_violation"), Some(SQLSTATE_UNIQUE_VIOLATION));
        assert_eq!(get_sqlstate("no_data_found"), Some(SQLSTATE_NO_DATA_FOUND));
        assert_eq!(get_sqlstate("unknown_condition"), None);
    }

    #[test]
    fn test_sqlstate_matches() {
        assert!(sqlstate_matches("22012", "division_by_zero"));
        assert!(sqlstate_matches("23505", "unique_violation"));
        assert!(!sqlstate_matches("22012", "unique_violation"));
    }

    #[test]
    fn test_get_error_class() {
        assert_eq!(get_error_class("22012"), "22");
        assert_eq!(get_error_class("23505"), "23");
        assert_eq!(get_error_class("P0001"), "P0");
    }

    #[test]
    fn test_get_error_category() {
        assert_eq!(get_error_category("22012"), "Data Exception");
        assert_eq!(get_error_category("23505"), "Integrity Constraint Violation");
        assert_eq!(get_error_category("P0001"), "PL/pgSQL Error");
    }
}
