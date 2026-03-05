//! Result Set Rewriter
//!
//! This module handles the transformation of SQLite result sets into PostgreSQL-compatible
//! responses. It focuses on:
//! - Column name handling (including PostgreSQL's `?column?` convention for anonymous columns)
//! - Type OID mapping from the shadow catalog
//! - Row encoding for the PostgreSQL wire protocol

use pgwire::api::Type;

/// Information about a column for result set construction
#[derive(Debug, Clone)]
pub struct ColumnFieldInfo {
    pub name: String,
    pub pg_type: Type,
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
}
