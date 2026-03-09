# Password Authentication Implementation Plan (Option B)

## Overview

Implement full PostgreSQL-compatible password authentication by validating credentials against the `rolpassword` column in `__pg_authid__`. This provides standard PostgreSQL behavior where:
- Users can be created with passwords via `CREATE USER`
- Passwords can be changed via `ALTER USER`
- Connections are validated against stored passwords during startup

---

## Current State

```rust
// src/main.rs - Currently accepts ALL connections
fn startup_handler(&self) -> Arc<impl pgwire::api::auth::StartupHandler> {
    Arc::new(pgwire::api::NoopHandler)  // No validation
}
```

The `__pg_authid__` table already has a `rolpassword TEXT` column, but it's unused.

---

## Implementation

### Phase 1: Password Authentication Handler

**New file: `src/auth.rs`**

```rust
use std::sync::Arc;
use async_trait::async_trait;
use pgwire::api::auth::{StartupHandler, Password, AuthContext};
use pgwire::api::ClientInfo;
use pgwire::error::{PgWireResult, PgWireError};
use rusqlite::Connection;

/// PostgreSQL-compatible password authenticator
pub struct PasswordAuthHandler {
    conn: Arc<std::sync::Mutex<Connection>>,
}

impl PasswordAuthHandler {
    pub fn new(conn: Arc<std::sync::Mutex<Connection>>) -> Self {
        Self { conn }
    }
    
    /// Verify username/password against __pg_authid__
    fn verify_credentials(&self, user: &str, password: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        
        // Look up user and password hash
        let result: Option<String> = conn.query_row(
            "SELECT rolpassword FROM __pg_authid__ WHERE rolname = ?1 AND rolcanlogin = 1",
            [user],
            |row| row.get(0)
        ).optional()?;
        
        match result {
            Some(stored_hash) => {
                // If no password set (NULL or empty), allow connection
                if stored_hash.is_empty() {
                    return Ok(true);
                }
                // Verify password using PostgreSQL-compatible MD5 or SCRAM
                Ok(verify_password(password, &stored_hash))
            }
            None => {
                // User doesn't exist - auto-create with no password for backward compatibility
                // This maintains existing behavior for new users
                conn.execute(
                    "INSERT INTO __pg_authid__ (rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin) 
                     VALUES (?1, 1, 1, 1, 1, 1)",
                    [user]
                )?;
                Ok(true)
            }
        }
    }
}

#[async_trait]
impl StartupHandler for PasswordAuthHandler {
    async fn on_startup<C>(&self, _client: &mut C, message: &pgwire::messages::startup::StartupMessage) -> PgWireResult<()>
    where C: ClientInfo + Send + Unpin {
        // Extract user from startup parameters
        let user = message.parameters()
            .get("user")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "postgres".to_string());
        
        // For now, accept connection and defer password check to later
        // Full implementation requires handling AuthMessage::Password
        Ok(())
    }
    
    async fn on_auth<C>(&self, _client: &mut C, auth: &Password) -> PgWireResult<()>
    where C: ClientInfo + Send + Unpin {
        // This is called when client sends password
        if self.verify_credentials(&auth.username, &auth.password)
            .map_err(|e| PgWireError::ApiError(Box::new(e)))? {
            Ok(())
        } else {
            Err(PgWireError::AuthError("password authentication failed".to_string()))
        }
    }
}

/// Verify password against stored hash
/// Supports: MD5 (PostgreSQL default) and plain text
fn verify_password(password: &str, stored_hash: &str) -> bool {
    if stored_hash.starts_with("md5") {
        // PostgreSQL MD5 format: md5<hash>
        verify_md5_password(password, stored_hash)
    } else {
        // Plain text comparison (not recommended for production)
        password == stored_hash
    }
}

fn verify_md5_password(password: &str, stored_hash: &str) -> bool {
    use md5::{Md5, Digest};
    
    // PostgreSQL MD5 format: md5<md5(password + username)>
    // For now, implement simple MD5 of password
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    let result = format!("md5{:x}", hasher.finalize());
    
    result == stored_hash
}

/// Hash password for storage
pub fn hash_password(password: &str, username: &str) -> String {
    use md5::{Md5, Digest};
    
    // PostgreSQL style: md5(password + username)
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    hasher.update(username.as_bytes());
    format!("md5{:x}", hasher.finalize())
}
```

