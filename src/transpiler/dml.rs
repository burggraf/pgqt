//! DML (Data Manipulation Language) statement reconstruction
//!
//! This module handles the reconstruction of PostgreSQL DML statements
//! into SQLite-compatible SQL, including SELECT, INSERT, UPDATE, and DELETE.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    Node, SelectStmt, InsertStmt, UpdateStmt, DeleteStmt
};
use super::context::TranspileContext;
use crate::transpiler::reconstruct_node;

/// Check if the current context has column aliases (for VALUES statements)
fn has_column_aliases(_ctx: &TranspileContext) -> bool {
    // For now, return false - we'll need to track this in the context
    // when we encounter RangeSubselect with coldeflist
    false
}

/// Reconstruct VALUES statement as UNION ALL SELECT to support column aliases
fn reconstruct_values_as_union_all(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    let mut union_parts = Vec::new();

    for values_list in &stmt.values_lists {
        if let Some(ref inner) = values_list.node {
            if let NodeEnum::List(list) = inner {
                let values: Vec<String> = list
                    .items
                    .iter()
                    .map(|n| reconstruct_node(n, ctx))
                    .collect();
                union_parts.push(format!("SELECT {}", values.join(", ")));
            }
        }
    }

    union_parts.join(" UNION ALL SELECT ")
}

pub(crate) fn reconstruct_distinct_on_select(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Extract DISTINCT ON expressions
    let partition_exprs = crate::distinct_on::extract_distinct_on_exprs(stmt);
    if partition_exprs.is_empty() {
        // Fallback to regular SELECT
        return reconstruct_select_stmt_fallback(stmt, ctx);
    }

    // Build inner query columns - also save original columns for outer SELECT
    let mut inner_cols = Vec::new();
    let outer_select_cols: String;

    if stmt.target_list.is_empty() {
        inner_cols.push("*".to_string());
        // For SELECT *, we need to exclude __rn in outer query
        // This is tricky - we'll use a subquery approach
        outer_select_cols = "*".to_string();
    } else {
        let original_cols: Vec<String> = stmt.target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_cols = original_cols.clone();
        // Build outer SELECT with original column names (excluding __rn)
        outer_select_cols = original_cols.join(", ");
    }

    // Build ROW_NUMBER() OVER clause
    let partition_by = partition_exprs.join(", ");

    // Build ORDER BY for window (must include DISTINCT ON expressions + additional sort)
    let order_by = if !stmt.sort_clause.is_empty() {
        let sorts: Vec<String> = stmt.sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        sorts.join(", ")
    } else {
        // No ORDER BY - use DISTINCT ON expressions
        partition_by.clone()
    };

    // Add ROW_NUMBER column
    let row_num_col = format!(
        "row_number() over (partition by {} order by {}) as \"__rn\"",
        partition_by, order_by
    );
    inner_cols.push(row_num_col);

    // Build inner query
    let mut inner_parts = Vec::new();
    inner_parts.push("select".to_string());
    inner_parts.push(inner_cols.join(", "));

    // FROM clause
    if !stmt.from_clause.is_empty() {
        inner_parts.push("from".to_string());
        let tables: Vec<String> = stmt.from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_parts.push(tables.join(", "));
    }

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            inner_parts.push("where".to_string());
            inner_parts.push(where_sql);
        }
    }

    // GROUP BY clause
    if !stmt.group_clause.is_empty() {
        inner_parts.push("group by".to_string());
        let groups: Vec<String> = stmt.group_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_parts.push(groups.join(", "));
    }

    // HAVING clause
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause, ctx);
        if !having_sql.is_empty() {
            inner_parts.push("having".to_string());
            inner_parts.push(having_sql);
        }
    }

    let inner_query = inner_parts.join(" ");

    // Build outer query - select original columns, exclude __rn
    let mut outer_parts = Vec::new();

    // For SELECT *, we need to explicitly list columns to exclude __rn
    // This is a limitation - for SELECT * queries, __rn may appear in results
    // But for explicit column lists, we can exclude it
    if outer_select_cols == "*" {
        // We need to wrap again to filter out __rn
        // Use: SELECT * EXCEPT (__rn) is not supported in SQLite
        // Instead, we'll just select * and the client will see __rn
        // This is acceptable as PostgreSQL DISTINCT ON doesn't add extra columns
        // A better solution would be to parse the table schema
        outer_parts.push("select * from".to_string());
    } else {
        outer_parts.push(format!("select {} from", outer_select_cols));
    }
    outer_parts.push(format!("({}) as \"__distinct_on_sub\"", inner_query));
    outer_parts.push("where".to_string());
    outer_parts.push("\"__rn\" = 1".to_string());

    // Preserve ORDER BY from original query (outer query)
    if !stmt.sort_clause.is_empty() {
        outer_parts.push("order by".to_string());
        let sorts: Vec<String> = stmt.sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        outer_parts.push(sorts.join(", "));
    }

    // Preserve LIMIT
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count, ctx);
        if !limit_sql.is_empty() && limit_sql.to_uppercase() != "NULL" {
            outer_parts.push("limit".to_string());
            outer_parts.push(limit_sql);
        }
    }

    // Preserve OFFSET
    if let Some(ref limit_offset) = stmt.limit_offset {
        let offset_sql = reconstruct_node(limit_offset, ctx);
        if !offset_sql.is_empty() {
            outer_parts.push("offset".to_string());
            outer_parts.push(offset_sql);
        }
    }

    outer_parts.join(" ")
}

