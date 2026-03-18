//! TLS/SSL configuration and certificate management for PGQT
//!
//! This module provides TLS support for encrypted connections, including:
//! - Loading certificates from files
//! - Generating ephemeral self-signed certificates
//! - Configuring rustls for secure connections

use anyhow::{Context, Result};
use rustls::pki_types::CertificateDer;
use rustls::ServerConfig;
use std::path::Path;
use std::sync::Arc;

/// TLS configuration for PGQT server
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Whether TLS is enabled
    #[allow(dead_code)]
    pub enabled: bool,
    /// Server configuration for rustls
    pub server_config: Option<Arc<ServerConfig>>,
    /// Path to certificate file (if loaded from files)
    #[allow(dead_code)]
    pub cert_path: Option<String>,
    /// Path to key file (if loaded from files)
    #[allow(dead_code)]
    pub key_path: Option<String>,
    /// Path to CA certificate for client verification (optional)
    #[allow(dead_code)]
    pub ca_path: Option<String>,
    /// Whether this is an ephemeral (self-signed) certificate
    #[allow(dead_code)]
    pub ephemeral: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_config: None,
            cert_path: None,
            key_path: None,
            ca_path: None,
            ephemeral: false,
        }
    }
}

impl TlsConfig {
    /// Create a new disabled TLS config
    #[allow(dead_code)]
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Load TLS configuration from certificate and key files
    ///
    /// # Arguments
    /// * `cert_path` - Path to the certificate file (PEM format)
    /// * `key_path` - Path to the private key file (PEM format)
    /// * `ca_path` - Optional path to CA certificate for client verification
    ///
    /// # Example
    /// ```
    /// let tls_config = TlsConfig::from_files(
    ///     "/etc/pgqt/server.crt",
    ///     "/etc/pgqt/server.key",
    ///     None::<&str>,
    /// )?;
    /// ```
    pub fn from_files(
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
        _ca_path: Option<impl AsRef<Path>>,
    ) -> Result<Self> {
        let cert_path = cert_path.as_ref();
        let key_path = key_path.as_ref();

        // Read certificate chain
        let cert_file = std::fs::read(cert_path)
            .with_context(|| format!("Failed to read certificate file: {}", cert_path.display()))?;
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_file.as_slice())
            .map_err(|e| anyhow::anyhow!("Failed to parse certificate file: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to read certificates: {}", e))?;

        if certs.is_empty() {
            anyhow::bail!("No certificates found in file: {}", cert_path.display());
        }

        // Read private key
        let key_file = std::fs::read(key_path)
            .with_context(|| format!("Failed to read key file: {}", key_path.display()))?;
        
        // Try to parse as PKCS8 first, then RSA
        let key = rustls_pemfile::pkcs8_private_keys(&mut key_file.as_slice())
            .map_err(|e| anyhow::anyhow!("Failed to parse private key file: {}", e))?
            .next()
            .map(|k| k.ok())
            .flatten()
            .or_else(|| {
                // Try RSA format
                rustls_pemfile::rsa_private_keys(&mut key_file.as_slice())
                    .ok()
                    .and_then(|mut keys| keys.next())
                    .map(|k| k.ok())
                    .flatten()
            })
            .ok_or_else(|| anyhow::anyhow!("No private key found in file: {}", key_path.display()))?;

        // Build server config
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("Failed to create TLS server config - certificate and key may not match")?;

        Ok(Self {
            enabled: true,
            server_config: Some(Arc::new(config)),
            cert_path: Some(cert_path.to_string_lossy().to_string()),
            key_path: Some(key_path.to_string_lossy().to_string()),
            ca_path: None,
            ephemeral: false,
        })
    }

    /// Generate an ephemeral self-signed certificate for development/testing
    ///
    /// This creates a temporary certificate that is valid for:
    /// - localhost
    /// - 127.0.0.1
    /// - ::1
    ///
    /// # Example
    /// ```
    /// let tls_config = TlsConfig::generate_ephemeral()?;
    /// ```
    pub fn generate_ephemeral() -> Result<Self> {
        // Generate a self-signed certificate using rcgen
        let key_pair = rcgen::KeyPair::generate()
            .context("Failed to generate key pair")?;
        let params = rcgen::CertificateParams::new(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
        ])?;
        
        let cert = params.self_signed(&key_pair)
            .context("Failed to generate ephemeral certificate")?;
        
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        // Parse the generated certificate
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to parse generated certificate: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to read generated certificates: {}", e))?;

        let key = rustls_pemfile::pkcs8_private_keys(&mut key_pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to parse generated key: {}", e))?
            .next()
            .map(|k| k.ok())
            .flatten()
            .ok_or_else(|| anyhow::anyhow!("No private key in generated certificate"))?;

        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("Failed to create TLS server config from ephemeral certificate")?;

        Ok(Self {
            enabled: true,
            server_config: Some(Arc::new(server_config)),
            cert_path: None,
            key_path: None,
            ca_path: None,
            ephemeral: true,
        })
    }

    /// Check if TLS is enabled
    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.server_config.is_some()
    }

    /// Get the server configuration for use with tokio-rustls
    pub fn server_config(&self) -> Option<Arc<ServerConfig>> {
        self.server_config.clone()
    }
}

