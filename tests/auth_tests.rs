use pgqt::auth::{hash_password, verify_password};

#[test]
fn test_password_hashing() {
    let hash = hash_password("secret", "postgres");
    assert!(hash.starts_with("md5"));
    assert_eq!(hash.len(), 35); // "md5" + 32 hex chars
}

#[test]
fn test_password_verification_md5() {
    let hash = hash_password("secret", "postgres");
    
    // Correct password should verify
    assert!(verify_password("secret", &hash, "postgres"));
    
    // Wrong password should fail
    assert!(!verify_password("wrong", &hash, "postgres"));
    
    // Wrong username should fail (MD5 includes username)
    assert!(!verify_password("secret", &hash, "otheruser"));
}

#[test]
fn test_password_verification_plaintext() {
    // Plain text comparison
    assert!(verify_password("secret", "secret", "anyuser"));
    assert!(!verify_password("wrong", "secret", "anyuser"));
}

#[test]
fn test_empty_password_only_empty_works() {
    // User with no (empty) password should only be able to connect with empty password
    assert!(verify_password("", "", "anyuser"));
    assert!(!verify_password("anything", "", "anyuser"));
}

#[test]
fn test_password_hash_deterministic() {
    // Same password and username should produce same hash
    let hash1 = hash_password("secret", "user");
    let hash2 = hash_password("secret", "user");
    assert_eq!(hash1, hash2);
    
    // Different username should produce different hash
    let hash3 = hash_password("secret", "otheruser");
    assert_ne!(hash1, hash3);
}

#[test]
fn test_password_hash_different_passwords() {
    // Different passwords should produce different hashes for same user
    let hash1 = hash_password("password1", "user");
    let hash2 = hash_password("password2", "user");
    assert_ne!(hash1, hash2);
}

#[test]
fn test_create_role_stores_hashed_password() {
    use pgqt::transpiler::transpile;
    
    // CREATE ROLE with password should result in hashed password
    let sql = "CREATE ROLE testuser WITH LOGIN PASSWORD 'testpass'";
    let result = transpile(sql);
    
    // The result should contain a hashed password (md5 prefix)
    assert!(result.contains("INSERT INTO __pg_authid__"));
    assert!(result.contains("md5"));
}

#[test]
fn test_alter_role_updates_password() {
    use pgqt::transpiler::transpile;
    
    // ALTER ROLE with password should result in hashed password
    let sql = "ALTER ROLE testuser WITH PASSWORD 'newpass'";
    let result = transpile(sql);
    
    // The result should contain a hashed password (md5 prefix)
    assert!(result.contains("UPDATE __pg_authid__"));
    assert!(result.contains("md5"));
}

#[test]
fn test_create_user_stores_hashed_password() {
    use pgqt::transpiler::transpile;
    
    // CREATE USER is equivalent to CREATE ROLE WITH LOGIN
    let sql = "CREATE USER testuser WITH PASSWORD 'testpass'";
    let result = transpile(sql);
    
    // The result should contain a hashed password (md5 prefix)
    assert!(result.contains("INSERT INTO __pg_authid__"));
    assert!(result.contains("md5"));
}