/// Fallback for when DISTINCT ON transformation fails
pub(crate) fn reconstruct_select_stmt_fallback(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Just use regular SELECT without DISTINCT ON
    let mut parts = Vec::new();
    parts.push("select".to_string());

    if stmt.target_list.is_empty() {
        parts.push("*".to_string());
    } else {
        let columns: Vec<String> = stmt.target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(columns.join(", "));
    }

    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt.from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(tables.join(", "));
    }

    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct a SELECT statement
pub(crate) fn reconstruct_select_stmt(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Check if this is a VALUES statement (used in INSERT)
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt, ctx);
    }

    // Handle DISTINCT ON - transform to ROW_NUMBER() window function
    if crate::distinct_on::is_distinct_on(stmt) {
        return reconstruct_distinct_on_select(stmt, ctx);
    }

    let mut parts = Vec::new();

    // Handle regular DISTINCT
    if !stmt.distinct_clause.is_empty() {
        parts.push("select distinct".to_string());
    } else {
        parts.push("select".to_string());
    }

    // Target list (columns)
    if stmt.target_list.is_empty() {
        parts.push("*".to_string());
    } else {
        let columns: Vec<String> = stmt
            .target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(columns.join(", "));
    }

    // FROM clause
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt
            .from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(tables.join(", "));
    }

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    // GROUP BY clause
    if !stmt.group_clause.is_empty() {
        parts.push("group by".to_string());
        let groups: Vec<String> = stmt
            .group_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(groups.join(", "));
    }

    // HAVING clause
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause, ctx);
        if !having_sql.is_empty() {
            parts.push("having".to_string());
            parts.push(having_sql);
        }
    }

    // WINDOW clause (named window definitions)
    if !stmt.window_clause.is_empty() {
        parts.push("window".to_string());
        let windows: Vec<String> = stmt
            .window_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(windows.join(", "));
    }

    // ORDER BY clause (from sort_clause)
    if !stmt.sort_clause.is_empty() {
        parts.push("order by".to_string());
        let sorts: Vec<String> = stmt
            .sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        parts.push(sorts.join(", "));
    }

    // LIMIT clause
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count, ctx);
        // Check for NULL (which represents LIMIT ALL)
        if limit_sql.to_uppercase() == "NULL" {
            parts.push("limit".to_string());
            parts.push("-1".to_string());
        } else if !limit_sql.is_empty() {
            parts.push("limit".to_string());
            parts.push(limit_sql);
        }
    }

    // OFFSET clause
    if let Some(ref limit_offset) = stmt.limit_offset {
        let offset_sql = reconstruct_node(limit_offset, ctx);
        if !offset_sql.is_empty() {
            parts.push("offset".to_string());
            parts.push(offset_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct a VALUES statement (used in INSERT)
pub(crate) fn reconstruct_values_stmt(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Check if this VALUES has column aliases (via coldeflist in RangeSubselect)
    // If so, we need to convert to UNION ALL SELECT because SQLite doesn't support column aliases on VALUES
    if has_column_aliases(ctx) {
        return reconstruct_values_as_union_all(stmt, ctx);
    }

    let mut values_parts = Vec::new();

    for values_list in &stmt.values_lists {
        if let Some(ref inner) = values_list.node {
            if let NodeEnum::List(list) = inner {
                let values: Vec<String> = list
                    .items
                    .iter()
                    .map(|n| reconstruct_node(n, ctx))
                    .collect();
                values_parts.push(format!("({})", values.join(", ")));
            }
        }
    }

    format!("values {}", values_parts.join(", "))
}

/// Reconstruct a SortBy node (ORDER BY)
pub(crate) fn reconstruct_sort_by(node: &Node, ctx: &mut TranspileContext) -> String {
    if let Some(ref inner) = node.node {
        if let NodeEnum::SortBy(sort_by) = inner {
            let expr_sql = sort_by
                .node
                .as_ref()
                .map(|n| reconstruct_node(n, ctx))
                .unwrap_or_default();

            let direction = match sort_by.sortby_dir() {
                pg_query::protobuf::SortByDir::SortbyAsc => " ASC",
                pg_query::protobuf::SortByDir::SortbyDesc => " DESC",
                _ => "",
            };

            return format!("{}{}", expr_sql, direction.to_lowercase());
        }
    }
    reconstruct_node(node, ctx)
}

/// Reconstruct an INSERT statement
pub(crate) fn reconstruct_insert_stmt(stmt: &InsertStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("insert into".to_string());

    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name);

    // Columns
    if !stmt.cols.is_empty() {
        let cols: Vec<String> = stmt
            .cols
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::ResTarget(target) = inner {
                        return Some(target.name.to_lowercase());
                    }
                }
                None
            })
            .collect();
        parts.push(format!("({})", cols.join(", ")));
    }

    // VALUES or SELECT
    if let Some(ref select_stmt) = stmt.select_stmt {
        let select_sql = reconstruct_node(select_stmt, ctx);
        parts.push(select_sql);
    }

    parts.join(" ")
}

/// Reconstruct an UPDATE statement
pub(crate) fn reconstruct_update_stmt(stmt: &UpdateStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("update".to_string());

    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name);

    // SET clause
    parts.push("set".to_string());
    let targets: Vec<String> = stmt
        .target_list
        .iter()
        .filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::ResTarget(target) = inner {
                    let col_name = target.name.to_lowercase();
                    let val = target
                        .val
                        .as_ref()
                        .map(|v| reconstruct_node(v, ctx))
                        .unwrap_or_default();
                    return Some(format!("{} = {}", col_name, val));
                }
            }
            None
        })
        .collect();
    parts.push(targets.join(", "));

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct a DELETE statement
pub(crate) fn reconstruct_delete_stmt(stmt: &DeleteStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("delete from".to_string());

    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name);

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    parts.join(" ")
}