/// Helper module for parsing PEM files
mod rustls_pemfile {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::io::BufRead;

    /// Parse certificates from PEM data
    pub fn certs(
        rd: &mut &[u8],
    ) -> Result<impl Iterator<Item = Result<CertificateDer<'static>, std::io::Error>>, std::io::Error> {
        let mut certs = Vec::new();
        let reader = std::io::BufReader::new(rd);
        let mut in_cert = false;
        let mut cert_base64 = String::new();
        
        for line in reader.lines() {
            let line = line?;
            if line.starts_with("-----BEGIN CERTIFICATE-----") {
                in_cert = true;
                cert_base64.clear();
            } else if line.starts_with("-----END CERTIFICATE-----") {
                in_cert = false;
                if let Ok(cert_der) = base64_decode(&cert_base64) {
                    certs.push(Ok(CertificateDer::from(cert_der)));
                }
            } else if in_cert {
                cert_base64.push_str(&line);
            }
        }
        
        Ok(certs.into_iter())
    }

    /// Parse PKCS8 private keys from PEM data
    pub fn pkcs8_private_keys(
        rd: &mut &[u8],
    ) -> Result<impl Iterator<Item = Result<PrivateKeyDer<'static>, std::io::Error>>, std::io::Error> {
        let mut keys = Vec::new();
        let reader = std::io::BufReader::new(rd);
        let mut in_key = false;
        let mut key_base64 = String::new();
        
        for line in reader.lines() {
            let line = line?;
            if line.starts_with("-----BEGIN PRIVATE KEY-----") {
                in_key = true;
                key_base64.clear();
            } else if line.starts_with("-----END PRIVATE KEY-----") {
                in_key = false;
                if let Ok(key_der) = base64_decode(&key_base64) {
                    keys.push(Ok(PrivateKeyDer::try_from(key_der).map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid key")
                    })?));
                }
            } else if in_key {
                key_base64.push_str(&line);
            }
        }
        
        Ok(keys.into_iter())
    }

    /// Parse RSA private keys from PEM data
    pub fn rsa_private_keys(
        rd: &mut &[u8],
    ) -> Result<impl Iterator<Item = Result<PrivateKeyDer<'static>, std::io::Error>>, std::io::Error> {
        let mut keys = Vec::new();
        let reader = std::io::BufReader::new(rd);
        let mut in_key = false;
        let mut key_base64 = String::new();
        
        for line in reader.lines() {
            let line = line?;
            if line.starts_with("-----BEGIN RSA PRIVATE KEY-----") {
                in_key = true;
                key_base64.clear();
            } else if line.starts_with("-----END RSA PRIVATE KEY-----") {
                in_key = false;
                if let Ok(key_der) = base64_decode(&key_base64) {
                    keys.push(Ok(PrivateKeyDer::try_from(key_der).map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid key")
                    })?));
                }
            } else if in_key {
                key_base64.push_str(&line);
            }
        }
        
        Ok(keys.into_iter())
    }

    /// Decode base64 data
    fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
        // Remove whitespace
        let input: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        
        // Use standard base64 decoding
        let mut result = Vec::with_capacity(input.len() * 3 / 4);
        let chars: Vec<char> = input.chars().collect();
        
        for chunk in chars.chunks(4) {
            if chunk.len() < 2 {
                break;
            }
            
            let b0 = decode_char(chunk[0]);
            let b1 = decode_char(chunk[1]);
            
            result.push((b0 << 2) | (b1 >> 4));
            
            if chunk.len() > 2 && chunk[2] != '=' {
                let b2 = decode_char(chunk[2]);
                result.push(((b1 & 0x0f) << 4) | (b2 >> 2));
                
                if chunk.len() > 3 && chunk[3] != '=' {
                    let b3 = decode_char(chunk[3]);
                    result.push(((b2 & 0x03) << 6) | b3);
                }
            }
        }
        
        Ok(result)
    }

    fn decode_char(c: char) -> u8 {
        match c {
            'A'..='Z' => c as u8 - b'A',
            'a'..='z' => c as u8 - b'a' + 26,
            '0'..='9' => c as u8 - b'0' + 52,
            '+' => 62,
            '/' => 63,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_disabled() {
        let config = TlsConfig::disabled();
        assert!(!config.is_enabled());
        assert!(config.server_config().is_none());
    }

    #[test]
    fn test_generate_ephemeral() {
        let config = TlsConfig::generate_ephemeral();
        assert!(config.is_ok());
        
        let config = config.unwrap();
        assert!(config.is_enabled());
        assert!(config.ephemeral);
        assert!(config.server_config().is_some());
    }
}
