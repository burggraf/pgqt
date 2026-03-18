//! Configuration management for PGQT multi-port support
//!
//! This module provides JSON-based configuration for running multiple
//! independent PGQT listeners on different ports, each with its own
//! database and configuration.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

/// Memory-mapped I/O configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmapConfig {
    /// Enable memory-mapped I/O for large values (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Minimum size in bytes to use mmap (default: 65536)
    #[serde(default = "default_mmap_min_size")]
    pub min_size: usize,
    /// Maximum total mmap memory in bytes (default: 1048576 = 1MB)
    #[serde(default = "default_mmap_max_memory")]
    pub max_memory: usize,
    /// Temporary directory for mmap files (default: system temp)
    pub temp_dir: Option<String>,
}

impl Default for MmapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_size: default_mmap_min_size(),
            max_memory: default_mmap_max_memory(),
            temp_dir: None,
        }
    }
}

fn default_mmap_min_size() -> usize {
    65536 // 64KB
}

fn default_mmap_max_memory() -> usize {
    1048576 // 1MB
}

/// Buffer pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferPoolConfig {
    /// Enable buffer pool (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Maximum number of buffers to keep in the pool (default: 50)
    #[serde(default = "default_buffer_pool_size")]
    pub pool_size: usize,
    /// Initial capacity for new buffers (default: 4096)
    #[serde(default = "default_buffer_initial_capacity")]
    pub initial_capacity: usize,
    /// Maximum capacity for buffers (default: 65536)
    #[serde(default = "default_buffer_max_capacity")]
    pub max_capacity: usize,
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pool_size: default_buffer_pool_size(),
            initial_capacity: default_buffer_initial_capacity(),
            max_capacity: default_buffer_max_capacity(),
        }
    }
}

fn default_buffer_pool_size() -> usize {
    50
}

fn default_buffer_initial_capacity() -> usize {
    4096
}

fn default_buffer_max_capacity() -> usize {
    65536
}

/// Memory monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMonitoringConfig {
    /// Enable memory monitoring (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Memory threshold in bytes for normal operation (default: 67108864 = 64MB)
    #[serde(default = "default_memory_threshold")]
    pub threshold: usize,
    /// High memory threshold for aggressive cleanup (default: 134217728 = 128MB)
    #[serde(default = "default_high_memory_threshold")]
    pub high_threshold: usize,
    /// Check interval in seconds (default: 10)
    #[serde(default = "default_memory_check_interval")]
    pub check_interval: u64,
    /// Enable automatic cleanup when thresholds are exceeded (default: false)
    #[serde(default)]
    pub auto_cleanup: bool,
}

impl Default for MemoryMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: default_memory_threshold(),
            high_threshold: default_high_memory_threshold(),
            check_interval: default_memory_check_interval(),
            auto_cleanup: false,
        }
    }
}

fn default_memory_threshold() -> usize {
    67108864 // 64MB
}

fn default_high_memory_threshold() -> usize {
    134217728 // 128MB
}

fn default_memory_check_interval() -> u64 {
    10
}

/// Combined memory configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    /// Buffer pool configuration
    #[serde(default)]
    pub buffer_pool: BufferPoolConfig,
    /// Memory monitoring configuration
    #[serde(default)]
    pub monitoring: MemoryMonitoringConfig,
    /// Memory-mapped I/O configuration
    #[serde(default)]
    pub mmap: MmapConfig,
}

/// SQLite journal mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JournalMode {
    Delete,
    Truncate,
    Persist,
    Memory,
    Wal,
    Off,
}

impl Default for JournalMode {
    fn default() -> Self {
        JournalMode::Wal
    }
}

impl std::fmt::Display for JournalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalMode::Delete => write!(f, "DELETE"),
            JournalMode::Truncate => write!(f, "TRUNCATE"),
            JournalMode::Persist => write!(f, "PERSIST"),
            JournalMode::Memory => write!(f, "MEMORY"),
            JournalMode::Wal => write!(f, "WAL"),
            JournalMode::Off => write!(f, "OFF"),
        }
    }
}

impl std::str::FromStr for JournalMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "delete" => Ok(JournalMode::Delete),
            "truncate" => Ok(JournalMode::Truncate),
            "persist" => Ok(JournalMode::Persist),
            "memory" => Ok(JournalMode::Memory),
            "wal" => Ok(JournalMode::Wal),
            "off" => Ok(JournalMode::Off),
            _ => Err(format!("Invalid journal mode: {}", s)),
        }
    }
}

