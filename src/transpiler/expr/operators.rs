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
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_left(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_left(&lexpr_sql, &rexpr_sql)
            } else {
                // Default to bitwise shift for integers
                format!("{} << {}", lexpr_sql, rexpr_sql)
            }
        },
        ">>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            
            if geo::is_geo_operation(&lexpr_lower, &rexpr_lower) {
                geo::geo_right(&lexpr_sql, &rexpr_sql)
            } else if ranges::is_range_operation(&lexpr_sql, &rexpr_sql) {
                ranges::range_right(&lexpr_sql, &rexpr_sql)
            } else {
                // Default to bitwise shift for integers
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
        // JSONB key removal
        "-" => {
            let rexpr_trimmed = rexpr_sql.trim();
            if rexpr_trimmed.starts_with("'[") || rexpr_trimmed.starts_with("[") {
                let array_str = rexpr_trimmed.trim_matches(|c| c == '\'');
                if let Ok(keys) = serde_json::from_str::<Vec<String>>(array_str) {
                    let paths: Vec<String> = keys.iter().map(|k| format!("'$.{}'", k)).collect();
                    format!("json_remove({}, {})", lexpr_sql, paths.join(", "))
                } else {
                    format!("json_remove({}, '$.' || {})", lexpr_sql, rexpr_sql)
                }
            } else {
                format!("json_remove({}, '$.' || {})", lexpr_sql, rexpr_sql)
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
        _ => format!("{} {} {}", lexpr_sql, op_name, rexpr_sql),
    }
}