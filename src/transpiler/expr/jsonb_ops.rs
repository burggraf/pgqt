//! JSONB operator detection and handling
//!
//! This module provides functions to detect JSONB operations and transpile
//! PostgreSQL JSONB operators to SQLite equivalents.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::Node;

/// Check if an AST node represents a JSON/JSONB value
///
/// This detects:
/// - TypeCast nodes to json/jsonb: '...'::jsonb
/// - String literals containing JSON objects or arrays: '{"a":1}', '[1,2,3]'
/// - Expressions with json/jsonb in their name (columns, functions)
pub(crate) fn is_node_jsonb(node: &Node) -> bool {
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::TypeCast(type_cast) => {
                // Check if casting to json/jsonb
                if let Some(ref type_name) = type_cast.type_name {
                    for name_node in &type_name.names {
                        if let Some(ref name_inner) = name_node.node {
                            if let NodeEnum::String(s) = name_inner {
                                let type_str = s.sval.to_lowercase();
                                if type_str == "json" || type_str == "jsonb" {
                                    return true;
                                }
                            }
                        }
                    }
                }
                false
            }
            NodeEnum::AConst(const_node) => {
                // Check for string literals containing JSON
                if let Some(ref val) = const_node.val {
                    if let pg_query::protobuf::a_const::Val::Sval(sval) = val {
                        let s = sval.sval.trim();
                        // Check for JSON array: [1,2,3] or [{"a":1}]
                        // But NOT simple string arrays like ["admin"] which are PostgreSQL text arrays
                        if s.starts_with("[") && s.ends_with("]") {
                            // Only detect as JSON if it contains objects, colons, or numbers
                            // Simple string arrays should be treated as PostgreSQL arrays
                            return s.contains("{") || s.contains(":") || 
                                   (s.chars().any(|c| c.is_ascii_digit()) && s.contains(','));
                        }
                        // Check for JSON object: {"key":"value"} (has colon)
                        // PostgreSQL arrays are {"a","b"} (no colon, quoted elements)
                        if s.starts_with("{") && s.ends_with("}") {
                            // If it contains ":" it's likely JSON, not a PostgreSQL array
                            return s.contains(':');
                        }
                    }
                }
                false
            }
            NodeEnum::FuncCall(func_call) => {
                // Check for json/jsonb functions
                if let Some(ref funcname) = func_call.funcname.first() {
                    if let Some(ref name_node) = funcname.node {
                        if let NodeEnum::String(s) = name_node {
                            let name = s.sval.to_lowercase();
                            return name.starts_with("json_") || name.starts_with("jsonb_");
                        }
                    }
                }
                false
            }
            _ => false,
        }
    } else {
        false
    }
}

/// Check if an SQL expression represents a JSON/JSONB value
///
/// This detects:
/// - String literals containing JSON objects or arrays: '{"a":1}', '[1,2,3]'
/// - Cast expressions to json/jsonb: '...'::jsonb, cast(... as jsonb)
/// - Expressions with json/jsonb in their name (columns, functions)
#[allow(dead_code)]
pub(crate) fn is_jsonb_expression(expr: &str) -> bool {
    let lower = expr.to_lowercase();
    let trimmed = expr.trim();

    // Check for cast to json/jsonb
    if lower.contains("::json") || lower.contains("::jsonb") {
        return true;
    }

    // Check for cast() syntax
    if lower.contains("cast(") && lower.contains("as json") {
        return true;
    }

    // Check for string literal containing JSON object or array
    if (trimmed.starts_with("'{") && trimmed.contains("}"))
        || (trimmed.starts_with("'[") && trimmed.contains("]"))
    {
        return true;
    }

    // Check for json/jsonb functions or column names
    if lower.contains("jsonb") {
        return true;
    }

    // Check for json_ functions that return JSON
    if lower.starts_with("json_") || lower.starts_with("jsonb_") {
        return true;
    }

    false
}

/// Check if this is a JSONB contains operation (@>)
///
/// Both operands should be JSON/JSONB expressions
#[allow(dead_code)]
pub(crate) fn is_jsonb_contains_operation(left: &str, right: &str) -> bool {
    is_jsonb_expression(left) && is_jsonb_expression(right)
}

/// Generate SQLite SQL for JSONB contains operator (@>)
/// Uses the jsonb_contains function registered in src/jsonb.rs
pub(crate) fn jsonb_contains(left: &str, right: &str) -> String {
    format!("jsonb_contains({}, {})", left, right)
}

/// Generate SQLite SQL for JSONB contained by operator (<@)
/// Uses the jsonb_contained function registered in src/jsonb.rs
pub(crate) fn jsonb_contained(left: &str, right: &str) -> String {
    format!("jsonb_contained({}, {})", left, right)
}

/// Generate SQLite SQL for JSONB key exists operator (?)
/// Uses the jsonb_exists function registered in src/jsonb.rs
pub(crate) fn jsonb_key_exists(json: &str, key: &str) -> String {
    format!("jsonb_exists({}, {})", json, key)
}

/// Generate SQLite SQL for JSONB any key exists operator (?|)
/// Uses the jsonb_exists_any function registered in src/jsonb.rs
pub(crate) fn jsonb_exists_any(json: &str, keys: &str) -> String {
    format!("jsonb_exists_any({}, {})", json, keys)
}

/// Generate SQLite SQL for JSONB all keys exist operator (?&)
/// Uses the jsonb_exists_all function registered in src/jsonb.rs
pub(crate) fn jsonb_exists_all(json: &str, keys: &str) -> String {
    format!("jsonb_exists_all({}, {})", json, keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_jsonb_expression_with_cast() {
        assert!(is_jsonb_expression("'{\"a\":1}'::jsonb"));
        assert!(is_jsonb_expression("'{\"a\":1}'::json"));
        assert!(is_jsonb_expression("data::jsonb"));
    }

    #[test]
    fn test_is_jsonb_expression_with_string_literal() {
        assert!(is_jsonb_expression("'{\"a\":1}'"));
        assert!(is_jsonb_expression("'[1,2,3]'"));
    }

    #[test]
    fn test_is_jsonb_expression_with_function() {
        assert!(is_jsonb_expression("jsonb_build_object('a', 1)"));
        assert!(is_jsonb_expression("json_build_array(1, 2, 3)"));
    }

    #[test]
    fn test_is_jsonb_expression_with_column() {
        // Column names alone can't be detected as JSONB without context
        // assert!(is_jsonb_expression("props")); // This would need metadata to detect
        assert!(is_jsonb_expression("data::jsonb"));
    }

    #[test]
    fn test_is_jsonb_expression_negative() {
        assert!(!is_jsonb_expression("'hello'"));
        assert!(!is_jsonb_expression("123"));
        assert!(!is_jsonb_expression("column_name"));
    }

    #[test]
    fn test_jsonb_contains_sql() {
        let result = jsonb_contains("props", "'{\"a\":1}'");
        assert_eq!(result, "jsonb_contains(props, '{\"a\":1}')");
    }
}
