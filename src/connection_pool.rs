//! Connection pool for per-session SQLite connections
//!
//! This module provides a connection pool that allows each PostgreSQL session
//! to have its own isolated SQLite connection with proper transaction support.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use anyhow::{anyhow, Result};
use rusqlite::Connection;

use crate::config::PoolConfig;
use crate::config::SqlitePragmaConfig;

/// A handle representing a checked-out connection
/// When dropped, the connection is returned to the pool
pub struct ConnectionHandle {
    client_id: u32,
    #[allow(dead_code)]
    pool: Weak<Mutex<Vec<Arc<Mutex<Connection>>>>>,
    in_use: Weak<Mutex<HashSet<u32>>>,
}

impl std::fmt::Debug for ConnectionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionHandle")
            .field("client_id", &self.client_id)
            .finish()
    }
}

impl ConnectionHandle {
    fn new(
        client_id: u32,
        pool: Weak<Mutex<Vec<Arc<Mutex<Connection>>>>>,
        in_use: Weak<Mutex<HashSet<u32>>>,
    ) -> Self {
        Self {
            client_id,
            pool,
            in_use,
        }
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        if let (Some(_pool), Some(in_use)) = (self.pool.upgrade(), self.in_use.upgrade()) {
            // Mark client_id as no longer having a connection
            if let Ok(mut in_use) = in_use.lock() {
                in_use.remove(&self.client_id);
            }
        }
    }
}

/// A pool of SQLite connections for session management
pub struct ConnectionPool {
    db_path: PathBuf,
    available: Arc<Mutex<Vec<Arc<Mutex<Connection>>>>>,
    in_use: Arc<Mutex<HashSet<u32>>>,
    max_connections: usize,
    /// Function to initialize new connections (e.g. register UDFs)
    on_init: Option<Arc<dyn Fn(&Connection) -> Result<()> + Send + Sync>>,
}

impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            db_path: self.db_path.clone(),
            available: self.available.clone(),
            in_use: self.in_use.clone(),
            max_connections: self.max_connections,
            on_init: self.on_init.clone(),
        }
    }
}

impl ConnectionPool {
    /// Create a new connection pool
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `max_connections` - Maximum number of connections in the pool
    #[allow(dead_code)]
    pub fn new(db_path: &Path, max_connections: usize) -> Result<Self> {
        Self::with_init(db_path, max_connections, None)
    }

    /// Create a new connection pool with an initialization function
    pub fn with_init(db_path: &Path, max_connections: usize, on_init: Option<Arc<dyn Fn(&Connection) -> Result<()> + Send + Sync>>) -> Result<Self> {
        if max_connections == 0 {
            return Err(anyhow!("max_connections must be greater than 0"));
        }

        let pool = Self {
            db_path: db_path.to_path_buf(),
            available: Arc::new(Mutex::new(Vec::with_capacity(max_connections))),
            in_use: Arc::new(Mutex::new(HashSet::new())),
            max_connections,
            on_init,
        };

        // Pre-initialize one connection to verify database is accessible
        let initial_conn = pool.create_connection()?;
        pool.available.lock().unwrap().push(Arc::new(Mutex::new(initial_conn)));

        Ok(pool)
    }

    /// Create a new connection pool with full configuration
    pub fn with_config(
        db_path: &Path,
        config: PoolConfig,
        on_init: Option<Arc<dyn Fn(&Connection) -> Result<()> + Send + Sync>>,
        _pragma_config: SqlitePragmaConfig,
    ) -> Result<Self> {
        if config.max_connections == 0 {
            return Err(anyhow!("max_connections must be greater than 0"));
        }

        let pool = Self {
            db_path: db_path.to_path_buf(),
            available: Arc::new(Mutex::new(Vec::with_capacity(config.max_connections))),
            in_use: Arc::new(Mutex::new(HashSet::new())),
            max_connections: config.max_connections,
            on_init,
        };

        // Pre-initialize connections up to pool_size
        let initial_size = config.pool_size.min(config.max_connections);
        for _ in 0..initial_size {
            match pool.create_connection() {
                Ok(conn) => {
                    pool.available.lock().unwrap().push(Arc::new(Mutex::new(conn)));
                }
                Err(e) => {
                    eprintln!("Warning: Failed to create initial connection: {}", e);
                    break;
                }
            }
        }

        Ok(pool)
    }

    /// Check out a connection for a client
    ///
    /// Returns an Arc<Mutex<Connection>> that can be stored and shared.
    /// The ConnectionHandle manages the lifecycle and marks the connection as returned when dropped.
    pub fn checkout(&self, client_id: u32) -> Result<(Arc<Mutex<Connection>>, ConnectionHandle)> {
        // Check if client already has a connection
        {
            let in_use = self.in_use.lock().unwrap();
            if in_use.contains(&client_id) {
                return Err(anyhow!(
                    "Client {} already has a checked-out connection",
                    client_id
                ));
            }
        }

        // Try to get an available connection
        let conn = {
            let mut available = self.available.lock().unwrap();
            available.pop()
        };

        let conn = match conn {
            Some(conn) => conn,
            None => {
                // Check if we can create a new connection
                let current_count = {
                    let in_use = self.in_use.lock().unwrap();
                    let available = self.available.lock().unwrap();
                    in_use.len() + available.len()
                };

                if current_count < self.max_connections {
                    let new_conn = self.create_connection()?;
                    Arc::new(Mutex::new(new_conn))
                } else {
                    return Err(anyhow!(
                        "Connection pool exhausted (max: {})",
                        self.max_connections
                    ));
                }
            }
        };

        // Mark client as having a connection
        {
            let mut in_use = self.in_use.lock().unwrap();
            in_use.insert(client_id);
        }

        let handle = ConnectionHandle::new(
            client_id,
            Arc::downgrade(&self.available),
            Arc::downgrade(&self.in_use),
        );

        Ok((conn, handle))
    }

