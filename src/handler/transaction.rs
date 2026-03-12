//! Transaction control handling for PostgreSQL-compatible transactions
//!
//! This module handles BEGIN, COMMIT, ROLLBACK, SAVEPOINT, and related commands.
//! It integrates with the per-session connection pool for proper transaction isolation.

use anyhow::{anyhow, Result};
use pgwire::api::results::{Response, Tag};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

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

/// Transaction command types
#[derive(Debug, Clone)]
pub enum TransactionCommand {
    Begin,
    Commit,
    Rollback,
    Savepoint(String),
    RollbackToSavepoint(String),
    ReleaseSavepoint(String),
}

/// Parse a transaction control SQL statement
pub fn parse_transaction_command(sql: &str) -> Option<TransactionCommand> {
    let upper_sql = sql.trim().trim_end_matches(';').trim().to_uppercase();

    if upper_sql == "BEGIN" || upper_sql.starts_with("BEGIN ") || upper_sql.starts_with("START TRANSACTION") {
        Some(TransactionCommand::Begin)
    } else if upper_sql == "COMMIT" || upper_sql.starts_with("COMMIT ") || upper_sql == "END" || upper_sql.starts_with("END ") {
        Some(TransactionCommand::Commit)
    } else if upper_sql == "ROLLBACK" || (upper_sql.starts_with("ROLLBACK") && !upper_sql.contains(" TO ")) {
        Some(TransactionCommand::Rollback)
    } else if upper_sql.starts_with("SAVEPOINT ") {
        let parts: Vec<&str> = sql.trim().split_whitespace().collect();
        if parts.len() >= 2 {
            let sp_name = parts[1].trim_end_matches(';').to_string();
            Some(TransactionCommand::Savepoint(sp_name))
        } else {
            None
        }
    } else if upper_sql.starts_with("ROLLBACK TO ") || upper_sql.starts_with("ROLLBACK TO SAVEPOINT ") {
        let parts: Vec<&str> = sql.trim().split_whitespace().collect();
        let sp_name = if parts.len() >= 4 { parts[3] } else { parts[2] }.trim_end_matches(';').to_string();
        Some(TransactionCommand::RollbackToSavepoint(sp_name))
    } else if upper_sql.starts_with("RELEASE SAVEPOINT ") || upper_sql.starts_with("RELEASE ") {
        let parts: Vec<&str> = sql.trim().split_whitespace().collect();
        let sp_name = if parts.len() >= 3 { parts[2] } else { parts[1] }.trim_end_matches(';').to_string();
        Some(TransactionCommand::ReleaseSavepoint(sp_name))
    } else {
        None
    }
}

/// Execute a transaction command on the given connection
///
/// This function updates the session state and executes the corresponding
/// SQLite transaction command on the provided connection.
/// Returns TransactionStart/TransactionEnd responses for wire protocol integration.
pub fn execute_transaction_command(
    cmd: TransactionCommand,
    session: &mut SessionContext,
    conn: &Connection,
) -> Result<Vec<Response>> {
    match cmd {
        TransactionCommand::Begin => {
            if session.transaction_status != TransactionStatus::Idle {
                // Already in a transaction - PostgreSQL allows this and just returns success
                return Ok(vec![Response::TransactionStart(Tag::new("BEGIN"))]);
            }

            // Execute SQLite BEGIN
            conn.execute("BEGIN", [])?;
            session.transaction_status = TransactionStatus::InTransaction;
            Ok(vec![Response::TransactionStart(Tag::new("BEGIN"))])
        }

        TransactionCommand::Commit => {
            if session.transaction_status == TransactionStatus::Idle {
                // Not in a transaction - PostgreSQL returns success anyway
                return Ok(vec![Response::TransactionEnd(Tag::new("COMMIT"))]);
            }

            // Execute SQLite COMMIT
            conn.execute("COMMIT", [])?;
            session.transaction_status = TransactionStatus::Idle;
            session.savepoints.clear();
            Ok(vec![Response::TransactionEnd(Tag::new("COMMIT"))])
        }

        TransactionCommand::Rollback => {
            // Execute SQLite ROLLBACK (works even if not in a transaction)
            let _ = conn.execute("ROLLBACK", []);
            session.transaction_status = TransactionStatus::Idle;
            session.savepoints.clear();
            Ok(vec![Response::TransactionEnd(Tag::new("ROLLBACK"))])
        }

        TransactionCommand::Savepoint(name) => {
            // Savepoints can only be created inside a transaction
            let started_transaction = session.transaction_status == TransactionStatus::Idle;
            if started_transaction {
                // PostgreSQL automatically starts a transaction for SAVEPOINT
                conn.execute("BEGIN", [])?;
                session.transaction_status = TransactionStatus::InTransaction;
            }

            conn.execute(&format!("SAVEPOINT {}", escape_identifier(&name)), [])?;
            session.savepoints.push(name);

            // Return TransactionStart if we just started a transaction, otherwise Execution
            if started_transaction {
                Ok(vec![Response::TransactionStart(Tag::new("SAVEPOINT"))])
            } else {
                Ok(vec![Response::Execution(Tag::new("SAVEPOINT"))])
            }
        }

        TransactionCommand::RollbackToSavepoint(name) => {
            if !session.savepoints.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                return Err(anyhow!("savepoint \"{}\" does not exist", name));
            }

            conn.execute(&format!("ROLLBACK TO {}", escape_identifier(&name)), [])?;

            // Remove savepoints after the rolled-back one
            if let Some(idx) = session.savepoints.iter().rposition(|s| s.eq_ignore_ascii_case(&name)) {
                session.savepoints.truncate(idx + 1);
            }

            // Rolling back to savepoint clears error state but stays in transaction
            if session.transaction_status == TransactionStatus::InError {
                session.transaction_status = TransactionStatus::InTransaction;
            }

            Ok(vec![Response::Execution(Tag::new("ROLLBACK"))])
        }

        TransactionCommand::ReleaseSavepoint(name) => {
            if !session.savepoints.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                return Err(anyhow!("savepoint \"{}\" does not exist", name));
            }

            conn.execute(&format!("RELEASE {}", escape_identifier(&name)), [])?;

            // Remove the savepoint and any after it
            if let Some(idx) = session.savepoints.iter().rposition(|s| s.eq_ignore_ascii_case(&name)) {
                session.savepoints.truncate(idx);
            }

            Ok(vec![Response::Execution(Tag::new("RELEASE"))])
        }
    }
}

