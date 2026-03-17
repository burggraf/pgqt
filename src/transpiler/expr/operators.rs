//! Operator reconstruction
//!
//! Handles PostgreSQL-specific operators and their SQLite equivalents.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::AExpr;
use crate::transpiler::TranspileContext;

use super::{reconstruct_node, is_array_expr, is_json_array_string};
use super::arrays;
use super::ranges;
use super::geo;

/// Check if a SQL expression looks like an integer type or integer literal
fn is_integer_expression(expr: &str) -> bool {
    let lower = expr.to_lowercase();
    // Check for integer type casts
    if lower.contains("::int") || lower.contains("::integer") ||
       lower.contains("::smallint") || lower.contains("::bigint") ||
       lower.contains("::int2") || lower.contains("::int4") || lower.contains("::int8") {
        return true;
    }
    // Check for cast() function with integer types
    if lower.contains("cast(") && 
       (lower.contains("as int") || lower.contains("as integer") ||
        lower.contains("as smallint") || lower.contains("as bigint")) {
        return true;
    }
    // Check if it's a simple integer literal
    if expr.trim().parse::<i64>().is_ok() {
        return true;
    }
    false
}

/// Check if a SQL expression is a datetime function or value
fn is_datetime_expression(expr: &str) -> bool {
    let lower = expr.to_lowercase();
    lower.contains("now()") || 
    lower.contains("datetime") ||
    lower.contains("current_timestamp") ||
    lower.contains("current_date") ||
    lower.contains("current_time")
}

/// Check if a SQL expression is an interval value
fn is_interval_expression(expr: &str) -> bool {
    let lower = expr.to_lowercase();
    // Check for cast to text that contains interval-like values
    if lower.contains("cast(") && lower.contains("as text") {
        // Extract the inner value
        if let Some(start) = lower.find('(') {
            if let Some(end) = lower.find("as text") {
                let inner = &lower[start+1..end].trim();
                // Check for interval patterns like '1 day', '2 hours', etc.
                return inner.contains("day") || 
                       inner.contains("hour") || 
                       inner.contains("minute") ||
                       inner.contains("second") ||
                       inner.contains("month") ||
                       inner.contains("year") ||
                       inner.contains("week");
            }
        }
    }
    false
}

/// Extract interval value from a cast expression
fn extract_interval_value(expr: &str) -> Option<String> {
    let lower = expr.to_lowercase();
    if lower.contains("cast(") && lower.contains("as text") {
        if let Some(start) = expr.find('(') {
            if let Some(end) = expr.find("as text") {
                let inner = expr[start+1..end].trim();
                // Remove quotes if present
                if inner.starts_with('\'') && inner.contains('\'') {
                    if let Some(quote_end) = inner[1..].find('\'') {
                        return Some(inner[1..=quote_end].to_string());
                    }
                }
            }
        }
    }
    None
}

