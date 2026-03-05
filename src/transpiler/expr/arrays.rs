//! Array expression reconstruction
//!
//! Handles PostgreSQL array expressions and operators, converting them
//! to SQLite JSON array equivalents.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{Node, ArrayExpr, AArrayExpr};
use crate::transpiler::TranspileContext;

/// Reconstruct an ArrayExpr node (ARRAY[...] syntax)
/// Converts PostgreSQL ARRAY expressions to SQLite JSON arrays
pub(crate) fn reconstruct_array_expr(array_expr: &ArrayExpr, ctx: &mut TranspileContext, reconstruct_node: impl Fn(&Node, &mut TranspileContext) -> String) -> String {
    let elements: Vec<serde_json::Value> = array_expr
        .elements
        .iter()
        .map(|n| {
            let val = reconstruct_node(n, ctx);
            json_value_from_sql(&val)
        })
        .collect();

    // Store as JSON array string
    format!("'{}'", serde_json::to_string(&elements).unwrap_or_else(|_| "[]".to_string()))
}

/// Reconstruct an AArrayExpr node (ARRAY[...] syntax in parsed SQL)
/// Converts PostgreSQL ARRAY expressions to SQLite JSON arrays
pub(crate) fn reconstruct_a_array_expr(a_array_expr: &AArrayExpr, ctx: &mut TranspileContext, reconstruct_node: impl Fn(&Node, &mut TranspileContext) -> String) -> String {
    let elements: Vec<serde_json::Value> = a_array_expr
        .elements
        .iter()
        .map(|n| {
            let val = reconstruct_node(n, ctx);
            json_value_from_sql(&val)
        })
        .collect();

    // Store as JSON array string
    format!("'{}'", serde_json::to_string(&elements).unwrap_or_else(|_| "[]".to_string()))
}

/// Convert a SQL value string to a JSON value
fn json_value_from_sql(val: &str) -> serde_json::Value {
    // If the value is already quoted (a string), use it as-is for JSON
    // Otherwise, it's a literal that needs to be included in JSON
    if val.starts_with('\'') && val.ends_with('\'') {
        // It's a string literal - extract the inner value for JSON
        let inner = &val[1..val.len()-1];
        serde_json::Value::String(inner.to_string())
    } else if val == "NULL" {
        serde_json::Value::Null
    } else if val == "1" || val == "0" {
        // Boolean values (converted to 1/0 by reconstruct_aconst)
        serde_json::Value::Bool(val == "1")
    } else if let Ok(num) = val.parse::<i64>() {
        serde_json::Value::Number(num.into())
    } else if let Ok(num) = val.parse::<f64>() {
        serde_json::Number::from_f64(num)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::String(val.to_string()))
    } else {
        serde_json::Value::String(val.to_string())
    }
}

/// Check if a node is an array expression (ArrayExpr or AArrayExpr)
pub(crate) fn is_array_expr(node: &Node) -> bool {
    if let Some(ref inner) = node.node {
        matches!(inner, NodeEnum::ArrayExpr(_) | NodeEnum::AArrayExpr(_))
    } else {
        false
    }
}

/// Check if a node is a string literal containing a JSON array
pub(crate) fn is_json_array_string(node: &Node) -> bool {
    if let Some(ref inner) = node.node {
        if let NodeEnum::AConst(const_node) = inner {
            if let Some(ref val) = const_node.val {
                if let pg_query::protobuf::a_const::Val::Sval(sval) = val {
                    let s = &sval.sval;
                    // Check if it looks like a JSON array: starts with [ and ends with ]
                    return s.trim().starts_with('[') && s.trim().ends_with(']');
                }
            }
        }
    }
    false
}

/// Check if operands indicate an array operation
pub(crate) fn is_array_operation(lexpr_is_array: bool, rexpr_is_array: bool, lexpr_sql: &str, rexpr_sql: &str) -> bool {
    let lexpr_lower = lexpr_sql.to_lowercase();
    let rexpr_lower = rexpr_sql.to_lowercase();
    
    // Priority: geo > array
    // Check if it's NOT geo first
    let is_geo = looks_like_geo(&lexpr_lower) || looks_like_geo(&rexpr_lower);
    
    if is_geo {
        return false;
    }
    
    lexpr_is_array || rexpr_is_array || lexpr_lower.contains('[') || rexpr_lower.contains('[')
}

/// Check if a SQL value looks like a geometric type
fn looks_like_geo(val: &str) -> bool {
    val.contains('<') ||
    (!val.contains('[') && val.contains('(') && val.contains(',') && val.contains(')'))
}

/// Reconstruct array overlap operator (&&)
pub(crate) fn array_overlap(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("array_overlap({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct array contains operator (@>)
pub(crate) fn array_contains(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("array_contains({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct array contained operator (<@)
pub(crate) fn array_contained(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("array_contained({}, {})", lexpr_sql, rexpr_sql)
}