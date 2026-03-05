//! Result Set Rewriter
//!
//! This module handles the transformation of SQLite result sets into PostgreSQL-compatible
//! responses. It focuses on:
//! - Column name handling (including PostgreSQL's `?column?` convention for anonymous columns)
//! - Type OID mapping from the shadow catalog
//! - Row encoding for the PostgreSQL wire protocol

use std::sync::Arc;
use anyhow::Result;
use rusqlite::{Connection, Statement};
use pgwire::api::results::{FieldFormat, FieldInfo};
use pgwire::api::Type;

use crate::catalog::get_table_columns_with_defaults;
use crate::transpiler::metadata::MetadataProvider;

/// Information about a column for result set construction
#[derive(Debug, Clone)]
pub struct ColumnFieldInfo {
    pub name: String,
    pub table_name: Option<String>,
    pub original_type: Option<String>,
    pub pg_type: Type,
}

/// Result Set Rewriter
///
/// Transforms SQLite result sets into PostgreSQL-compatible responses,
/// handling column names and type OIDs based on catalog metadata.
pub struct ResultSetRewriter {
    /// Connection to the SQLite database for catalog lookups
    conn: Arc<std::sync::Mutex<Connection>>,
    /// Metadata provider for column type lookups
    metadata_provider: Option<Arc<dyn MetadataProvider>>,
}

impl ResultSetRewriter {
    /// Create a new ResultSetRewriter
    pub fn new(conn: Arc<std::sync::Mutex<Connection>>) -> Self {
        Self {
            conn,
            metadata_provider: None,
        }
    }

    /// Set the metadata provider for column type lookups
    pub fn with_metadata_provider(mut self, provider: Arc<dyn MetadataProvider>) -> Self {
        self.metadata_provider = Some(provider);
        self
    }

    /// Rewrite field information for a SQLite statement
    ///
    /// For each column in the result set:
    /// 1. Get the column name from SQLite
    /// 2. If it's a simple column reference, look up its original PostgreSQL type
    /// 3. Map the type to a pgwire Type
    /// 4. For expressions, use PostgreSQL conventions (`?column?`) or infer types
    ///
    /// # Arguments
    /// * `sqlite_stmt` - The prepared SQLite statement
    /// * `referenced_tables` - Tables referenced in the query (for catalog lookups)
    pub fn rewrite_field_info(
        &self,
        sqlite_stmt: &Statement,
        referenced_tables: &[String],
    ) -> Result<Vec<FieldInfo>> {
        let col_count = sqlite_stmt.column_count();
        let mut fields = Vec::with_capacity(col_count);

        // Build a map of table -> columns from the catalog
        let table_columns = self.get_table_columns_from_catalog(referenced_tables)?;

        for i in 0..col_count {
            let col_name = sqlite_stmt.column_name(i)?.to_string();
            
            // Determine the column's origin and type
            let field_info = self.determine_field_info(&col_name, &table_columns, referenced_tables);
            
            fields.push(FieldInfo::new(
                field_info.name,
                None,  // No table OID for simplicity
                None,  // No column attribute number
                field_info.pg_type,
                FieldFormat::Text,
            ));
        }

        Ok(fields)
    }

    /// Determine field info for a column
    ///
    /// Checks if the column is a simple column reference or an expression,
    /// and returns appropriate name and type.
    fn determine_field_info(
        &self,
        col_name: &str,
        table_columns: &std::collections::HashMap<String, Vec<ColumnFieldInfo>>,
        referenced_tables: &[String],
    ) -> ColumnFieldInfo {
        // Check if this is a known column from one of the referenced tables
        for table_name in referenced_tables {
            if let Some(columns) = table_columns.get(table_name) {
                for col in columns {
                    if col.name == col_name {
                        return ColumnFieldInfo {
                            name: col_name.to_string(),
                            table_name: Some(table_name.clone()),
                            original_type: Some(col.original_type.clone().unwrap_or_default()),
                            pg_type: col.pg_type.clone(),
                        };
                    }
                }
            }
        }

        // Check for special column names that indicate expressions
        if col_name == "?column?" {
            // Anonymous expression column
            return ColumnFieldInfo {
                name: "?column?".to_string(),
                table_name: None,
                original_type: None,
                pg_type: Type::TEXT,
            };
        }

        // Check for common aggregate/function patterns in the column name
        let lower_name = col_name.to_lowercase();
        let inferred_type = self.infer_type_from_column_name(&lower_name);

        // If the column name looks like an expression (contains parentheses, operators, etc.)
        // use PostgreSQL's ?column? convention
        if self.is_expression_column_name(col_name) {
            return ColumnFieldInfo {
                name: "?column?".to_string(),
                table_name: None,
                original_type: None,
                pg_type: inferred_type,
            };
        }

        // Default: use the column name as-is with inferred type
        ColumnFieldInfo {
            name: col_name.to_string(),
            table_name: None,
            original_type: None,
            pg_type: inferred_type,
        }
    }

