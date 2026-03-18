# Multi-Port Configuration Implementation Plan

## Overview

Add support for running multiple independent PGQT listeners on different ports, each with its own database and configuration. Use JSON-based configuration with CLI fallback.

---

## 1. New Files to Create

### 1.1 `src/config.rs` - Configuration Management Module

**Purpose**: Parse and validate JSON configuration files, merge with CLI args

```rust
// src/config.rs
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
    ) -> Self {
        Self {
            ports: vec![PortConfig {
                host,
                port,
                database,
                output,
                error_output,
                debug,
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
```

---

## 2. Files to Modify

### 2.1 `Cargo.toml` - Add Dependencies

```toml
[dependencies]
# ... existing dependencies ...

# Add these:
serde = { version = "1.0", features = ["derive"] }  # Already present, ensure derive feature
anyhow = "1.0"  # Already present
```

**Note**: `serde` with derive feature is already in your dependencies, just need to use it.

---

### 2.2 `src/main.rs` - Major Refactoring

**Changes needed**:

1. Add `mod config;` 
2. Add `--config` CLI option
3. Modify `main()` to support multiple listeners
4. Extract listener spawning into reusable function
5. Maintain backward compatibility

```rust
// Add to imports
mod config;
use config::{AppConfig, PortConfig, find_default_config};

// Modify CLI struct
#[derive(Parser, Debug)]
#[command(name = "pgqt")]
#[command(about = "A PostgreSQL wire protocol proxy for SQLite")]
struct Cli {
    /// Path to JSON configuration file
    /// If not specified, looks for pgqt.json in the executable directory
    /// If not found, uses other CLI arguments for single-port mode
    #[arg(short = 'c', long, env = "PGQT_CONFIG")]
    config: Option<PathBuf>,
    
    // ... existing args (host, port, database, etc.) marked as optional when config is used
    /// Host address to listen on (used when no config file)
    #[arg(short = 'H', long, env = "PGQT_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on (used when no config file)
    #[arg(short, long, env = "PGQT_PORT", default_value = "5432")]
    port: u16,

    /// Path to the SQLite database file (used when no config file)
    #[arg(short, long, env = "PGQT_DB", default_value = "test.db")]
    database: String,

    // ... rest of existing args
}

// Modify main() function
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
        )
    };
    
    // Spawn listeners for each configured port
    let mut handles = Vec::new();
    
    for port_config in app_config.ports {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_listener(port_config).await {
                eprintln!("Error on port {}: {}", port_config.port, e);
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

// New function: spawn a single listener
async fn run_listener(config: PortConfig) -> Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await
        .with_context(|| format!("Failed to bind to {}", addr))?;
    
    println!("Listening on {} -> database: {}", addr, config.database);
    
    // Setup output redirection for this port
    let output_dest = parse_output_dest(&config.output)?;
    let error_dest = config.error_output
        .as_ref()
        .map(|o| parse_output_dest(o))
        .transpose()?
        .unwrap_or_else(|| OutputDest::File(PathBuf::from(format!("{}.error.log", config.database))));
    
    // Create handler for this port's database
    let handler = Arc::new(SqliteHandler::new(&config.database)?);
    let factory = Arc::new(HandlerFactory { 
        handler: handler.clone(),
    });
    
    // Accept loop
    loop {
        let (incoming_socket, client_addr) = listener.accept().await?;
        let factory = factory.clone();
        
        tokio::spawn(async move {
            if let Err(e) = process_socket(incoming_socket, None, factory).await {
                eprintln!("Connection error from {}: {}", client_addr, e);
            }
        });
    }
}

// Helper to parse output destination string
fn parse_output_dest(s: &str) -> Result<OutputDest> {
    match s.to_uppercase().as_str() {
        "STDOUT" => Ok(OutputDest::Stdout),
        "STDERR" => Ok(OutputDest::Stderr),
        "NULL" | "/DEV/NULL" => Ok(OutputDest::Null),
        _ => Ok(OutputDest::File(PathBuf::from(s))),
    }
}
```

---

## 3. Example Configuration File

Create `pgqt.json.example`:

