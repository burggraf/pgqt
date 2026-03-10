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
//! # Multi-port configuration via JSON file
//! ./pgqt --config pgqt.json
//!
//! # Environment variables
//! PGQT_DB=myapp.db PGQT_PORT=5433 ./pgqt
//! ```

use std::sync::Arc;
use std::path::PathBuf;

use anyhow::{Result, Context};
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
mod config;
mod validation;

use debug::set_debug;
use schema::SearchPath;
use handler::{SqliteHandler, SessionContext};
use handler::query::QueryExecution;
use config::{AppConfig, PortConfig, find_default_config};

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

impl std::fmt::Display for OutputDest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputDest::Stdout => write!(f, "stdout"),
            OutputDest::Stderr => write!(f, "stderr"),
            OutputDest::Null => write!(f, "null"),
            OutputDest::File(path) => write!(f, "{}", path.display()),
        }
    }
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
    /// Path to JSON configuration file
    /// If not specified, looks for pgqt.json in the executable directory
    /// If not found, uses other CLI arguments for single-port mode
    #[arg(short = 'c', long, env = "PGQT_CONFIG")]
    config: Option<PathBuf>,

    /// Host address to listen on (used when no config file)
    #[arg(short = 'H', long, env = "PG_LITE_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on (used when no config file)
    #[arg(short, long, env = "PG_LITE_PORT", default_value = "5432")]
    port: u16,

    /// Path to the SQLite database file (used when no config file)
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
        // Get the current user from client metadata
        let metadata = client.metadata();
        let user = metadata.get("user").map(|s| s.to_string()).unwrap_or_else(|| "postgres".to_string());

        // Set the current user in thread-local storage for current_user() function
        crate::handler::set_current_user(&user);

        // Initialize session from client metadata if not already set
        if self.sessions.is_empty() {
            self.sessions.insert(0, SessionContext {
                authenticated_user: user.clone(),
                current_user: user,
                search_path: SearchPath::default(), transaction_status: crate::handler::TransactionStatus::Idle, savepoints: Vec::new(),
            });
        } else {
            // Update the existing session with current user
            if let Some(mut session) = self.sessions.get_mut(&0) {
                session.current_user = user.clone();
                session.authenticated_user = user;
            }
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

    // Determine configuration source
    let app_config = if let Some(config_path) = cli.config {
        // User specified config file
        AppConfig::from_file(&config_path)?
    } else if let Some(default_config) = find_default_config() {
        // Found pgqt.json in executable directory
        println!("Using default config file: {}", default_config.display());
        AppConfig::from_file(&default_config)?
    } else {
        // Use CLI arguments for single-port mode
        AppConfig::from_cli(
            cli.host,
            cli.port,
            cli.database,
            cli.output.to_string(),
            cli.error_output.map(|o| o.to_string()),
            cli.debug,
            cli.trust_mode,
        )
    };

    // Set global debug flag if ANY port has debug enabled
    // (debug is a global setting since the debug! macro checks a static variable)
    let any_debug = app_config.ports.iter().any(|p| p.debug);
    if any_debug {
        set_debug(true);
        println!("Debug mode enabled (at least one port has debug: true)");
    }

    // Spawn listeners for each configured port
    let mut handles = Vec::new();

    for port_config in app_config.ports {
        let port = port_config.port;
        let handle = tokio::spawn(async move {
            if let Err(e) = run_listener(port_config).await {
                eprintln!("Error on port {}: {}", port, e);
            }
        });
        handles.push(handle);
    }

    // Wait for all listeners (runs indefinitely unless error)
    for handle in handles {
        handle.await?;
    }

    Ok(())
}

/// Parse output destination string to OutputDest enum
fn parse_output_dest(s: &str) -> Result<OutputDest> {
    match s.to_uppercase().as_str() {
        "STDOUT" => Ok(OutputDest::Stdout),
        "STDERR" => Ok(OutputDest::Stderr),
        "NULL" | "/DEV/NULL" => Ok(OutputDest::Null),
        _ => Ok(OutputDest::File(PathBuf::from(s))),
    }
}

/// Run a single listener for a port configuration
async fn run_listener(config: PortConfig) -> Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    // Note: Debug and output redirection are global process operations.
    // In multi-port mode, these are set once before spawning listeners.
    // All listeners share the same debug flag and stdout/stderr.

    println!("Server listening on {}", addr);
    println!("Using database: {}", config.database);

    // Create handler for this port's database
    let handler = Arc::new(SqliteHandler::new(&config.database)?);
    let factory = Arc::new(HandlerFactory {
        handler: handler.clone(),
        trust_mode: config.trust_mode,
    });

    // Accept loop
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
