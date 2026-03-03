//! RLS query augmentation logic
//!
//! This module contains the main functions for augmenting SQL statements
//! with RLS predicates for secure query execution.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    Node, SelectStmt, InsertStmt, UpdateStmt, DeleteStmt
};
use rusqlite::Connection;
use crate::rls::RlsContext;
use crate::catalog::{is_rls_enabled, get_applicable_policies};
use super::super::context::{TranspileContext, TranspileResult, OperationType};
use crate::transpiler::reconstruct_node;
use crate::transpiler::dml::reconstruct_values_stmt;
use crate::transpiler::dml::reconstruct_sort_by;
use crate::transpiler::transpile_with_metadata;
use crate::transpiler::reconstruct_sql_with_metadata;
use crate::rls::{get_rls_where_clause, can_bypass_rls, build_rls_expression};
use super::utils::{
    extract_table_name_from_select, extract_table_name_from_insert,
    extract_table_name_from_update, extract_table_name_from_delete
};
use super::policy::{reconstruct_create_policy_stmt, reconstruct_drop_policy_stmt};

/// Main entry point for transpiling SQL with RLS augmentation
pub fn transpile_with_rls(
    sql: &str,
    conn: &Connection,
    rls_ctx: &RlsContext,
) -> Result<String, String> {
    let upper_sql = sql.trim().to_uppercase();

    // Handle CREATE POLICY directly
    if upper_sql.starts_with("CREATE POLICY") {
        let sqlite_sql = reconstruct_create_policy_stmt(sql);
        return Ok(sqlite_sql);
    }

    // Handle DROP POLICY directly
    if upper_sql.starts_with("DROP POLICY") {
        let sqlite_sql = reconstruct_drop_policy_stmt(sql);
        return Ok(sqlite_sql);
    }

    let mut ctx = TranspileContext::new();

    match pg_query::parse(sql) {
        Ok(result) => {
            if let Some(raw_stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = raw_stmt.stmt {
                    if let Some(ref inner) = stmt_node.node {
                        match inner {
                            NodeEnum::SelectStmt(select_stmt) => {
                                let table_name = extract_table_name_from_select(select_stmt);
                                let sql = reconstruct_select_stmt_with_rls(select_stmt, &mut ctx, conn, rls_ctx, &table_name);
                                return Ok(sql);
                            }
                            NodeEnum::InsertStmt(insert_stmt) => {
                                let table_name = extract_table_name_from_insert(insert_stmt);
                                let sql = reconstruct_insert_stmt_with_rls(insert_stmt, &mut ctx, conn, rls_ctx, &table_name);
                                return Ok(sql);
                            }
                            NodeEnum::UpdateStmt(update_stmt) => {
                                let table_name = extract_table_name_from_update(update_stmt);
                                let sql = reconstruct_update_stmt_with_rls(update_stmt, &mut ctx, conn, rls_ctx, &table_name);
                                return Ok(sql);
                            }
                            NodeEnum::DeleteStmt(delete_stmt) => {
                                let table_name = extract_table_name_from_delete(delete_stmt);
                                let sql = reconstruct_delete_stmt_with_rls(delete_stmt, &mut ctx, conn, rls_ctx, &table_name);
                                return Ok(sql);
                            }
                            _ => {}
                        }
                    }
                }
            }
            // Fallback to regular transpilation
            let result = transpile_with_metadata(sql);
            Ok(result.sql)
        }
        Err(e) => Err(format!("Parse error: {}", e)),
    }
}

