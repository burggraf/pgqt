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
use pgwire::tokio::{process_socket, TlsAcceptor};
#[cfg(unix)]
use pgwire::tokio::process_socket_unix;
use tokio::net::TcpListener;

use crate::handler::errors::PgError;

mod catalog;
mod connection_pool;
mod copy;
mod distinct_on;
mod float_special;
mod rls;
mod rls_inject;
mod transpiler;
mod fts;
mod vector;
mod schema;
mod array;
mod range;
mod interval;
mod geo;
mod plpgsql;
mod functions;
mod stats;
mod handler;
mod debug;
mod auth;
mod config;
mod validation;
mod trigger;
mod regex_funcs;
mod array_agg;
mod bool_aggregates;
mod stats_accum;
mod hypothetical_rank;
mod jsonb;
mod cache;
#[cfg(feature = "tls")]
mod tls;

use debug::set_debug;
use handler::{SqliteHandler, SessionContext};
use handler::query::QueryExecution;
use config::{AppConfig, PortConfig, find_default_config, SqlitePragmaConfig, CacheConfig, PoolConfig, MemoryConfig, BufferPoolConfig, MemoryMonitoringConfig, MmapConfig, JournalMode, SynchronousMode, TempStore, NetworkConfig, TlsConfigOptions};
#[cfg(feature = "tls")]
use tls::TlsConfig;

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
#[command(version)]
struct Cli {
    /// Path to JSON configuration file
    /// If not specified, looks for pgqt.json in the executable directory
    /// If not found, uses other CLI arguments for single-port mode
    #[arg(short = 'c', long, env = "PGQT_CONFIG")]
    config: Option<PathBuf>,

    /// Host address to listen on (used when no config file)
    #[arg(short = 'H', long, env = "PGQT_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on (used when no config file)
    #[arg(short, long, env = "PGQT_PORT", default_value = "5432")]
    port: u16,

    /// Path to the SQLite database file (used when no config file)
    #[arg(short, long, env = "PGQT_DB", default_value = "test.db")]
    database: String,

    /// Where to send server output (info messages, query logs).
    /// Options: STDOUT (default), STDERR, NULL (suppress), or a file path.
    #[arg(short = 'o', long, env = "PGQT_OUTPUT", default_value = "STDOUT")]
    output: OutputDest,

    /// Where to send server error output (errors, warnings).
    /// Options: STDERR, STDOUT, NULL (suppress), or a file path.
    /// Default: <database>.error.log (e.g., test.db.error.log)
    #[arg(short = 'e', long, env = "PGQT_ERROR_OUTPUT")]
    error_output: Option<OutputDest>,

    /// Enable debug output
    #[arg(short = 'D', long, env = "PGQT_DEBUG")]
    debug: bool,

    /// Disable password authentication (trust mode)
    #[arg(long, env = "PGQT_TRUST_MODE", help = "Disable password authentication, allow any connection")]
    trust_mode: bool,

    /// Auto-create users that don't exist (insecure, for development only)
    #[arg(long, env = "PGQT_AUTO_CREATE_USERS", help = "Auto-create users that don't exist (insecure, for development only)")]
    auto_create_users: bool,

    // SQLite PRAGMA Configuration
    /// SQLite journal mode: delete, truncate, persist, memory, wal (default: wal)
    #[arg(long, env = "PGQT_JOURNAL_MODE", default_value = "wal")]
    journal_mode: JournalMode,

    /// SQLite synchronous mode: off, normal, full, extra (default: normal)
    #[arg(long, env = "PGQT_SYNCHRONOUS", default_value = "normal")]
    synchronous: SynchronousMode,

    /// SQLite cache size in pages (default: -2000, meaning 2000KB)
    #[arg(long, env = "PGQT_CACHE_SIZE", default_value = "-2000")]
    cache_size: i32,

    /// SQLite memory-mapped I/O size in bytes (default: 0, disabled)
    #[arg(long, env = "PGQT_MMAP_SIZE", default_value = "0")]
    mmap_size: i64,

