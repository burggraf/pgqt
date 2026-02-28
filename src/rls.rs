//! Row-Level Security (RLS) via AST injection
//!
//! This module provides PostgreSQL-compatible RLS by injecting WHERE clauses
//! into queries during transpilation.

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::catalog::{RlsPolicy, get_applicable_policies, is_rls_enabled, is_rls_forced};

/// RLS context for policy evaluation
#[derive(Debug, Clone)]
pub struct RlsContext {
    pub current_user: String,
    pub user_roles: Vec<String>,
    pub bypass_rls: bool, // Superuser or table owner can bypass RLS
}

impl RlsContext {
    pub fn new(current_user: String) -> Self {
        Self {
            current_user,
            user_roles: vec!["PUBLIC".to_string()],
            bypass_rls: false,
        }
    }

    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.user_roles = roles;
        self.user_roles.push("PUBLIC".to_string());
        self
    }

    pub fn with_bypass(mut self, bypass: bool) -> Self {
        self.bypass_rls = bypass;
        self
    }
}

/// Check if the current user can bypass RLS for a table
pub fn can_bypass_rls(
    conn: &Connection,
    table_name: &str,
    ctx: &RlsContext,
) -> Result<bool> {
    // Superusers can bypass RLS
    if ctx.bypass_rls {
        return Ok(true);
    }

    // Check if user is the table owner
    let owner_result: Result<i64, _> = conn.query_row(
        "SELECT relowner FROM __pg_relation_meta__ WHERE relname = ?1",
        [table_name],
        |row| row.get(0),
    );

    if let Ok(owner_oid) = owner_result {
        let user_oid: i64 = conn.query_row(
            "SELECT oid FROM __pg_authid__ WHERE rolname = ?1",
            [&ctx.current_user],
            |row| row.get(0),
        )?;

        if owner_oid == user_oid {
            // Check if RLS is forced - if so, even owner can't bypass
            let forced = is_rls_forced(conn, table_name)?;
            return Ok(!forced);
        }
    }

    Ok(false)
}

/// Build a combined RLS expression for a set of policies
/// 
/// PostgreSQL semantics:
/// - Multiple PERMISSIVE policies are combined with OR
/// - RESTRICTIVE policies are combined with AND
/// - The final expression is: (permissive_expr) AND (restrictive_expr)
/// 
/// For USING expressions (SELECT, UPDATE, DELETE on existing rows)
/// For WITH CHECK expressions (INSERT, UPDATE on new rows)
pub fn build_rls_expression(
    policies: &[RlsPolicy],
    for_using: bool, // true = USING, false = WITH CHECK
) -> Option<String> {
    let mut permissive_clauses: Vec<String> = Vec::new();
    let mut restrictive_clauses: Vec<String> = Vec::new();

    for policy in policies {
        let expr = if for_using {
            policy.using_expr.clone()
        } else {
            policy.with_check_expr.clone().or_else(|| policy.using_expr.clone())
        };

        if let Some(expr_str) = expr {
            if policy.permissive {
                permissive_clauses.push(expr_str);
            } else {
                restrictive_clauses.push(expr_str);
            }
        }
    }

    let permissive_part = if permissive_clauses.is_empty() {
        None
    } else if permissive_clauses.len() == 1 {
        Some(permissive_clauses[0].clone())
    } else {
        // Multiple PERMISSIVE policies are ORed together
        Some(format!("({})", permissive_clauses.join(") OR (")))
    };

    let restrictive_part = if restrictive_clauses.is_empty() {
        None
    } else {
        // RESTRICTIVE policies are ANDed together
        Some(format!("({})", restrictive_clauses.join(") AND (")))
    };

    match (permissive_part, restrictive_part) {
        (None, None) => None,
        (Some(p), None) => Some(p),
        (None, Some(r)) => Some(r),
        (Some(p), Some(r)) => Some(format!("({}) AND ({})", p, r)),
    }
}

/// Get RLS WHERE clause for a table (for use by transpiler)
/// 
/// Returns the combined RLS expression that should be added to the WHERE clause,
/// or None if no RLS applies.
pub fn get_rls_where_clause(
    conn: &Connection,
    table_name: &str,
    ctx: &RlsContext,
    command: &str, // SELECT, INSERT, UPDATE, DELETE
) -> Result<Option<String>> {
    // Check if RLS is enabled
    if !is_rls_enabled(conn, table_name)? {
        return Ok(None);
    }

    // Check if user can bypass RLS
    if can_bypass_rls(conn, table_name, ctx)? {
        return Ok(None);
    }

    // Get applicable policies
    let policies = get_applicable_policies(conn, table_name, command, &ctx.user_roles)?;

    if policies.is_empty() {
        // No applicable policies - return FALSE to deny all access
        return Ok(Some("FALSE".to_string()));
    }

    // Build and return the RLS expression
    let using_expr = build_rls_expression(&policies, true);
    Ok(using_expr)
}

