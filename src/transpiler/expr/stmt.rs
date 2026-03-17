//! Statement component reconstruction
//!
//! Handles reconstruction of various statement components like JOINs, subqueries,
//! CASE expressions, NULL tests, etc.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    JoinExpr, SubLink, NullTest, CaseExpr, CoalesceExpr, ResTarget, 
    RangeVar, RangeSubselect
};
use crate::transpiler::TranspileContext;

use super::reconstruct_node;

/// Reconstruct a JOIN expression
pub(crate) fn reconstruct_join_expr(join_expr: &JoinExpr, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    // Check if this is a NATURAL JOIN
    let is_natural = join_expr.is_natural;

    // Left side
    if let Some(ref left) = join_expr.larg {
        let left_sql = reconstruct_node(left, ctx);
        // If left side is a join expression and this join has a USING clause,
        // we need to wrap it in parentheses to avoid "ON" clause ambiguity
        if left.node.as_ref().map(|n| matches!(n, NodeEnum::JoinExpr(_))).unwrap_or(false)
            && (!join_expr.using_clause.is_empty() || join_expr.join_using_alias.is_some()) {
            parts.push(format!("({})", left_sql));
        } else {
            parts.push(left_sql);
        }
    }

    // Determine join type
    let join_type = if is_natural {
        match join_expr.jointype() {
            pg_query::protobuf::JoinType::JoinInner => "natural join",
            pg_query::protobuf::JoinType::JoinLeft => "natural left join",
            pg_query::protobuf::JoinType::JoinRight => "natural left join",
            pg_query::protobuf::JoinType::JoinFull => "natural left join",
            _ => "natural join",
        }
    } else {
        match join_expr.jointype() {
            pg_query::protobuf::JoinType::JoinInner => "join",
            pg_query::protobuf::JoinType::JoinLeft => "left join",
            pg_query::protobuf::JoinType::JoinRight => "left join",
            pg_query::protobuf::JoinType::JoinFull => "left join",
            _ => "join",
        }
    };
    parts.push(join_type.to_string());

    // Right side
    if let Some(ref right) = join_expr.rarg {
        parts.push(reconstruct_node(right, ctx));
    }

    // ON clause (not used with NATURAL or USING)
    if let Some(ref qual) = join_expr.quals {
        let qual_sql = reconstruct_node(qual, ctx);
        if !qual_sql.is_empty() {
            parts.push("on".to_string());
            parts.push(qual_sql);
        }
    }

    // USING clause
    if !join_expr.using_clause.is_empty() {
        parts.push("using".to_string());
        let cols: Vec<String> = join_expr
            .using_clause
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::String(s) = inner {
                        return Some(s.sval.to_lowercase());
                    }
                }
                None
            })
            .collect();
        parts.push(format!("({})", cols.join(", ")));
    }

    let join_sql = parts.join(" ");

    // Handle JOIN aliases by wrapping in a subquery
    // SQLite doesn't support "JOIN ... USING (...) AS alias" syntax directly
    let using_alias_name = join_expr.join_using_alias.as_ref()
        .map(|a| a.aliasname.to_lowercase());
    let join_alias_name = join_expr.alias.as_ref()
        .map(|a| a.aliasname.to_lowercase());
    
    if let Some(alias_name) = using_alias_name.or(join_alias_name) {
        // For SQLite compatibility, wrap the JOIN in a subquery with the alias
        format!("(select * from {}) as {}", join_sql, alias_name)
    } else {
        join_sql
    }
}

/// Reconstruct a SubLink (subquery)
pub(crate) fn reconstruct_sub_link(sub_link: &SubLink, ctx: &mut TranspileContext) -> String {
    ctx.enter_subquery();
    let subquery = sub_link
        .subselect
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    ctx.exit_subquery();

    match sub_link.sub_link_type() {
        pg_query::protobuf::SubLinkType::ExistsSublink => format!("exists ({})", subquery),
        pg_query::protobuf::SubLinkType::AnySublink => {
            let test_expr = sub_link
                .testexpr
                .as_ref()
                .map(|n| reconstruct_node(n, ctx))
                .unwrap_or_default();
            format!("{} in ({})", test_expr, subquery)
        }
        pg_query::protobuf::SubLinkType::AllSublink => {
            let test_expr = sub_link
                .testexpr
                .as_ref()
                .map(|n| reconstruct_node(n, ctx))
                .unwrap_or_default();
            format!("{} in ({})", test_expr, subquery)
        }
        _ => format!("({})", subquery),
    }
}

