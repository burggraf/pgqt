//! Type validation module
//! 
//! This module contains type-specific validation logic for PostgreSQL types.

use crate::validation::ValidationError;

/// Parse type modifier from a type string like "VARCHAR(10)" or "CHAR(5)"
/// Returns Some(length) if a modifier is found, None otherwise
pub fn parse_type_modifier(type_str: &str) -> Option<usize> {
    let type_upper = type_str.to_uppercase();
    
    // Look for pattern like TYPE_NAME(n) or TYPE_NAME (n)
    if let Some(start) = type_upper.find('(') {
        if let Some(end) = type_upper.find(')') {
            let modifier_str = &type_str[start + 1..end].trim();
            // Parse the modifier as a number
            if let Ok(n) = modifier_str.parse::<usize>() {
                return Some(n);
            }
        }
    }
    
    None
}

/// Extract the base type name without modifiers
/// e.g., "VARCHAR(10)" -> "VARCHAR", "CHAR" -> "CHAR"
pub fn extract_base_type(type_str: &str) -> String {
    let type_upper = type_str.to_uppercase();
    
    if let Some(idx) = type_upper.find('(') {
        type_str[..idx].trim().to_uppercase()
    } else {
        type_str.trim().to_uppercase()
    }
}

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

/// Validates that a date string is valid
/// Checks for invalid dates like Feb 29 in non-leap years
pub fn validate_date(value: &str) -> Result<(), ValidationError> {
    let value = value.trim_matches('\'');
    
    // Try to parse the date using chrono if available, otherwise manual validation
    // For now, we'll do basic format validation and leap year checking
    
    // Common date formats: YYYY-MM-DD, YYYY/MM/DD
    let normalized = value.replace('/', "-");
    let parts: Vec<&str> = normalized.split('-').collect();
    
    if parts.len() != 3 {
        return Err(ValidationError {
            code: "22008".to_string(),
            message: format!("date/time field value out of range: \"{}\"", value),
            position: None,
        });
    }
    
    let year: i32 = match parts[0].parse() {
        Ok(y) if y > 0 && y <= 9999 => y,
        _ => return Err(ValidationError {
            code: "22008".to_string(),
            message: format!("date/time field value out of range: \"{}\"", value),
            position: None,
        }),
    };
    
    let month: u32 = match parts[1].parse() {
        Ok(m) if m >= 1 && m <= 12 => m,
        _ => return Err(ValidationError {
            code: "22008".to_string(),
            message: format!("date/time field value out of range: \"{}\"", value),
            position: None,
        }),
    };
    
    let day: u32 = match parts[2].parse() {
        Ok(d) if d >= 1 && d <= 31 => d,
        _ => return Err(ValidationError {
            code: "22008".to_string(),
            message: format!("date/time field value out of range: \"{}\"", value),
            position: None,
        }),
    };
    
    // Check days in month
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            // Leap year check
            let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            if is_leap { 29 } else { 28 }
        }
        _ => return Err(ValidationError {
            code: "22008".to_string(),
            message: format!("date/time field value out of range: \"{}\"", value),
            position: None,
        }),
    };
    
    if day > days_in_month {
        return Err(ValidationError {
            code: "22008".to_string(),
            message: format!("date/time field value out of range: \"{}\"", value),
            position: None,
        });
    }
    
    Ok(())
}

/// Validates a REAL (float4) value is within range
/// PostgreSQL REAL range: approximately ±3.4E+38
pub fn validate_float4(value: &str) -> Result<(), ValidationError> {
    let value = value.trim_matches('\'');
    
    match value.parse::<f32>() {
        Ok(v) => {
            // Check for infinity (which indicates overflow)
            if v.is_infinite() {
                return Err(ValidationError {
                    code: "22003".to_string(),
                    message: format!("value \"{}\" is out of range for type real", value),
                    position: None,
                });
            }
            Ok(())
        }
        Err(_) => {
            // Try scientific notation
            if value.to_uppercase().contains('E') {
                let parts: Vec<&str> = value.split('E').collect();
                if parts.len() == 2 {
                    if let Ok(exp) = parts[1].parse::<i32>() {
                        // float4 max exponent is approximately 38
                        if exp > 38 || exp < -38 {
                            return Err(ValidationError {
                                code: "22003".to_string(),
                                message: format!("value \"{}\" is out of range for type real", value),
                                position: None,
                            });
                        }
                    }
                }
            }
            
            Err(ValidationError {
                code: "22P02".to_string(),
                message: format!("invalid input syntax for type real: \"{}\"", value),
                position: None,
            })
        }
    }
}

/// Validates a DOUBLE PRECISION (float8) value is within range
/// PostgreSQL DOUBLE PRECISION range: approximately ±1.7E+308
pub fn validate_float8(value: &str) -> Result<(), ValidationError> {
    let value = value.trim_matches('\'');
    
    match value.parse::<f64>() {
        Ok(v) => {
            // Check for infinity (which indicates overflow)
            if v.is_infinite() {
                return Err(ValidationError {
                    code: "22003".to_string(),
                    message: format!("value \"{}\" is out of range for type double precision", value),
                    position: None,
                });
            }
            Ok(())
        }
        Err(_) => {
            // Try scientific notation
            if value.to_uppercase().contains('E') {
                let parts: Vec<&str> = value.split('E').collect();
                if parts.len() == 2 {
                    if let Ok(exp) = parts[1].parse::<i32>() {
                        // float8 max exponent is approximately 308
                        if exp > 308 || exp < -308 {
                            return Err(ValidationError {
                                code: "22003".to_string(),
                                message: format!("value \"{}\" is out of range for type double precision", value),
                                position: None,
                            });
                        }
                    }
                }
            }
            
            Err(ValidationError {
                code: "22P02".to_string(),
                message: format!("invalid input syntax for type double precision: \"{}\"", value),
                position: None,
            })
        }
    }
}