**Add to Cargo.toml:**
```toml
[dependencies]
md5 = "0.7"  # For password hashing
```

---

### Phase 2: SQL Functions for Password Management

**Modify: `src/handler/mod.rs`** - Add built-in functions

```rust
/// Register built-in PostgreSQL-compatible functions
pub fn register_builtin_functions(&self, conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    
    // ... existing functions ...
    
    // Password hashing function for CREATE/ALTER USER
    conn.create_scalar_function(
        "pg_md5_hash",
        2, // password, username
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let password: String = ctx.get(0)?;
            let username: String = ctx.get(1)?;
            Ok(crate::auth::hash_password(&password, &username))
        }
    )?;
    
    Ok(())
}
```

---

### Phase 3: Transpile CREATE/ALTER USER with Password

**Modify: `src/transpiler/ddl.rs`**

Add handling for:
- `CREATE USER name WITH PASSWORD 'secret'`
- `CREATE ROLE name LOGIN PASSWORD 'secret'`
- `ALTER USER name WITH PASSWORD 'secret'`

```rust
// In DDL transpilation, detect password clauses and:
// 1. Hash the password using PostgreSQL-compatible MD5
// 2. Store in __pg_authid__.rolpassword

// Example transformation:
// CREATE USER john WITH PASSWORD 'secret';
// → INSERT INTO __pg_authid__ (rolname, rolpassword, rolcanlogin) 
//   VALUES ('john', pg_md5_hash('secret', 'john'), 1);
```

---

### Phase 4: Update Startup Handler Factory

**Modify: `src/main.rs`**

```rust
mod auth;
use auth::PasswordAuthHandler;

impl PgWireServerHandlers for HandlerFactory {
    fn startup_handler(&self) -> Arc<impl pgwire::api::auth::StartupHandler> {
        // Replace NoopHandler with password validator
        Arc::new(PasswordAuthHandler::new(self.handler.conn.clone()))
    }
    
    // ... other handlers unchanged ...
}
```

---

### Phase 5: Backward Compatibility Mode

Add a CLI flag to disable password checking (for existing deployments):

```rust
#[derive(Parser, Debug)]
struct Cli {
    /// Disable password authentication (trust mode)
    #[arg(long, env = "PGQT_TRUST_MODE")]
    trust_mode: bool,
    // ... other args ...
}

// In HandlerFactory:
fn startup_handler(&self) -> Arc<impl StartupHandler> {
    if self.trust_mode {
        Arc::new(pgwire::api::NoopHandler)
    } else {
        Arc::new(PasswordAuthHandler::new(self.handler.conn.clone()))
    }
}
```

---

## Testing Strategy

### Unit Tests (`src/auth.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hash_password() {
        let hash = hash_password("secret", "postgres");
        assert!(hash.starts_with("md5"));
        assert_eq!(hash.len(), 35); // "md5" + 32 hex chars
    }
    
    #[test]
    fn test_verify_password() {
        let hash = hash_password("secret", "postgres");
        assert!(verify_password("secret", &hash));
        assert!(!verify_password("wrong", &hash));
    }
    
    #[test]
    fn test_empty_password_allows_login() {
        // User with no password should be able to connect
        assert!(verify_password("anything", ""));
    }
}
```

### Integration Tests (`tests/auth_tests.rs`)

```rust
use pgqt::test_utils::TestServer;

