use std::fmt::Debug;
use std::sync::Arc;
use async_trait::async_trait;
use futures::sink::{Sink, SinkExt};
use pgwire::api::ClientInfo;
use pgwire::api::auth::{DefaultServerParameterProvider, ServerParameterProvider, StartupHandler};
use pgwire::api::PgWireConnectionState;
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::response::{ErrorResponse, ReadyForQuery, TransactionStatus};
use pgwire::messages::startup::Authentication;
use pgwire::messages::{PgWireBackendMessage, PgWireFrontendMessage};
use rusqlite::{Connection, OptionalExtension};

/// PostgreSQL-compatible password authenticator
#[derive(Clone)]
pub struct PasswordAuthHandler {
    conn: Arc<std::sync::Mutex<Connection>>,
}

impl PasswordAuthHandler {
    pub fn new(conn: Arc<std::sync::Mutex<Connection>>) -> Self {
        Self { conn }
    }
    
    /// Check if user requires a password
    /// Returns: (user_exists, needs_password, can_login)
    fn check_user_password_status(&self, user: &str) -> Result<(bool, bool, bool), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        
        let result: Option<(Option<String>, bool)> = conn.query_row(
            "SELECT rolpassword, rolcanlogin FROM __pg_authid__ WHERE rolname = ?1",
            [user],
            |row| {
                let pwd: Option<String> = row.get(0)?;
                let can_login: bool = row.get(1)?;
                Ok((pwd, can_login))
            }
        ).optional()?;
        