/// SQLite synchronous mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SynchronousMode {
    Off,
    Normal,
    Full,
    Extra,
}

impl Default for SynchronousMode {
    fn default() -> Self {
        SynchronousMode::Normal
    }
}

impl std::fmt::Display for SynchronousMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SynchronousMode::Off => write!(f, "0"),
            SynchronousMode::Normal => write!(f, "1"),
            SynchronousMode::Full => write!(f, "2"),
            SynchronousMode::Extra => write!(f, "3"),
        }
    }
}

impl std::str::FromStr for SynchronousMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" => Ok(SynchronousMode::Off),
            "normal" => Ok(SynchronousMode::Normal),
            "full" => Ok(SynchronousMode::Full),
            "extra" => Ok(SynchronousMode::Extra),
            _ => Err(format!("Invalid synchronous mode: {}", s)),
        }
    }
}

/// SQLite temp store mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TempStore {
    Default,
    File,
    Memory,
}

impl Default for TempStore {
    fn default() -> Self {
        TempStore::Default
    }
}

impl std::fmt::Display for TempStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TempStore::Default => write!(f, "0"),
            TempStore::File => write!(f, "1"),
            TempStore::Memory => write!(f, "2"),
        }
    }
}

impl std::str::FromStr for TempStore {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(TempStore::Default),
            "file" => Ok(TempStore::File),
            "memory" => Ok(TempStore::Memory),
            _ => Err(format!("Invalid temp store mode: {}", s)),
        }
    }
}

/// SQLite PRAGMA configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlitePragmaConfig {
    /// Journal mode: delete, truncate, persist, memory, wal (default: wal)
    #[serde(default)]
    pub journal_mode: JournalMode,

    /// Synchronous mode: off, normal, full, extra (default: normal)
    #[serde(default)]
    pub synchronous: SynchronousMode,

    /// Cache size in pages (default: -2000, meaning 2000KB)
    #[serde(default = "default_cache_size")]
    pub cache_size: i32,

    /// Memory-mapped I/O size in bytes (default: 0, disabled)
    #[serde(default)]
    pub mmap_size: i64,

    /// Temp store mode: default, file, memory (default: default)
    #[serde(default)]
    pub temp_store: TempStore,
}

impl Default for SqlitePragmaConfig {
    fn default() -> Self {
        Self {
            journal_mode: JournalMode::default(),
            synchronous: SynchronousMode::default(),
            cache_size: default_cache_size(),
            mmap_size: 0,
            temp_store: TempStore::default(),
        }
    }
}

fn default_cache_size() -> i32 {
    -2000 // 2000KB (negative means KB, positive means pages)
}

/// Cache configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Transpile cache size (number of entries, default: 256)
    #[serde(default = "default_transpile_cache_size")]
    pub transpile_cache_size: usize,

    /// Transpile cache TTL in seconds (default: 0, meaning no TTL)
    #[serde(default)]
    pub transpile_cache_ttl: u64,

    /// Enable query result caching (default: false)
    #[serde(default)]
    pub enable_result_cache: bool,

    /// Query result cache size (number of entries, default: 64)
    #[serde(default = "default_result_cache_size")]
    pub result_cache_size: usize,

    /// Query result cache TTL in seconds (default: 60)
    #[serde(default = "default_result_cache_ttl")]
    pub result_cache_ttl: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            transpile_cache_size: default_transpile_cache_size(),
            transpile_cache_ttl: 0,
            enable_result_cache: false,
            result_cache_size: default_result_cache_size(),
            result_cache_ttl: default_result_cache_ttl(),
        }
    }
}

fn default_transpile_cache_size() -> usize {
    256
}

fn default_result_cache_size() -> usize {
    64
}

fn default_result_cache_ttl() -> u64 {
    60
}

/// Connection pool configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Maximum number of connections allowed (default: 100)
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Initial pool size - number of connections to create upfront (default: 8)
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,

    /// Connection timeout in seconds when checking out (default: 30)
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,

    /// Idle timeout in seconds before closing unused connections (default: 300)
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u64,

    /// Health check interval in seconds (default: 60)
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: u64,

    /// Maximum number of retries for failed checkouts (default: 3)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Enable connection pooling (default: false for backward compatibility)
    #[serde(default)]
    pub use_pooling: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            pool_size: default_pool_size(),
            connection_timeout: default_connection_timeout(),
            idle_timeout: default_idle_timeout(),
            health_check_interval: default_health_check_interval(),
            max_retries: default_max_retries(),
            use_pooling: false,
        }
    }
}

