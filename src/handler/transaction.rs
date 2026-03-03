//! Transaction handling module
//!
//! This module contains transaction-related handling.
//! Note: SQLite handles transactions automatically, so we mostly
//! just acknowledge transaction control statements.

use anyhow::Result;
use pgwire::api::results::{Response, Tag};

/// Handle transaction control statements
///
/// SQLite handles transactions automatically, so we just return OK
/// for BEGIN, COMMIT, and ROLLBACK statements to maintain compatibility
/// with PostgreSQL clients.
pub fn handle_transaction_control(sql: &str) -> Option<Result<Vec<Response>>> {
    let upper_sql = sql.trim().to_uppercase();

    // Check for transaction control statements
    if upper_sql == "BEGIN"
        || upper_sql == "COMMIT"
        || upper_sql == "ROLLBACK"
        || upper_sql.starts_with("BEGIN ")
        || upper_sql.starts_with("START TRANSACTION")
    {
        // SQLite handles transactions automatically
        // Just return OK to acknowledge the statement
        Some(Ok(vec![Response::Execution(Tag::new("OK"))]))
    } else {
        None
    }
}

/// Check if the SQL statement is a transaction control statement
pub fn is_transaction_control(sql: &str) -> bool {
    let upper_sql = sql.trim().to_uppercase();

    upper_sql == "BEGIN"
        || upper_sql == "COMMIT"
        || upper_sql == "ROLLBACK"
        || upper_sql.starts_with("BEGIN ")
        || upper_sql.starts_with("START TRANSACTION")
        || upper_sql.starts_with("COMMIT ")
        || upper_sql.starts_with("ROLLBACK ")
        || upper_sql.starts_with("END")
}
