//! Types and structs for the transpilation context.

use dashmap::DashMap;
use std::sync::Arc;

/// Metadata for a column extracted from a CREATE TABLE statement
#[derive(Debug, Clone)]
pub struct ColumnTypeInfo {
    pub column_name: String,
    pub original_type: String,
    pub constraints: Option<String>,
}

/// Result of transpiling a SQL statement
#[derive(Debug)]
pub struct TranspileResult {
    pub sql: String,
    pub create_table_metadata: Option<CreateTableMetadata>,
    pub copy_metadata: Option<crate::copy::CopyStatement>,
    pub referenced_tables: Vec<String>,
    pub operation_type: OperationType,
    pub errors: Vec<String>,
}

/// Type of SQL operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    SELECT,
    INSERT,
    UPDATE,
    DELETE,
    DDL,
    OTHER,
}

/// Metadata extracted from a CREATE TABLE statement
#[derive(Debug)]
pub struct CreateTableMetadata {
    pub table_name: String,
    pub columns: Vec<ColumnTypeInfo>,
}

/// Context for the transpilation process
pub struct TranspileContext {
    pub referenced_tables: Vec<String>,
    pub errors: Vec<String>,
    pub functions: Option<Arc<DashMap<String, crate::catalog::FunctionMetadata>>>,
    /// Column aliases for VALUES statements (when VALUES is used with AS alias (col1, col2))
    pub values_column_aliases: Vec<String>,
}

impl TranspileContext {
    pub fn new() -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: None,
            values_column_aliases: Vec::new(),
        }
    }

    pub fn with_functions(functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>) -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: Some(functions),
            values_column_aliases: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn set_values_column_aliases(&mut self, aliases: Vec<String>) {
        self.values_column_aliases = aliases;
    }

    pub fn clear_values_column_aliases(&mut self) {
        self.values_column_aliases.clear();
    }
}