/// Handle transaction control statements (legacy API for backwards compatibility)
///
/// This is the original API that doesn't use per-session connections.
/// For new code, use parse_transaction_command + execute_transaction_command.
pub fn handle_transaction_control(sql: &str, session: &mut SessionContext) -> Option<Result<Vec<Response>>> {
    parse_transaction_command(sql).map(|cmd| {
        // This is a shim that doesn't actually execute on a connection.
        // The actual execution happens in query.rs using the shared connection.
        // TODO: Refactor to use execute_transaction_command with per-session connection

        match cmd {
            TransactionCommand::Begin => {
                if session.transaction_status != TransactionStatus::Idle {
                    return Ok(vec![Response::TransactionStart(Tag::new("BEGIN"))]);
                }
                session.transaction_status = TransactionStatus::InTransaction;
                Ok(vec![Response::TransactionStart(Tag::new("BEGIN"))])
            }
            TransactionCommand::Commit => {
                if session.transaction_status == TransactionStatus::Idle {
                    return Ok(vec![Response::TransactionEnd(Tag::new("COMMIT"))]);
                }
                session.transaction_status = TransactionStatus::Idle;
                session.savepoints.clear();
                Ok(vec![Response::TransactionEnd(Tag::new("COMMIT"))])
            }
            TransactionCommand::Rollback => {
                session.transaction_status = TransactionStatus::Idle;
                session.savepoints.clear();
                Ok(vec![Response::TransactionEnd(Tag::new("ROLLBACK"))])
            }
            TransactionCommand::Savepoint(name) => {
                let started_transaction = session.transaction_status == TransactionStatus::Idle;
                if started_transaction {
                    session.transaction_status = TransactionStatus::InTransaction;
                }
                session.savepoints.push(name);
                if started_transaction {
                    Ok(vec![Response::TransactionStart(Tag::new("SAVEPOINT"))])
                } else {
                    Ok(vec![Response::Execution(Tag::new("SAVEPOINT"))])
                }
            }
            TransactionCommand::RollbackToSavepoint(name) => {
                if !session.savepoints.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    return Err(anyhow!("savepoint \"{}\" does not exist", name));
                }
                if let Some(idx) = session.savepoints.iter().rposition(|s| s.eq_ignore_ascii_case(&name)) {
                    session.savepoints.truncate(idx + 1);
                    if session.transaction_status == TransactionStatus::InError {
                        session.transaction_status = TransactionStatus::InTransaction;
                    }
                }
                Ok(vec![Response::Execution(Tag::new("ROLLBACK"))])
            }
            TransactionCommand::ReleaseSavepoint(name) => {
                if !session.savepoints.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    return Err(anyhow!("savepoint \"{}\" does not exist", name));
                }
                if let Some(idx) = session.savepoints.iter().rposition(|s| s.eq_ignore_ascii_case(&name)) {
                    session.savepoints.truncate(idx);
                }
                Ok(vec![Response::Execution(Tag::new("RELEASE"))])
            }
        }
    })
}

/// Escape a SQL identifier to prevent injection
fn escape_identifier(name: &str) -> String {
    // Simple escaping: replace " with ""
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transaction_control() {
        assert!(is_transaction_control("BEGIN"));
        assert!(is_transaction_control("begin"));
        assert!(is_transaction_control("COMMIT"));
        assert!(is_transaction_control("ROLLBACK"));
        assert!(is_transaction_control("SAVEPOINT sp1"));
        assert!(!is_transaction_control("SELECT 1"));
    }

    #[test]
    fn test_parse_transaction_command() {
        assert!(matches!(parse_transaction_command("BEGIN"), Some(TransactionCommand::Begin)));
        assert!(matches!(parse_transaction_command("COMMIT"), Some(TransactionCommand::Commit)));
        assert!(matches!(parse_transaction_command("ROLLBACK"), Some(TransactionCommand::Rollback)));
        
        let cmd = parse_transaction_command("SAVEPOINT my_sp");
        assert!(matches!(cmd, Some(TransactionCommand::Savepoint(_))));
        if let Some(TransactionCommand::Savepoint(name)) = cmd {
            assert_eq!(name, "my_sp");
        }
        
        assert!(parse_transaction_command("SELECT 1").is_none());
    }

    #[test]
    fn test_escape_identifier() {
        assert_eq!(escape_identifier("my_table"), "\"my_table\"");
        assert_eq!(escape_identifier("my\"table"), "\"my\"\"table\"");
    }
}
