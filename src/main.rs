//! PGQT — PostgreSQL wire-compatible proxy for SQLite (main binary)
//!
//! This binary crate provides the TCP server and command-line interface.
//! It parses CLI arguments, initializes the SQLite database, and starts the
//! PostgreSQL wire protocol listener. The actual protocol handling is delegated
//! to the [`crate::handler::SqliteHandler`].
//!
//! ## Usage
//!
//! ```bash
//! # Start with defaults (test.db on port 5432)
//! ./pgqt
//!
//! # Custom database and port
//! ./pgqt --database myapp.db --port 5433
//!
//! # Environment variables
//! PGQT_DB=myapp.db PGQT_PORT=5433 ./pgqt
//! ```

use std::sync::Arc;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use pgwire::api::query::SimpleQueryHandler;
use pgwire::api::results::Response;
use pgwire::api::{ClientInfo, PgWireServerHandlers};
use pgwire::error::PgWireResult;
use pgwire::tokio::process_socket;
use tokio::net::TcpListener;

use crate::handler::errors::PgError;

mod catalog;
mod copy;
mod distinct_on;
mod rls;
mod rls_inject;
mod transpiler;
mod fts;
mod vector;
mod schema;
mod array;
mod range;
mod geo;
mod plpgsql;
mod functions;
mod stats;
mod handler;
mod debug;
mod auth;

use debug::set_debug;
use schema::SearchPath;
use handler::{SqliteHandler, SessionContext};
use handler::query::QueryExecution;

#[derive(Debug, Clone, PartialEq)]
enum OutputDest {
    /// Send to stdout
    Stdout,
    /// Send to stderr
    Stderr,
    /// Send to a file (path)
    File(PathBuf),
    /// Suppress output
    Null,
}

impl std::str::FromStr for OutputDest {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "STDOUT" => Ok(OutputDest::Stdout),
            "STDERR" => Ok(OutputDest::Stderr),
            "NULL" | "/dev/null" => Ok(OutputDest::Null),
            _ => Ok(OutputDest::File(PathBuf::from(s))),
        }
    }
}

/// PGQT proxy server
#[derive(Parser, Debug)]
#[command(name = "pgqt")]
#[command(about = "A PostgreSQL wire protocol proxy for SQLite")]

struct Cli {
    /// Host address to listen on
    #[arg(short = 'H', long, env = "PG_LITE_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(short, long, env = "PG_LITE_PORT", default_value = "5432")]
    port: u16,

    /// Path to the SQLite database file
    #[arg(short, long, env = "PG_LITE_DB", default_value = "test.db")]
    database: String,

    /// Where to send server output (info messages, query logs).
    /// Options: STDOUT (default), STDERR, NULL (suppress), or a file path.
    #[arg(short = 'o', long, env = "PG_LITE_OUTPUT", default_value = "STDOUT")]
    output: OutputDest,

    /// Where to send server error output (errors, warnings).
    /// Options: STDERR, STDOUT, NULL (suppress), or a file path.
    /// Default: <database>.error.log (e.g., test.db.error.log)
    #[arg(short = 'e', long, env = "PG_LITE_ERROR_OUTPUT")]
    error_output: Option<OutputDest>,

    /// Enable debug output
    #[arg(short = 'D', long, env = "PG_LITE_DEBUG")]
    debug: bool,

    /// Disable password authentication (trust mode)
    #[arg(long, env = "PGQT_TRUST_MODE", help = "Disable password authentication, allow any connection")]
    trust_mode: bool,
}
impl Cli {
    /// Get the error output destination, defaulting to <database>.error.log
    fn error_output_dest(&self) -> OutputDest {
        self.error_output.clone().unwrap_or_else(|| {
            OutputDest::File(PathBuf::from(format!("{}.error.log", self.database)))
        })
    }
}


struct HandlerFactory {
    handler: Arc<SqliteHandler>,
    trust_mode: bool,
}

impl PgWireServerHandlers for HandlerFactory {
    fn startup_handler(&self) -> Arc<impl pgwire::api::auth::StartupHandler> {
        if self.trust_mode {
            // Trust mode: accept all connections without authentication
            Arc::new(auth::FlexibleAuthHandler::new_trust())
        } else {
            // Password authentication mode
            Arc::new(auth::FlexibleAuthHandler::new_password(self.handler.conn.clone()))
        }
    }

    fn simple_query_handler(&self) -> Arc<impl pgwire::api::query::SimpleQueryHandler> {
        self.handler.clone()
    }

    fn extended_query_handler(&self) -> Arc<impl pgwire::api::query::ExtendedQueryHandler> {
        self.handler.clone()
    }