/// Reconstruct a NullTest (IS NULL / IS NOT NULL)
pub(crate) fn reconstruct_null_test(null_test: &NullTest, ctx: &mut TranspileContext) -> String {
    let arg = null_test
        .arg
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();

    match null_test.nulltesttype() {
        pg_query::protobuf::NullTestType::IsNull => format!("{} is null", arg),
        pg_query::protobuf::NullTestType::IsNotNull => format!("{} is not null", arg),
        _ => arg,
    }
}

/// Reconstruct a Case expression
pub(crate) fn reconstruct_case_expr(case_expr: &CaseExpr, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();
    parts.push("case".to_string());

    // CASE expression (if present) - this is the simple CASE form: CASE expr WHEN ...
    if let Some(ref arg) = case_expr.arg {
        parts.push(reconstruct_node(arg, ctx));
    }

    // WHEN clauses
    for when in &case_expr.args {
        if let Some(ref inner) = when.node {
            if let NodeEnum::CaseWhen(case_when) = inner {
                let when_expr = case_when.expr.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();
                let when_result = case_when.result.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();

                parts.push(format!("when {} then {}", when_expr, when_result));
            }
        }
    }

    // ELSE clause
    if let Some(ref default_result) = case_expr.defresult {
        let default_sql = reconstruct_node(default_result, ctx);
        parts.push(format!("else {}", default_sql));
    }

    parts.push("end".to_string());
    parts.join(" ")
}

/// Reconstruct a CoalesceExpr node
pub(crate) fn reconstruct_coalesce_expr(coalesce_expr: &CoalesceExpr, ctx: &mut TranspileContext) -> String {
    let args: Vec<String> = coalesce_expr
        .args
        .iter()
        .map(|n| reconstruct_node(n, ctx))
        .collect();

    format!("coalesce({})", args.join(", "))
}

/// Reconstruct a ResTarget node (SELECT column or alias)
pub(crate) fn reconstruct_res_target(target: &ResTarget, ctx: &mut TranspileContext) -> String {
    let name = &target.name;
    if let Some(ref val) = target.val {
        let val_sql = reconstruct_node(val, ctx);
        if name.is_empty() {
            val_sql
        } else {
            format!("{} as \"{}\"", val_sql, name)
        }
    } else if !name.is_empty() {
        format!("\"{}\"", name)
    } else {
        String::new()
    }
}

/// Reconstruct a RangeVar node (table reference)
pub(crate) fn reconstruct_range_var(range_var: &RangeVar, ctx: &mut TranspileContext) -> String {
    let table_name = range_var.relname.to_lowercase();
    ctx.referenced_tables.push(table_name.clone());
    let schema_name = range_var.schemaname.to_lowercase();
    let alias = range_var.alias.as_ref().map(|a| a.aliasname.to_lowercase());

    // Check for column renaming (e.g., J1_TBL t1 (a, b, c))
    // Note: SQLite doesn't support this syntax, so we emit a warning
    let has_col_renames = range_var.alias.as_ref()
        .map(|a| !a.colnames.is_empty())
        .unwrap_or(false);
    
    if has_col_renames {
        ctx.add_error("Column renaming in table aliases (e.g., 'table t (col1, col2)') is not supported in SQLite".to_string());
    }

    let full_table = if schema_name.is_empty() || schema_name == "public" || schema_name == "pg_catalog" {
        table_name.clone()
    } else {
        format!("{}.{}", schema_name, table_name)
    };

    if let Some(a) = alias {
        if a != table_name && a != format!("{}.{}", schema_name, table_name) {
            format!("{} as {}", full_table, a)
        } else {
            full_table
        }
    } else {
        full_table
    }
}