    /// Check if a column name looks like an expression rather than a simple identifier
    fn is_expression_column_name(&self, name: &str) -> bool {
        // Contains parentheses (function call)
        if name.contains('(') || name.contains(')') {
            return true;
        }
        // Contains operators
        if name.contains('+') || name.contains('-') || name.contains('*') || name.contains('/') {
            return true;
        }
        // Starts with a digit (computed value)
        if name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return true;
        }
        // Contains spaces (expression with spaces)
        if name.contains(' ') {
            return true;
        }
        false
    }

    /// Infer PostgreSQL type from column name patterns
    fn infer_type_from_column_name(&self, name: &str) -> Type {
        // Common aggregate functions
        if name.starts_with("count(") || name.starts_with("count ") {
            return Type::INT8;
        }
        if name.starts_with("sum(") || name.starts_with("sum ") {
            return Type::NUMERIC;
        }
        if name.starts_with("avg(") || name.starts_with("avg ") {
            return Type::NUMERIC;
        }
        if name.starts_with("min(") || name.starts_with("max(") || 
           name.starts_with("min ") || name.starts_with("max ") {
            // Could be any type, default to text
            return Type::TEXT;
        }
        
        // Common string functions
        if name.starts_with("lower(") || name.starts_with("upper(") ||
           name.starts_with("trim(") || name.starts_with("substr(") ||
           name.starts_with("substring(") || name.starts_with("replace(") {
            return Type::TEXT;
        }
        
        // Common numeric functions
        if name.starts_with("abs(") || name.starts_with("round(") ||
           name.starts_with("floor(") || name.starts_with("ceil(") ||
           name.starts_with("ceiling(") {
            return Type::NUMERIC;
        }
        
        // Boolean expressions
        if name.starts_with("exists(") || name.contains(" is ") || 
           name.ends_with(" isnull") || name.ends_with(" notnull") {
            return Type::BOOL;
        }

        // Default to TEXT
        Type::TEXT
    }

    /// Get column information from the catalog for referenced tables
    fn get_table_columns_from_catalog(
        &self,
        referenced_tables: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<ColumnFieldInfo>>> {
        let mut result = std::collections::HashMap::new();
        
        let conn = self.conn.lock().unwrap();
        
        for table_name in referenced_tables {
            let columns = get_table_columns_with_defaults(&conn, table_name)?;
            
            let field_infos: Vec<ColumnFieldInfo> = columns
                .iter()
                .map(|col| {
                    let pg_type = map_original_type_to_pg_type(&col.original_type);
                    ColumnFieldInfo {
                        name: col.column_name.clone(),
                        table_name: Some(table_name.clone()),
                        original_type: Some(col.original_type.clone()),
                        pg_type,
                    }
                })
                .collect();
            
            result.insert(table_name.clone(), field_infos);
        }
        
        Ok(result)
    }
}

