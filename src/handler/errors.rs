//! PostgreSQL error code mapping from SQLite and internal errors.
//!
//! This module provides centralized error mapping to convert SQLite errors
//! and internal errors into PostgreSQL SQLSTATE codes for proper client
//! compatibility.

use pgwire::error::ErrorInfo;

/// PostgreSQL SQLSTATE error codes
/// 
/// See: https://www.postgresql.org/docs/current/errcodes-appendix.html
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgErrorCode {
    /// Successful completion
    Success,
    /// Integrity constraint violation (generic)
    IntegrityConstraintViolation,
    /// Unique violation
    UniqueViolation,
    /// Foreign key violation
    ForeignKeyViolation,
    /// Check constraint violation
    CheckViolation,
    /// Not null violation
    NotNullViolation,
    /// Syntax error or access rule violation
    SyntaxError,
    /// Undefined table
    UndefinedTable,
    /// Undefined column
    UndefinedColumn,
    /// Insufficient privilege
    InsufficientPrivilege,
    /// Invalid cursor state
    InvalidCursorState,
    /// Invalid cursor name
    InvalidCursorName,
    /// Internal error (catch-all)
    InternalError,
    /// Query canceled
    QueryCanceled,
    /// Connection exception
    ConnectionException,
    /// Invalid parameter value
    InvalidParameterValue,
    /// Data exception
    DataException,
    /// Programming error
    ProgrammingError,
    /// String data right truncation
    StringDataRightTruncation,
    /// Invalid text representation
    InvalidTextRepresentation,
    /// Invalid datetime format
    InvalidDatetimeFormat,
    /// Datetime field overflow
    DatetimeFieldOverflow,
    /// Numeric value out of range
    NumericValueOutOfRange,
    /// Feature not supported
    FeatureNotSupported,
    /// Invalid authorization specification
    InvalidAuthorizationSpecification,
    /// Transaction rollback (generic)
    TransactionRollback,
    /// Serialization failure (concurrent update)
    SerializationFailure,
    /// In failed SQL transaction (25P02)
    InFailedSqlTransaction,
}

impl PgErrorCode {
    /// Get the SQLSTATE code as a string
    pub fn code(&self) -> &'static str {
        match self {
            PgErrorCode::Success => "00000",
            PgErrorCode::IntegrityConstraintViolation => "23000",
            PgErrorCode::UniqueViolation => "23505",
            PgErrorCode::ForeignKeyViolation => "23503",
            PgErrorCode::CheckViolation => "23514",
            PgErrorCode::NotNullViolation => "23502",
            PgErrorCode::SyntaxError => "42601",
            PgErrorCode::UndefinedTable => "42P01",
            PgErrorCode::UndefinedColumn => "42703",
            PgErrorCode::InsufficientPrivilege => "42501",
            PgErrorCode::InvalidCursorState => "24000",
            PgErrorCode::InvalidCursorName => "34000",
            PgErrorCode::InternalError => "XX000",
            PgErrorCode::QueryCanceled => "57014",
            PgErrorCode::ConnectionException => "08000",
            PgErrorCode::InvalidParameterValue => "22023",
            PgErrorCode::DataException => "22000",
            PgErrorCode::ProgrammingError => "42000",
            PgErrorCode::StringDataRightTruncation => "22001",
            PgErrorCode::InvalidTextRepresentation => "22P02",
            PgErrorCode::InvalidDatetimeFormat => "22007",
            PgErrorCode::DatetimeFieldOverflow => "22008",
            PgErrorCode::NumericValueOutOfRange => "22003",
            PgErrorCode::FeatureNotSupported => "0A000",
            PgErrorCode::InvalidAuthorizationSpecification => "28000",
            PgErrorCode::TransactionRollback => "40000",
            PgErrorCode::SerializationFailure => "40001",
            PgErrorCode::InFailedSqlTransaction => "25P02",
        }
    }
}

/// PostgreSQL error with full diagnostic information
#[derive(Debug, Clone)]
pub struct PgError {
    /// SQLSTATE code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Severity level (ERROR, FATAL, PANIC)
    pub severity: String,
    /// Optional detail message with additional context
    pub detail: Option<String>,
    /// Optional hint for resolving the error
    pub hint: Option<String>,
}

