//! RLS policy management functions
//!
//! This module contains functions for parsing and reconstructing
//! CREATE POLICY and DROP POLICY statements.

/// Reconstruct CREATE POLICY statement as an INSERT into __pg_rls_policies__
pub(crate) fn reconstruct_create_policy_stmt(sql: &str) -> String {
    

    let sql_upper = sql.to_uppercase();

    
    let policy_name = extract_policy_name(sql);

    
    let table_name = extract_policy_table_name(sql);

    
    let permissive = !sql_upper.contains("RESTRICTIVE");

    
    let command = extract_policy_command(sql);

    
    let roles = extract_policy_roles(sql);

    
    let using_expr = extract_policy_using(sql);

    
    let with_check_expr = extract_policy_with_check(sql);

    
    let roles_str = if roles.is_empty() {
        "NULL".to_string()
    } else {
        format!("'{}'", roles.join(","))
    };

    let using_str = using_expr.map(|e| format!("'{}'", e.replace('\'', "''"))).unwrap_or_else(|| "NULL".to_string());
    let with_check_str = with_check_expr.map(|e| format!("'{}'", e.replace('\'', "''"))).unwrap_or_else(|| "NULL".to_string());

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

/// Reconstruct DROP POLICY statement as a DELETE from __pg_rls_policies__
pub(crate) fn reconstruct_drop_policy_stmt(sql: &str) -> String {
    let policy_name = extract_drop_policy_name(sql);
    let table_name = extract_drop_policy_table_name(sql);

    format!(
        "DELETE FROM __pg_rls_policies__ WHERE polname = '{}' AND polrelid = '{}'",
        policy_name, table_name
    )
}

/// Extract policy name from CREATE POLICY statement
pub(crate) fn extract_policy_name(sql: &str) -> String {
    
    let re = regex::Regex::new(r"CREATE\s+POLICY\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract table name from CREATE POLICY statement
pub(crate) fn extract_policy_table_name(sql: &str) -> String {
    
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
        "ALL".to_string() 
    }
}

/// Extract roles from CREATE POLICY statement
pub(crate) fn extract_policy_roles(sql: &str) -> Vec<String> {
    let sql_upper = sql.to_uppercase();

    
    if let Some(to_pos) = sql_upper.find("TO") {
        
        let end_pos = sql_upper.find("USING")
            .or_else(|| sql_upper.find("WITH CHECK"))
            .unwrap_or(sql.len());

        let to_clause = &sql[to_pos..end_pos];

        
        to_clause
            .replace("TO", "")
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![] 
    }
}

/// Extract USING expression from CREATE POLICY statement
pub(crate) fn extract_policy_using(sql: &str) -> Option<String> {
    let sql_upper = sql.to_uppercase();

    if let Some(using_pos) = sql_upper.find("USING") {
        
        let expr_start = using_pos + "USING".len();

        
        let expr_end = sql_upper[expr_start..].find("WITH CHECK")
            .map(|pos| expr_start + pos)
            .unwrap_or(sql.len());

        let expr = &sql[expr_start..expr_end].trim();

        
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
        
        let expr_start = with_check_pos + "WITH CHECK".len();

        let expr = &sql[expr_start..];

        
        let expr = expr.strip_prefix('(').unwrap_or(expr);
        let expr = expr.strip_suffix(')').unwrap_or(expr);

        Some(expr.trim().to_string())
    } else {
        None
    }
}

/// Extract policy name from DROP POLICY statement
pub(crate) fn extract_drop_policy_name(sql: &str) -> String {
    let re = regex::Regex::new(r"DROP\s+POLICY\s+(?:IF\s+EXISTS\s+)?(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract table name from DROP POLICY statement
pub(crate) fn extract_drop_policy_table_name(sql: &str) -> String {
    let re = regex::Regex::new(r"DROP\s+POLICY\s+(?:IF\s+EXISTS\s+)?\w+\s+ON\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}
