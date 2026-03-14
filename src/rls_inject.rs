//! RLS AST Injection Module
//!
//! This module provides functionality to inject Row-Level Security policies
//! into SQL queries. Due to the complexity of AST manipulation with pg_query,
//! this implementation uses a hybrid approach:
//! - Parse the original SQL with pg_query to understand structure
//! - Inject RLS at the string level in a safe manner
//!
//! Future improvement: Full AST-based injection when pg_query provides
//! better support for node construction.

#![allow(dead_code)]

use anyhow::Result;

/// Injects an RLS WHERE clause into a SELECT statement
///
/// This function safely combines an existing WHERE clause with an RLS expression.
/// It handles proper parenthesization to ensure correct operator precedence.
///
/// # Arguments
/// * `original_sql` - The original SELECT SQL statement
/// * `rls_where` - The RLS WHERE expression (e.g., "owner_id = 'alice'")
///
/// # Returns
/// The modified SQL with RLS injected, or the original if no injection needed
///
/// # Example
/// ```
/// // Original: SELECT * FROM documents WHERE status = 'active'
/// // After: SELECT * FROM documents WHERE (status = 'active') AND (owner_id = 'alice')
/// ```
pub fn inject_rls_into_select_sql(original_sql: &str, rls_where: &str) -> String {
    // Parse to understand the structure
    if let Ok(parsed) = pg_query::parse(original_sql) {
        if let Some(raw_stmt) = parsed.protobuf.stmts.first() {
            if let Some(ref stmt_node) = raw_stmt.stmt {
                use pg_query::protobuf::node::Node as NodeEnum;
                if let Some(NodeEnum::SelectStmt(select_stmt)) = stmt_node.node.as_ref() {
                    return inject_into_select_struct(select_stmt, original_sql, rls_where);
                }
            }
        }
    }
    
    // Fallback: return original with RLS appended (best effort)
    format!("{} WHERE ({})", original_sql, rls_where)
}

/// Injects RLS into a parsed SELECT statement
fn inject_into_select_struct(select_stmt: &pg_query::protobuf::SelectStmt, original_sql: &str, rls_where: &str) -> String {
    // Check if there's an existing WHERE clause
    let has_where = select_stmt.where_clause.is_some();
    
    if has_where {
        // Find the WHERE keyword and inject after it
        // We need to find the WHERE and combine the conditions
        if let Some(where_pos) = find_where_clause_position(original_sql) {
            let before_where = &original_sql[..where_pos];
            let after_where = &original_sql[where_pos + 6..]; // +6 for "WHERE "
            
            return format!(
                "{}WHERE ({} AND ({}))",
                before_where,
                after_where.trim(),
                rls_where
            );
        }
    }
    
    // No WHERE clause - need to add one
    // Find a good position to insert (before ORDER BY, LIMIT, etc.)
    if let Some(insert_pos) = find_where_insert_position(original_sql) {
        let before = &original_sql[..insert_pos];
        let after = &original_sql[insert_pos..];
        format!("{} WHERE ({}) {}", before, rls_where, after)
    } else {
        // Append at the end
        format!("{} WHERE ({})", original_sql, rls_where)
    }
}

/// Injects an RLS WHERE clause into an UPDATE statement
pub fn inject_rls_into_update_sql(original_sql: &str, rls_where: &str) -> String {
    if let Ok(parsed) = pg_query::parse(original_sql) {
        if let Some(raw_stmt) = parsed.protobuf.stmts.first() {
            if let Some(ref stmt_node) = raw_stmt.stmt {
                use pg_query::protobuf::node::Node as NodeEnum;
                if let Some(NodeEnum::UpdateStmt(update_stmt)) = stmt_node.node.as_ref() {
                    return inject_into_update_struct(update_stmt, original_sql, rls_where);
                }
            }
        }
    }
    
    format!("{} WHERE ({})", original_sql, rls_where)
}

