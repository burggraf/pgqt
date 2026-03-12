//! Connection pool for per-session SQLite connections
//!
//! This module provides a connection pool that allows each PostgreSQL session
//! to have its own isolated SQLite connection with proper transaction support.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use anyhow::{anyhow, Result};
use rusqlite::Connection;

/// A pooled SQLite connection that returns to the pool when dropped
pub struct PooledConnection {
    conn: Option<Connection>,
    pool: Weak<Mutex<Vec<Connection>>>,
    in_use: Weak<Mutex<HashSet<u32>>>,
    client_id: u32,
}

impl PooledConnection {
    /// Create a new pooled connection wrapper
    fn new(
        conn: Connection,
        pool: Weak<Mutex<Vec<Connection>>>,
        in_use: Weak<Mutex<HashSet<u32>>>,
        client_id: u32,
    ) -> Self {
        Self {
            conn: Some(conn),
            pool,
            in_use,
            client_id,
        }
    }

    /// Get a reference to the underlying connection
    pub fn get(&self) -> Result<&Connection> {
        self.conn
            .as_ref()
            .ok_or_else(|| anyhow!("Connection already returned to pool"))
    }

    /// Get a mutable reference to the underlying connection
    pub fn get_mut(&mut self) -> Result<&mut Connection> {
        self.conn
            .as_mut()
            .ok_or_else(|| anyhow!("Connection already returned to pool"))
    }

    /// Execute a function with mutable access to the connection
    pub fn with_mut<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R>,
    {
        match self.conn.as_mut() {
            Some(conn) => f(conn),
            None => Err(anyhow!("Connection already returned to pool")),
        }
    }

    /// Execute a function with immutable access to the connection
    pub fn with<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> Result<R>,
    {
        match self.conn.as_ref() {
            Some(conn) => f(conn),
            None => Err(anyhow!("Connection already returned to pool")),
        }
    }
}

impl std::fmt::Debug for PooledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PooledConnection")
            .field("client_id", &self.client_id)
            .field("has_connection", &self.conn.is_some())
            .finish()
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let (Some(conn), Some(pool), Some(in_use)) =
            (self.conn.take(), self.pool.upgrade(), self.in_use.upgrade())
        {
            // Rollback any active transaction before returning to pool
            // Ignore errors since connection might be in a bad state
            let _ = conn.execute_batch("ROLLBACK");

            // Return connection to pool
            if let Ok(mut pool) = pool.lock() {
                pool.push(conn);
            }

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
    available: Arc<Mutex<Vec<Connection>>>,
    in_use: Arc<Mutex<HashSet<u32>>>,
    max_connections: usize,
}

impl ConnectionPool {
    /// Create a new connection pool
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `max_connections` - Maximum number of connections in the pool
    pub fn new(db_path: &Path, max_connections: usize) -> Result<Self> {
        if max_connections == 0 {
            return Err(anyhow!("max_connections must be greater than 0"));
        }

        let pool = Self {
            db_path: db_path.to_path_buf(),
            available: Arc::new(Mutex::new(Vec::with_capacity(max_connections))),
            in_use: Arc::new(Mutex::new(HashSet::new())),
            max_connections,
        };

        // Pre-initialize one connection to verify database is accessible
        let initial_conn = pool.create_connection()?;
        pool.available.lock().unwrap().push(initial_conn);

        Ok(pool)
    }

    /// Check out a connection for a client
    ///
    /// If the client already has a connection, returns an error.
    /// If the pool is exhausted, creates a new connection up to max_connections.
    pub fn checkout(&self, client_id: u32) -> Result<PooledConnection> {
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
                    self.create_connection()?
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

        Ok(PooledConnection::new(
            conn,
            Arc::downgrade(&self.available),
            Arc::downgrade(&self.in_use),
            client_id,
        ))
    }

    /// Check if a client has a checked-out connection
    pub fn has_connection(&self, client_id: u32) -> bool {
        let in_use = self.in_use.lock().unwrap();
        in_use.contains(&client_id)
    }

