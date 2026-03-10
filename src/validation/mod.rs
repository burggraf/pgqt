pub mod types;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: String,  // SQLSTATE code
    pub message: String,
    #[allow(dead_code)]
    pub position: Option<usize>,
}

pub fn validate_varchar(value: &str, max_length: usize) -> Result<(), ValidationError> {
    if value.len() > max_length {
        return Err(ValidationError {
            code: "22001".to_string(),
            message: format!("value too long for type character varying({})", max_length),
            position: None,
        });
    }
    Ok(())
}

pub fn validate_char(value: &str, length: usize) -> Result<(), ValidationError> {
    if value.len() > length {
        return Err(ValidationError {
            code: "22001".to_string(),
            message: format!("value too long for type character({})", length),
            position: None,
        });
    }
    Ok(())
}

/// Main validation function that validates a value against a column type
/// Returns Ok(()) if valid, or Err(ValidationError) if invalid
pub fn validate_value(value: &str, column_type: &str) -> Result<(), ValidationError> {
    use crate::validation::types::{parse_type_modifier, extract_base_type};
    
    let base_type = extract_base_type(column_type);
    let modifier = parse_type_modifier(column_type);
    
    match base_type.as_str() {
        "VARCHAR" | "CHARACTER VARYING" => {
            if let Some(max_len) = modifier {
                validate_varchar(value, max_len)
            } else {
                Ok(()) // Unbounded VARCHAR - no length check
            }
        }
        "CHAR" | "CHARACTER" | "BPCHAR" => {
            let len = modifier.unwrap_or(1); // CHAR defaults to CHAR(1)
            validate_char(value, len)
        }
        "DATE" => types::validate_date(value),
        "REAL" | "FLOAT4" => types::validate_float4(value),
        "DOUBLE PRECISION" | "FLOAT8" => types::validate_float8(value),
        "SMALLINT" | "INT2" => types::validate_int2(value),
        "INTEGER" | "INT" | "INT4" => types::validate_int4(value),
        "BIGINT" | "INT8" => types::validate_int8(value),
        _ => Ok(()), // Unknown types pass through
    }
}
