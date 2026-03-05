//! Metadata provider trait for transpiler schema lookups
//!
//! This module defines the interface for the transpiler to query
//! database schema information during SQL transpilation.

/// Information about a table column
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ColumnInfo {
    pub name: String,
    pub original_type: String,
    pub default_expr: Option<String>,
    pub is_nullable: bool,
    /// Optional type OID for PostgreSQL type mapping
    #[allow(dead_code)]
    pub type_oid: Option<u32>,
}

impl ColumnInfo {
    /// Create a new ColumnInfo with basic information
    pub fn new(name: String, original_type: String, is_nullable: bool) -> Self {
        Self {
            name,
            original_type,
            default_expr: None,
            is_nullable,
            type_oid: None,
        }
    }

    /// Create a ColumnInfo with all fields
    pub fn with_all(
        name: String,
        original_type: String,
        default_expr: Option<String>,
        is_nullable: bool,
        type_oid: Option<u32>,
    ) -> Self {
        Self {
            name,
            original_type,
            default_expr,
            is_nullable,
            type_oid,
        }
    }
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

    /// Get the PostgreSQL type OID for a column
    ///
    /// Returns the OID if known, or None if the type is unknown.
    /// This is used by the result set rewriter to provide accurate type metadata.
    #[allow(dead_code)]
    fn get_column_type_oid(&self, table_name: &str, column_name: &str) -> Option<u32> {
        // Default implementation: look up column info and extract type OID
        self.get_table_columns(table_name)
            .and_then(|cols| {
                cols.into_iter()
                    .find(|c| c.name == column_name)
                    .and_then(|c| c.type_oid)
            })
    }
}

/// A no-op metadata provider that returns no information
///
/// Used when no catalog access is needed or available.
#[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoOpMetadataProvider {
    fn default() -> Self {
        Self::new()
    }
}