#[test]
fn test_create_user_with_password() {
    let server = TestServer::new();
    
    // Create user with password
    server.execute("CREATE USER testuser WITH PASSWORD 'secret'");
    
    // Verify user exists with password hash
    let result = server.query("SELECT rolname, rolpassword IS NOT NULL as has_password 
                               FROM __pg_authid__ WHERE rolname = 'testuser'");
    assert_eq!(result[0]["rolname"], "testuser");
    assert_eq!(result[0]["has_password"], "true");
}

#[test]
fn test_password_auth_required() {
    let server = TestServer::new();
    server.execute("CREATE USER testuser WITH PASSWORD 'secret'");
    
    // Try to connect without password - should fail
    let result = server.try_connect("testuser", "");
    assert!(result.is_err());
    
    // Connect with correct password - should succeed
    let result = server.try_connect("testuser", "secret");
    assert!(result.is_ok());
}

#[test]
fn test_trust_mode_bypasses_auth() {
    let server = TestServer::with_args(&["--trust-mode"]);
    
    // Should connect without password even if user has one
    let result = server.try_connect("anyuser", "");
    assert!(result.is_ok());
}
```

### E2E Test (`tests/auth_e2e_test.py`)

```python
#!/usr/bin/env python3
"""
End-to-end tests for password authentication.
"""
import subprocess
import time
import psycopg2
import os

def test_password_auth():
    """Test that password authentication works correctly."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host="127.0.0.1",
            port=5432,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create user with password
        cur.execute("CREATE USER testuser WITH PASSWORD 'testpass'")
        conn.commit()
        cur.close()
        conn.close()
        
        # Try to connect as new user with wrong password - should fail
        try:
            conn = psycopg2.connect(
                host="127.0.0.1",
                port=5432,
                database="postgres",
                user="testuser",
                password="wrongpassword"
            )
            assert False, "Should have failed with wrong password"
        except psycopg2.OperationalError:
            pass  # Expected
        
        # Connect with correct password - should succeed
        conn = psycopg2.connect(
            host="127.0.0.1",
            port=5432,
            database="postgres",
            user="testuser",
            password="testpass"
        )
        conn.close()
        
        print("test_password_auth: PASSED")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    test_password_auth()
```

---

## Implementation Steps

### Step 1: Create auth module (30 min)
- Create `src/auth.rs` with PasswordAuthHandler
- Add password hashing functions
- Add unit tests

### Step 2: Add password functions (15 min)
- Add `pg_md5_hash` to built-in functions
- Add `verify_password` helper

### Step 3: Modify startup handler (15 min)
- Replace NoopHandler with PasswordAuthHandler
- Add trust_mode CLI flag

### Step 4: Support CREATE/ALTER USER (45 min)
- Modify DDL transpiler to handle PASSWORD clause
- Convert to INSERT/UPDATE on __pg_authid__

### Step 5: Testing (45 min)
- Unit tests for password hashing
- Integration tests for auth flow
- E2E test with psycopg2

### Step 6: Documentation (15 min)
- Update README with auth info
- Add example for creating users

**Total estimated time: ~2.5 hours**

---

## Security Considerations

1. **Password Storage**: Using MD5 hash (PostgreSQL compatible). Consider:
   - Adding SCRAM-SHA-256 support for better security
   - Allowing configurable hash methods

2. **Connection Security**: 
   - Passwords sent in plain text unless using TLS
   - Recommend SSL/TLS for production

3. **Trust Mode**: 
   - `--trust-mode` flag for backward compatibility
   - Should log warning when enabled

---

## Future Enhancements

1. **SCRAM Authentication**: More secure than MD5
2. **LDAP/External Auth**: Delegate to external providers
3. **Connection Limits**: Per-user max connections
4. **Password Policies**: Expiration, complexity requirements
5. **Audit Logging**: Log all authentication attempts

---

## Success Criteria

- [ ] CREATE USER WITH PASSWORD stores hashed password
- [ ] ALTER USER WITH PASSWORD updates password
- [ ] Connection rejected with wrong password
- [ ] Connection accepted with correct password
- [ ] Trust mode flag works for backward compatibility
- [ ] Users without passwords can still connect
- [ ] All tests pass