fn default_max_connections() -> usize {
    100
}

fn default_pool_size() -> usize {
    8
}

fn default_connection_timeout() -> u64 {
    30
}

fn default_idle_timeout() -> u64 {
    300
}

fn default_health_check_interval() -> u64 {
    60
}

fn default_max_retries() -> u32 {
    3
}

/// Network configuration options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    /// Directory for Unix socket files (Unix only)
    pub socket_dir: Option<String>,
    /// Disable TCP listener (Unix socket only)
    #[serde(default)]
    pub no_tcp: bool,
}

/// TLS/SSL configuration options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsConfigOptions {
    /// Enable TLS/SSL
    #[serde(default)]
    pub ssl: bool,
    /// Path to SSL certificate file
    pub ssl_cert: Option<String>,
    /// Path to SSL private key file
    pub ssl_key: Option<String>,
    /// Path to SSL CA certificate for client verification
    pub ssl_ca: Option<String>,
    /// Use ephemeral (self-signed) certificate
    #[serde(default)]
    pub ssl_ephemeral: bool,
}

/// Configuration for a single port/database instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConfig {
    /// Port to listen on (required)
    pub port: u16,

    /// Host address to bind to (default: 127.0.0.1)
    #[serde(default = "default_host")]
    pub host: String,

    /// Path to SQLite database file (required)
    pub database: String,

    /// Output destination: "stdout", "stderr", "null", or file path
    #[serde(default = "default_output")]
    pub output: String,

    /// Error output destination (default: <database>.error.log)
    pub error_output: Option<String>,

    /// Enable debug mode
    #[serde(default)]
    pub debug: bool,

    /// Disable password authentication (trust mode)
    #[serde(default)]
    pub trust_mode: bool,

    /// Auto-create users that don't exist (default: false for security)
    #[serde(default)]
    pub auto_create_users: bool,

    /// SQLite PRAGMA configuration
    #[serde(default)]
    pub pragma: SqlitePragmaConfig,

    /// Cache configuration
    #[serde(default)]
    pub cache: CacheConfig,

    /// Connection pool configuration
    #[serde(default)]
    pub pool: PoolConfig,

    /// Memory management configuration
    #[serde(default)]
    pub memory: MemoryConfig,

    /// Network configuration
    #[serde(default)]
    pub network: NetworkConfig,

    /// TLS/SSL configuration
    #[serde(default)]
    pub tls: TlsConfigOptions,
}

/// Top-level configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// List of port configurations
    pub ports: Vec<PortConfig>,
}

impl PortConfig {
    /// Validate that all required fields are present and valid
    pub fn validate(&self) -> Result<()> {
        if self.port == 0 {
            anyhow::bail!("Port must be specified and non-zero");
        }
        if self.database.is_empty() {
            anyhow::bail!("Database path must be specified");
        }
        Ok(())
    }
}

impl AppConfig {
    /// Load configuration from a JSON file
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: AppConfig = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse JSON in config file: {}", path.display()))?;

        // Validate all port configs
        for (idx, port_config) in config.ports.iter().enumerate() {
            port_config.validate()
                .with_context(|| format!("Invalid configuration for port entry {}", idx))?;
        }

        // Check for duplicate ports
        let mut seen_ports = std::collections::HashSet::new();
        for port_config in &config.ports {
            if !seen_ports.insert(port_config.port) {
                anyhow::bail!("Duplicate port {} in configuration", port_config.port);
            }
        }