    /// SQLite temp store mode: default, file, memory (default: default)
    #[arg(long, env = "PGQT_TEMP_STORE", default_value = "default")]
    temp_store: TempStore,

    // Cache Configuration
    /// Transpile cache size (number of entries, default: 256)
    #[arg(long, env = "PGQT_TRANSPILE_CACHE_SIZE", default_value = "256")]
    transpile_cache_size: usize,

    /// Transpile cache TTL in seconds (default: 0, no TTL)
    #[arg(long, env = "PGQT_TRANSPILE_CACHE_TTL", default_value = "0")]
    transpile_cache_ttl: u64,

    /// Enable query result caching (default: false)
    #[arg(long, env = "PGQT_ENABLE_RESULT_CACHE")]
    enable_result_cache: bool,

    /// Query result cache size (number of entries, default: 64)
    #[arg(long, env = "PGQT_RESULT_CACHE_SIZE", default_value = "64")]
    result_cache_size: usize,

    /// Query result cache TTL in seconds (default: 60)
    #[arg(long, env = "PGQT_RESULT_CACHE_TTL", default_value = "60")]
    result_cache_ttl: u64,

    
    /// Maximum number of connections allowed (default: 100)
    #[arg(long, env = "PGQT_MAX_CONNECTIONS", default_value = "100")]
    max_connections: usize,

    /// Initial pool size - number of connections to create upfront (default: 8)
    #[arg(long, env = "PGQT_POOL_SIZE", default_value = "8")]
    pool_size: usize,

    /// Connection timeout in seconds when checking out (default: 30)
    #[arg(long, env = "PGQT_CONNECTION_TIMEOUT", default_value = "30")]
    connection_timeout: u64,

    /// Idle timeout in seconds before closing unused connections (default: 300)
    #[arg(long, env = "PGQT_IDLE_TIMEOUT", default_value = "300")]
    idle_timeout: u64,

    /// Health check interval in seconds (default: 60)
    #[arg(long, env = "PGQT_HEALTH_CHECK_INTERVAL", default_value = "60")]
    health_check_interval: u64,

    /// Maximum number of retries for failed checkouts (default: 3)
    #[arg(long, env = "PGQT_MAX_RETRIES", default_value = "3")]
    max_retries: u32,

    /// Enable connection pooling (default: false for backward compatibility)
    #[arg(long, env = "PGQT_USE_POOLING")]
    use_pooling: bool,

    // Memory Management Configuration
    /// Enable buffer pool for efficient memory reuse (default: false)
    #[arg(long, env = "PGQT_ENABLE_BUFFER_POOL")]
    enable_buffer_pool: bool,

    /// Buffer pool size - maximum number of buffers to keep (default: 50)
    #[arg(long, env = "PGQT_BUFFER_POOL_SIZE", default_value = "50")]
    buffer_pool_size: usize,

    /// Buffer initial capacity in bytes (default: 4096)
    #[arg(long, env = "PGQT_BUFFER_INITIAL_CAPACITY", default_value = "4096")]
    buffer_initial_capacity: usize,

    /// Buffer maximum capacity in bytes (default: 65536)
    #[arg(long, env = "PGQT_BUFFER_MAX_CAPACITY", default_value = "65536")]
    buffer_max_capacity: usize,

    /// Enable automatic memory cleanup when thresholds are exceeded (default: false)
    #[arg(long, env = "PGQT_AUTO_CLEANUP")]
    auto_cleanup: bool,

    /// Enable memory monitoring (default: false)
    #[arg(long, env = "PGQT_MEMORY_MONITORING")]
    memory_monitoring: bool,

    /// Memory threshold in bytes for normal operation (default: 67108864 = 64MB)
    #[arg(long, env = "PGQT_MEMORY_THRESHOLD", default_value = "67108864")]
    memory_threshold: usize,

    /// High memory threshold for aggressive cleanup (default: 134217728 = 128MB)
    #[arg(long, env = "PGQT_HIGH_MEMORY_THRESHOLD", default_value = "134217728")]
    high_memory_threshold: usize,

