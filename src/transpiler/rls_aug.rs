//! Row-Level Security (RLS) augmentation for the transpiler
//!
//! This module handles the injection of RLS policy predicates into SQL statements
//! during the transpilation phase. When RLS is enabled on a table, queries that
//! reference that table have additional `WHERE` clause conditions injected based
//! on the applicable policies and the current session role.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    Node, SelectStmt, InsertStmt, UpdateStmt, DeleteStmt, CreateRoleStmt, DropRoleStmt,
    GrantStmt, GrantRoleStmt
};
use rusqlite::Connection;
use crate::rls::RlsContext;
use crate::catalog::{is_rls_enabled, get_applicable_policies};
use super::context::{TranspileContext, TranspileResult, OperationType};
use crate::transpiler::reconstruct_node;
use crate::transpiler::dml::reconstruct_values_stmt;
use crate::transpiler::dml::reconstruct_sort_by;
use crate::transpiler::transpile_with_metadata;
use crate::transpiler::reconstruct_sql_with_metadata;
use crate::rls::{get_rls_where_clause, can_bypass_rls, build_rls_expression};

pub(crate) fn reconstruct_create_role_stmt(stmt: &CreateRoleStmt, _ctx: &mut TranspileContext) -> String {
    let role_name = stmt.role.clone();

    let mut superuser = false;
    let mut inherit = true;
    let mut createrole = false;
    let mut createdb = false;
    let mut canlogin = false;
    let mut password = "NULL".to_string();

    for opt in &stmt.options {
        if let Some(ref node) = opt.node {
            if let NodeEnum::DefElem(ref def) = node {
                match def.defname.as_str() {
                    "superuser" => {
                        superuser = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "inherit" => {
                        inherit = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "createrole" => {
                        createrole = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "createdb" => {
                        createdb = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "canlogin" => {
                        canlogin = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "password" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref val) = arg.node {
                                if let NodeEnum::String(ref s) = val {
                                    password = format!("'{}'", s.sval.replace('\'', "''"));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    format!(
        "INSERT INTO __pg_authid__ (rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin, rolpassword) \
         VALUES ('{}', {}, {}, {}, {}, {}, {})",
        role_name.to_lowercase(),
        if superuser { 1 } else { 0 },
        if inherit { 1 } else { 0 },
        if createrole { 1 } else { 0 },
        if createdb { 1 } else { 0 },
        if canlogin { 1 } else { 0 },
        password
    )
}

/// Reconstruct a DROP ROLE statement as a DELETE from __pg_authid__
pub(crate) fn reconstruct_drop_role_stmt(stmt: &DropRoleStmt) -> String {
    let roles: Vec<String> = stmt.roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(format!("'{}'", role.rolename.to_lowercase()));
            }
        }
        None
    }).collect();

    format!("DELETE FROM __pg_authid__ WHERE rolname IN ({})", roles.join(", "))
}

/// Reconstruct a GRANT statement as an INSERT into __pg_acl__
pub(crate) fn reconstruct_grant_stmt(stmt: &GrantStmt) -> String {
    let is_grant = stmt.is_grant;
    let objtype = stmt.objtype;

    // Only support OBJECT_TABLE for now
    if objtype != pg_query::protobuf::ObjectType::ObjectTable as i32 &&
       objtype != pg_query::protobuf::ObjectType::ObjectView as i32 {
        return "SELECT 1".to_string(); // Unsupported for now
    }

    let objects: Vec<String> = stmt.objects.iter().filter_map(|o| {
        if let Some(ref node) = o.node {
            if let NodeEnum::RangeVar(ref rv) = node {
                return Some(rv.relname.to_lowercase());
            }
        }
        None
    }).collect();

    let privileges: Vec<String> = stmt.privileges.iter().filter_map(|p| {
        if let Some(ref node) = p.node {
            if let NodeEnum::AccessPriv(ref ap) = node {
                return Some(ap.priv_name.to_uppercase());
            }
        }
        None
    }).collect();

    let grantees: Vec<String> = stmt.grantees.iter().filter_map(|g| {
        if let Some(ref node) = g.node {
            if let NodeEnum::RoleSpec(ref rs) = node {
                if rs.roletype == pg_query::protobuf::RoleSpecType::RolespecPublic as i32 {
                    return Some("PUBLIC".to_string());
                }
                return Some(rs.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    if is_grant {
        if objects.is_empty() || privileges.is_empty() || grantees.is_empty() {
            return "SELECT 1".to_string();
        }

        let obj = &objects[0];
        let priv_ = &privileges[0];
        let grantee = &grantees[0];
        format!(
            "INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) \
             SELECT c.oid, 'relation', COALESCE(r.oid, 0), '{}', 10 \
             FROM pg_class c LEFT JOIN pg_roles r ON r.rolname = '{}' \
             WHERE c.relname = '{}'",
            priv_, grantee, obj
        )
    } else {
        format!(
            "DELETE FROM __pg_acl__ WHERE object_id IN (SELECT oid FROM pg_class WHERE relname IN ({})) \
             AND grantee_id IN (SELECT oid FROM pg_roles WHERE rolname IN ({})) \
             AND privilege IN ({})",
            objects.iter().map(|o| format!("'{}'", o)).collect::<Vec<_>>().join(", "),
            grantees.iter().map(|g| format!("'{}'", g)).collect::<Vec<_>>().join(", "),
            privileges.iter().map(|p| format!("'{}'", p)).collect::<Vec<_>>().join(", ")
        )
    }
}

/// Reconstruct a GRANT role statement as an INSERT into __pg_auth_members__
pub(crate) fn reconstruct_grant_role_stmt(stmt: &GrantRoleStmt) -> String {
    let is_grant = stmt.is_grant;

    let granted_roles: Vec<String> = stmt.granted_roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(role.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    let grantee_roles: Vec<String> = stmt.grantee_roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(role.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    if is_grant {
        if granted_roles.is_empty() || grantee_roles.is_empty() {
            return "SELECT 1".to_string();
        }
        format!(
            "INSERT INTO __pg_auth_members__ (roleid, member, grantor) \
             SELECT r.oid, m.oid, 10 \
             FROM pg_roles r, pg_roles m \
             WHERE r.rolname = '{}' AND m.rolname = '{}'",
            granted_roles[0], grantee_roles[0]
        )
    } else {
        format!(
            "DELETE FROM __pg_auth_members__ WHERE roleid IN (SELECT oid FROM pg_roles WHERE rolname IN ({})) \
             AND member IN (SELECT oid FROM pg_roles WHERE rolname IN ({}))",
            granted_roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", "),
            grantee_roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", ")
        )
    }
}

/// Reconstruct ALTER TABLE statement with RLS support

pub(crate) fn reconstruct_create_policy_stmt(sql: &str) -> String {
    // Parse the CREATE POLICY statement
    // Since pg_query may not fully support CREATE POLICY, we do manual parsing

    let sql_upper = sql.to_uppercase();

    // Extract policy name
    let policy_name = extract_policy_name(sql);

    // Extract table name
    let table_name = extract_policy_table_name(sql);

    // Determine PERMISSIVE vs RESTRICTIVE (default is PERMISSIVE)
    let permissive = !sql_upper.contains("RESTRICTIVE");

    // Determine command (ALL, SELECT, INSERT, UPDATE, DELETE)
    let command = extract_policy_command(sql);

    // Extract roles
    let roles = extract_policy_roles(sql);

    // Extract USING expression
    let using_expr = extract_policy_using(sql);

    // Extract WITH CHECK expression
    let with_check_expr = extract_policy_with_check(sql);

    // Build the INSERT statement for the policy
    let roles_str = if roles.is_empty() {
        "NULL".to_string()
    } else {
        format!("'{}'", roles.join(","))
    };

    let using_str = using_expr.map(|e| format!("'{}'", e.replace("'", "''"))).unwrap_or_else(|| "NULL".to_string());
    let with_check_str = with_check_expr.map(|e| format!("'{}'", e.replace("'", "''"))).unwrap_or_else(|| "NULL".to_string());

    format!(
        "INSERT OR REPLACE INTO __pg_rls_policies__
         (polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled)
         VALUES ('{}', '{}', '{}', {}, {}, {}, {}, TRUE)",
        policy_name,
        table_name,
        command,
        permissive,
        roles_str,
        using_str,
        with_check_str
    )
}

/// Extract policy name from CREATE POLICY statement
pub(crate) fn extract_policy_name(sql: &str) -> String {
    // CREATE POLICY name ON ...
    let re = regex::Regex::new(r"CREATE\s+POLICY\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract table name from CREATE POLICY statement
pub(crate) fn extract_policy_table_name(sql: &str) -> String {
    // CREATE POLICY name ON table_name ...
    let re = regex::Regex::new(r"CREATE\s+POLICY\s+\w+\s+ON\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract command from CREATE POLICY statement
pub(crate) fn extract_policy_command(sql: &str) -> String {
    let sql_upper = sql.to_uppercase();

    if sql_upper.contains("FOR SELECT") {
        "SELECT".to_string()
    } else if sql_upper.contains("FOR INSERT") {
        "INSERT".to_string()
    } else if sql_upper.contains("FOR UPDATE") {
        "UPDATE".to_string()
    } else if sql_upper.contains("FOR DELETE") {
        "DELETE".to_string()
    } else {
        "ALL".to_string() // Default
    }
}

/// Extract roles from CREATE POLICY statement
pub(crate) fn extract_policy_roles(sql: &str) -> Vec<String> {
    let sql_upper = sql.to_uppercase();

    // Check if TO clause exists
    if let Some(to_pos) = sql_upper.find("TO") {
        // Find the end of the TO clause (before USING or WITH CHECK)
        let end_pos = sql_upper.find("USING")
            .or_else(|| sql_upper.find("WITH CHECK"))
            .unwrap_or(sql.len());

        let to_clause = &sql[to_pos..end_pos];

        // Parse comma-separated roles
        to_clause
            .replace("TO", "")
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![] // Empty means PUBLIC
    }
}

/// Extract USING expression from CREATE POLICY statement
pub(crate) fn extract_policy_using(sql: &str) -> Option<String> {
    let sql_upper = sql.to_uppercase();

    if let Some(using_pos) = sql_upper.find("USING") {
        // Find the start of the expression (after USING)
        let expr_start = using_pos + "USING".len();

        // Find the end (before WITH CHECK or end of statement)
        let expr_end = sql_upper[expr_start..].find("WITH CHECK")
            .map(|pos| expr_start + pos)
            .unwrap_or(sql.len());

        let expr = sql[expr_start..expr_end].trim();

        // Remove surrounding parentheses if present
        let expr = expr.strip_prefix('(').unwrap_or(expr);
        let expr = expr.strip_suffix(')').unwrap_or(expr);

        Some(expr.trim().to_string())
    } else {
        None
    }
}

/// Extract WITH CHECK expression from CREATE POLICY statement
pub(crate) fn extract_policy_with_check(sql: &str) -> Option<String> {
    let sql_upper = sql.to_uppercase();

    if let Some(with_check_pos) = sql_upper.find("WITH CHECK") {
        // Find the start of the expression (after WITH CHECK)
        let expr_start = with_check_pos + "WITH CHECK".len();

        let expr = &sql[expr_start..];

        // Remove surrounding parentheses if present
        let expr = expr.strip_prefix('(').unwrap_or(expr);
        let expr = expr.strip_suffix(')').unwrap_or(expr);

        Some(expr.trim().to_string())
    } else {
        None
    }
}

/// Reconstruct DROP POLICY statement
#[allow(dead_code)]
pub(crate) fn reconstruct_drop_policy_stmt(sql: &str) -> String {
    // DROP POLICY [IF EXISTS] name ON table_name [CASCADE | RESTRICT]
    let policy_name = extract_drop_policy_name(sql);
    let table_name = extract_drop_policy_table_name(sql);

    format!(
        "DELETE FROM __pg_rls_policies__ WHERE polname = '{}' AND polrelid = '{}'",
        policy_name, table_name
    )
}

/// Extract policy name from DROP POLICY statement
#[allow(dead_code)]
pub(crate) fn extract_drop_policy_name(sql: &str) -> String {
    // DROP POLICY [IF EXISTS] name ON ...
    let re = regex::Regex::new(r"DROP\s+POLICY\s+(?:IF\s+EXISTS\s+)?(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract table name from DROP POLICY statement
#[allow(dead_code)]
pub(crate) fn extract_drop_policy_table_name(sql: &str) -> String {
    // DROP POLICY name ON table_name ...
    let re = regex::Regex::new(r"DROP\s+POLICY\s+(?:IF\s+EXISTS\s+)?\w+\s+ON\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extended transpile function that handles RLS injection
///
/// This function takes a connection and RLS context to properly inject
/// RLS predicates into the transpiled SQL using AST manipulation.
#[allow(dead_code)]
pub fn transpile_with_rls(
    sql: &str,
    conn: &Connection,
    rls_ctx: &RlsContext,
) -> TranspileResult {
    let upper_sql = sql.trim().to_uppercase();

    // Handle CREATE POLICY specially
    if upper_sql.starts_with("CREATE POLICY") {
        let sqlite_sql = reconstruct_create_policy_stmt(sql);
        return TranspileResult {
            sql: sqlite_sql,
            create_table_metadata: None, copy_metadata: None,
            referenced_tables: vec![extract_policy_table_name(sql)],
            operation_type: OperationType::DDL,
            errors: Vec::new(),
        };
    }

    // Handle DROP POLICY specially
    if upper_sql.starts_with("DROP POLICY") {
        let sqlite_sql = reconstruct_drop_policy_stmt(sql);
        return TranspileResult {
            sql: sqlite_sql,
            create_table_metadata: None, copy_metadata: None,
            referenced_tables: vec![extract_drop_policy_table_name(sql)],
            operation_type: OperationType::DDL,
            errors: Vec::new(),
        };
    }

    // Parse the SQL for AST-based RLS injection
    match pg_query::parse(sql) {
        Ok(result) => {
            if let Some(raw_stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = raw_stmt.stmt {
                    return transpile_with_rls_ast(stmt_node, conn, rls_ctx, sql);
                }
            }
        }
        Err(_) => {}
    }

    // Fallback to standard transpilation if parsing fails
    transpile_with_metadata(sql)
}

/// Transpile SQL with RLS injection using AST manipulation
#[allow(dead_code)]
pub(crate) fn transpile_with_rls_ast(
    node: &Node,
    conn: &Connection,
    rls_ctx: &RlsContext,
    _original_sql: &str,
) -> TranspileResult {
    let mut ctx = TranspileContext::new();

    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::SelectStmt(ref select_stmt) => {
                let table_name = extract_table_name_from_select(select_stmt);
                let sql = reconstruct_select_stmt_with_rls(select_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None, copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::SELECT,
                    errors: ctx.errors.clone(),
                }
            }
            NodeEnum::InsertStmt(ref insert_stmt) => {
                let table_name = extract_table_name_from_insert(insert_stmt);
                let sql = reconstruct_insert_stmt_with_rls(insert_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None, copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::INSERT,
                    errors: ctx.errors.clone(),
                }
            }
            NodeEnum::UpdateStmt(ref update_stmt) => {
                let table_name = extract_table_name_from_update(update_stmt);
                let sql = reconstruct_update_stmt_with_rls(update_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None, copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::UPDATE,
                    errors: ctx.errors.clone(),
                }
            }
            NodeEnum::DeleteStmt(ref delete_stmt) => {
                let table_name = extract_table_name_from_delete(delete_stmt);
                let sql = reconstruct_delete_stmt_with_rls(delete_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None, copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DELETE,
                    errors: ctx.errors.clone(),
                }
            }
            _ => reconstruct_sql_with_metadata(node, &mut ctx),
        }
    } else {
        reconstruct_sql_with_metadata(node, &mut ctx)
    }
}

/// Extract table name from SELECT statement
pub(crate) fn extract_table_name_from_select(stmt: &SelectStmt) -> String {
    if !stmt.from_clause.is_empty() {
        if let Some(ref node) = stmt.from_clause[0].node {
            if let NodeEnum::RangeVar(r) = node {
                return r.relname.to_lowercase();
            }
        }
    }
    String::new()
}

/// Extract table name from INSERT statement
pub(crate) fn extract_table_name_from_insert(stmt: &InsertStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Extract table name from UPDATE statement
pub(crate) fn extract_table_name_from_update(stmt: &UpdateStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Extract table name from DELETE statement
pub(crate) fn extract_table_name_from_delete(stmt: &DeleteStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Reconstruct SELECT statement with RLS predicate injection
pub(crate) fn reconstruct_select_stmt_with_rls(
    stmt: &SelectStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    // Get RLS predicate if applicable
    let rls_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match get_rls_where_clause(conn, table_name, rls_ctx, "SELECT") {
            Ok(pred) => pred,
            Err(_) => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    // Handle VALUES clause (not applicable for RLS)
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt, ctx);
    }

    // SELECT clause
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

    // Target list
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

    // WHERE clause with RLS injection
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

    // ORDER BY clause
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

/// Reconstruct INSERT statement with RLS WITH CHECK validation
pub(crate) fn reconstruct_insert_stmt_with_rls(
    stmt: &InsertStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    // Get WITH CHECK predicate if applicable
    let with_check_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        if is_rls_enabled(conn, table_name).unwrap_or(false) {
            if !can_bypass_rls(conn, table_name, rls_ctx).unwrap_or(false) {
                let policies = get_applicable_policies(conn, table_name, "INSERT", &rls_ctx.user_roles).unwrap_or_default();
                if policies.is_empty() {
                    Some("FALSE".to_string())
                } else {
                    build_rls_expression(&policies, false) // false = WITH CHECK
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    parts.push("insert into".to_string());

    // Table name
    let table_name_full = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}.{}", r.catalogname.to_lowercase(), r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name_full);

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

    // Handle INSERT with WITH CHECK by converting to INSERT...SELECT pattern
    if let Some(ref select_stmt) = stmt.select_stmt {
        if let Some(ref inner) = select_stmt.node {
            if let NodeEnum::SelectStmt(sel) = inner {
                if !sel.values_lists.is_empty() {
                    // This is a VALUES clause - convert to SELECT with WHERE
                    if let Some(ref check_expr) = with_check_predicate {
                        let values_sql = reconstruct_values_stmt(sel, ctx);
                        // Convert VALUES to SELECT with RLS check
                        let cols_str = if !stmt.cols.is_empty() {
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
                            cols.join(", ")
                        } else {
                            "*".to_string()
                        };

                        // Build INSERT...SELECT with WHERE clause for RLS
                        parts.push(format!(
                            "select {} from ({} {}) where ({})",
                            cols_str,
                            values_sql.replace("values ", ""),
                            if stmt.cols.is_empty() { "" } else { "as v(" },
                            check_expr
                        ));
                    } else {
                        let select_sql = reconstruct_node(select_stmt, ctx);
                        parts.push(select_sql);
                    }
                } else {
                    let select_sql = reconstruct_node(select_stmt, ctx);
                    parts.push(select_sql);
                }
            } else {
                let select_sql = reconstruct_node(select_stmt, ctx);
                parts.push(select_sql);
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
    // Get USING predicate (for filtering rows that can be updated)
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

    // Table name
    let table_name_full = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}.{}", r.catalogname.to_lowercase(), r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name_full);

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

    // WHERE clause with RLS injection
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

    // FROM clause
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
    // Get RLS predicate if applicable
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

    // Table name
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

    // WHERE clause with RLS injection
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

    // USING clause (for additional tables)
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

// ============================================================================
// Window Function Support
// ============================================================================

/// Frame option bitmasks from PostgreSQL (parsenodes.h)
#[allow(dead_code)]
pub mod frame_options {
    pub const NONDEFAULT: i32 = 0x00001;
    pub const RANGE: i32 = 0x00002;
    pub const ROWS: i32 = 0x00004;
    pub const GROUPS: i32 = 0x00008;
    pub const BETWEEN: i32 = 0x00010;
    pub const START_UNBOUNDED_PRECEDING: i32 = 0x00020;
    pub const END_UNBOUNDED_PRECEDING: i32 = 0x00040; // disallowed
    pub const START_UNBOUNDED_FOLLOWING: i32 = 0x00080; // disallowed
    pub const END_UNBOUNDED_FOLLOWING: i32 = 0x00100;
    pub const START_CURRENT_ROW: i32 = 0x00200;
    pub const END_CURRENT_ROW: i32 = 0x00400;
    pub const START_OFFSET_PRECEDING: i32 = 0x00800;
    pub const END_OFFSET_PRECEDING: i32 = 0x01000;
    pub const START_OFFSET_FOLLOWING: i32 = 0x02000;
    pub const END_OFFSET_FOLLOWING: i32 = 0x04000;
    pub const EXCLUDE_CURRENT_ROW: i32 = 0x08000;
    pub const EXCLUDE_GROUP: i32 = 0x10000;
    pub const EXCLUDE_TIES: i32 = 0x20000;
    pub const EXCLUSION: i32 = EXCLUDE_CURRENT_ROW | EXCLUDE_GROUP | EXCLUDE_TIES;
}