    /// Return a connection to the pool
    /// Called when a client disconnects or we're done with the connection
    #[allow(dead_code)]
    pub fn return_connection(&self, conn: Arc<Mutex<Connection>>) {
        // Rollback any active transaction before returning to pool
        if let Ok(guard) = conn.lock() {
            let _ = guard.execute_batch("ROLLBACK");
        }
        
        let mut available = self.available.lock().unwrap();
        available.push(conn);
    }

    /// Check if a client has a checked-out connection
    #[allow(dead_code)]
    pub fn has_connection(&self, client_id: u32) -> bool {
        let in_use = self.in_use.lock().unwrap();
        in_use.contains(&client_id)
    }

    /// Get the number of available connections in the pool
    #[allow(dead_code)]
    pub fn available_count(&self) -> usize {
        self.available.lock().unwrap().len()
    }

    /// Get the number of connections currently checked out
    #[allow(dead_code)]
    pub fn in_use_count(&self) -> usize {
        self.in_use.lock().unwrap().len()
    }

    /// Create a new SQLite connection with proper configuration
    fn create_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;

        // Configure WAL mode for better concurrency
        // WAL mode allows multiple readers and one writer concurrently
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .unwrap_or_else(|_| "delete".to_string());
        
        if journal_mode.to_lowercase() != "wal" {
            eprintln!("Warning: Could not enable WAL mode, journal mode is: {}", journal_mode);
        }

        // Set busy timeout to 5 seconds
        // This causes SQLite to retry busy locks for 5 seconds before returning SQLITE_BUSY
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys=ON", [])?;

        // Call initialization function if provided
        if let Some(on_init) = &self.on_init {
            on_init(&conn)?;
        }

        Ok(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_db_path() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        (temp_dir, db_path)
    }

    #[test]
    fn test_pool_creation() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();
        assert_eq!(pool.available_count(), 1);
        assert_eq!(pool.in_use_count(), 0);
    }

    #[test]
    fn test_checkout_and_drop() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        // Checkout a connection
        let (conn, _handle) = pool.checkout(1).unwrap();
        assert_eq!(pool.in_use_count(), 1);
        assert_eq!(pool.available_count(), 0);

        // Can execute queries through the Arc<Mutex<>>
        {
            let guard = conn.lock().unwrap();
            guard.execute("CREATE TABLE test (id INT)", []).unwrap();
        }

        // Return connection to pool
        drop(_handle);
        pool.return_connection(conn);

        // Connection returned to pool
        assert_eq!(pool.in_use_count(), 0);
        assert_eq!(pool.available_count(), 1);
    }

    #[test]
    fn test_duplicate_checkout_fails() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        let (_conn, _handle) = pool.checkout(1).unwrap();
        let result = pool.checkout(1);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already has a checked-out connection"));
    }

    #[test]
    fn test_pool_exhaustion() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 2).unwrap();

        // Checkout both connections
        let (_conn1, _handle1) = pool.checkout(1).unwrap();
        let (_conn2, _handle2) = pool.checkout(2).unwrap();

        // Third checkout should fail
        let result = pool.checkout(3);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Connection pool exhausted"));
    }

    #[test]
    fn test_has_connection() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        assert!(!pool.has_connection(1));
        let (_conn, _handle) = pool.checkout(1).unwrap();
        assert!(pool.has_connection(1));
    }

    #[test]
    fn test_connection_isolation() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        // Client 1 creates a table
        {
            let (conn, handle) = pool.checkout(1).unwrap();
            {
                let guard = conn.lock().unwrap();
                guard.execute("CREATE TABLE test (id INT)", []).unwrap();
                guard.execute("INSERT INTO test VALUES (1)", []).unwrap();
            }
            drop(handle);
            pool.return_connection(conn);
        }

        // Client 2 should see the table (same database)
        {
            let (conn, _handle) = pool.checkout(2).unwrap();
            let guard = conn.lock().unwrap();
            let count: i64 = guard
                .query_row("SELECT COUNT(*) FROM test", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn test_transaction_rollback_on_return() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        // Create table with client 1
        {
            let (conn, handle) = pool.checkout(1).unwrap();
            {
                let guard = conn.lock().unwrap();
                guard.execute("CREATE TABLE rollback_test (id INT)", []).unwrap();
                guard.execute("INSERT INTO rollback_test VALUES (1)", []).unwrap();
            }
            drop(handle);
            pool.return_connection(conn);
        }

        // Start transaction with client 2, insert, then return without commit
        {
            let (conn, handle) = pool.checkout(2).unwrap();
            {
                let guard = conn.lock().unwrap();
                guard.execute("BEGIN", []).unwrap();
                guard.execute("INSERT INTO rollback_test VALUES (2)", []).unwrap();
            }
            // Return connection - should rollback
            drop(handle);
            pool.return_connection(conn);
        }

        // Check that row 2 was not committed
        {
            let (conn, _handle) = pool.checkout(3).unwrap();
            let guard = conn.lock().unwrap();
            let count: i64 = guard
                .query_row("SELECT COUNT(*) FROM rollback_test", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1); // Only row 1 should exist
        }
    }
}
