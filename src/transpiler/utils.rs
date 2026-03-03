//! Utility functions for type extraction and rewriting
//!
//! This module provides helper functions for:
//! - Extracting original PostgreSQL type names from AST nodes
//! - Rewriting PostgreSQL types to SQLite-compatible equivalents

use pg_query::protobuf::{TypeName};
use pg_query::protobuf::node::Node as NodeEnum;

/// Extract the original PostgreSQL type name from TypeName
/// Maps internal PostgreSQL type names back to user-facing names
pub(super) fn extract_original_type(type_name: &Option<TypeName>) -> String {
    if let Some(tn) = type_name {
        let names: Vec<String> = tn
            .names
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::String(s) = inner {
                        return Some(s.sval.clone());
                    }
                }
                None
            })
            .collect();

        if names.is_empty() {
            return "TEXT".to_string();
        }

        let base_type = names.last().unwrap().to_uppercase();

        // Map internal PostgreSQL type names to user-facing names
        let mapped_type = match base_type.as_str() {
            "TIMESTAMPTZ" => "TIMESTAMP WITH TIME ZONE",
            "TIMESTAMP" => "TIMESTAMP WITHOUT TIME ZONE",
            "TIMETZ" => "TIME WITH TIME ZONE",
            "TIME" => "TIME WITHOUT TIME ZONE",
            "VARBIT" => "BIT VARYING",
            "BPCHAR" => "CHARACTER",
            "VARCHAR" => "VARCHAR",
            "CHAR" => "CHARACTER",
            "INT8" => "BIGINT",
            "INT4" => "INTEGER",
            "INT2" => "SMALLINT",
            "FLOAT4" => "REAL",
            "FLOAT8" => "DOUBLE PRECISION",
            "BOOL" => "BOOLEAN",
            _ => &base_type,
        };

        if tn.typmods.is_empty() {
            mapped_type.to_string()
        } else {
            let mods: Vec<String> = tn
                .typmods
                .iter()
                .filter_map(|m| {
                    if let Some(ref inner) = m.node {
                        if let NodeEnum::AConst(ref aconst) = inner {
                            if let Some(ref val) = aconst.val {
                                if let pg_query::protobuf::a_const::Val::Ival(i) = val {
                                    return Some(i.ival.to_string());
                                }
                            }
                        }
                    }
                    None
                })
                .collect();

            if mods.is_empty() {
                mapped_type.to_string()
            } else {
                format!("{}({})", mapped_type, mods.join(", "))
            }
        }
    } else {
        "TEXT".to_string()
    }
}

/// Rewrite PostgreSQL types to SQLite-compatible types
/// Comprehensive coverage of all PostgreSQL data types
/// Returns lowercase types for consistency
pub(super) fn rewrite_type_for_sqlite(pg_type: &str) -> String {
    let upper = pg_type.to_uppercase();

    // Serial types (auto-increment)
    if upper.starts_with("SERIAL") || upper.starts_with("SMALLSERIAL") || upper.starts_with("BIGSERIAL") {
        return "integer primary key autoincrement".to_string();
    }

    // Character/String types
    if upper.starts_with("VARCHAR")
        || upper.starts_with("CHARACTER VARYING")
        || upper.starts_with("CHAR")
        || upper.starts_with("CHARACTER")
        || upper.starts_with("BPCHAR")
        || upper == "TEXT"
    {
        return "text".to_string();
    }

    // Array types - stored as JSON text (check before INT to handle INT[])
    if upper.ends_with("[]") || upper.starts_with("ARRAY") {
        return "text".to_string();
    }

    // Range types - stored as TEXT
    if upper == "INT4RANGE" 
        || upper == "INT8RANGE" 
        || upper == "NUMRANGE" 
        || upper == "TSRANGE"
        || upper == "TSTZRANGE"
        || upper == "DATERANGE"
    {
        return "text".to_string();
    }

    // Integer types
    if upper.starts_with("INT") 
        || upper.starts_with("INTEGER") 
        || upper.starts_with("BIGINT") 
        || upper.starts_with("SMALLINT")
        || upper.starts_with("INT2")
        || upper.starts_with("INT4")
        || upper.starts_with("INT8")
    {
        return "integer".to_string();
    }

    // Floating point and numeric types
    if upper.starts_with("REAL")
        || upper.starts_with("FLOAT")
        || upper.starts_with("FLOAT4")
        || upper.starts_with("FLOAT8")
        || upper.starts_with("DOUBLE")
        || upper.starts_with("NUMERIC")
        || upper.starts_with("DECIMAL")
    {
        return "real".to_string();
    }

    // Boolean type
    if upper == "BOOLEAN" || upper == "BOOL" {
        return "integer".to_string();
    }

    // Date/Time types
    if upper.starts_with("TIMESTAMP")
        || upper.starts_with("DATE")
        || upper.starts_with("TIME")
        || upper.starts_with("INTERVAL")
    {
        return "text".to_string();
    }

    // JSON types
    if upper == "JSON" || upper == "JSONB" || upper.starts_with("JSON") {
        return "text".to_string();
    }

    // UUID type
    if upper == "UUID" {
        return "text".to_string();
    }

    // Binary data
    if upper == "BYTEA" {
        return "blob".to_string();
    }

    // VECTOR type (pgvector compatibility) - stored as TEXT (JSON format)
    if upper.starts_with("VECTOR") {
        return "text".to_string();
    }

    // Money type - store as REAL (or TEXT for precision)
    if upper == "MONEY" {
        return "real".to_string();
    }

    // Bit string types
    if upper.starts_with("BIT") || upper.starts_with("VARBIT") {
        return "text".to_string();
    }

    // XML type
    if upper == "XML" {
        return "text".to_string();
    }

    // Network address types
    if upper == "INET" || upper == "CIDR" || upper == "MACADDR" || upper == "MACADDR8" {
        return "text".to_string();
    }

    // Geometric types - all stored as TEXT (representations)
    if upper == "POINT" 
        || upper == "LINE" 
        || upper == "LSEG" 
        || upper == "BOX" 
        || upper == "PATH" 
        || upper == "POLYGON" 
        || upper == "CIRCLE" 
    {
        return "text".to_string();
    }

    // Full-text search types
    if upper == "TSVECTOR" || upper == "TSQUERY" {
        return "text".to_string();
    }

    // Default to TEXT for unknown types (ENUM, DOMAIN, composite types, etc.)
    "text".to_string()
}
