//! Configuration management for PGQT multi-port support
//!
//! This module provides JSON-based configuration for running multiple
//! independent PGQT listeners on different ports, each with its own
//! database and configuration.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

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