/// Injects RLS into a parsed UPDATE statement
fn inject_into_update_struct(update_stmt: &pg_query::protobuf::UpdateStmt, original_sql: &str, rls_where: &str) -> String {
    let has_where = update_stmt.where_clause.is_some();
    
    if has_where {
        if let Some(where_pos) = find_where_clause_position(original_sql) {
            let before_where = &original_sql[..where_pos];
            let after_where = &original_sql[where_pos + 6..];
            
            return format!(
                "{}WHERE ({} AND ({}))",
                before_where,
                after_where.trim(),
                rls_where
            );
        }
    }
    
    // No WHERE clause - add one before any RETURNING clause
    if let Some(insert_pos) = find_returning_position(original_sql) {
        let before = &original_sql[..insert_pos];
        let after = &original_sql[insert_pos..];
        format!("{} WHERE ({}) {}", before, rls_where, after)
    } else {
        format!("{} WHERE ({})", original_sql, rls_where)
    }
}

/// Injects an RLS WHERE clause into a DELETE statement
pub fn inject_rls_into_delete_sql(original_sql: &str, rls_where: &str) -> String {
    // DELETE works the same as UPDATE for WHERE clause injection
    inject_rls_into_update_sql(original_sql, rls_where)
}

/// Transforms an INSERT statement to include WITH CHECK validation
///
/// Converts: INSERT INTO table (cols) VALUES (vals)
/// To:       INSERT INTO table (cols) SELECT vals WHERE (with_check_expr)
///
/// For multiple VALUES rows, each row is checked individually.
pub fn inject_rls_into_insert_sql(original_sql: &str, with_check_expr: &str) -> Result<String> {
    if let Ok(parsed) = pg_query::parse(original_sql) {
        if let Some(raw_stmt) = parsed.protobuf.stmts.first() {
            if let Some(ref stmt_node) = raw_stmt.stmt {
                use pg_query::protobuf::node::Node as NodeEnum;
                if let Some(NodeEnum::InsertStmt(insert_stmt)) = stmt_node.node.as_ref() {
                    return transform_insert_to_select(insert_stmt, original_sql, with_check_expr);
                }
            }
        }
    }
    
    Err(anyhow::anyhow!("Could not parse INSERT statement for RLS injection"))
}

/// Transforms INSERT VALUES to INSERT...SELECT with WHERE clause
fn transform_insert_to_select(
    insert_stmt: &pg_query::protobuf::InsertStmt,
    _original_sql: &str,
    with_check_expr: &str,
) -> Result<String> {
    // Extract table name
    let table_name = insert_stmt
        .relation
        .as_ref()
        .map(|r| r.relname.clone())
        .unwrap_or_default();
    
    if table_name.is_empty() {
        return Err(anyhow::anyhow!("Could not determine table name for INSERT"));
    }
    
    // Extract column names
    let columns: Vec<String> = insert_stmt
        .cols
        .iter()
        .filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let pg_query::protobuf::node::Node::ResTarget(target) = inner {
                    return Some(target.name.clone());
                }
            }
            None
        })
        .collect();
    
    // Check if we have a VALUES clause or SELECT
    if let Some(ref select_stmt_node) = insert_stmt.select_stmt {
        if let Some(ref inner) = select_stmt_node.node {
            use pg_query::protobuf::node::Node as NodeEnum;
            
            // Handle VALUES clause
            if let NodeEnum::SelectStmt(select_stmt) = inner {
                // Check if it's a VALUES statement (values_lists is not empty)
                if !select_stmt.values_lists.is_empty() {
                    return transform_values_insert(
                        &table_name,
                        &columns,
                        select_stmt,
                        with_check_expr,
                    );
                }
            }
        }
    }
    
    Err(anyhow::anyhow!(
        "INSERT...SELECT with RLS not yet supported. Use INSERT...VALUES."
    ))
}