    fn copy_handler(&self) -> Arc<impl pgwire::api::copy::CopyHandler> {
        Arc::new(self.handler.copy_handler.clone())
    }
}
#[async_trait]
impl SimpleQueryHandler for SqliteHandler {
    async fn do_query<C>(&self, client: &mut C, query: &str) -> PgWireResult<Vec<Response>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        // Initialize session from client metadata if not already set
        if self.sessions.is_empty() {
            let metadata = client.metadata();
            let user = metadata.get("user").map(|s| s.to_string()).unwrap_or_else(|| "postgres".to_string());
            self.sessions.insert(0, SessionContext {
                authenticated_user: user.clone(),
                current_user: user,
                search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
            });
        }

        debug!("Received query: {}", query);
        match self.execute_query(query) {
            Ok(responses) => Ok(responses),
            Err(e) => {
                eprintln!("Error executing query: {}", e);
                let pg_err = PgError::from_anyhow(e);
                Ok(vec![Response::Error(Box::new(pg_err.into_error_info()))])
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        set_debug(true);
    }

    // Set up output redirection
    let error_dest = cli.error_output_dest();
    setup_output_redirection(&cli.output, &error_dest)?;

    let addr = format!("{}:{}", cli.host, cli.port);

    let listener = TcpListener::bind(&addr).await?;
    log_output(&format!("Server listening on {}", addr));
    log_output(&format!("Using database: {}", cli.database));
    log_error(&format!("Error log started for database: {}", cli.database));

    let handler = Arc::new(SqliteHandler::new(&cli.database)?);
    let factory = Arc::new(HandlerFactory { 
        handler: handler.clone(),
        trust_mode: cli.trust_mode,
    });

    loop {
        let (incoming_socket, client_addr) = listener.accept().await?;
        debug!("New connection from {}", client_addr);

        let factory = factory.clone();

        tokio::spawn(async move {
            let _ = process_socket(
                incoming_socket,
                None,
                factory,
            )
            .await;
        });
    }
}

/// Set up output redirection based on CLI arguments
fn setup_output_redirection(output: &OutputDest, error_output: &OutputDest) -> Result<()> {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    // Handle stdout redirection
    match output {
        OutputDest::Stdout => {
            // Keep stdout as is
        }
        OutputDest::Stderr => {
            // Redirect stdout to stderr
            unsafe {
                libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO);
            }
        }
        OutputDest::Null => {
            // Redirect stdout to /dev/null
            let null_file = OpenOptions::new()
                .write(true)
                .open("/dev/null")?;
            unsafe {
                libc::dup2(null_file.as_raw_fd(), libc::STDOUT_FILENO);
            }
        }
        OutputDest::File(path) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            unsafe {
                libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO);
            }
        }
    }

    // Handle stderr redirection
    match error_output {
        OutputDest::Stderr => {
            // Keep stderr as is
        }
        OutputDest::Stdout => {
            // Redirect stderr to stdout
            unsafe {
                libc::dup2(libc::STDOUT_FILENO, libc::STDERR_FILENO);
            }
        }
        OutputDest::Null => {
            // Redirect stderr to /dev/null
            let null_file = OpenOptions::new()
                .write(true)
                .open("/dev/null")?;
            unsafe {
                libc::dup2(null_file.as_raw_fd(), libc::STDERR_FILENO);
            }
        }
        OutputDest::File(path) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            unsafe {
                libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
            }
        }
    }

    Ok(())
}

/// Log a message to the configured output destination
fn log_output(msg: &str) {
    println!("{}", msg);
}

/// Log an error message to the configured error output destination
fn log_error(msg: &str) {
    eprintln!("{}", msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db_path(name: &str) -> String {
        let temp_dir = std::env::temp_dir();
        temp_dir.join(name).to_str().unwrap().to_string()
    }

    fn cleanup_db(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_handler_initializes_catalog() {
        let db_path = temp_db_path("test_pg_lite.db");
        cleanup_db(&db_path);

        let handler = SqliteHandler::new(&db_path).unwrap();

        let conn = handler.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='__pg_meta__'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
        cleanup_db(&db_path);
    }

    #[test]
    fn test_create_table_stores_metadata() {
        let db_path = temp_db_path("test_pg_lite_meta.db");
        cleanup_db(&db_path);

        let handler = SqliteHandler::new(&db_path).unwrap();

        let _ = handler.execute_query("CREATE TABLE test_table (id SERIAL, name VARCHAR(10), created_at TIMESTAMP WITH TIME ZONE)");

        let conn = handler.conn.lock().unwrap();
        let metadata = catalog::get_table_metadata(&conn, "test_table").unwrap();

        assert_eq!(metadata.len(), 3);

        let types: Vec<String> = metadata.iter().map(|m| m.original_type.clone()).collect();
        assert!(types.contains(&"SERIAL".to_string()));
        assert!(types.contains(&"VARCHAR(10)".to_string()));
        assert!(types.contains(&"TIMESTAMP WITH TIME ZONE".to_string()));

        cleanup_db(&db_path);
    }
}