    /// Memory check interval in seconds (default: 10)
    #[arg(long, env = "PGQT_MEMORY_CHECK_INTERVAL", default_value = "10")]
    memory_check_interval: u64,

    /// Enable memory-mapped I/O for large values (default: false)
    #[arg(long, env = "PGQT_ENABLE_MMAP")]
    enable_mmap: bool,

    /// Minimum size in bytes to use mmap (default: 65536)
    #[arg(long, env = "PGQT_MMAP_MIN_SIZE", default_value = "65536")]
    mmap_min_size: usize,

    /// Maximum total mmap memory in bytes (default: 1048576 = 1MB)
    #[arg(long, env = "PGQT_MMAP_MAX_MEMORY", default_value = "1048576")]
    mmap_max_memory: usize,

    /// Temporary directory for mmap files (default: system temp)
    #[arg(long, env = "PGQT_TEMP_DIR")]
    temp_dir: Option<String>,

    // Network Configuration
    /// Directory for Unix socket files (Unix only)
    #[arg(long, env = "PGQT_SOCKET_DIR")]
    socket_dir: Option<String>,

    /// Disable TCP listener (Unix socket only)
    #[arg(long, env = "PGQT_NO_TCP")]
    no_tcp: bool,

    // TLS/SSL Configuration
    /// Enable TLS/SSL encryption
    #[arg(long, env = "PGQT_SSL")]
    ssl: bool,

    /// Path to SSL certificate file (PEM format)
    #[arg(long, env = "PGQT_SSL_CERT")]
    ssl_cert: Option<String>,

    /// Path to SSL private key file (PEM format)
    #[arg(long, env = "PGQT_SSL_KEY")]
    ssl_key: Option<String>,

    /// Path to SSL CA certificate for client verification
    #[arg(long, env = "PGQT_SSL_CA")]
    ssl_ca: Option<String>,

    /// Use ephemeral (self-signed) certificate for development
    #[arg(long, env = "PGQT_SSL_EPHEMERAL")]
    ssl_ephemeral: bool,
}

impl Cli {
    #[allow(dead_code)]
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
    auto_create_users: bool,
}