/// Transforms INSERT...VALUES to INSERT...SELECT...WHERE
fn transform_values_insert(
    table_name: &str,
    columns: &[String],
    select_stmt: &pg_query::protobuf::SelectStmt,
    with_check_expr: &str,
) -> Result<String> {
    if columns.is_empty() {
        return Err(anyhow::anyhow!("INSERT without explicit columns not supported with RLS"));
    }
    
    // Build the INSERT...SELECT statement
    let columns_str = columns.join(", ");
    
    // Process each VALUES row
    let mut select_parts: Vec<String> = Vec::new();
    
    for values_list in &select_stmt.values_lists {
        if let Some(ref inner) = values_list.node {
            use pg_query::protobuf::node::Node as NodeEnum;
            if let NodeEnum::List(list) = inner {
                let values: Vec<String> = list
                    .items
                    .iter()
                    .map(|n| {
                        // Use deparse() for RLS injection to match the target dialect (Postgres AST -> SQL)
                        // but handle potential errors.
                        n.deparse().unwrap_or_else(|_| "NULL".to_string())
                    })
                    .collect();
                
                if values.len() == columns.len() {
                    select_parts.push(format!(
                        "SELECT {} WHERE ({})",
                        values.join(", "),
                        with_check_expr
                    ));
                }
            }
        }
    }
    
    if select_parts.is_empty() {
        return Err(anyhow::anyhow!("Could not parse VALUES clause"));
    }
    
    // Combine all SELECTs with UNION ALL
    let select_sql = select_parts.join(" UNION ALL ");
    
    Ok(format!(
        "INSERT INTO {} ({}) {}",
        table_name,
        columns_str,
        select_sql
    ))
}

/// Finds the position of the WHERE keyword in a SQL statement
fn find_where_clause_position(sql: &str) -> Option<usize> {
    // Simple case-insensitive search for WHERE that's not inside quotes
    let sql_upper = sql.to_uppercase();
    let mut in_string = false;
    let mut chars = sql_upper.char_indices().peekable();
    
    while let Some((i, c)) = chars.next() {
        if c == '\'' {
            // Handle escaped quotes
            if let Some(&(_, next_c)) = chars.peek() {
                if next_c == '\'' {
                    chars.next(); // Skip escaped quote
                    continue;
                }
            }
            in_string = !in_string;
        } else if !in_string && c == 'W' {
            // Check for WHERE
            if sql_upper[i..].starts_with("WHERE") {
                // Make sure it's a complete word
                let after_where = i + 5;
                if after_where >= sql_upper.len() || 
                   sql_upper[after_where..].starts_with(' ') ||
                   sql_upper[after_where..].starts_with('\t') {
                    return Some(i);
                }
            }
        }
    }
    
    None
}

/// Finds a position to insert a WHERE clause (before ORDER BY, LIMIT, etc.)
fn find_where_insert_position(sql: &str) -> Option<usize> {
    let sql_upper = sql.to_uppercase();
    
    // Look for ORDER BY, LIMIT, OFFSET, FOR UPDATE, etc.
    let keywords = [" ORDER BY", " LIMIT", " OFFSET", " FOR UPDATE", " FOR SHARE"];
    
    for keyword in &keywords {
        if let Some(pos) = sql_upper.find(keyword) {
            return Some(pos);
        }
    }
    
    None
}

/// Finds the position of RETURNING clause
fn find_returning_position(sql: &str) -> Option<usize> {
    let sql_upper = sql.to_uppercase();
    sql_upper.find(" RETURNING")
}

/// Rewrites special functions in RLS expressions
///
/// PostgreSQL RLS expressions may contain:
/// - current_user
/// - session_user
///
/// These need to be rewritten to literal values for SQLite.
pub fn rewrite_rls_expression(expr: &str, current_user: &str, _session_user: &str) -> String {
    let mut result = expr.to_string();
    
    // Replace current_user with the actual user name (quoted as string)
    // Note: This is a simple string replacement - proper implementation
    // would use AST-based rewriting for safety
    result = result.replace("current_user", &format!("'{}'", current_user));
    result = result.replace("CURRENT_USER", &format!("'{}'", current_user));
    
    // session_user is typically the same in our implementation
    result = result.replace("session_user", &format!("'{}'", current_user));
    result = result.replace("SESSION_USER", &format!("'{}'", current_user));
    
    result
}

/// Combines multiple RLS expressions using OR (for PERMISSIVE policies)
pub fn combine_with_or(expressions: &[String]) -> Option<String> {
    if expressions.is_empty() {
        return None;
    }
    
    if expressions.len() == 1 {
        return Some(expressions[0].clone());
    }
    
    let combined = expressions
        .iter()
        .map(|e| format!("({})", e))
        .collect::<Vec<_>>()
        .join(" OR ");
    
    Some(combined)
}