/// Reconstruct SELECT statement with RLS predicate injection
pub(crate) fn reconstruct_select_stmt_with_rls(
    stmt: &SelectStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    
    let rls_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match get_rls_where_clause(conn, table_name, rls_ctx, "SELECT") {
            Ok(pred) => pred,
            Err(_) => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt, ctx);
    }

    
    if !stmt.distinct_clause.is_empty() {
        let has_expressions = stmt.distinct_clause.iter().any(|n| {
            if let Some(ref inner) = n.node {
                matches!(inner, NodeEnum::ColumnRef(_) | NodeEnum::ResTarget(_))
            } else {
                false
            }
        });

        if has_expressions {
            parts.push("select distinct".to_string());
        } else {
            parts.push("select distinct".to_string());
        }
    } else {
        parts.push("select".to_string());
    }

    
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

    
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt
            .from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(tables.join(", "));
    }

    
    let where_sql = if let Some(ref where_clause) = stmt.where_clause {
        let existing_where = reconstruct_node(where_clause, ctx);
        if !existing_where.is_empty() {
            if let Some(ref rls) = rls_predicate {
                format!("({}) AND ({})", existing_where, rls)
            } else {
                existing_where
            }
        } else if let Some(ref rls) = rls_predicate {
            rls.clone()
        } else {
            String::new()
        }
    } else if let Some(ref rls) = rls_predicate {
        rls.clone()
    } else {
        String::new()
    };

    if !where_sql.is_empty() {
        parts.push("where".to_string());
        parts.push(where_sql);
    }

    
    if !stmt.group_clause.is_empty() {
        parts.push("group by".to_string());
        let groups: Vec<String> = stmt
            .group_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(groups.join(", "));
    }

    
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause, ctx);
        if !having_sql.is_empty() {
            parts.push("having".to_string());
            parts.push(having_sql);
        }
    }

    
    if !stmt.sort_clause.is_empty() {
        parts.push("order by".to_string());
        let sorts: Vec<String> = stmt
            .sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        parts.push(sorts.join(", "));
    }

    
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count, ctx);
        if !limit_sql.is_empty() {
            parts.push("limit".to_string());
            parts.push(limit_sql);
        }
    }

    
    if let Some(ref limit_offset) = stmt.limit_offset {
        let offset_sql = reconstruct_node(limit_offset, ctx);
        if !offset_sql.is_empty() {
            parts.push("offset".to_string());
            parts.push(offset_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct INSERT statement with RLS WITH CHECK predicate injection
pub(crate) fn reconstruct_insert_stmt_with_rls(
    stmt: &InsertStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    
    let with_check_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match is_rls_enabled(conn, table_name) {
            Ok(true) => {
                let policies = get_applicable_policies(conn, table_name, "INSERT", &rls_ctx.user_roles).unwrap_or_default();
                if policies.iter().any(|p| p.with_check_expr.is_some()) {
                    match get_rls_where_clause(conn, table_name, rls_ctx, "INSERT") {
                        Ok(pred) => pred,
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    parts.push("insert into".to_string());

    
    let table_name_full = stmt
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
    parts.push(table_name_full);

    
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

    
    if let Some(ref select_stmt) = stmt.select_stmt {
        if let Some(ref inner) = select_stmt.node {
            match inner {
                NodeEnum::SelectStmt(sel) => {
                    if !sel.values_lists.is_empty() {
                        
                        let values_sql = reconstruct_values_stmt(sel, ctx);
                        parts.push(values_sql);
                    } else {
                        
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
                        let select_sql = reconstruct_node(select_stmt, ctx);
                        parts.push(select_sql);
                    }
                }
                _ => {
                    let select_sql = reconstruct_node(select_stmt, ctx);
                    parts.push(select_sql);
                }
            }
        } else {
            let select_sql = reconstruct_node(select_stmt, ctx);
            parts.push(select_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct UPDATE statement with RLS predicate injection
pub(crate) fn reconstruct_update_stmt_with_rls(
    stmt: &UpdateStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    
    let using_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match get_rls_where_clause(conn, table_name, rls_ctx, "UPDATE") {
            Ok(pred) => pred,
            Err(_) => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    parts.push("update".to_string());

    
    let table_name_full = stmt
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
    parts.push(table_name_full);

    
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

    
    let where_sql = if let Some(ref where_clause) = stmt.where_clause {
        let existing_where = reconstruct_node(where_clause, ctx);
        if !existing_where.is_empty() {
            if let Some(ref rls) = using_predicate {
                format!("({}) AND ({})", existing_where, rls)
            } else {
                existing_where
            }
        } else if let Some(ref rls) = using_predicate {
            rls.clone()
        } else {
            String::new()
        }
    } else if let Some(ref rls) = using_predicate {
        rls.clone()
    } else {
        String::new()
    };

    if !where_sql.is_empty() {
        parts.push("where".to_string());
        parts.push(where_sql);
    }

    
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let from_items: Vec<String> = stmt
            .from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(from_items.join(", "));
    }

    parts.join(" ")
}

/// Reconstruct DELETE statement with RLS predicate injection
pub(crate) fn reconstruct_delete_stmt_with_rls(
    stmt: &DeleteStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    
    let rls_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match get_rls_where_clause(conn, table_name, rls_ctx, "DELETE") {
            Ok(pred) => pred,
            Err(_) => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    parts.push("delete from".to_string());

    
    let table_name_full = stmt
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
    parts.push(table_name_full);

    
    let where_sql = if let Some(ref where_clause) = stmt.where_clause {
        let existing_where = reconstruct_node(where_clause, ctx);
        if !existing_where.is_empty() {
            if let Some(ref rls) = rls_predicate {
                format!("({}) AND ({})", existing_where, rls)
            } else {
                existing_where
            }
        } else if let Some(ref rls) = rls_predicate {
            rls.clone()
        } else {
            String::new()
        }
    } else if let Some(ref rls) = rls_predicate {
        rls.clone()
    } else {
        String::new()
    };

    if !where_sql.is_empty() {
        parts.push("where".to_string());
        parts.push(where_sql);
    }

    
    if !stmt.using_clause.is_empty() {
        parts.push("using".to_string());
        let using_items: Vec<String> = stmt
            .using_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(using_items.join(", "));
    }

    parts.join(" ")
}
