//! Transaction handling module
//!
//! This module contains transaction-related handling.

use anyhow::Result;
use pgwire::api::results::{Response, Tag};
use crate::handler::{SessionContext, TransactionStatus};

/// Check if the SQL statement is a transaction control statement
pub fn is_transaction_control(sql: &str) -> bool {
    let upper_sql = sql.trim().trim_end_matches(';').trim().to_uppercase();

    upper_sql == "BEGIN"
        || upper_sql == "COMMIT"
        || upper_sql == "ROLLBACK"
        || upper_sql.starts_with("BEGIN ")
        || upper_sql.starts_with("START TRANSACTION")
        || upper_sql.starts_with("COMMIT ")
        || upper_sql.starts_with("ROLLBACK ")
        || upper_sql == "END"
        || upper_sql.starts_with("END ")
        || upper_sql.starts_with("SAVEPOINT ")
        || upper_sql.starts_with("RELEASE ")
}

/// Handle transaction control statements
pub fn handle_transaction_control(sql: &str, session: &mut SessionContext) -> Option<Result<Vec<Response>>> {
    let upper_sql = sql.trim().trim_end_matches(';').trim().to_uppercase();

    if upper_sql == "BEGIN" || upper_sql.starts_with("BEGIN ") || upper_sql.starts_with("START TRANSACTION") {
        if session.transaction_status != TransactionStatus::Idle {
            // Already in a transaction, typically PG issues a warning but continues
            return Some(Ok(vec![Response::Execution(Tag::new("BEGIN"))]));
        }
        session.transaction_status = TransactionStatus::InTransaction;
        return Some(Ok(vec![Response::Execution(Tag::new("BEGIN"))]));
    } else if upper_sql == "COMMIT" || upper_sql.starts_with("COMMIT ") || upper_sql == "END" || upper_sql.starts_with("END ") {
        if session.transaction_status == TransactionStatus::Idle {
            // Not in a transaction, PG issues warning
            return Some(Ok(vec![Response::Execution(Tag::new("COMMIT"))]));
        }
        session.transaction_status = TransactionStatus::Idle;
        session.savepoints.clear();
        return Some(Ok(vec![Response::Execution(Tag::new("COMMIT"))]));
    } else if upper_sql == "ROLLBACK" || upper_sql.starts_with("ROLLBACK") && !upper_sql.contains(" TO ") {
        session.transaction_status = TransactionStatus::Idle;
        session.savepoints.clear();
        return Some(Ok(vec![Response::Execution(Tag::new("ROLLBACK"))]));
    } else if upper_sql.starts_with("SAVEPOINT ") {
        let parts: Vec<&str> = sql.trim().split_whitespace().collect();
        if parts.len() >= 2 {
            let sp_name = parts[1].trim_end_matches(';').to_string();
            session.savepoints.push(sp_name);
        }
        return Some(Ok(vec![Response::Execution(Tag::new("SAVEPOINT"))]));
    } else if upper_sql.starts_with("ROLLBACK TO ") || upper_sql.starts_with("ROLLBACK TO SAVEPOINT ") {
        // Find the savepoint and rollback to it
        let parts: Vec<&str> = sql.trim().split_whitespace().collect();
        let sp_name = if parts.len() >= 4 { parts[3] } else { parts[2] }.trim_end_matches(';').to_string();
        
        if let Some(idx) = session.savepoints.iter().rposition(|s| s.eq_ignore_ascii_case(&sp_name)) {
            session.savepoints.truncate(idx + 1); // Keep the savepoint itself
            if session.transaction_status == TransactionStatus::InError {
                session.transaction_status = TransactionStatus::InTransaction;
            }
        } else {
            // Savepoint not found error
            return Some(Err(anyhow::anyhow!("savepoint \"{}\" does not exist", sp_name)));
        }
        return Some(Ok(vec![Response::Execution(Tag::new("ROLLBACK"))]));
    } else if upper_sql.starts_with("RELEASE SAVEPOINT ") || upper_sql.starts_with("RELEASE ") {
        let parts: Vec<&str> = sql.trim().split_whitespace().collect();
        let sp_name = if parts.len() >= 3 { parts[2] } else { parts[1] }.trim_end_matches(';').to_string();
        
        if let Some(idx) = session.savepoints.iter().rposition(|s| s.eq_ignore_ascii_case(&sp_name)) {
            session.savepoints.truncate(idx); // Remove this savepoint and later ones
        } else {
            // Savepoint not found error
            return Some(Err(anyhow::anyhow!("savepoint \"{}\" does not exist", sp_name)));
        }
        return Some(Ok(vec![Response::Execution(Tag::new("RELEASE"))]));
    }

    None
}