/// Reconstruct an AExpr node (operators)
pub(crate) fn reconstruct_a_expr(a_expr: &AExpr, ctx: &mut TranspileContext) -> String {
    // Check if operands are array expressions before reconstructing
    let lexpr_is_array = a_expr.lexpr.as_ref().is_some_and(|n| is_array_expr(n) || is_json_array_string(n));
    let rexpr_is_array = a_expr.rexpr.as_ref().is_some_and(|n| is_array_expr(n) || is_json_array_string(n));
    
    let lexpr_sql = a_expr
        .lexpr
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    let rexpr_sql = a_expr
        .rexpr
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    let op_name = a_expr
        .name
        .first()
        .and_then(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::String(s) = inner {
                    return Some(s.sval.clone());
                }
            }
            None
        })
        .unwrap_or_default();

    // Handle IN expressions
    match a_expr.kind() {
        pg_query::protobuf::AExprKind::AexprIn => {
            return format!("{} in ({})", lexpr_sql, rexpr_sql);
        }
        pg_query::protobuf::AExprKind::AexprOpAny => {
            return format!("{} = any ({})", lexpr_sql, rexpr_sql);
        }
        pg_query::protobuf::AExprKind::AexprOpAll => {
            return format!("{} = all ({})", lexpr_sql, rexpr_sql);
        }
        pg_query::protobuf::AExprKind::AexprBetween => {
            // PostgreSQL allows BETWEEN x, y syntax (with comma)
            // SQLite requires BETWEEN x AND y
            // The rexpr_sql contains both bounds, we need to replace comma with AND
            let bounds = rexpr_sql.replace(", ", " AND ").replace(",", " AND ");
            return format!("{} BETWEEN {}", lexpr_sql, bounds);
        }
        pg_query::protobuf::AExprKind::AexprNotBetween => {
            let bounds = rexpr_sql.replace(", ", " AND ").replace(",", " AND ");
            return format!("{} NOT BETWEEN {}", lexpr_sql, bounds);
        }
        pg_query::protobuf::AExprKind::AexprBetweenSym => {
            // Symmetric BETWEEN - treat as regular BETWEEN for now
            let bounds = rexpr_sql.replace(", ", " AND ").replace(",", " AND ");
            return format!("{} BETWEEN {}", lexpr_sql, bounds);
        }
        pg_query::protobuf::AExprKind::AexprNotBetweenSym => {
            let bounds = rexpr_sql.replace(", ", " AND ").replace(",", " AND ");
            return format!("{} NOT BETWEEN {}", lexpr_sql, bounds);
        }
        _ => {}
    }

    // Handle PostgreSQL-specific operators
    match op_name.as_str() {
        "~~" | "~~*" => format!("{} like {}", lexpr_sql, rexpr_sql),
        "!~~" | "!~~*" => format!("{} not like {}", lexpr_sql, rexpr_sql),
        "~" => format!("regexp({}, {})", rexpr_sql, lexpr_sql),
        "~*" => format!("regexpi({}, {})", rexpr_sql, lexpr_sql),
        "!~" => format!("NOT regexp({}, {})", rexpr_sql, lexpr_sql),
        "!~*" => format!("NOT regexpi({}, {})", rexpr_sql, lexpr_sql),
        "@@" => format!("fts_match({}, {})", lexpr_sql, rexpr_sql),
        "@>@" => format!("fts_contains({}, {})", lexpr_sql, rexpr_sql),
        "<@@" => format!("fts_contained({}, {})", lexpr_sql, rexpr_sql),
        // Array, Range, and Geo operators (PostgreSQL compatibility)
        "&&" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_overlaps(&lexpr_sql, &rexpr_sql)
            } else if arrays::is_array_operation(lexpr_is_array, rexpr_is_array, &lexpr_sql, &rexpr_sql) {
                arrays::array_overlap(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_overlaps(&lexpr_sql, &rexpr_sql)
            } else {
                arrays::array_overlap(&lexpr_sql, &rexpr_sql)
            }
        }
        "@>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_contains(&lexpr_sql, &rexpr_sql)
            } else if arrays::is_array_operation(lexpr_is_array, rexpr_is_array, &lexpr_sql, &rexpr_sql) {
                arrays::array_contains(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_contains(&lexpr_sql, &rexpr_sql)
            } else {
                arrays::array_contains(&lexpr_sql, &rexpr_sql)
            }
        }
        "<@" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_contained(&lexpr_sql, &rexpr_sql)
            } else if arrays::is_array_operation(lexpr_is_array, rexpr_is_array, &lexpr_sql, &rexpr_sql) {
                arrays::array_contained(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_contained(&lexpr_sql, &rexpr_sql)
            } else {
                arrays::array_contained(&lexpr_sql, &rexpr_sql)
            }
        }
        "<<" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            // Check if this is an integer bitwise shift operation
            if is_integer_expression(&lexpr_lower) || is_integer_expression(&rexpr_lower) {
                format!("{} << {}", lexpr_sql, rexpr_sql)
            } else if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_left(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_left(&lexpr_sql, &rexpr_sql)
            } else {
                // Default to bitwise shift
                format!("{} << {}", lexpr_sql, rexpr_sql)
            }
        },
        ">>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            // Check if this is an integer bitwise shift operation
            if is_integer_expression(&lexpr_lower) || is_integer_expression(&rexpr_lower) {
                format!("{} >> {}", lexpr_sql, rexpr_sql)
            } else if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_right(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_right(&lexpr_sql, &rexpr_sql)
            } else {
                // Default to bitwise shift
                format!("{} >> {}", lexpr_sql, rexpr_sql)
            }
        },
        "<<|" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_below(&lexpr_sql, &rexpr_sql)
            } else {
                format!("{} <<| {}", lexpr_sql, rexpr_sql)
            }
        },
        "|>>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_above(&lexpr_sql, &rexpr_sql)
            } else {
                format!("{} |>> {}", lexpr_sql, rexpr_sql)
            }
        },
        "-|-" => ranges::range_adjacent(&lexpr_sql, &rexpr_sql),
        "&<" => ranges::range_no_extend_right(&lexpr_sql, &rexpr_sql),
        "&>" => ranges::range_no_extend_left(&lexpr_sql, &rexpr_sql),
        // JSONB operators (PostgreSQL compatibility)
        "?" => format!("json_type({}, '$.' || {}) IS NOT NULL", lexpr_sql, rexpr_sql),
        "?|" => format!("EXISTS (SELECT 1 FROM json_each({}) WHERE json_type({}, '$.' || value) IS NOT NULL)", rexpr_sql, lexpr_sql),
        "?&" => format!("NOT EXISTS (SELECT 1 FROM json_each({}) WHERE json_type({}, '$.' || value) IS NULL)", rexpr_sql, lexpr_sql),
        // || operator is overloaded in PostgreSQL
        "||" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            let lexpr_trimmed = lexpr_sql.trim();
            let rexpr_trimmed = rexpr_sql.trim();

            // Check for tsvector context
            if lexpr_lower.contains("to_tsvector") || rexpr_lower.contains("to_tsvector") ||
               lexpr_lower.contains("tsvector") || rexpr_lower.contains("tsvector") {
                format!("tsvector_concat({}, {})", lexpr_sql, rexpr_sql)
            }
            // Check for JSON context
            else if lexpr_trimmed.starts_with("'{") || rexpr_trimmed.starts_with("'{") ||
                    lexpr_trimmed.starts_with("'[") || rexpr_trimmed.starts_with("'[") ||
                    lexpr_lower.contains("json") || rexpr_lower.contains("json") ||
                    lexpr_lower.contains("props") || rexpr_lower.contains("props") {
                format!("json_patch({}, {})", lexpr_sql, rexpr_sql)
            }
            // Default to SQLite's string concatenation
            else {
                format!("{} || {}", lexpr_sql, rexpr_sql)
            }
        }
        // JSONB key removal (only if not datetime - interval)
        "-" => {
            // Handle unary minus (negative numbers/expression)
            if lexpr_sql.is_empty() {
                return format!("-{}", rexpr_sql);
            }
            
            // First check if this is datetime - interval
            if is_datetime_expression(&lexpr_sql) && is_interval_expression(&rexpr_sql) {
                // Transform: datetime - interval -> datetime(datetime, '-interval_value')
                if let Some(interval_val) = extract_interval_value(&rexpr_sql) {
                    // If lexpr is already a datetime() call, use it directly
                    if lexpr_sql.starts_with("datetime(") {
                        // Extract the inner part
                        if let Some(inner_start) = lexpr_sql.find('(') {
                            if let Some(inner_end) = lexpr_sql.rfind(')') {
                                let inner = &lexpr_sql[inner_start+1..inner_end];
                                return format!("datetime({}, '-{}')", inner, interval_val);
                            }
                        }
                    }
                    return format!("datetime({}, '-{}')", lexpr_sql, interval_val);
                }
            }
            
            // Handle array element removal: array - element
            // Only treat as array removal if left operand is actually an array
            let rexpr_trimmed = rexpr_sql.trim();
            if rexpr_trimmed.starts_with("'[") || rexpr_trimmed.starts_with("[") {
                let array_str = rexpr_trimmed.trim_matches(|c| c == '\'');
                if let Ok(keys) = serde_json::from_str::<Vec<String>>(array_str) {
                    let paths: Vec<String> = keys.iter().map(|k| format!("'$.{}'", k)).collect();
                    format!("json_remove({}, {})", lexpr_sql, paths.join(", "))
                } else {
                    format!("json_remove({}, '$.' || {})", lexpr_sql, rexpr_sql)
                }
            } else if lexpr_is_array || lexpr_sql.trim().starts_with("'[") {
                // Only treat as array removal if left operand is actually an array
                format!("json_remove({}, '$.' || {})", lexpr_sql, rexpr_sql)
            } else {
                // Regular numeric subtraction
                format!("{} - {}", lexpr_sql, rexpr_sql)
            }
        }
        // Vector distance operators (pgvector compatibility) and geometric distance
        "<->" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_distance(&lexpr_sql, &rexpr_sql)
            } else {
                format!("vector_l2_distance({}, {})", lexpr_sql, rexpr_sql)
            }
        },
        "<=>" => format!("vector_cosine_distance({}, {})", lexpr_sql, rexpr_sql),
        "<#>" => format!("vector_inner_product({}, {})", lexpr_sql, rexpr_sql),
        "<+>" => format!("vector_l1_distance({}, {})", lexpr_sql, rexpr_sql),
        // Datetime + Interval operations
        "+" => {
            if is_datetime_expression(&lexpr_sql) && is_interval_expression(&rexpr_sql) {
                // Transform: datetime + interval -> datetime(datetime, '+interval_value')
                if let Some(interval_val) = extract_interval_value(&rexpr_sql) {
                    // If lexpr is already a datetime() call, use it directly
                    if lexpr_sql.starts_with("datetime(") {
                        // Extract the inner part
                        if let Some(inner_start) = lexpr_sql.find('(') {
                            if let Some(inner_end) = lexpr_sql.rfind(')') {
                                let inner = &lexpr_sql[inner_start+1..inner_end];
                                return format!("datetime({}, '+{}')", inner, interval_val);
                            }
                        }
                    }
                    format!("datetime({}, '+{}')", lexpr_sql, interval_val)
                } else {
                    format!("{} + {}", lexpr_sql, rexpr_sql)
                }
            } else {
                format!("{} + {}", lexpr_sql, rexpr_sql)
            }
        }
        "^" => format!("power({}, {})", lexpr_sql, rexpr_sql),
        _ => format!("{} {} {}", lexpr_sql, op_name, rexpr_sql),
    }
}