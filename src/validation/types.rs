//! Type validation module
//! 
//! This module contains type-specific validation logic for PostgreSQL types.

use crate::validation::ValidationError;

/// Validates that a string is a valid UUID format
/// UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx (8-4-4-4-12 hex digits)
pub fn validate_uuid(value: &str) -> Result<(), ValidationError> {
    // Remove any quotes that might be present
    let value = value.trim_matches('\'');
    
    // Check length (36 characters for standard UUID with dashes)
    if value.len() != 36 {
        return Err(ValidationError {
            code: "22P02".to_string(),
            message: format!("invalid input syntax for type uuid: \"{}\"", value),
            position: None,
        });
    }
    
    // Check UUID format: 8-4-4-4-12 hex digits
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 5 {
        return Err(ValidationError {
            code: "22P02".to_string(),
            message: format!("invalid input syntax for type uuid: \"{}\"", value),
            position: None,
        });
    }
    
    let expected_lengths = [8, 4, 4, 4, 12];
    for (i, (part, expected_len)) in parts.iter().zip(expected_lengths.iter()).enumerate() {
        if part.len() != *expected_len {
            return Err(ValidationError {
                code: "22P02".to_string(),
                message: format!("invalid input syntax for type uuid: \"{}\"", value),
                position: None,
            });
        }
        
        // Check that all characters are valid hex digits
        for (j, c) in part.chars().enumerate() {
            if !c.is_ascii_hexdigit() {
                return Err(ValidationError {
                    code: "22P02".to_string(),
                    message: format!("invalid input syntax for type uuid: \"{}\"", value),
                    position: None,
                });
            }
        }
    }
    
    Ok(())
}

/// List of valid PostgreSQL timezones
/// This is a subset of the full IANA timezone database
pub const VALID_TIMEZONES: &[&str] = &[
    "UTC", "GMT", "UCT", "Universal", "Zulu",
    "America/New_York", "America/Chicago", "America/Denver", "America/Los_Angeles",
    "America/Anchorage", "America/Honolulu", "America/Phoenix", "America/Detroit",
    "America/Toronto", "America/Vancouver", "America/Mexico_City", "America/Sao_Paulo",
    "America/Buenos_Aires", "America/Santiago", "America/Bogota", "America/Lima",
    "Europe/London", "Europe/Paris", "Europe/Berlin", "Europe/Moscow", "Europe/Rome",
    "Europe/Madrid", "Europe/Amsterdam", "Europe/Brussels", "Europe/Vienna",
    "Europe/Stockholm", "Europe/Oslo", "Europe/Copenhagen", "Europe/Helsinki",
    "Europe/Zurich", "Europe/Istanbul", "Europe/Warsaw", "Europe/Prague",
    "Asia/Tokyo", "Asia/Shanghai", "Asia/Hong_Kong", "Asia/Singapore", "Asia/Seoul",
    "Asia/Taipei", "Asia/Bangkok", "Asia/Jakarta", "Asia/Manila", "Asia/Kolkata",
    "Asia/Mumbai", "Asia/Dubai", "Asia/Tehran", "Asia/Baghdad", "Asia/Riyadh",
    "Asia/Karachi", "Asia/Dhaka", "Asia/Ho_Chi_Minh", "Asia/Kuala_Lumpur",
    "Australia/Sydney", "Australia/Melbourne", "Australia/Brisbane", "Australia/Perth",
    "Australia/Adelaide", "Australia/Darwin", "Australia/Hobart", "Australia/Canberra",
    "Pacific/Auckland", "Pacific/Fiji", "Pacific/Honolulu", "Pacific/Guam",
    "Pacific/Samoa", "Pacific/Tahiti", "Pacific/Noumea", "Pacific/Port_Moresby",
    "Africa/Cairo", "Africa/Johannesburg", "Africa/Lagos", "Africa/Nairobi",
    "Africa/Addis_Ababa", "Africa/Accra", "Africa/Casablanca", "Africa/Tunis",
    "Atlantic/Reykjavik", "Atlantic/Azores", "Atlantic/Bermuda", "Atlantic/Canary",
    "Indian/Maldives", "Indian/Mauritius", "Indian/Reunion", "Indian/Seychelles",
    "Antarctica/Palmer", "Antarctica/McMurdo", "Antarctica/South_Pole",
    "Arctic/Longyearbyen",
    "US/Eastern", "US/Central", "US/Mountain", "US/Pacific", "US/Alaska", "US/Hawaii",
    "US/Arizona", "US/East-Indiana", "US/Michigan", "US/Samoa",
    "Canada/Atlantic", "Canada/Central", "Canada/Eastern", "Canada/Mountain",
    "Canada/Newfoundland", "Canada/Pacific", "Canada/Saskatchewan", "Canada/Yukon",
    "Brazil/East", "Brazil/West", "Brazil/Acre", "Brazil/DeNoronha",
    "Mexico/General", "Mexico/BajaNorte", "Mexico/BajaSur",
    "Chile/Continental", "Chile/EasterIsland",
    "CET", "CST6CDT", "EET", "EST", "EST5EDT", "HST", "MET", "MST", "MST7MDT",
    "PST8PDT", "WET",
];

