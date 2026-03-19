pub mod types;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: String,  
    pub message: String,
    #[allow(dead_code)]
    pub position: Option<usize>,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    
    // Strip quotes from the value for validation
    let unquoted_value = value.trim_matches('\'');
    
    match base_type.as_str() {
        "VARCHAR" | "CHARACTER VARYING" => {
            if let Some(max_len) = modifier {
                // Use trimming validation to match PostgreSQL behavior
                types::validate_varchar_value(unquoted_value, max_len)
            } else {
                Ok(()) 
            }
        }
        "CHAR" | "CHARACTER" | "BPCHAR" => {
            let len = modifier.unwrap_or(1); 
            // Use trimming validation to match PostgreSQL behavior
            types::validate_char_value(unquoted_value, len)
        }
        "DATE" => types::validate_date(value),
        "REAL" | "FLOAT4" => types::validate_float4(value),
        "DOUBLE PRECISION" | "FLOAT8" => types::validate_float8(value),
        "SMALLINT" | "INT2" => types::validate_int2(value),
        "INTEGER" | "INT" | "INT4" => types::validate_int4(value),
        "BIGINT" | "INT8" => types::validate_int8(value),
        "INTERVAL" => types::validate_interval(value),
        "JSON" | "JSONB" => types::validate_json(value),
        _ => Ok(()), 
    }
}