        Ok(config)
    }

    /// Create a single-port config from CLI arguments (backward compatibility)
    pub fn from_cli(
        host: String,
        port: u16,
        database: String,
        output: String,
        error_output: Option<String>,
        debug: bool,
        trust_mode: bool,
        auto_create_users: bool,
        pragma: Option<SqlitePragmaConfig>,
        cache: Option<CacheConfig>,
        pool: Option<PoolConfig>,
        memory: Option<MemoryConfig>,
        network: Option<NetworkConfig>,
        tls: Option<TlsConfigOptions>,
    ) -> Self {
        Self {
            ports: vec![PortConfig {
                host,
                port,
                database,
                output,
                error_output,
                debug,
                trust_mode,
                auto_create_users,
                pragma: pragma.unwrap_or_default(),
                cache: cache.unwrap_or_default(),
                pool: pool.unwrap_or_default(),
                memory: memory.unwrap_or_default(),
                network: network.unwrap_or_default(),
                tls: tls.unwrap_or_default(),
            }],
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_output() -> String {
    "stdout".to_string()
}

/// Find default config file in executable directory
pub fn find_default_config() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe_path| exe_path.parent().map(|p| p.join("pgqt.json")))
        .filter(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_port_config_validation() {
        let valid_config = PortConfig {
            port: 5432,
            host: "127.0.0.1".to_string(),
            database: "test.db".to_string(),
            output: "stdout".to_string(),
            error_output: None,
            debug: false,
            trust_mode: false,
            auto_create_users: false,
            pragma: SqlitePragmaConfig::default(),
            cache: CacheConfig::default(),
            pool: PoolConfig::default(),
            memory: MemoryConfig::default(),
            network: NetworkConfig::default(),
            tls: TlsConfigOptions::default(),
        };
        assert!(valid_config.validate().is_ok());

        let invalid_port = PortConfig {
            port: 0,
            host: "127.0.0.1".to_string(),
            database: "test.db".to_string(),
            output: "stdout".to_string(),
            error_output: None,
            debug: false,
            trust_mode: false,
            auto_create_users: false,
            pragma: SqlitePragmaConfig::default(),
            cache: CacheConfig::default(),
            pool: PoolConfig::default(),
            memory: MemoryConfig::default(),
            network: NetworkConfig::default(),
            tls: TlsConfigOptions::default(),
        };
        assert!(invalid_port.validate().is_err());

        let invalid_db = PortConfig {
            port: 5432,
            host: "127.0.0.1".to_string(),
            database: "".to_string(),
            output: "stdout".to_string(),
            error_output: None,
            debug: false,
            trust_mode: false,
            auto_create_users: false,
            pragma: SqlitePragmaConfig::default(),
            cache: CacheConfig::default(),
            pool: PoolConfig::default(),
            memory: MemoryConfig::default(),
            network: NetworkConfig::default(),
            tls: TlsConfigOptions::default(),
        };
        assert!(invalid_db.validate().is_err());
    }

    #[test]
    fn test_app_config_from_file() {
        let json = r#"{
            "ports": [
                {
                    "port": 5432,
                    "host": "127.0.0.1",
                    "database": "/var/lib/pgqt/tenant1.db",
                    "output": "stdout",
                    "error_output": "/var/log/pgqt/tenant1.error.log",
                    "debug": false,
                    "trust_mode": false
                },
                {
                    "port": 5433,
                    "database": "/var/lib/pgqt/tenant2.db"
                }
            ]
        }"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let config = AppConfig::from_file(&temp_file.path().to_path_buf()).unwrap();
        assert_eq!(config.ports.len(), 2);
        assert_eq!(config.ports[0].port, 5432);
        assert_eq!(config.ports[0].host, "127.0.0.1");
        assert_eq!(config.ports[1].port, 5433);
        assert_eq!(config.ports[1].host, "127.0.0.1"); // default
        assert_eq!(config.ports[1].output, "stdout"); // default
    }

    #[test]
    fn test_duplicate_port_detection() {
        let json = r#"{
            "ports": [
                {"port": 5432, "database": "db1.db"},
                {"port": 5432, "database": "db2.db"}
            ]
        }"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let result = AppConfig::from_file(&temp_file.path().to_path_buf());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate port"));
    }

    #[test]
    fn test_app_config_from_cli() {
        let config = AppConfig::from_cli(
            "127.0.0.1".to_string(),
            5432,
            "test.db".to_string(),
            "stdout".to_string(),
            None,
            false,
            false,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert_eq!(config.ports.len(), 1);
        assert_eq!(config.ports[0].port, 5432);
        assert_eq!(config.ports[0].host, "127.0.0.1");
        assert_eq!(config.ports[0].database, "test.db");
    }

    #[test]
    fn test_default_values() {
        let json = r#"{"ports": [{"port": 5432, "database": "test.db"}]}"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();

        let config = AppConfig::from_file(&temp_file.path().to_path_buf()).unwrap();
        assert_eq!(config.ports[0].host, "127.0.0.1");
        assert_eq!(config.ports[0].output, "stdout");
        assert!(!config.ports[0].debug);
        assert!(!config.ports[0].trust_mode);
    }
}