/// Apply RLS to a SQL query by injecting WHERE clauses
/// 
/// This is a simplified implementation that appends RLS predicates to the SQL.
/// In a full implementation, we would properly modify the AST.
pub fn apply_rls_to_sql(
    conn: &Connection,
    sql: &str,
    ctx: &RlsContext,
    operation_type: crate::transpiler::OperationType,
    table_name: &str,
) -> Result<String> {
    // Check if RLS is enabled on this table
    if !is_rls_enabled(conn, table_name)? {
        return Ok(sql.to_string());
    }

    // Check if user can bypass RLS
    if can_bypass_rls(conn, table_name, ctx)? {
        return Ok(sql.to_string());
    }

    // Get applicable policies
    let command = match operation_type {
        crate::transpiler::OperationType::SELECT => "SELECT",
        crate::transpiler::OperationType::INSERT => "INSERT",
        crate::transpiler::OperationType::UPDATE => "UPDATE",
        crate::transpiler::OperationType::DELETE => "DELETE",
        _ => return Ok(sql.to_string()),
    };

    let policies = get_applicable_policies(conn, table_name, command, &ctx.user_roles)?;

    if policies.is_empty() {
        // No applicable policies - deny all access (PostgreSQL default behavior)
        return Ok(format!("{} WHERE FALSE -- RLS: no applicable policies", sql));
    }

    // Build RLS expressions
    let using_expr = build_rls_expression(&policies, true);

    if let Some(expr) = using_expr {
        // Append RLS predicate to the query
        // For a proper implementation, we'd need to parse and modify the AST
        Ok(format!("{} WHERE ({})", sql, expr))
    } else {
        Ok(sql.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{init_catalog, enable_rls, disable_rls, store_rls_policy};
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_catalog(&conn).unwrap();
        conn
    }

    #[test]
    fn test_rls_context() {
        let ctx = RlsContext::new("test_user".to_string())
            .with_roles(vec!["role1".to_string(), "role2".to_string()]);
        
        assert_eq!(ctx.current_user, "test_user");
        assert!(ctx.user_roles.contains(&"PUBLIC".to_string()));
        assert!(ctx.user_roles.contains(&"role1".to_string()));
    }

    #[test]
    fn test_build_rls_expression() {
        // Single permissive policy
        let policies = vec![RlsPolicy {
            name: "p1".to_string(),
            table_name: "t".to_string(),
            command: "SELECT".to_string(),
            permissive: true,
            roles: vec![],
            using_expr: Some("user_id = current_user".to_string()),
            with_check_expr: None,
            enabled: true,
        }];

        let expr = build_rls_expression(&policies, true);
        assert_eq!(expr, Some("user_id = current_user".to_string()));

        // Multiple permissive policies (ORed together)
        let policies = vec![
            RlsPolicy {
                name: "p1".to_string(),
                table_name: "t".to_string(),
                command: "SELECT".to_string(),
                permissive: true,
                roles: vec![],
                using_expr: Some("user_id = 1".to_string()),
                with_check_expr: None,
                enabled: true,
            },
            RlsPolicy {
                name: "p2".to_string(),
                table_name: "t".to_string(),
                command: "SELECT".to_string(),
                permissive: true,
                roles: vec![],
                using_expr: Some("role = 'admin'".to_string()),
                with_check_expr: None,
                enabled: true,
            },
        ];

        let expr = build_rls_expression(&policies, true);
        assert!(expr.unwrap().contains("OR"));

        // Permissive + Restrictive (ANDed)
        let policies = vec![
            RlsPolicy {
                name: "p1".to_string(),
                table_name: "t".to_string(),
                command: "SELECT".to_string(),
                permissive: true,
                roles: vec![],
                using_expr: Some("user_id = 1".to_string()),
                with_check_expr: None,
                enabled: true,
            },
            RlsPolicy {
                name: "r1".to_string(),
                table_name: "t".to_string(),
                command: "SELECT".to_string(),
                permissive: false, // RESTRICTIVE
                roles: vec![],
                using_expr: Some("tenant_id = 1".to_string()),
                with_check_expr: None,
                enabled: true,
            },
        ];

        let expr = build_rls_expression(&policies, true);
        let expr_str = expr.unwrap();
        assert!(expr_str.contains("AND"));
        assert!(expr_str.contains("user_id = 1"));
        assert!(expr_str.contains("tenant_id = 1"));
    }

    #[test]
    fn test_enable_disable_rls() {
        let conn = setup_test_db();

        // Initially not enabled
        assert!(!is_rls_enabled(&conn, "test_table").unwrap());

        // Enable RLS
        enable_rls(&conn, "test_table", false).unwrap();
        assert!(is_rls_enabled(&conn, "test_table").unwrap());
        assert!(!is_rls_forced(&conn, "test_table").unwrap());

        // Enable with force
        enable_rls(&conn, "test_table", true).unwrap();
        assert!(is_rls_enabled(&conn, "test_table").unwrap());
        assert!(is_rls_forced(&conn, "test_table").unwrap());

        // Disable RLS
        disable_rls(&conn, "test_table").unwrap();
        assert!(!is_rls_enabled(&conn, "test_table").unwrap());
    }

    #[test]
    fn test_store_and_get_policies() {
        let conn = setup_test_db();

        let policy = RlsPolicy {
            name: "user_policy".to_string(),
            table_name: "documents".to_string(),
            command: "SELECT".to_string(),
            permissive: true,
            roles: vec!["user".to_string()],
            using_expr: Some("owner = current_user".to_string()),
            with_check_expr: None,
            enabled: true,
        };

        store_rls_policy(&conn, &policy).unwrap();

        let policies = crate::catalog::get_table_policies(&conn, "documents").unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "user_policy");

        // Test applicable policies
        let ctx = RlsContext::new("alice".to_string())
            .with_roles(vec!["user".to_string()]);
        
        let applicable = get_applicable_policies(&conn, "documents", "SELECT", &ctx.user_roles).unwrap();
        assert_eq!(applicable.len(), 1);

        // Test with non-matching role
        let ctx2 = RlsContext::new("bob".to_string())
            .with_roles(vec!["admin".to_string()]);
        
        let applicable = get_applicable_policies(&conn, "documents", "SELECT", &ctx2.user_roles).unwrap();
        assert_eq!(applicable.len(), 0);
    }

    #[test]
    fn test_drop_policy() {
        let conn = setup_test_db();

        let policy = RlsPolicy {
            name: "test_policy".to_string(),
            table_name: "test_table".to_string(),
            command: "ALL".to_string(),
            permissive: true,
            roles: vec![],
            using_expr: Some("TRUE".to_string()),
            with_check_expr: None,
            enabled: true,
        };

        store_rls_policy(&conn, &policy).unwrap();
        assert_eq!(crate::catalog::get_table_policies(&conn, "test_table").unwrap().len(), 1);

        crate::catalog::drop_rls_policy(&conn, "test_policy", "test_table").unwrap();
        assert_eq!(crate::catalog::get_table_policies(&conn, "test_table").unwrap().len(), 0);
    }

    #[test]
    fn test_get_rls_where_clause() {
        let conn = setup_test_db();

        // Create a table with RLS enabled
        enable_rls(&conn, "docs", false).unwrap();

        // Create a policy
        let policy = RlsPolicy {
            name: "owner_policy".to_string(),
            table_name: "docs".to_string(),
            command: "SELECT".to_string(),
            permissive: true,
            roles: vec![],
            using_expr: Some("owner = 'alice'".to_string()),
            with_check_expr: None,
            enabled: true,
        };
        store_rls_policy(&conn, &policy).unwrap();

        // Test with a user who should get the RLS clause
        let ctx = RlsContext::new("alice".to_string());
        let clause = get_rls_where_clause(&conn, "docs", &ctx, "SELECT").unwrap();
        assert_eq!(clause, Some("owner = 'alice'".to_string()));

        // Test with RLS disabled
        disable_rls(&conn, "docs").unwrap();
        let clause = get_rls_where_clause(&conn, "docs", &ctx, "SELECT").unwrap();
        assert_eq!(clause, None);
    }

    #[test]
    fn test_get_rls_where_clause_no_policies() {
        let conn = setup_test_db();

        // Enable RLS but don't create any policies
        enable_rls(&conn, "docs", false).unwrap();

        let ctx = RlsContext::new("alice".to_string());
        let clause = get_rls_where_clause(&conn, "docs", &ctx, "SELECT").unwrap();
        
        // No policies = deny all access (FALSE)
        assert_eq!(clause, Some("FALSE".to_string()));
    }
}