impl PgError {
    /// Create a new PgError with the given code and message
    pub fn new(code: PgErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.code().to_string(),
            message: message.into(),
            severity: "ERROR".to_string(),
            detail: None,
            hint: None,
        }
    }

    /// Create a new PgError from a code enum variant and message
    pub fn with_code(code: PgErrorCode, message: impl Into<String>) -> Self {
        Self::new(code, message)
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(PgErrorCode::InternalError, message)
    }

    /// Create a syntax error
    pub fn syntax(message: impl Into<String>) -> Self {
        Self::new(PgErrorCode::SyntaxError, message)
    }

    /// Add detail to the error
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add hint to the error
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Convert to pgwire ErrorInfo for wire protocol response
    pub fn into_error_info(self) -> ErrorInfo {
        ErrorInfo::new(self.severity, self.code, self.message)
    }

    /// Convert from an anyhow::Error by attempting to downcast to known types
    pub fn from_anyhow(error: anyhow::Error) -> Self {
        // Try to downcast to rusqlite::Error
        if let Some(sqlite_err) = error.downcast_ref::<rusqlite::Error>() {
            return Self::from_rusqlite_ref(sqlite_err);
        }

        // Try to extract pg_query::Error
        let err_str = error.to_string();
        if err_str.contains("syntax error") || err_str.contains("parse error") {
            return Self::new(PgErrorCode::SyntaxError, err_str);
        }

        // Check for specific error patterns in the message
        if err_str.contains("permission denied") {
            return Self::new(PgErrorCode::InsufficientPrivilege, err_str);
        }

        // Default to internal error
        Self::new(PgErrorCode::InternalError, err_str)
    }

    /// Convert from a rusqlite::Error reference to PgError
    fn from_rusqlite_ref(error: &rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::SqliteFailure(err, msg) => {
                let code = Self::map_sqlite_error_code(err);
                let message = if code == PgErrorCode::SerializationFailure {
                    // Use PostgreSQL-compatible message for busy errors
                    "could not serialize access due to concurrent update".to_string()
                } else {
                    msg.clone().unwrap_or_else(|| {
                        // Use the SQLite error string representation
                        rusqlite::ffi::code_to_str(err.extended_code).to_string()
                    })
                };
                
                Self {
                    code: code.code().to_string(),
                    message,
                    severity: "ERROR".to_string(),
                    detail: Some(format!("SQLite error code: {:?}", err.code)),
                    hint: None,
                }
            }
            rusqlite::Error::QueryReturnedNoRows => {
                Self::new(PgErrorCode::InternalError, "Query returned no rows")
            }
            rusqlite::Error::InvalidParameterCount(expected, actual) => {
                Self::new(PgErrorCode::ProgrammingError, 
                    format!("Invalid parameter count: expected {}, got {}", expected, actual))
            }
            rusqlite::Error::InvalidColumnType(idx, name, ty) => {
                Self::new(PgErrorCode::DataException,
                    format!("Invalid column type at index {}: {} (type {:?})", idx, name, ty))
            }
            rusqlite::Error::InvalidColumnName(name) => {
                Self::new(PgErrorCode::UndefinedColumn, format!("Invalid column name: {}", name))
            }
            rusqlite::Error::InvalidQuery => {
                Self::new(PgErrorCode::SyntaxError, "Invalid query")
            }
            rusqlite::Error::InvalidColumnIndex(idx) => {
                Self::new(PgErrorCode::UndefinedColumn, format!("Invalid column index: {}", idx))
            }
            rusqlite::Error::IntegralValueOutOfRange(idx, val) => {
                Self::new(PgErrorCode::DataException,
                    format!("Integer value {} out of range at index {}", val, idx))
            }
            rusqlite::Error::UserFunctionError(e) => {
                // Check for specific user function error messages
                let msg = e.to_string();
                if msg.contains("no such table") {
                    Self::new(PgErrorCode::UndefinedTable, msg)
                } else if msg.contains("no such column") {
                    Self::new(PgErrorCode::UndefinedColumn, msg)
                } else {
                    Self::new(PgErrorCode::InternalError, msg)
                }
            }
            _ => {
                Self::new(PgErrorCode::InternalError, error.to_string())
            }
        }
    }

    /// Map SQLite error codes to PostgreSQL error codes
    fn map_sqlite_error_code(err: &rusqlite::ffi::Error) -> PgErrorCode {
        // First check the primary code
        match err.code {
            rusqlite::ffi::ErrorCode::ConstraintViolation => {
                // Check extended code for specific constraint type
                match err.extended_code {
                    // SQLITE_CONSTRAINT_UNIQUE (2067)
                    2067 | 1555 => PgErrorCode::UniqueViolation, // SQLITE_CONSTRAINT_UNIQUE, SQLITE_CONSTRAINT_PRIMARYKEY
                    // SQLITE_CONSTRAINT_FOREIGNKEY (787)
                    787 => PgErrorCode::ForeignKeyViolation,
                    // SQLITE_CONSTRAINT_NOTNULL (1299)
                    1299 => PgErrorCode::NotNullViolation,
                    // SQLITE_CONSTRAINT_CHECK (275)
                    275 => PgErrorCode::CheckViolation,
                    // SQLITE_CONSTRAINT_TRIGGER (1811)
                    1811 => PgErrorCode::CheckViolation, // Treat trigger constraints as check
                    _ => PgErrorCode::IntegrityConstraintViolation,
                }
            }
            rusqlite::ffi::ErrorCode::PermissionDenied => {
                PgErrorCode::InsufficientPrivilege
            }
            rusqlite::ffi::ErrorCode::DatabaseBusy => {
                PgErrorCode::SerializationFailure
            }
            rusqlite::ffi::ErrorCode::DatabaseLocked => {
                PgErrorCode::SerializationFailure
            }
            rusqlite::ffi::ErrorCode::ReadOnly => {
                PgErrorCode::InsufficientPrivilege
            }
            rusqlite::ffi::ErrorCode::OperationInterrupted => {
                PgErrorCode::QueryCanceled
            }
            rusqlite::ffi::ErrorCode::TypeMismatch => {
                PgErrorCode::DataException
            }
            rusqlite::ffi::ErrorCode::NotFound => {
                // This is typically for internal operations, not table not found
                PgErrorCode::InternalError
            }
            rusqlite::ffi::ErrorCode::Unknown => {
                // SQLITE_ERROR - often syntax errors or "no such table"
                // Check extended code for more details
                PgErrorCode::SyntaxError
            }
            _ => PgErrorCode::InternalError,
        }
    }

    /// Get a PostgreSQL-compatible error message for SQLite errors
    /// This provides better error messages that match PostgreSQL conventions
    pub fn message_for_sqlite_error(err: &rusqlite::Error) -> String {
        match err {
            rusqlite::Error::SqliteFailure(sqlite_err, _) => {
                match sqlite_err.code {
                    rusqlite::ffi::ErrorCode::DatabaseBusy | rusqlite::ffi::ErrorCode::DatabaseLocked => {
                        "could not serialize access due to concurrent update".to_string()
                    }
                    _ => err.to_string(),
                }
            }
            _ => err.to_string(),
        }
    }
}

