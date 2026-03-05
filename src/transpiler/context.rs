//! Types and structs for the transpilation context.

use dashmap::DashMap;
use std::sync::Arc;
use super::metadata::{ColumnInfo, MetadataProvider};

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
    /// Whether we're currently in a subquery context (for VALUES handling)
    pub in_subquery: bool,
    /// Metadata provider for schema lookups during transpilation
    metadata_provider: Option<Arc<dyn MetadataProvider>>,
    /// Current column index when processing VALUES (for DEFAULT resolution)
    pub current_column_index: usize,
    /// Current table name when processing INSERT (for metadata lookups)
    pub current_table: Option<String>,
    /// Whether we are inside a VALUES clause (for column naming: column1, column2, ...)
    pub in_values_clause: bool,
}

impl Default for TranspileContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TranspileContext {
    pub fn new() -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: None,
            values_column_aliases: Vec::new(),
            in_subquery: false,
            metadata_provider: None,
            current_column_index: 0,
            current_table: None,
            in_values_clause: false,
        }
    }

    pub fn with_functions(functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>) -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: Some(functions),
            values_column_aliases: Vec::new(),
            in_subquery: false,
            metadata_provider: None,
            current_column_index: 0,
            current_table: None,
            in_values_clause: false,
        }
    }

    /// Create a new context with a metadata provider
    #[allow(dead_code)]
    pub fn with_metadata_provider(provider: Arc<dyn MetadataProvider>) -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: None,
            values_column_aliases: Vec::new(),
            in_subquery: false,
            metadata_provider: Some(provider),
            current_column_index: 0,
            current_table: None,
            in_values_clause: false,
        }
    }
    
    /// Set the metadata provider
    pub fn set_metadata_provider(&mut self, provider: Arc<dyn MetadataProvider>) {
        self.metadata_provider = Some(provider);
    }
    
    /// Get column information for a table
    pub fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>> {
        self.metadata_provider.as_ref()
            .and_then(|p| p.get_table_columns(table_name))
    }
    
    /// Get default expression for a column
    pub fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String> {
        self.metadata_provider.as_ref()
            .and_then(|p| p.get_column_default(table_name, column_name))
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

    pub fn enter_subquery(&mut self) {
        self.in_subquery = true;
    }

    pub fn exit_subquery(&mut self) {
        self.in_subquery = false;
    }
}