/// Validates an INT2 (smallint) value is within range
/// PostgreSQL INT2 range: -32768 to 32767
pub fn validate_int2(value: &str) -> Result<(), ValidationError> {
    let value = value.trim_matches('\'');
    
    match value.parse::<i16>() {
        Ok(_) => Ok(()),
        Err(_) => Err(ValidationError {
            code: "22003".to_string(),
            message: format!("value \"{}\" is out of range for type smallint", value),
            position: None,
        }),
    }
}

/// Validates an INT4 (integer) value is within range
/// PostgreSQL INT4 range: -2147483648 to 2147483647
pub fn validate_int4(value: &str) -> Result<(), ValidationError> {
    let value = value.trim_matches('\'');
    
    match value.parse::<i32>() {
        Ok(_) => Ok(()),
        Err(_) => Err(ValidationError {
            code: "22003".to_string(),
            message: format!("value \"{}\" is out of range for type integer", value),
            position: None,
        }),
    }
}

/// Validates an INT8 (bigint) value is within range
/// PostgreSQL INT8 range: -9223372036854775808 to 9223372036854775807
pub fn validate_int8(value: &str) -> Result<(), ValidationError> {
    let value = value.trim_matches('\'');
    
    match value.parse::<i64>() {
        Ok(_) => Ok(()),
        Err(_) => Err(ValidationError {
            code: "22003".to_string(),
            message: format!("value \"{}\" is out of range for type bigint", value),
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

    // Type modifier parsing tests
    #[test]
    fn test_parse_varchar_modifier() {
        assert_eq!(parse_type_modifier("VARCHAR(10)"), Some(10));
        assert_eq!(parse_type_modifier("varchar(255)"), Some(255));
        assert_eq!(parse_type_modifier("VARCHAR(1)"), Some(1));
    }

    #[test]
    fn test_parse_char_modifier() {
        assert_eq!(parse_type_modifier("CHAR(5)"), Some(5));
        assert_eq!(parse_type_modifier("char(1)"), Some(1));
        assert_eq!(parse_type_modifier("CHARACTER(100)"), Some(100));
    }

    #[test]
    fn test_parse_no_modifier() {
        assert_eq!(parse_type_modifier("TEXT"), None);
        assert_eq!(parse_type_modifier("INTEGER"), None);
        assert_eq!(parse_type_modifier("VARCHAR"), None);
    }

    #[test]
    fn test_extract_base_type() {
        assert_eq!(extract_base_type("VARCHAR(10)"), "VARCHAR");
        assert_eq!(extract_base_type("char(5)"), "CHAR");
        assert_eq!(extract_base_type("TEXT"), "TEXT");
        assert_eq!(extract_base_type("integer"), "INTEGER");
    }

    // Date validation tests
    #[test]
    fn test_valid_date() {
        assert!(validate_date("'2024-03-15'").is_ok());
        assert!(validate_date("2024-03-15").is_ok());
        assert!(validate_date("'2024-12-31'").is_ok());
    }

    #[test]
    fn test_valid_leap_year_date() {
        // 2024 is a leap year
        assert!(validate_date("'2024-02-29'").is_ok());
    }

    #[test]
    fn test_invalid_date_non_leap_year() {
        // 2023 is not a leap year
        let result = validate_date("'2023-02-29'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22008");
    }

    #[test]
    fn test_invalid_date_month() {
        let result = validate_date("'2024-13-01'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22008");
    }

    #[test]
    fn test_invalid_date_day() {
        let result = validate_date("'2024-04-31'"); // April has 30 days
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22008");
    }

    // Float4 validation tests
    #[test]
    fn test_valid_float4() {
        assert!(validate_float4("'3.14'").is_ok());
        assert!(validate_float4("'1.5e10'").is_ok());
        assert!(validate_float4("'-2.5'").is_ok());
    }

    #[test]
    fn test_float4_overflow() {
        // 10e70 is way beyond f32 range
        let result = validate_float4("'10e70'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22003");
    }

    #[test]
    fn test_float4_large_exponent() {
        let result = validate_float4("'1e100'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22003");
    }

    // Float8 validation tests
    #[test]
    fn test_valid_float8() {
        assert!(validate_float8("'3.14159265359'").is_ok());
        assert!(validate_float8("'1.5e100'").is_ok());
    }

    #[test]
    fn test_float8_overflow() {
        // 1e400 is beyond f64 range
        let result = validate_float8("'1e400'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22003");
    }

    // Integer validation tests
    #[test]
    fn test_valid_int2() {
        assert!(validate_int2("'32767'").is_ok());
        assert!(validate_int2("'-32768'").is_ok());
        assert!(validate_int2("'100'").is_ok());
    }

    #[test]
    fn test_int2_overflow() {
        let result = validate_int2("'32768'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22003");
    }

    #[test]
    fn test_valid_int4() {
        assert!(validate_int4("'2147483647'").is_ok());
        assert!(validate_int4("'-2147483648'").is_ok());
    }

    #[test]
    fn test_int4_overflow() {
        let result = validate_int4("'2147483648'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22003");
    }

    #[test]
    fn test_valid_int8() {
        assert!(validate_int8("'9223372036854775807'").is_ok());
        assert!(validate_int8("'-9223372036854775808'").is_ok());
    }

    #[test]
    fn test_int8_overflow() {
        let result = validate_int8("'9223372036854775808'");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "22003");
    }
}
