pub mod types;

use crate::transpiler::context::TranspileContext;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: String,  // SQLSTATE code
    pub message: String,
    pub position: Option<usize>,
}

pub trait Validator {
    fn validate(&self, ctx: &TranspileContext) -> Result<(), ValidationError>;
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
