//! Tests for session configuration (set_config / current_setting)

use pgqt::handler::SessionContext;

#[test]
fn test_session_context_default_settings() {
    let ctx = SessionContext::new("postgres".to_string());
    
    // Check that default settings are present
    assert_eq!(ctx.settings.get("timezone"), Some(&"UTC".to_string()));
    assert_eq!(ctx.settings.get("TimeZone"), Some(&"UTC".to_string()));
    assert_eq!(ctx.settings.get("application_name"), Some(&"".to_string()));
    assert_eq!(ctx.settings.get("search_path"), Some(&"\"$user\", public".to_string()));
    assert_eq!(ctx.settings.get("client_encoding"), Some(&"UTF8".to_string()));
    assert_eq!(ctx.settings.get("standard_conforming_strings"), Some(&"on".to_string()));
}

#[test]
fn test_session_context_settings_mutability() {
    let mut ctx = SessionContext::new("postgres".to_string());
    
    // Insert a new setting
    ctx.settings.insert("custom.setting".to_string(), "custom_value".to_string());
    assert_eq!(ctx.settings.get("custom.setting"), Some(&"custom_value".to_string()));
    
    // Update an existing setting - note: both keys are stored separately
    ctx.settings.insert("timezone".to_string(), "America/New_York".to_string());
    ctx.settings.insert("TimeZone".to_string(), "America/New_York".to_string());
    assert_eq!(ctx.settings.get("timezone"), Some(&"America/New_York".to_string()));
    assert_eq!(ctx.settings.get("TimeZone"), Some(&"America/New_York".to_string()));
}

#[test]
fn test_session_context_application_name() {
    let ctx = SessionContext::new("postgres".to_string());
    
    // Default should be empty
    assert_eq!(ctx.settings.get("application_name"), Some(&"".to_string()));
}

#[test]
fn test_session_context_multiple_users() {
    let ctx1 = SessionContext::new("user1".to_string());
    let ctx2 = SessionContext::new("user2".to_string());
    
    // Both should have same defaults
    assert_eq!(ctx1.settings.get("timezone"), ctx2.settings.get("timezone"));
    assert_eq!(ctx1.authenticated_user, "user1");
    assert_eq!(ctx2.authenticated_user, "user2");
}
