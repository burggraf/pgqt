//! Shadow catalog (`__pg_meta__`) for storing PostgreSQL metadata in SQLite
//!
//! This module manages the hidden system tables that store original PostgreSQL
//! type information, RLS policies, function definitions, and system catalog views.
//! The shadow catalog enables reversible migrations back to PostgreSQL by preserving
//! type metadata that SQLite cannot natively represent.
//!
//! ## Sub-modules
//!
//! | Module          | Description                                                  |
//! |----------------|--------------------------------------------------------------|
//! | [`init`]        | Catalog initialization and pg_types population               |
//! | [`table`]       | Table and column metadata storage and retrieval              |
//! | [`function`]    | User-defined function (UDF) metadata storage                 |
//! | [`trigger`]     | Trigger metadata storage                                     |
//! | [`rls`]         | Row-Level Security policy storage                            |
//! | [`system_views`]| PostgreSQL-compatible system catalog views (pg_class, etc.)  |
//!
//! ## Shadow Tables
//!
//! - `__pg_meta__` — Column-level type metadata (table, column, original_type, constraints)
//! - `__pg_functions__` — User-defined function definitions
//! - `__pg_triggers__` — Trigger definitions
//! - `__pg_rls_policies__` — Row-Level Security policies
//! - `__pg_rls_tables__` — Tables with RLS enabled/forced

use serde::{Deserialize, Serialize};

mod init;
mod table;
mod function;
pub mod trigger;
mod rls;
mod system_views;

// Re-export all public items from submodules
pub use init::init_catalog;
#[allow(unused_imports)]
pub use init::init_pg_types;
pub use table::{
    store_table_metadata, store_relation_metadata,
    populate_pg_attribute, populate_pg_index, populate_pg_constraint,
};
#[allow(unused_imports)]
pub use table::{store_column_metadata, get_table_metadata, get_column_metadata, delete_table_metadata, get_table_columns_with_defaults, extract_default_from_constraints};
pub use function::{store_function, get_function, drop_function};
pub use trigger::{store_trigger, get_trigger, drop_trigger, get_triggers_for_table};
pub use rls::{is_rls_enabled, is_rls_forced, get_applicable_policies};
#[allow(unused_imports)]
pub use rls::{enable_rls, disable_rls, store_rls_policy, drop_rls_policy, get_table_policies};
pub use system_views::init_system_views;

/// Represents column metadata for a table
#[derive(Debug, Clone)]
pub struct ColumnMetadata {
    pub table_name: String,
    pub column_name: String,
    pub original_type: String,
    pub constraints: Option<String>,
}

/// Function parameter mode
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParamMode {
    In,
    Out,
    InOut,
    Variadic,
}

/// Function return type category
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReturnTypeKind {
    Scalar,
    SetOf,
    Table,
    Void,
}

/// Function metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetadata {
    pub oid: i64,
    pub name: String,
    pub schema: String,
    pub arg_types: Vec<String>,
    pub arg_names: Vec<String>,
    pub arg_modes: Vec<ParamMode>,
    pub return_type: String,
    pub return_type_kind: ReturnTypeKind,
    pub return_table_cols: Option<Vec<(String, String)>>,
    pub function_body: String,
    pub language: String,
    pub volatility: String,
    pub strict: bool,
    pub security_definer: bool,
    pub parallel: String,
    pub owner_oid: i64,
    pub created_at: Option<String>,
}

/// RLS Policy metadata
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RlsPolicy {
    pub name: String,
    pub table_name: String,
    pub command: String,
    pub permissive: bool,
    pub roles: Vec<String>,
    pub using_expr: Option<String>,
    pub with_check_expr: Option<String>,
    pub enabled: bool,
}

/// Trigger timing - BEFORE, AFTER, or INSTEAD OF
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

/// Trigger event - INSERT, UPDATE, DELETE, or TRUNCATE
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
    Truncate,
}

/// Row-level or statement-level trigger
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowOrStatement {
    Row,
    Statement,
}

/// Trigger metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMetadata {
    pub oid: i64,
    pub name: String,
    pub table_oid: i64,
    pub table_name: String,       // for convenience
    pub timing: TriggerTiming,    // BEFORE, AFTER, INSTEAD OF
    pub events: Vec<TriggerEvent>, // INSERT, UPDATE, DELETE, TRUNCATE
    pub row_or_statement: RowOrStatement, // ROW or STATEMENT
    pub enabled: bool,
    pub function_oid: i64,
    pub function_name: String,    // for convenience
    pub args: Vec<String>,        // trigger arguments
    pub is_internal: bool,
    pub is_constraint: bool,
    pub deferrable: bool,
    pub initially_deferred: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_catalog(&conn).unwrap();
        conn
    }

    #[test]
    fn test_init_catalog_creates_table() {
        let conn = setup_test_db();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = '__pg_meta__'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_store_and_retrieve_column_metadata() {
        let conn = setup_test_db();

        let metadata = ColumnMetadata {
            table_name: "test_table".to_string(),
            column_name: "name".to_string(),
            original_type: "VARCHAR(10)".to_string(),
            constraints: Some("NOT NULL".to_string()),
        };

        store_column_metadata(&conn, &metadata).unwrap();

        let retrieved =
            get_column_metadata(&conn, "test_table", "name")
                .unwrap()
                .expect("Should find metadata");

        assert_eq!(retrieved.table_name, "test_table");
        assert_eq!(retrieved.column_name, "name");
        assert_eq!(retrieved.original_type, "VARCHAR(10)");
        assert_eq!(retrieved.constraints, Some("NOT NULL".to_string()));
    }

    #[test]
    fn test_store_table_metadata() {
        let conn = setup_test_db();

        let columns = vec![
            ("id".to_string(), "SERIAL".to_string(), None),
            (
                "name".to_string(),
                "VARCHAR(10)".to_string(),
                Some("NOT NULL".to_string()),
            ),
            (
                "created_at".to_string(),
                "TIMESTAMP WITH TIME ZONE".to_string(),
                None,
            ),
        ];

        store_table_metadata(&conn, "test_table", &columns).unwrap();

        let metadata = get_table_metadata(&conn, "test_table").unwrap();
        assert_eq!(metadata.len(), 3);

        let types: Vec<String> = metadata
            .iter()
            .map(|m| m.original_type.clone())
            .collect();
        assert!(types.contains(&"SERIAL".to_string()));
        assert!(types.contains(&"VARCHAR(10)".to_string()));
        assert!(types.contains(&"TIMESTAMP WITH TIME ZONE".to_string()));
    }

    #[test]
    fn test_get_nonexistent_column() {
        let conn = setup_test_db();
        let result = get_column_metadata(&conn, "nonexistent", "col").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_table_metadata() {
        let conn = setup_test_db();

        let metadata = ColumnMetadata {
            table_name: "test_table".to_string(),
            column_name: "name".to_string(),
            original_type: "VARCHAR(10)".to_string(),
            constraints: None,
        };

        store_column_metadata(&conn, &metadata).unwrap();
        delete_table_metadata(&conn, "test_table").unwrap();

        let result = get_table_metadata(&conn, "test_table").unwrap();
        assert!(result.is_empty());
    }
}
