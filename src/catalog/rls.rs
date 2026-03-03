//! Row-Level Security (RLS) policy storage and retrieval
//!
//! This module manages the persistence of RLS policies and table RLS state in the
//! `__pg_rls_policies__` and `__pg_rls_enabled__` shadow tables.
//!
//! ## Key Functions
//! - [`enable_rls`] / [`disable_rls`] — Enable or disable RLS on a table
//! - [`is_rls_enabled`] / [`is_rls_forced`] — Check current RLS state for a table
//! - [`store_rls_policy`] — Persist an RLS policy (USING / WITH CHECK expressions)
//! - [`get_applicable_policies`] — Retrieve policies for a given table and operation
//! - [`get_table_policies`] — List all policies on a table
//! - [`drop_rls_policy`] — Remove a named policy from a table

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::RlsPolicy;

/// Enable RLS on a table
#[allow(dead_code)]
pub fn enable_rls(conn: &Connection, table_name: &str, force: bool) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_rls_enabled__ (relname, rls_enabled, rls_forced) VALUES (?1, TRUE, ?2)",
        (table_name, force),
    )
    .context("Failed to enable RLS on table")?;
    Ok(())
}

/// Disable RLS on a table
#[allow(dead_code)]
pub fn disable_rls(conn: &Connection, table_name: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_rls_enabled__ (relname, rls_enabled, rls_forced) VALUES (?1, FALSE, FALSE)",
        [table_name],
    )
    .context("Failed to disable RLS on table")?;
    Ok(())
}

/// Check if RLS is enabled on a table
pub fn is_rls_enabled(conn: &Connection, table_name: &str) -> Result<bool> {
    let result: Result<bool, _> = conn.query_row(
        "SELECT rls_enabled FROM __pg_rls_enabled__ WHERE relname = ?1",
        [table_name],
        |row| row.get(0),
    );
    match result {
        Ok(enabled) => Ok(enabled),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Check if RLS is forced on a table (bypass for table owner)
#[allow(dead_code)]
pub fn is_rls_forced(conn: &Connection, table_name: &str) -> Result<bool> {
    let result: Result<bool, _> = conn.query_row(
        "SELECT rls_forced FROM __pg_rls_enabled__ WHERE relname = ?1",
        [table_name],
        |row| row.get(0),
    );
    match result {
        Ok(forced) => Ok(forced),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Store an RLS policy
#[allow(dead_code)]
pub fn store_rls_policy(conn: &Connection, policy: &RlsPolicy) -> Result<()> {
    let roles_str = if policy.roles.is_empty() {
        None
    } else {
        Some(policy.roles.join(","))
    };

    conn.execute(
        "INSERT OR REPLACE INTO __pg_rls_policies__ 
         (polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        (
            &policy.name,
            &policy.table_name,
            &policy.command,
            policy.permissive,
            roles_str,
            &policy.using_expr,
            &policy.with_check_expr,
            policy.enabled,
        ),
    )
    .context("Failed to store RLS policy")?;
    Ok(())
}

/// Get all policies for a table applicable to a specific command and roles
pub fn get_applicable_policies(
    conn: &Connection,
    table_name: &str,
    command: &str, 
    user_roles: &[String],
) -> Result<Vec<RlsPolicy>> {
    let mut stmt = conn.prepare(
        "SELECT polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled
         FROM __pg_rls_policies__
         WHERE polrelid = ?1 
         AND polenabled = TRUE
         AND (polcmd = 'ALL' OR polcmd = ?2)"
    )?;

    let rows = stmt.query_map([table_name, command], |row| {
        let roles_str: Option<String> = row.get(4)?;
        let roles = roles_str
            .map(|s| s.split(',').map(|r| r.to_string()).collect())
            .unwrap_or_default();

        Ok(RlsPolicy {
            name: row.get(0)?,
            table_name: row.get(1)?,
            command: row.get(2)?,
            permissive: row.get(3)?,
            roles,
            using_expr: row.get(5)?,
            with_check_expr: row.get(6)?,
            enabled: row.get(7)?,
        })
    })?;

    let mut policies = Vec::new();
    for row in rows {
        let policy = row?;
        
        
        if policy.roles.is_empty() 
            || policy.roles.contains(&"PUBLIC".to_string())
            || user_roles.iter().any(|r| policy.roles.contains(r)) {
            policies.push(policy);
        }
    }

    Ok(policies)
}

/// Drop an RLS policy
#[allow(dead_code)]
pub fn drop_rls_policy(conn: &Connection, policy_name: &str, table_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM __pg_rls_policies__ WHERE polname = ?1 AND polrelid = ?2",
        (policy_name, table_name),
    )
    .context("Failed to drop RLS policy")?;
    Ok(())
}

/// Get all policies for a table (for admin/inspection)
#[allow(dead_code)]
pub fn get_table_policies(conn: &Connection, table_name: &str) -> Result<Vec<RlsPolicy>> {
    let mut stmt = conn.prepare(
        "SELECT polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled
         FROM __pg_rls_policies__
         WHERE polrelid = ?1"
    )?;

    let rows = stmt.query_map([table_name], |row| {
        let roles_str: Option<String> = row.get(4)?;
        let roles = roles_str
            .map(|s| s.split(',').map(|r| r.to_string()).collect())
            .unwrap_or_default();

        Ok(RlsPolicy {
            name: row.get(0)?,
            table_name: row.get(1)?,
            command: row.get(2)?,
            permissive: row.get(3)?,
            roles,
            using_expr: row.get(5)?,
            with_check_expr: row.get(6)?,
            enabled: row.get(7)?,
        })
    })?;

    let mut policies = Vec::new();
    for row in rows {
        policies.push(row?);
    }

    Ok(policies)
}