```json
{
  "ports": [
    {
      "port": 5432,
      "host": "127.0.0.1",
      "database": "/var/lib/pgqt/tenant1.db",
      "output": "stdout",
      "error_output": "/var/log/pgqt/tenant1.error.log",
      "debug": false
    },
    {
      "port": 5433,
      "host": "127.0.0.1", 
      "database": "/var/lib/pgqt/tenant2.db",
      "output": "/var/log/pgqt/tenant2.log",
      "error_output": "/var/log/pgqt/tenant2.error.log",
      "debug": false
    },
    {
      "port": 5434,
      "host": "0.0.0.0",
      "database": "/var/lib/pgqt/shared.db",
      "output": "null",
      "error_output": null,
      "debug": true
    }
  ]
}
```

---

## 4. Implementation Steps (in order)

### Phase 1: Core Configuration (30 min)
1. Create `src/config.rs` with `PortConfig` and `AppConfig` structs
2. Add JSON parsing with serde
3. Add validation logic

### Phase 2: CLI & Main Refactoring (45 min)
1. Add `--config` CLI option to `Cli` struct
2. Modify `main()` to detect and load config
3. Add `find_default_config()` helper
4. Implement `run_listener()` function
5. Add `parse_output_dest()` helper

### Phase 3: Output Redirection per Port (30 min)
1. Modify output redirection to work per-listener
2. Ensure each port can have independent log files
3. Handle the case where multiple ports write to stdout/stderr

### Phase 4: Testing (45 min)
1. Create `tests/multi_port_tests.rs` integration tests
2. Test config file loading
3. Test duplicate port detection
4. Test backward compatibility (CLI-only mode)
5. Test multiple simultaneous connections

### Phase 5: Documentation (15 min)
1. Update README.md with multi-port usage
2. Add `pgqt.json.example` to repo
3. Document config file precedence

---

## 5. Testing Strategy

### Integration Tests (`tests/multi_port_tests.rs`)

```rust
use std::process::Command;
use std::fs;
use std::time::Duration;
use tokio::net::TcpStream;

#[tokio::test]
async fn test_multi_port_config() {
    // Create temp config file
    let config = r#"
    {
      "ports": [
        {"port": 15432, "database": "/tmp/test1.db"},
        {"port": 15433, "database": "/tmp/test2.db"}
      ]
    }
    "#;
    let config_path = "/tmp/test_pgqt.json";
    fs::write(config_path, config).unwrap();
    
    // Start server with config
    let mut child = Command::new("./target/release/pgqt")
        .arg("--config").arg(config_path)
        .spawn()
        .expect("Failed to start pgqt");
    
    // Give it time to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Test both ports are listening
    assert!(TcpStream::connect("127.0.0.1:15432").await.is_ok());
    assert!(TcpStream::connect("127.0.0.1:15433").await.is_ok());
    
    // Cleanup
    let _ = child.kill();
    let _ = fs::remove_file(config_path);
    let _ = fs::remove_file("/tmp/test1.db");
    let _ = fs::remove_file("/tmp/test2.db");
}

#[test]
fn test_duplicate_port_detection() {
    let config = r#"
    {
      "ports": [
        {"port": 5432, "database": "db1.db"},
        {"port": 5432, "database": "db2.db"}
      ]
    }
    "#;
    // Should fail validation
}
```

---

## 6. Backward Compatibility

The implementation maintains 100% backward compatibility:

| Scenario | Behavior |
|----------|----------|
| No config file, CLI args provided | Works exactly as before (single port) |
| Config file via `--config` | Uses config file, ignores other CLI args |
| `pgqt.json` exists in exe dir | Auto-loads, ignores CLI args |
| Both config file and CLI args | Config file takes precedence, CLI args ignored (with warning) |

---

## 7. Binary Size Impact

| Component | Size Impact |
|-----------|-------------|
| `serde` derive macros | Already included |
| Config parsing code | ~+10 KB |
| Multi-listener spawn logic | ~+5 KB |
| **Total** | **~+15 KB** (negligible) |

---

## 8. Estimated Timeline

| Phase | Time |
|-------|------|
| Implementation | 2-3 hours |
| Testing | 1 hour |
| Documentation | 30 min |
| **Total** | **~4 hours** |

---

## 9. Potential Edge Cases to Handle

1. **Port in use**: Gracefully report which port failed
2. **Database file doesn't exist**: SQLite creates it, but parent directory must exist
3. **Permission errors**: Clear error messages per port
4. **Signal handling**: Ctrl+C should stop all listeners
5. **Mixed output**: Multiple ports writing to stdout could interleave - acceptable for logs

---

## Summary

This plan provides a clean, maintainable implementation with full backward compatibility and minimal binary size impact (~15 KB).
