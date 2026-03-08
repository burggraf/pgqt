//! Expression and node reconstruction
//!
//! This module handles the reconstruction of PostgreSQL expressions
//! and AST nodes into SQLite-compatible SQL.
//!
//! # Module Organization
//!
//! - `arrays` - Array expressions and operators
//! - `ranges` - Range types and operators
//! - `geo` - Geometric types and operators
//! - `utils` - Shared utility functions
//! - `sql_value` - SQL value functions (CURRENT_TIMESTAMP, etc.)
//! - `stmt` - Statement components (JOINs, subqueries, CASE, etc.)
//! - `operators` - PostgreSQL operators

mod arrays;
mod ranges;
mod geo;
mod utils;
mod sql_value;
mod stmt;
mod operators;

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{Node, BoolExpr, TypeCast};
use crate::transpiler::TranspileContext;
use crate::transpiler::func::reconstruct_func_call;
use crate::transpiler::dml::reconstruct_select_stmt;
use crate::transpiler::dml::reconstruct_sort_by;

// Re-export utility functions for backward compatibility
pub(crate) use utils::transform_default_expression;
pub(crate) use arrays::{is_array_expr, is_json_array_string};

/// Main entry point for node reconstruction
pub(crate) fn reconstruct_node(node: &Node, ctx: &mut TranspileContext) -> String {
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::ResTarget(ref res_target) => stmt::reconstruct_res_target(res_target, ctx),
            NodeEnum::RangeVar(ref range_var) => stmt::reconstruct_range_var(range_var, ctx),
            NodeEnum::RangeSubselect(ref range_subselect) => {
                stmt::reconstruct_range_subselect(range_subselect, ctx)
            }
            NodeEnum::AStar(_) => "*".to_string(),
            NodeEnum::ColumnRef(ref col_ref) => utils::reconstruct_column_ref(col_ref, ctx),
            NodeEnum::String(s) => s.sval.clone(),
            NodeEnum::FuncCall(ref func_call) => reconstruct_func_call(func_call, ctx),
            NodeEnum::AConst(ref aconst) => {
                let val = utils::reconstruct_aconst(aconst);
                // Check if this constant is a string and looks like a range literal
                if val.starts_with('\'') && val.ends_with('\'') {
                    let trimmed = val[1..val.len()-1].trim();
                    if ranges::looks_like_range_literal(trimmed) {
                        // Check if it's a geometric type (point, box, circle, etc.)
                        if geo::looks_like_geo_literal(trimmed) {
                            return val; // Don't canonicalize geometric types
                        }
                        // Check if it looks like a JSON array
                        let is_json_array = trimmed.starts_with('[') && 
                            (trimmed.contains('"') || trimmed.chars().any(|c| c == '[' || c == ']'));
                        if is_json_array {
                            return val; // Don't canonicalize JSON arrays
                        }
                        return format!("range_canonicalize({})", val);
                    }
                }
                val
            },
            NodeEnum::TypeCast(ref type_cast) => reconstruct_type_cast(type_cast, ctx),
            NodeEnum::AExpr(ref a_expr) => operators::reconstruct_a_expr(a_expr, ctx),
            NodeEnum::BoolExpr(ref bool_expr) => reconstruct_bool_expr(bool_expr, ctx),
            NodeEnum::JoinExpr(ref join_expr) => stmt::reconstruct_join_expr(join_expr, ctx),
            NodeEnum::SelectStmt(ref select_stmt) => reconstruct_select_stmt(select_stmt, ctx),
            NodeEnum::AIndirection(ref ind) => {
                let mut arg_sql = ind.arg.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();
                
                // If the argument is a SubLink (subquery), ensure it's parenthesized correctly
                if let Some(ref arg_node) = ind.arg {
                    if let Some(NodeEnum::SubLink(_)) = arg_node.node {
                        if !arg_sql.starts_with('(') {
                            arg_sql = format!("({})", arg_sql);
                        }
                    }
                }

                let mut json_path = String::new();
                for node in &ind.indirection {
                    if let Some(ref inner) = node.node {
                        match inner {
                            NodeEnum::AIndices(ref indices) => {
                                if let Some(ref uidx) = indices.uidx {
                                    let idx_sql = reconstruct_node(uidx, ctx);
                                    let trimmed = idx_sql.trim_matches('\'');
                                    if let Ok(idx) = trimmed.parse::<i64>() {
                                        json_path.push_str(&format!("[{}]", idx - 1));
                                    } else {
                                        json_path.push_str(&format!("[{} - 1]", trimmed));
                                    }
                                }
                            }
                            NodeEnum::String(ref s) => {
                                json_path.push_str(&format!(".{}", s.sval));
                            }
                            _ => {}
                        }
                    }
                }
                
                if json_path.is_empty() {
                    arg_sql
                } else {
                    format!("json_extract({}, '${}')", arg_sql, json_path)
                }
            }
            NodeEnum::SubLink(ref sub_link) => stmt::reconstruct_sub_link(sub_link, ctx),
            NodeEnum::NullTest(ref null_test) => stmt::reconstruct_null_test(null_test, ctx),
            NodeEnum::BooleanTest(ref boolean_test) => {
                let arg = boolean_test.arg.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();
                let test_type = match boolean_test.booltesttype() {
                    pg_query::protobuf::BoolTestType::IsTrue => "IS TRUE",
                    pg_query::protobuf::BoolTestType::IsNotTrue => "IS NOT TRUE",
                    pg_query::protobuf::BoolTestType::IsFalse => "IS FALSE",
                    pg_query::protobuf::BoolTestType::IsNotFalse => "IS NOT FALSE",
                    pg_query::protobuf::BoolTestType::IsUnknown => "IS NULL",
                    pg_query::protobuf::BoolTestType::IsNotUnknown => "IS NOT NULL",
                    _ => "",
                };
                format!("{} {}", arg, test_type)
            }
            NodeEnum::CaseExpr(ref case_expr) => stmt::reconstruct_case_expr(case_expr, ctx),
            NodeEnum::CoalesceExpr(ref coalesce_expr) => {
                stmt::reconstruct_coalesce_expr(coalesce_expr, ctx)
            }
            NodeEnum::SortBy(_) => reconstruct_sort_by(node, ctx),
            NodeEnum::List(ref list) => {
                let items: Vec<String> = list.items.iter().map(|n| reconstruct_node(n, ctx)).collect();
                items.join(", ")
            }
            NodeEnum::CaseWhen(_) => {
                // CaseWhen is handled within reconstruct_case_expr, not standalone
                "".to_string()
            }
            NodeEnum::SqlvalueFunction(ref sql_val) => {
                sql_value::reconstruct_sql_value_function(sql_val)
            }
            NodeEnum::ArrayExpr(ref array_expr) => {
                arrays::reconstruct_array_expr(array_expr, ctx, reconstruct_node)
            }
            NodeEnum::AArrayExpr(ref a_array_expr) => {
                arrays::reconstruct_a_array_expr(a_array_expr, ctx, reconstruct_node)
            }
            NodeEnum::RangeFunction(ref range_func) => {
                ranges::reconstruct_range_function(range_func, ctx, reconstruct_node)
            }
            NodeEnum::SetToDefault(_) => {
                if let Some(ref table_name) = ctx.current_table {
                    if let Some(col_aliases) = ctx.values_column_aliases.get(ctx.current_column_index) {
                        if let Some(default_expr) = ctx.get_column_default(table_name, col_aliases) {
                            let sqlite_default = transform_default_expression(&default_expr);
                            return sqlite_default;
                        }
                    }
                }
                "NULL".to_string()
            },
            NodeEnum::WindowDef(ref window_def) => {
                // Handle named window definitions (e.g., WINDOW w AS (PARTITION BY ...))
                let window_sql = crate::transpiler::window::reconstruct_window_def(window_def, ctx);
                format!("{} as ({})", window_def.name.clone(), window_sql)
            }
            _ => node.deparse().unwrap_or_else(|_| "".to_string()).to_lowercase(),
        }
    } else {
        String::new()
    }
}

/// Reconstruct a TypeCast node
pub(crate) fn reconstruct_type_cast(type_cast: &TypeCast, ctx: &mut TranspileContext) -> String {
    utils::reconstruct_type_cast(type_cast, ctx, reconstruct_node)
}

/// Reconstruct a BoolExpr node (AND, OR, NOT)
pub(crate) fn reconstruct_bool_expr(bool_expr: &BoolExpr, ctx: &mut TranspileContext) -> String {
    utils::reconstruct_bool_expr(bool_expr, ctx, reconstruct_node)
}

#[allow(dead_code)]
/// Check if a node represents LIMIT ALL
pub(crate) fn is_limit_all(node: &Node) -> bool {
    utils::is_limit_all(node)
}