/// Map a PostgreSQL type string to a pgwire Type
pub fn map_original_type_to_pg_type(original_type: &str) -> Type {
    let upper = original_type.to_uppercase();
    
    // Handle array types first - return TEXT since we store arrays as JSON text
    if upper.ends_with("[]") || upper.starts_with("ARRAY") {
        return Type::TEXT;
    }
    
    // Handle common PostgreSQL types
    match upper.trim() {
        // Integer types
        "SMALLINT" | "INT2" | "SMALLSERIAL" => Type::INT2,
        "INTEGER" | "INT" | "INT4" | "SERIAL" => Type::INT4,
        "BIGINT" | "INT8" | "BIGSERIAL" => Type::INT8,
        
        // Floating point types
        "REAL" | "FLOAT4" => Type::FLOAT4,
        "DOUBLE PRECISION" | "FLOAT8" => Type::FLOAT8,
        "NUMERIC" | "DECIMAL" => Type::NUMERIC,
        
        // Boolean
        "BOOLEAN" | "BOOL" => Type::BOOL,
        
        // String types
        "TEXT" => Type::TEXT,
        "VARCHAR" | "CHARACTER VARYING" => Type::VARCHAR,
        "CHAR" | "CHARACTER" | "BPCHAR" => Type::BPCHAR,
        "NAME" => Type::NAME,
        
        // Binary
        "BYTEA" => Type::BYTEA,
        
        // Date/Time types
        "DATE" => Type::DATE,
        "TIME" | "TIME WITHOUT TIME ZONE" => Type::TIME,
        "TIMETZ" | "TIME WITH TIME ZONE" => Type::TIMETZ,
        "TIMESTAMP" | "TIMESTAMP WITHOUT TIME ZONE" => Type::TIMESTAMP,
        "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => Type::TIMESTAMPTZ,
        "INTERVAL" => Type::INTERVAL,
        
        // JSON types
        "JSON" => Type::JSON,
        "JSONB" => Type::JSONB,
        
        // UUID
        "UUID" => Type::UUID,
        
        // Network types
        "INET" => Type::INET,
        "CIDR" => Type::CIDR,
        "MACADDR" => Type::MACADDR,
        "MACADDR8" => Type::MACADDR8,
        
        // Monetary
        "MONEY" => Type::MONEY,
        
        // Geometric types
        "POINT" => Type::POINT,
        "LINE" => Type::LINE,
        "LSEG" => Type::LSEG,
        "BOX" => Type::BOX,
        "PATH" => Type::PATH,
        "POLYGON" => Type::POLYGON,
        "CIRCLE" => Type::CIRCLE,
        
        // Full-text search
        "TSVECTOR" => Type::TS_VECTOR,
        "TSQUERY" => Type::TSQUERY,
        
        // XML
        "XML" => Type::XML,
        
        // Bit types
        "BIT" => Type::BIT,
        "VARBIT" | "BIT VARYING" => Type::VARBIT,
        
        // OID types
        "OID" => Type::OID,
        "REGCLASS" => Type::REGCLASS,
        "REGTYPE" => Type::REGTYPE,
        "REGPROC" => Type::REGPROC,
        
        // Range types - return TEXT since we store ranges as text
        "INT4RANGE" | "INT8RANGE" | "NUMRANGE" | "TSRANGE" | 
        "TSTZRANGE" | "DATERANGE" => Type::TEXT,
        
        // Vector types (pgvector) - stored as text
        t if t.starts_with("VECTOR") => Type::TEXT,
        
        // Default to TEXT
        _ => Type::TEXT,
    }
}