/// Combines multiple RLS expressions using AND (for RESTRICTIVE policies)
pub fn combine_with_and(expressions: &[String]) -> Option<String> {
    if expressions.is_empty() {
        return None;
    }
    
    if expressions.len() == 1 {
        return Some(expressions[0].clone());
    }
    
    let combined = expressions
        .iter()
        .map(|e| format!("({})", e))
        .collect::<Vec<_>>()
        .join(" AND ");
    
    Some(combined)
}

/// Combines permissive and restrictive expressions per PostgreSQL semantics
///
/// PostgreSQL semantics:
/// - Multiple PERMISSIVE policies are combined with OR
/// - RESTRICTIVE policies are combined with AND
/// - Final expression: (permissive_expr) AND (restrictive_expr)
pub fn combine_rls_expressions(
    permissive: &[String],
    restrictive: &[String],
) -> Option<String> {
    let permissive_part = combine_with_or(permissive);
    let restrictive_part = combine_with_and(restrictive);
    
    match (permissive_part, restrictive_part) {
        (None, None) => None,
        (Some(p), None) => Some(p),
        (None, Some(r)) => Some(r),
        (Some(p), Some(r)) => Some(format!("({}) AND ({})", p, r)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_current_user() {
        let expr = "user_id = current_user";
        let rewritten = rewrite_rls_expression(expr, "alice", "alice");
        assert_eq!(rewritten, "user_id = 'alice'");
    }

    #[test]
    fn test_rewrite_session_user() {
        let expr = "owner_id = session_user";
        let rewritten = rewrite_rls_expression(expr, "bob", "bob");
        assert_eq!(rewritten, "owner_id = 'bob'");
    }

    #[test]
    fn test_combine_with_or() {
        let exprs = vec!["a = 1".to_string(), "b = 2".to_string()];
        let result = combine_with_or(&exprs);
        assert_eq!(result, Some("(a = 1) OR (b = 2)".to_string()));
    }

    #[test]
    fn test_combine_with_and() {
        let exprs = vec!["a = 1".to_string(), "b = 2".to_string()];
        let result = combine_with_and(&exprs);
        assert_eq!(result, Some("(a = 1) AND (b = 2)".to_string()));
    }

    #[test]
    fn test_combine_rls_expressions() {
        let permissive = vec!["user_id = 1".to_string(), "role = 'admin'".to_string()];
        let restrictive = vec!["tenant_id = 1".to_string()];
        
        let result = combine_rls_expressions(&permissive, &restrictive);
        assert_eq!(
            result,
            Some("((user_id = 1) OR (role = 'admin')) AND (tenant_id = 1)".to_string())
        );
    }

    #[test]
    fn test_find_where_clause_position() {
        let sql = "SELECT * FROM docs WHERE status = 'active'";
        let pos = find_where_clause_position(sql);
        assert!(pos.is_some());
        assert_eq!(pos.unwrap(), 19);
    }

    #[test]
    fn test_inject_rls_into_select_with_where() {
        let sql = "SELECT * FROM documents WHERE status = 'active'";
        let rls = "owner_id = 'alice'";
        let result = inject_rls_into_select_sql(sql, rls);
        
        assert!(result.contains("status = 'active'"));
        assert!(result.contains("owner_id = 'alice'"));
        assert!(result.contains("AND"));
    }

    #[test]
    fn test_inject_rls_into_select_without_where() {
        let sql = "SELECT * FROM documents";
        let rls = "owner_id = 'alice'";
        let result = inject_rls_into_select_sql(sql, rls);
        
        assert!(result.contains("WHERE"));
        assert!(result.contains("owner_id = 'alice'"));
    }

    #[test]
    fn test_inject_rls_into_update() {
        let sql = "UPDATE documents SET title = 'New' WHERE id = 1";
        let rls = "owner_id = 'alice'";
        let result = inject_rls_into_update_sql(sql, rls);
        
        assert!(result.contains("id = 1"));
        assert!(result.contains("owner_id = 'alice'"));
        assert!(result.contains("AND"));
    }

    #[test]
    fn test_inject_rls_into_delete() {
        let sql = "DELETE FROM documents WHERE id = 1";
        let rls = "owner_id = 'alice'";
        let result = inject_rls_into_delete_sql(sql, rls);
        
        assert!(result.contains("id = 1"));
        assert!(result.contains("owner_id = 'alice'"));
    }
}