    /// Get the number of available connections in the pool
    pub fn available_count(&self) -> usize {
        self.available.lock().unwrap().len()
    }

    /// Get the number of connections currently checked out
    pub fn in_use_count(&self) -> usize {
        self.in_use.lock().unwrap().len()
    }

    /// Create a new SQLite connection with proper configuration
    fn create_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;

        // Configure WAL mode for better concurrency
        // PRAGMA journal_mode returns rows, so use query_row
        let _journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap_or_else(|_| "delete".to_string());

        // Set busy timeout to 5 seconds (doesn't return rows)
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys=ON", [])?;

        Ok(conn)
    }
}

impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            db_path: self.db_path.clone(),
            available: Arc::clone(&self.available),
            in_use: Arc::clone(&self.in_use),
            max_connections: self.max_connections,
        }
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
        {
            let mut conn = pool.checkout(1).unwrap();
            assert_eq!(pool.in_use_count(), 1);
            assert_eq!(pool.available_count(), 0);

            // Can execute queries using with_mut
            conn.with_mut(|c| c.execute("CREATE TABLE test (id INT)", []).map_err(|e| anyhow::anyhow!("{e}"))).unwrap();
        }

        // Connection returned to pool
        assert_eq!(pool.in_use_count(), 0);
        assert_eq!(pool.available_count(), 1);
    }

    #[test]
    fn test_duplicate_checkout_fails() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        let _conn = pool.checkout(1).unwrap();
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
        let _conn1 = pool.checkout(1).unwrap();
        let _conn2 = pool.checkout(2).unwrap();

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
        let _conn = pool.checkout(1).unwrap();
        assert!(pool.has_connection(1));
    }

    #[test]
    fn test_connection_isolation() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        // Client 1 creates a table
        {
            let mut conn = pool.checkout(1).unwrap();
            conn.with_mut(|c| c.execute("CREATE TABLE test (id INT)", []).map_err(|e| anyhow::anyhow!("{e}"))).unwrap();
            conn.with_mut(|c| c.execute("INSERT INTO test VALUES (1)", []).map_err(|e| anyhow::anyhow!("{e}"))).unwrap();
        }

        // Client 2 should see the table (same database)
        {
            let conn = pool.checkout(2).unwrap();
            let count: i64 = conn
                .with(|c| {
                    c.query_row("SELECT COUNT(*) FROM test", [], |row| row.get(0))
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
                .unwrap();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn test_transaction_rollback_on_drop() {
        let (_temp_dir, db_path) = temp_db_path();
        let pool = ConnectionPool::new(&db_path, 5).unwrap();

        // Create table with client 1
        {
            let mut conn = pool.checkout(1).unwrap();
            conn.with_mut(|c| c.execute("CREATE TABLE rollback_test (id INT)", []).map_err(|e| anyhow::anyhow!("{e}")))
                .unwrap();
            conn.with_mut(|c| c.execute("INSERT INTO rollback_test VALUES (1)", []).map_err(|e| anyhow::anyhow!("{e}")))
                .unwrap();
        }

        // Start transaction with client 2, insert, then drop without commit
        {
            let mut conn = pool.checkout(2).unwrap();
            conn.with_mut(|c| c.execute("BEGIN", []).map_err(|e| anyhow::anyhow!("{e}"))).unwrap();
            conn.with_mut(|c| c.execute("INSERT INTO rollback_test VALUES (2)", []).map_err(|e| anyhow::anyhow!("{e}")))
                .unwrap();
            // Connection dropped here, should rollback
        }

        // Check that row 2 was not committed
        {
            let conn = pool.checkout(3).unwrap();
            let count: i64 = conn
                .with(|c| {
                    c.query_row("SELECT COUNT(*) FROM rollback_test", [], |row| row.get(0))
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
                .unwrap();
            assert_eq!(count, 1); // Only row 1 should exist
        }
    }
}