/// Get the PostgreSQL type OID for a type name
pub fn get_type_oid(type_name: &str) -> u32 {
    map_original_type_to_pg_type(type_name).oid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_integer_types() {
        assert_eq!(map_original_type_to_pg_type("SMALLINT"), Type::INT2);
        assert_eq!(map_original_type_to_pg_type("INT2"), Type::INT2);
        assert_eq!(map_original_type_to_pg_type("INTEGER"), Type::INT4);
        assert_eq!(map_original_type_to_pg_type("INT4"), Type::INT4);
        assert_eq!(map_original_type_to_pg_type("INT"), Type::INT4);
        assert_eq!(map_original_type_to_pg_type("SERIAL"), Type::INT4);
        assert_eq!(map_original_type_to_pg_type("BIGINT"), Type::INT8);
        assert_eq!(map_original_type_to_pg_type("INT8"), Type::INT8);
        assert_eq!(map_original_type_to_pg_type("BIGSERIAL"), Type::INT8);
    }

    #[test]
    fn test_map_float_types() {
        assert_eq!(map_original_type_to_pg_type("REAL"), Type::FLOAT4);
        assert_eq!(map_original_type_to_pg_type("FLOAT4"), Type::FLOAT4);
        assert_eq!(map_original_type_to_pg_type("DOUBLE PRECISION"), Type::FLOAT8);
        assert_eq!(map_original_type_to_pg_type("FLOAT8"), Type::FLOAT8);
        assert_eq!(map_original_type_to_pg_type("NUMERIC"), Type::NUMERIC);
        assert_eq!(map_original_type_to_pg_type("DECIMAL"), Type::NUMERIC);
    }

    #[test]
    fn test_map_string_types() {
        assert_eq!(map_original_type_to_pg_type("TEXT"), Type::TEXT);
        assert_eq!(map_original_type_to_pg_type("VARCHAR"), Type::VARCHAR);
        assert_eq!(map_original_type_to_pg_type("CHARACTER VARYING"), Type::VARCHAR);
        assert_eq!(map_original_type_to_pg_type("CHAR"), Type::BPCHAR);
        assert_eq!(map_original_type_to_pg_type("BPCHAR"), Type::BPCHAR);
    }

    #[test]
    fn test_map_datetime_types() {
        assert_eq!(map_original_type_to_pg_type("DATE"), Type::DATE);
        assert_eq!(map_original_type_to_pg_type("TIME"), Type::TIME);
        assert_eq!(map_original_type_to_pg_type("TIMESTAMP"), Type::TIMESTAMP);
        assert_eq!(map_original_type_to_pg_type("TIMESTAMPTZ"), Type::TIMESTAMPTZ);
        assert_eq!(map_original_type_to_pg_type("TIMESTAMP WITH TIME ZONE"), Type::TIMESTAMPTZ);
    }

    #[test]
    fn test_map_special_types() {
        assert_eq!(map_original_type_to_pg_type("BOOLEAN"), Type::BOOL);
        assert_eq!(map_original_type_to_pg_type("BOOL"), Type::BOOL);
        assert_eq!(map_original_type_to_pg_type("JSON"), Type::JSON);
        assert_eq!(map_original_type_to_pg_type("JSONB"), Type::JSONB);
        assert_eq!(map_original_type_to_pg_type("UUID"), Type::UUID);
        assert_eq!(map_original_type_to_pg_type("BYTEA"), Type::BYTEA);
    }

    #[test]
    fn test_map_array_types() {
        assert_eq!(map_original_type_to_pg_type("INTEGER[]"), Type::TEXT);
        assert_eq!(map_original_type_to_pg_type("TEXT[]"), Type::TEXT);
    }

    #[test]
    fn test_map_unknown_type() {
        assert_eq!(map_original_type_to_pg_type("UNKNOWN_TYPE"), Type::TEXT);
        assert_eq!(map_original_type_to_pg_type("custom_type"), Type::TEXT);
    }

    #[test]
    fn test_is_expression_column_name() {
        let rewriter = ResultSetRewriter::new(Arc::new(std::sync::Mutex::new(
            Connection::open_in_memory().unwrap()
        )));
        
        // Simple identifiers
        assert!(!rewriter.is_expression_column_name("id"));
        assert!(!rewriter.is_expression_column_name("user_name"));
        assert!(!rewriter.is_expression_column_name("CreatedAt"));
        
        // Expressions
        assert!(rewriter.is_expression_column_name("count(*)"));
        assert!(rewriter.is_expression_column_name("1 + 2"));
        assert!(rewriter.is_expression_column_name("42"));
        assert!(rewriter.is_expression_column_name("sum(value)"));
    }

    #[test]
    fn test_infer_type_from_column_name() {
        let rewriter = ResultSetRewriter::new(Arc::new(std::sync::Mutex::new(
            Connection::open_in_memory().unwrap()
        )));
        
        assert_eq!(rewriter.infer_type_from_column_name("count(*)"), Type::INT8);
        assert_eq!(rewriter.infer_type_from_column_name("sum(amount)"), Type::NUMERIC);
        assert_eq!(rewriter.infer_type_from_column_name("avg(score)"), Type::NUMERIC);
        assert_eq!(rewriter.infer_type_from_column_name("lower(name)"), Type::TEXT);
        assert_eq!(rewriter.infer_type_from_column_name("round(value)"), Type::NUMERIC);
        assert_eq!(rewriter.infer_type_from_column_name("unknown_func()"), Type::TEXT);
    }
}