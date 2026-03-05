//! Metadata provider trait for transpiler schema lookups
//!
//! This module defines the interface for the transpiler to query
//! database schema information during SQL transpilation.

/// Information about a table column
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub original_type: String,
    pub default_expr: Option<String>,
    pub is_nullable: bool,
}

/// Trait for providing table metadata to the transpiler
///
/// Implementations of this trait allow the transpiler to query
/// the database catalog for schema information during transpilation.
pub trait MetadataProvider: Send + Sync {
    /// Get column information for a table
    ///
    /// Returns a vector of ColumnInfo structs in column order.
    /// Returns None if the table is not found.
    fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>>;

    /// Get default expression for a specific column
    ///
    /// Returns the default expression string, or None if no default.
    fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String>;
}

/// A no-op metadata provider that returns no information
///
/// Used when no catalog access is needed or available.
pub struct NoOpMetadataProvider;

impl MetadataProvider for NoOpMetadataProvider {
    fn get_table_columns(&self, _table_name: &str) -> Option<Vec<ColumnInfo>> {
        None
    }

    fn get_column_default(&self, _table_name: &str, _column_name: &str) -> Option<String> {
        None
    }
}

impl NoOpMetadataProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoOpMetadataProvider {
    fn default() -> Self {
        Self::new()
    }
}