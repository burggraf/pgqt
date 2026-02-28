//! Row-Level Security (RLS) emulation for SQLite
//!
//! PostgreSQL RLS allows restricting rows based on user policies.
//! We emulate this using views and INSTEAD OF triggers.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;

/// RLS Policy definition
#[derive(Debug, Clone)]
pub struct RlsPolicy {
    pub name: String,
    pub table_name: String,
    pub command: RlsCommand, // ALL, SELECT, INSERT, UPDATE, DELETE
    pub permissive: bool,    // true = PERMISSIVE, false = RESTRICTIVE
    pub using_expr: Option<String>,    // FOR SELECT/ALL
    pub with_check_expr: Option<String>, // FOR INSERT/UPDATE
    pub roles: Vec<String>, // Applies to these roles (empty = all)
}

#[derive(Debug, Clone)]
pub enum RlsCommand {
    All,
    Select,
    Insert,
    Update,
    Delete,
}

/// RLS Manager handles policy creation and enforcement
pub struct RlsManager {
    policies: HashMap<String, Vec<RlsPolicy>>, // table_name -> policies
}

impl RlsManager {
    pub fn new() -> Self {
        Self {
            policies: HashMap::new(),
        }
    }

    /// Enable RLS on a table
    pub fn enable_rls(conn: &Connection, table_name: &str) -> Result<()> {
        // In PostgreSQL: ALTER TABLE ... ENABLE ROW LEVEL SECURITY
        // In our emulation: We create a view and rename the base table
        
        let hidden_table = format!("_{}_data", table_name);
        
        // Rename original table to hidden name
        conn.execute(
            &format!("ALTER TABLE {} RENAME TO {}", table_name, hidden_table),
            [],
        )
        .context("Failed to rename table for RLS")?;
        
        // Create a view with the original name (initially just passes through)
        conn.execute(
            &format!(
                "CREATE VIEW {} AS SELECT * FROM {}",
                table_name, hidden_table
            ),
            [],
        )
        .context("Failed to create RLS view")?;
        
        Ok(())
    }

    /// Create a policy on a table
    pub fn create_policy(
        &mut self,
        conn: &Connection,
        policy: RlsPolicy,
    ) -> Result<()> {
        let table_name = policy.table_name.clone();
        
        // Store policy in memory
        self.policies
            .entry(table_name.clone())
            .or_default()
            .push(policy.clone());
        
        // Recreate the view with policy enforcement
        self.rebuild_rls_view(conn, &table_name)?;
        
        Ok(())
    }

    /// Rebuild the RLS view with current policies
    fn rebuild_rls_view(
        &self,
        conn: &Connection,
        table_name: &str,
    ) -> Result<()> {
        let hidden_table = format!("_{}_data", table_name);
        let policies = self.policies.get(table_name).cloned().unwrap_or_default();
        
        // Drop existing view
        conn.execute(
            &format!("DROP VIEW IF EXISTS {}", table_name),
            [],
        )?;
        
        // Build WHERE clause from policies
        let where_clause = self.build_where_clause(&policies);
        
        // Create new view with policy filters
        let view_sql = if where_clause.is_empty() {
            format!("CREATE VIEW {} AS SELECT * FROM {}", table_name, hidden_table)
        } else {
            format!(
                "CREATE VIEW {} AS SELECT * FROM {} WHERE {}",
                table_name, hidden_table, where_clause
            )
        };
        
        conn.execute(&view_sql, [])
            .context("Failed to create RLS view with policies")?;
        
        // Create INSTEAD OF triggers for write operations
        self.create_write_triggers(conn, table_name)?;
        
        Ok(())
    }

    /// Build WHERE clause from policies
    fn build_where_clause(&self,
        policies: &[RlsPolicy],
    ) -> String {
        let mut clauses = Vec::new();
        
        for policy in policies {
            // Skip non-SELECT policies for the view
            if matches!(policy.command, RlsCommand::Insert | RlsCommand::Update | RlsCommand::Delete) {
                continue;
            }
            
            if let Some(ref expr) = policy.using_expr {
                clauses.push(expr.clone());
            }
        }
        
        if clauses.is_empty() {
            String::new()
        } else {
            clauses.join(" AND ")
        }
    }

    /// Create INSTEAD OF triggers for write operations
    fn create_write_triggers(
        &self,
        _conn: &Connection,
        _table_name: &str,
    ) -> Result<()> {
        // TODO: Create triggers that enforce WITH CHECK expressions
        // on INSERT and UPDATE operations
        
        Ok(())
    }

    /// Check if RLS is enabled for a table
    pub fn is_rls_enabled(&self,
        table_name: &str,
    ) -> bool {
        self.policies.contains_key(table_name)
    }

    /// Get policies for a table
    pub fn get_policies(
        &self,
        table_name: &str,
    ) -> Option<&Vec<RlsPolicy>> {
        self.policies.get(table_name)
    }
}

/// Store policy in database metadata
pub fn store_policy_metadata(
    conn: &Connection,
    policy: &RlsPolicy,
) -> Result<()> {
    conn.execute(
        "INSERT INTO __pg_meta__ (table_name, column_name, original_type, constraints)
         VALUES (?1, ?2, ?3, ?4)",
        (
            &policy.table_name,
            &policy.name,
            "RLS_POLICY",
            Some(format!("{:?}", policy)),
        ),
    )?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        
        // Create test table
        conn.execute(
            "CREATE TABLE documents (id INTEGER PRIMARY KEY, owner TEXT, content TEXT)",
            [],
        ).unwrap();
        
        conn
    }

    #[test]
    fn test_enable_rls() {
        let conn = setup_test_db();
        
        RlsManager::enable_rls(&conn, "documents").unwrap();
        
        // Verify view was created
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='documents'",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(count, 1);
    }

    #[test]
    fn test_create_policy() {
        let conn = setup_test_db();
        let mut manager = RlsManager::new();
        
        RlsManager::enable_rls(&conn, "documents").unwrap();
        
        let policy = RlsPolicy {
            name: "owner_policy".to_string(),
            table_name: "documents".to_string(),
            command: RlsCommand::Select,
            permissive: true,
            using_expr: Some("owner = current_user".to_string()),
            with_check_expr: None,
            roles: vec![],
        };
        
        manager.create_policy(&conn, policy).unwrap();
        
        assert!(manager.is_rls_enabled("documents"));
        assert_eq!(manager.get_policies("documents").unwrap().len(), 1);
    }
}