/// Validates that a timezone string is recognized by PostgreSQL
pub fn validate_timezone(tz: &str) -> Result<(), ValidationError> {
    // Remove any quotes that might be present
    let tz = tz.trim_matches('\'');
    
    if !VALID_TIMEZONES.contains(&tz) {
        return Err(ValidationError {
            code: "22023".to_string(),
            message: format!("time zone \"{}\" not recognized", tz),
            position: None,
        });
    }
    Ok(())
}

/// Validates that a string is valid JSON
pub fn validate_json(value: &str) -> Result<(), ValidationError> {
    // Remove any quotes that might be present at the start/end
    let value = value.trim();
    
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(_) => Ok(()),
        Err(_) => Err(ValidationError {
            code: "22P02".to_string(),
            message: format!("invalid input syntax for type json: \"{}\"", value),
            position: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_uuid() {
        let valid_uuid = "11111111-1111-1111-1111-111111111111";
        assert!(validate_uuid(valid_uuid).is_ok());
    }

    #[test]
    fn test_valid_uuid_uppercase() {
        let valid_uuid = "11111111-1111-1111-1111-111111111111".to_uppercase();
        assert!(validate_uuid(&valid_uuid).is_ok());
    }

    #[test]
    fn test_valid_uuid_mixed_case() {
        let valid_uuid = "11111111-1111-AAAA-1111-111111111111";
        assert!(validate_uuid(valid_uuid).is_ok());
    }

    #[test]
    fn test_invalid_uuid_wrong_length() {
        let invalid_uuid = "11111111-1111-1111-1111-111111111111F";
        let result = validate_uuid(invalid_uuid);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22P02");
    }

    #[test]
    fn test_invalid_uuid_too_short() {
        let invalid_uuid = "11111111-1111-1111-1111";
        let result = validate_uuid(invalid_uuid);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22P02");
    }

    #[test]
    fn test_invalid_uuid_missing_dashes() {
        let invalid_uuid = "11111111111111111111111111111111";
        let result = validate_uuid(invalid_uuid);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22P02");
    }

    #[test]
    fn test_invalid_uuid_invalid_chars() {
        let invalid_uuid = "11111111-1111-1111-1111-11111111111G";
        let result = validate_uuid(invalid_uuid);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22P02");
    }

    #[test]
    fn test_invalid_uuid_wrong_part_lengths() {
        let invalid_uuid = "111-1111-1111-1111-111111111111";
        let result = validate_uuid(invalid_uuid);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22P02");
    }

    // Timezone validation tests
    #[test]
    fn test_valid_timezone_utc() {
        assert!(validate_timezone("UTC").is_ok());
    }

    #[test]
    fn test_valid_timezone_ny() {
        assert!(validate_timezone("America/New_York").is_ok());
    }

    #[test]
    fn test_valid_timezone_london() {
        assert!(validate_timezone("Europe/London").is_ok());
    }

    #[test]
    fn test_valid_timezone_tokyo() {
        assert!(validate_timezone("Asia/Tokyo").is_ok());
    }

    #[test]
    fn test_invalid_timezone() {
        let result = validate_timezone("America/Does_not_exist");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "22023");
        assert!(err.message.contains("not recognized"));
    }

    // JSON validation tests
    #[test]
    fn test_valid_json_object() {
        let valid_json = r#"{"key": "value"}"#;
        assert!(validate_json(valid_json).is_ok());
    }

    #[test]
    fn test_valid_json_array() {
        let valid_json = r#"[1, 2, 3]"#;
        assert!(validate_json(valid_json).is_ok());
    }

    #[test]
    fn test_valid_json_string() {
        let valid_json = r#""hello""#;
        assert!(validate_json(valid_json).is_ok());
    }

    #[test]
    fn test_valid_json_number() {
        let valid_json = "42";
        assert!(validate_json(valid_json).is_ok());
    }

    #[test]
    fn test_valid_json_boolean() {
        let valid_json = "true";
        assert!(validate_json(valid_json).is_ok());
    }

    #[test]
    fn test_valid_json_null() {
        let valid_json = "null";
        assert!(validate_json(valid_json).is_ok());
    }

    #[test]
    fn test_invalid_json() {
        let invalid_json = "{invalid json}";
        let result = validate_json(invalid_json);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22P02");
    }

    #[test]
    fn test_invalid_json_unclosed_string() {
        let invalid_json = r#"{"key": "value}"#;
        let result = validate_json(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_json_trailing_comma() {
        let invalid_json = r#"{"key": "value",}"#;
        let result = validate_json(invalid_json);
        assert!(result.is_err());
    }
}