        match result {
            Some((stored_hash, can_login)) => {
                let needs_password = stored_hash.as_ref().map(|h| !h.is_empty()).unwrap_or(false);
                Ok((true, needs_password, can_login))
            }
            None => {
                // Auto-create user with no password
                conn.execute(
                    "INSERT INTO __pg_authid__ (rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin) 
                     VALUES (?1, 1, 1, 1, 1, 1)",
                    [user]
                )?;
                Ok((false, false, true)) // New user, no password needed, can login
            }
        }
    }
    
    /// Verify username/password against __pg_authid__
    fn verify_credentials(&self, user: &str, password: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        
        // Look up user and password hash
        let result: Option<(Option<String>, bool)> = conn.query_row(
            "SELECT rolpassword, rolcanlogin FROM __pg_authid__ WHERE rolname = ?1",
            [user],
            |row| {
                let pwd: Option<String> = row.get(0)?;
                let can_login: bool = row.get(1)?;
                Ok((pwd, can_login))
            }
        ).optional()?;
        
        match result {
            Some((stored_hash, can_login)) => {
                // User must have login permission
                if !can_login {
                    return Ok(false);
                }
                
                // If no password set (NULL or empty), allow connection
                let stored_hash = stored_hash.unwrap_or_default();
                if stored_hash.is_empty() {
                    return Ok(true);
                }
                
                // Verify password using PostgreSQL-compatible MD5
                Ok(verify_password(password, &stored_hash, user))
            }
            None => {
                // User doesn't exist - auto-create with no password for backward compatibility
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
    async fn on_startup<C>(
        &self,
        client: &mut C,
        message: PgWireFrontendMessage,
    ) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        match message {
            PgWireFrontendMessage::Startup(ref startup) => {
                // Protocol negotiation first
                pgwire::api::auth::protocol_negotiation(client, startup).await?;
                // Save startup parameters
                pgwire::api::auth::save_startup_parameters_to_metadata(client, startup);
                
                // Get username from client metadata
                let user = client.metadata()
                    .get("user")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "postgres".to_string());
                
                // Check if user needs a password
                match self.check_user_password_status(&user) {
                    Ok((exists, needs_password, can_login)) => {
                        if !can_login {
                            // User cannot login
                            let error_info = ErrorInfo::new(
                                "FATAL".to_owned(),
                                "28000".to_owned(),
                                format!("role \"{}\" is not permitted to log in", user),
                            );
                            let error = ErrorResponse::from(error_info);
                            client.feed(PgWireBackendMessage::ErrorResponse(error)).await?;
                            client.close().await?;
                            return Ok(());
                        }
                        
                        if !needs_password {
                            // No password required - skip authentication
                            finish_authentication(client).await?;
                            return Ok(());
                        }
                        
                        // Password required - request it
                        client.set_state(PgWireConnectionState::AuthenticationInProgress);
                        client
                            .send(PgWireBackendMessage::Authentication(
                                Authentication::CleartextPassword,
                            ))
                            .await?;
                    }
                    Err(e) => {
                        return Err(PgWireError::ApiError(Box::new(e)));
                    }
                }
            }
            PgWireFrontendMessage::PasswordMessageFamily(pwd) => {
                let pwd = pwd.into_password()?;
                let password = &pwd.password;
                
                // Get username from client metadata
                let user = client.metadata()
                    .get("user")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "postgres".to_string());
                
                // Verify credentials
                match self.verify_credentials(&user, &password) {
                    Ok(true) => {
                        // Authentication successful - finish the process
                        finish_authentication(client).await?;
                    }
                    Ok(false) => {
                        // Authentication failed
                        let error_info = ErrorInfo::new(
                            "FATAL".to_owned(),
                            "28P01".to_owned(),
                            "password authentication failed".to_owned(),
                        );
                        let error = ErrorResponse::from(error_info);
                        client
                            .feed(PgWireBackendMessage::ErrorResponse(error))
                            .await?;
                        client.close().await?;
                    }
                    Err(e) => {
                        return Err(PgWireError::ApiError(Box::new(e)));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// A combined startup handler that can switch between trust mode and password auth
#[derive(Clone)]
pub enum FlexibleAuthHandler {
    /// Trust mode - accepts all connections
    Trust,
    /// Password authentication mode
    Password(PasswordAuthHandler),
}

impl FlexibleAuthHandler {
    pub fn new_trust() -> Self {
        Self::Trust
    }
    
    pub fn new_password(conn: Arc<std::sync::Mutex<Connection>>) -> Self {
        Self::Password(PasswordAuthHandler::new(conn))
    }
}

#[async_trait]
impl StartupHandler for FlexibleAuthHandler {
    async fn on_startup<C>(
        &self,
        client: &mut C,
        message: PgWireFrontendMessage,
    ) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        match self {
            Self::Trust => {
                // Use the NoopHandler pattern
                if let PgWireFrontendMessage::Startup(ref startup) = message {
                    pgwire::api::auth::protocol_negotiation(client, startup).await?;
                    pgwire::api::auth::save_startup_parameters_to_metadata(client, startup);
                    finish_authentication(client).await?;
                }
                Ok(())
            }
            Self::Password(handler) => {
                handler.on_startup(client, message).await
            }
        }
    }
}

/// Finish authentication process
async fn finish_authentication<C>(client: &mut C) -> PgWireResult<()>
where
    C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send,
    C::Error: Debug,
    PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
{
    let param_provider = DefaultServerParameterProvider::default();
    
    // Send authentication OK first (this is required before other messages)
    client
        .feed(PgWireBackendMessage::Authentication(
            Authentication::Ok,
        ))
        .await?;
    
    // Send parameter status messages
    if let Some(params) = param_provider.server_parameters(client) {
        for (key, value) in params {
            client
                .feed(PgWireBackendMessage::ParameterStatus(
                    pgwire::messages::startup::ParameterStatus::new(key, value),
                ))
                .await?;
        }
    }
    
    // Send backend key data
    let backend_data = pgwire::messages::startup::BackendKeyData::new(
        0, 
        pgwire::messages::startup::SecretKey::I32(0)
    );
    client
        .feed(PgWireBackendMessage::BackendKeyData(backend_data))
        .await?;
    
    // Send ready for query
    client
        .send(PgWireBackendMessage::ReadyForQuery(ReadyForQuery::new(
            TransactionStatus::Idle,
        )))
        .await?;
    
    client.set_state(PgWireConnectionState::ReadyForQuery);
    Ok(())
}

/// Verify password against stored hash
/// Supports: MD5 (PostgreSQL default) and plain text
pub fn verify_password(password: &str, stored_hash: &str, username: &str) -> bool {
    if stored_hash.starts_with("md5") {
        // PostgreSQL MD5 format: md5<hash>
        verify_md5_password(password, stored_hash, username)
    } else {
        // Plain text comparison (not recommended for production)
        password == stored_hash
    }
}

pub fn verify_md5_password(password: &str, stored_hash: &str, username: &str) -> bool {
    // PostgreSQL MD5 format: md5<md5(password + username)>
    // The stored hash should be "md5" + hex(md5(password + username))
    let computed_hash = hash_password(password, username);
    
    computed_hash == stored_hash
}

/// Hash password for storage
pub fn hash_password(password: &str, username: &str) -> String {
    // PostgreSQL style: md5(password + username)
    let mut hasher = md5::Context::new();
    hasher.consume(password.as_bytes());
    hasher.consume(username.as_bytes());
    format!("md5{:x}", hasher.compute())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hash_password() {
        let hash = hash_password("secret", "postgres");
        assert!(hash.starts_with("md5"));
        assert_eq!(hash.len(), 35); // "md5" + 32 hex chars
    }
    
    #[allow(unused_imports)]
    
    #[test]
    fn test_verify_password_md5() {
        let hash = hash_password("secret", "postgres");
        assert!(verify_password("secret", &hash, "postgres"));
        assert!(!verify_password("wrong", &hash, "postgres"));
        assert!(!verify_password("secret", &hash, "otheruser"));
    }
    
    #[test]
    fn test_verify_password_plaintext() {
        assert!(verify_password("secret", "secret", "anyuser"));
        assert!(!verify_password("wrong", "secret", "anyuser"));
    }
    
    #[test]
    fn test_empty_password_only_empty_works() {
        // User with no (empty) password should only be able to connect with empty password
        // This is because empty string doesn't match "md5" prefix, so falls through to plaintext
        assert!(verify_password("", "", "anyuser"));
        assert!(!verify_password("anything", "", "anyuser"));
    }
    
    #[test]
    fn test_verify_md5_password_consistency() {
        // Test that hashing and verification are consistent
        let password = "mysecretpassword";
        let username = "testuser";
        let hash = hash_password(password, username);
        
        // Correct password should verify
        assert!(verify_md5_password(password, &hash, username));
        
        // Wrong password should fail
        assert!(!verify_md5_password("wrongpassword", &hash, username));
        
        // Wrong username should fail
        assert!(!verify_md5_password(password, &hash, "wronguser"));
    }
    
    #[test]
    fn test_hash_is_deterministic() {
        // Same password and username should produce same hash
        let hash1 = hash_password("secret", "user");
        let hash2 = hash_password("secret", "user");
        assert_eq!(hash1, hash2);
        
        // Different username should produce different hash
        let hash3 = hash_password("secret", "otheruser");
        assert_ne!(hash1, hash3);
    }
}