impl From<PgError> for ErrorInfo {
    fn from(err: PgError) -> Self {
        err.into_error_info()
    }
}

impl std::fmt::Display for PgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(ref detail) = self.detail {
            write!(f, "\nDETAIL: {}", detail)?;
        }
        if let Some(ref hint) = self.hint {
            write!(f, "\nHINT: {}", hint)?;
        }
        Ok(())
    }
}

impl std::error::Error for PgError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pg_error_code_values() {
        assert_eq!(PgErrorCode::Success.code(), "00000");
        assert_eq!(PgErrorCode::UniqueViolation.code(), "23505");
        assert_eq!(PgErrorCode::ForeignKeyViolation.code(), "23503");
        assert_eq!(PgErrorCode::CheckViolation.code(), "23514");
        assert_eq!(PgErrorCode::NotNullViolation.code(), "23502");
        assert_eq!(PgErrorCode::SyntaxError.code(), "42601");
        assert_eq!(PgErrorCode::UndefinedTable.code(), "42P01");
        assert_eq!(PgErrorCode::UndefinedColumn.code(), "42703");
        assert_eq!(PgErrorCode::InsufficientPrivilege.code(), "42501");
        assert_eq!(PgErrorCode::InternalError.code(), "XX000");
    }

    #[test]
    fn test_pg_error_creation() {
        let err = PgError::new(PgErrorCode::UniqueViolation, "duplicate key value violates unique constraint");
        assert_eq!(err.code, "23505");
        assert_eq!(err.message, "duplicate key value violates unique constraint");
        assert_eq!(err.severity, "ERROR");
        assert!(err.detail.is_none());
        assert!(err.hint.is_none());
    }

    #[test]
    fn test_pg_error_with_detail_and_hint() {
        let err = PgError::new(PgErrorCode::UniqueViolation, "duplicate key")
            .with_detail("Key (id)=(1) already exists")
            .with_hint("Try using a different value");
        
        assert_eq!(err.detail, Some("Key (id)=(1) already exists".to_string()));
        assert_eq!(err.hint, Some("Try using a different value".to_string()));
    }

    #[test]
    fn test_pg_error_into_error_info() {
        let err = PgError::new(PgErrorCode::SyntaxError, "syntax error at or near \"SELECT\"");
        let info = err.into_error_info();
        
        // ErrorInfo has fields: severity, code, message
        assert_eq!(info.severity, "ERROR");
        assert_eq!(info.code, "42601");
        assert_eq!(info.message, "syntax error at or near \"SELECT\"");
    }

    #[test]
    fn test_from_anyhow_string_error() {
        let err = PgError::from_anyhow(anyhow::anyhow!("some internal error"));
        assert_eq!(err.code, "XX000");
        assert_eq!(err.message, "some internal error");
    }

    #[test]
    fn test_from_anyhow_permission_error() {
        let err = PgError::from_anyhow(anyhow::anyhow!("permission denied for table users"));
        assert_eq!(err.code, "42501");
    }

    #[test]
    fn test_from_anyhow_syntax_error() {
        let err = PgError::from_anyhow(anyhow::anyhow!("syntax error at SELECT"));
        assert_eq!(err.code, "42601");
    }

    #[test]
    fn test_display_format() {
        let err = PgError::new(PgErrorCode::UniqueViolation, "duplicate key")
            .with_detail("Key (id)=(1) already exists")
            .with_hint("Use a different value");
        
        let display = format!("{}", err);
        assert!(display.contains("[23505]"));
        assert!(display.contains("duplicate key"));
        assert!(display.contains("DETAIL:"));
        assert!(display.contains("HINT:"));
    }

    #[test]
    fn test_map_sqlite_constraint_unique() {
        // SQLITE_CONSTRAINT_UNIQUE (2067)
        let err = rusqlite::ffi::Error::new(2067);
        assert_eq!(PgError::map_sqlite_error_code(&err), PgErrorCode::UniqueViolation);
    }

    #[test]
    fn test_map_sqlite_constraint_foreignkey() {
        // SQLITE_CONSTRAINT_FOREIGNKEY (787)
        let err = rusqlite::ffi::Error::new(787);
        assert_eq!(PgError::map_sqlite_error_code(&err), PgErrorCode::ForeignKeyViolation);
    }

    #[test]
    fn test_map_sqlite_constraint_notnull() {
        // SQLITE_CONSTRAINT_NOTNULL (1299)
        let err = rusqlite::ffi::Error::new(1299);
        assert_eq!(PgError::map_sqlite_error_code(&err), PgErrorCode::NotNullViolation);
    }

    #[test]
    fn test_map_sqlite_constraint_check() {
        // SQLITE_CONSTRAINT_CHECK (275)
        let err = rusqlite::ffi::Error::new(275);
        assert_eq!(PgError::map_sqlite_error_code(&err), PgErrorCode::CheckViolation);
    }
}