/// Reconstruct a RangeSubselect node (subquery in FROM clause)
/// 
/// For LATERAL subqueries:
/// - If the subquery is a simple function call (like jsonb_each, generate_series),
///   we strip LATERAL and process normally (SQLite supports these as table-valued functions)
/// - If it's a complex correlated subquery, we report an error
pub(crate) fn reconstruct_range_subselect(range_subselect: &RangeSubselect, ctx: &mut TranspileContext) -> String {
    // Check if this is a simple function call that can work without LATERAL
    let is_simple_function_call = if range_subselect.lateral {
        is_lateral_function_call(range_subselect)
    } else {
        false
    };

    if range_subselect.lateral && !is_simple_function_call {
        ctx.add_error("LATERAL subqueries are not yet supported in SQLite; try a window function or CTE.".to_string());
    }

    let alias_name = range_subselect
        .alias
        .as_ref()
        .map(|a| a.aliasname.to_lowercase());

    let col_aliases = if let Some(ref alias) = range_subselect.alias {
        let cols: Vec<String> = alias.colnames.iter().filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::String(ref s) = inner {
                    return Some(s.sval.to_lowercase());
                }
            }
            None
        }).collect();
        if !cols.is_empty() {
            Some(cols)
        } else {
            None
        }
    } else {
        None
    };

    ctx.enter_subquery();
    
    if let Some(cols) = col_aliases {
        ctx.set_values_column_aliases(cols);
    }

    let subquery = range_subselect
        .subquery
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();

    ctx.exit_subquery();
    ctx.clear_values_column_aliases();

    if let Some(a) = alias_name {
        format!("({}) as {}", subquery, a)
    } else {
        format!("({})", subquery)
    }
}

/// Check if a LATERAL subquery is a simple function call that can work without LATERAL
/// 
/// SQLite supports table-valued functions like json_each(), json_tree(), generate_series()
/// For these, the LATERAL keyword can be safely stripped.
fn is_lateral_function_call(range_subselect: &RangeSubselect) -> bool {
    use pg_query::protobuf::node::Node as NodeEnum;
    
    // Check if the subquery is a simple SELECT with a function in FROM clause
    if let Some(ref subquery_node) = range_subselect.subquery {
        if let Some(ref inner) = subquery_node.node {
            if let NodeEnum::SelectStmt(select_stmt) = inner {
                // Check if this is a simple "SELECT * FROM func()" pattern
                // with no WHERE, GROUP BY, etc.
                if select_stmt.where_clause.is_some() 
                    || !select_stmt.group_clause.is_empty()
                    || select_stmt.having_clause.is_some()
                    || !select_stmt.sort_clause.is_empty() {
                    return false;
                }
                
                // Check if FROM clause has a single function call
                if select_stmt.from_clause.len() == 1 {
                    if let Some(ref from_node) = select_stmt.from_clause[0].node {
                        // Check for RangeFunction (table-valued function)
                        if let NodeEnum::RangeFunction(range_func) = from_node {
                            // Check if it's a supported table-valued function
                            // RangeFunction has a `functions` field which is a list of function calls
                            if !range_func.functions.is_empty() {
                                // The first function in the list is the main one
                                if let Some(func_node) = range_func.functions.first() {
                                    if let Some(ref func_inner) = func_node.node {
                                        // The function is wrapped in a List
                                        if let NodeEnum::List(list) = func_inner {
                                            if let Some(first_item) = list.items.first() {
                                                if let Some(ref item_inner) = first_item.node {
                                                    if let NodeEnum::FuncCall(func_call) = item_inner {
                                                        if let Some(ref func_name_node) = func_call.funcname.first() {
                                                            if let Some(ref name_inner) = func_name_node.node {
                                                                if let NodeEnum::String(s) = name_inner {
                                                                    let func_name = s.sval.to_lowercase();
                                                                    // List of functions that work as table-valued in SQLite
                                                                    return matches!(
                                                                        func_name.as_str(),
                                                                        "json_each" | "json_tree" | 
                                                                        "jsonb_each" | "jsonb_tree" |
                                                                        "generate_series" |
                                                                        "json_array_elements" | "jsonb_array_elements" |
                                                                        "json_object_keys" | "jsonb_object_keys"
                                                                    );
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}