impl PgWireServerHandlers for HandlerFactory {
    fn startup_handler(&self) -> Arc<impl pgwire::api::auth::StartupHandler> {
        if self.trust_mode {
            // Trust mode: accept all connections without authentication
            Arc::new(auth::FlexibleAuthHandler::new_trust())
        } else {
            // Password authentication mode
            Arc::new(auth::FlexibleAuthHandler::new_password(
                self.handler.conn.clone(),
                self.auto_create_users,
            ))
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
        // Get unique client identifier from PostgreSQL PID
        let (pid, _) = client.pid_and_secret_key();
        let client_id = pid as u32;

        // Get the current user from client metadata
        let metadata = client.metadata();
        let user = metadata.get("user").map(|s| s.to_string()).unwrap_or_else(|| "postgres".to_string());

        // Set the current user in thread-local storage for current_user() function
        crate::handler::set_current_user(&user);

        // Initialize session from client metadata if not already set
        if !self.sessions.contains_key(&client_id) {
            self.sessions.insert(client_id, SessionContext::new(user));
        } else {
            // Update the existing session with current user
            if let Some(mut session) = self.sessions.get_mut(&client_id) {
                session.current_user = user.clone();
                session.authenticated_user = user;
            }
        }
        debug!("Received query from client {}: {}", client_id, query);
        match self.execute_query(client_id, query) {
            Ok(responses) => Ok(responses),
            Err(e) => {
                eprintln!("Error executing query for client {}: {}", client_id, e);
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
        let pragma_config = SqlitePragmaConfig {
            journal_mode: cli.journal_mode,
            synchronous: cli.synchronous,
            cache_size: cli.cache_size,
            mmap_size: cli.mmap_size,
            temp_store: cli.temp_store,
        };
        let cache_config = CacheConfig {
            transpile_cache_size: cli.transpile_cache_size,
            transpile_cache_ttl: cli.transpile_cache_ttl,
            enable_result_cache: cli.enable_result_cache,
            result_cache_size: cli.result_cache_size,
            result_cache_ttl: cli.result_cache_ttl,
        };
        let pool_config = PoolConfig {
            max_connections: cli.max_connections,
            pool_size: cli.pool_size,
            connection_timeout: cli.connection_timeout,
            idle_timeout: cli.idle_timeout,
            health_check_interval: cli.health_check_interval,
            max_retries: cli.max_retries,
            use_pooling: cli.use_pooling,
        };
        let memory_config = MemoryConfig {
            buffer_pool: BufferPoolConfig {
                enabled: cli.enable_buffer_pool,
                pool_size: cli.buffer_pool_size,
                initial_capacity: cli.buffer_initial_capacity,
                max_capacity: cli.buffer_max_capacity,
            },
            monitoring: MemoryMonitoringConfig {
                enabled: cli.memory_monitoring,
                threshold: cli.memory_threshold,
                high_threshold: cli.high_memory_threshold,
                check_interval: cli.memory_check_interval,
                auto_cleanup: cli.auto_cleanup,
            },
            mmap: MmapConfig {
                enabled: cli.enable_mmap,
                min_size: cli.mmap_min_size,
                max_memory: cli.mmap_max_memory,
                temp_dir: cli.temp_dir,
            },
        };
        let network_config = NetworkConfig {
            socket_dir: cli.socket_dir,
            no_tcp: cli.no_tcp,
        };
        let tls_config = TlsConfigOptions {
            ssl: cli.ssl,
            ssl_cert: cli.ssl_cert,
            ssl_key: cli.ssl_key,
            ssl_ca: cli.ssl_ca,
            ssl_ephemeral: cli.ssl_ephemeral,
        };
        AppConfig::from_cli(
            cli.host,
            cli.port,
            cli.database,
            cli.output.to_string(),
            cli.error_output.map(|o| o.to_string()),
            cli.debug,
            cli.trust_mode,
            cli.auto_create_users,
            Some(pragma_config),
            Some(cache_config),
            Some(pool_config),
            Some(memory_config),
            Some(network_config),
            Some(tls_config),
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

#[allow(dead_code)]
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
    // Initialize TLS acceptor if enabled
    #[cfg(feature = "tls")]
    let tls_acceptor = if config.tls.ssl {
        let tls_config = if config.tls.ssl_ephemeral {
            match TlsConfig::generate_ephemeral() {
                Ok(tls) => {
                    println!("TLS enabled with ephemeral (self-signed) certificate");
                    Some(tls)
                }
                Err(e) => {
                    eprintln!("Failed to generate ephemeral TLS certificate: {}", e);
                    None
                }
            }
        } else if let (Some(cert), Some(key)) = (&config.tls.ssl_cert, &config.tls.ssl_key) {
            match TlsConfig::from_files(cert, key, config.tls.ssl_ca.as_ref()) {
                Ok(tls) => {
                    println!("TLS enabled with certificate: {}", cert);
                    Some(tls)
                }
                Err(e) => {
                    eprintln!("Failed to load TLS certificate: {}", e);
                    None
                }
            }
        } else {
            eprintln!("TLS enabled but no certificate provided. Use --ssl-cert/--ssl-key or --ssl-ephemeral");
            None
        };

        tls_config.and_then(|tls| {
            tls.server_config().map(|cfg| TlsAcceptor::from(cfg))
        })
    } else {
        None
    };

    #[cfg(not(feature = "tls"))]
    let tls_acceptor: Option<TlsAcceptor> = {
        if config.tls.ssl {
            eprintln!("Warning: TLS requested but not compiled in. Rebuild with --features tls");
        }
        None
    };

    // Create handler for this port's database
    let handler = Arc::new(SqliteHandler::with_pool_config(&config.database, Some(config.pool.clone()))?);
    let factory = Arc::new(HandlerFactory {
        handler: handler.clone(),
        trust_mode: config.trust_mode,
        auto_create_users: config.auto_create_users,
    });

    // Spawn TCP listener if not disabled
    let tcp_handle = if !config.network.no_tcp {
        let addr = format!("{}:{}", config.host, config.port);
        let listener = TcpListener::bind(&addr).await
            .with_context(|| format!("Failed to bind to {}", addr))?;
        
        println!("Server listening on {}", addr);
        println!("Using database: {}", config.database);

        let factory = factory.clone();
        let tls_acceptor = tls_acceptor.clone();

        Some(tokio::spawn(async move {
            run_tcp_listener(listener, factory, tls_acceptor).await;
        }))
    } else {
        println!("TCP listener disabled");
        None
    };

    // Spawn Unix socket listener if configured (Unix only)
    #[cfg(unix)]
    let unix_handle = if let Some(socket_dir) = &config.network.socket_dir {
        let socket_path = std::path::Path::new(socket_dir).join(format!("pgqt.{}.sock", config.port));
        
        // Remove old socket file if it exists
        let _ = tokio::fs::remove_file(&socket_path).await;
        
        // Create socket directory if it doesn't exist
        if let Some(parent) = socket_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        
        match tokio::net::UnixListener::bind(&socket_path) {
            Ok(listener) => {
                println!("Unix socket listening on {}", socket_path.display());
                
                let factory = factory.clone();
                
                Some(tokio::spawn(async move {
                    run_unix_listener(listener, factory).await;
                }))
            }
            Err(e) => {
                eprintln!("Failed to bind Unix socket at {}: {}", socket_path.display(), e);
                None
            }
        }
    } else {
        None
    };

    #[cfg(not(unix))]
    let unix_handle: Option<tokio::task::JoinHandle<()>> = None;

    // Wait for all listeners
    if let Some(handle) = tcp_handle {
        handle.await?;
    }
    if let Some(handle) = unix_handle {
        handle.await?;
    }

    Ok(())
}

/// Run TCP listener
async fn run_tcp_listener(
    listener: TcpListener,
    factory: Arc<HandlerFactory>,
    tls_acceptor: Option<TlsAcceptor>,
) {
    loop {
        match listener.accept().await {
            Ok((incoming_socket, client_addr)) => {
                debug!("New TCP connection from {}", client_addr);

                let factory = factory.clone();
                let tls_acceptor = tls_acceptor.clone();

                tokio::spawn(async move {
                    let _ = process_socket(
                        incoming_socket,
                        tls_acceptor,
                        factory,
                    ).await;
                });
            }
            Err(e) => {
                eprintln!("Failed to accept TCP connection: {}", e);
            }
        }
    }
}

/// Run Unix socket listener (Unix only)
#[cfg(unix)]
async fn run_unix_listener(
    listener: tokio::net::UnixListener,
    factory: Arc<HandlerFactory>,
) {
    loop {
        match listener.accept().await {
            Ok((incoming_socket, _client_addr)) => {
                debug!("New Unix socket connection");

                let factory = factory.clone();

                tokio::spawn(async move {
                    let _ = process_socket_unix(
                        incoming_socket,
                        factory,
                    ).await;
                });
            }
            Err(e) => {
                eprintln!("Failed to accept Unix socket connection: {}", e);
            }
        }
    }
}

#[allow(dead_code)]
/// Set up output redirection based on CLI arguments
#[cfg(unix)]
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

#[allow(dead_code)]
/// Stub for Windows - output redirection not supported
#[cfg(windows)]
fn setup_output_redirection(_output: &OutputDest, _error_output: &OutputDest) -> Result<()> {
    // Output redirection is not implemented for Windows
    // The output will go to stdout/stderr as normal
    Ok(())
}

#[allow(dead_code)]
/// Log a message to the configured output destination
fn log_output(msg: &str) {
    println!("{}", msg);
}

#[allow(dead_code)]
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

        let _ = handler.execute_query(1, "CREATE TABLE test_table (id SERIAL, name VARCHAR(10), created_at TIMESTAMP WITH TIME ZONE)");

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
