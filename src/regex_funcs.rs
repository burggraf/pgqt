//! Regular expression functions for PostgreSQL compatibility
//!
//! This module implements PostgreSQL regex functions using the Rust `regex` crate:
//! - regexp_replace(string, pattern, replacement [, flags])
//! - regexp_substr(string, pattern [, start [, flags]])
//! - regexp_instr(string, pattern [, start [, occurrence [, flags]]])

use regex::Regex;
use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;

/// Register all regex functions with the SQLite connection
pub fn register_regex_functions(conn: &Connection) -> anyhow::Result<()> {
    // regexp_replace(string, pattern, replacement [, flags])
    // PostgreSQL: replaces first match by default, 'g' flag for all matches
    conn.create_scalar_function("regexp_replace", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let replacement: String = ctx.get(2)?;
        
        match Regex::new(&pattern) {
            Ok(re) => Ok(re.replace(&string, &replacement).to_string()), // First match only
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_replace with 4 arguments (including flags)
    // Flags: 'g' = global (replace all), 'i' = case-insensitive, 'm' = multiline
    conn.create_scalar_function("regexp_replace", 4, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let replacement: String = ctx.get(2)?;
        let flags: String = ctx.get(3)?;
        
        // Build regex with flags
        let mut pattern_with_flags = pattern.clone();
        if flags.contains('i') {
            pattern_with_flags = format!("(?i){}", pattern);
        }
        if flags.contains('m') {
            pattern_with_flags = format!("(?m){}", pattern_with_flags);
        }
        if flags.contains('s') {
            pattern_with_flags = format!("(?s){}", pattern_with_flags);
        }
        if flags.contains('x') {
            pattern_with_flags = format!("(?x){}", pattern_with_flags);
        }
        
        let global = flags.contains('g');
        
        match Regex::new(&pattern_with_flags) {
            Ok(re) => {
                if global {
                    Ok(re.replace_all(&string, &replacement).to_string())
                } else {
                    Ok(re.replace(&string, &replacement).to_string())
                }
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_substr(string, pattern [, start [, flags]])
    conn.create_scalar_function("regexp_substr", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        
        match Regex::new(&pattern) {
            Ok(re) => {
                Ok(re.find(&string)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default())
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_substr with start position
    conn.create_scalar_function("regexp_substr", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let start: i64 = ctx.get(2)?;
        
        // PostgreSQL uses 1-based indexing
        let start_pos = ((start - 1).max(0) as usize).min(string.len());
        let substr = &string[start_pos..];
        
        match Regex::new(&pattern) {
            Ok(re) => {
                Ok(re.find(substr)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default())
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_substr with start position and flags
    conn.create_scalar_function("regexp_substr", 4, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let start: i64 = ctx.get(2)?;
        let flags: String = ctx.get(3)?;
        
        // Build regex with flags
        let mut pattern_with_flags = pattern.clone();
        if flags.contains('i') {
            pattern_with_flags = format!("(?i){}", pattern);
        }
        if flags.contains('m') {
            pattern_with_flags = format!("(?m){}", pattern_with_flags);
        }
        
        // PostgreSQL uses 1-based indexing
        let start_pos = ((start - 1).max(0) as usize).min(string.len());
        let substr = &string[start_pos..];
        
        match Regex::new(&pattern_with_flags) {
            Ok(re) => {
                Ok(re.find(substr)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default())
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_instr(string, pattern [, start [, occurrence [, flags [, subexpr]]]])
    conn.create_scalar_function("regexp_instr", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        
        match Regex::new(&pattern) {
            Ok(re) => {
                Ok(re.find(&string)
                    .map(|m| (m.start() + 1) as i64) // 1-indexed
                    .unwrap_or(0))
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_instr with start position
    conn.create_scalar_function("regexp_instr", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let start: i64 = ctx.get(2)?;
        
        // PostgreSQL uses 1-based indexing
        let start_pos = ((start - 1).max(0) as usize).min(string.len());
        let substr = &string[start_pos..];
        
        match Regex::new(&pattern) {
            Ok(re) => {
                Ok(re.find(substr)
                    .map(|m| (start_pos + m.start() + 1) as i64) // 1-indexed
                    .unwrap_or(0))
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_instr with start position and occurrence
    conn.create_scalar_function("regexp_instr", 4, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let start: i64 = ctx.get(2)?;
        let occurrence: i64 = ctx.get(3)?;
        
        // PostgreSQL uses 1-based indexing
        let start_pos = ((start - 1).max(0) as usize).min(string.len());
        let substr = &string[start_pos..];
        
        match Regex::new(&pattern) {
            Ok(re) => {
                let matches: Vec<_> = re.find_iter(substr).collect();
                let occ_idx = (occurrence - 1).max(0) as usize;
                Ok(matches.get(occ_idx)
                    .map(|m| (start_pos + m.start() + 1) as i64) // 1-indexed
                    .unwrap_or(0))
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_instr with 5 arguments (start, occurrence, flags)
    conn.create_scalar_function("regexp_instr", 5, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        let start: i64 = ctx.get(2)?;
        let occurrence: i64 = ctx.get(3)?;
        let flags: String = ctx.get(4)?;
        
        // Build regex with flags
        let mut pattern_with_flags = pattern.clone();
        if flags.contains('i') {
            pattern_with_flags = format!("(?i){}", pattern);
        }
        if flags.contains('m') {
            pattern_with_flags = format!("(?m){}", pattern_with_flags);
        }
        
        // PostgreSQL uses 1-based indexing
        let start_pos = ((start - 1).max(0) as usize).min(string.len());
        let substr = &string[start_pos..];
        
        match Regex::new(&pattern_with_flags) {
            Ok(re) => {
                let matches: Vec<_> = re.find_iter(substr).collect();
                let occ_idx = (occurrence - 1).max(0) as usize;
                Ok(matches.get(occ_idx)
                    .map(|m| (start_pos + m.start() + 1) as i64) // 1-indexed
                    .unwrap_or(0))
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_match(string, pattern [, flags]) - returns array of matched groups
    conn.create_scalar_function("regexp_match", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        
        match Regex::new(&pattern) {
            Ok(re) => {
                if let Some(caps) = re.captures(&string) {
                    // Return as PostgreSQL array format
                    let groups: Vec<String> = caps.iter()
                        .skip(1) // Skip the full match
                        .map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
                        .collect();
                    if groups.is_empty() {
                        Ok("{}".to_string())
                    } else {
                        Ok(format!("{{{}}}", groups.join(",")))
                    }
                } else {
                    Ok("".to_string()) // No match returns empty string (NULL in PostgreSQL)
                }
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_matches(string, pattern [, flags]) - returns all matches as arrays
    // This is typically a set-returning function, but we implement as scalar returning JSON-like string
    conn.create_scalar_function("regexp_matches", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        
        match Regex::new(&pattern) {
            Ok(re) => {
                let all_matches: Vec<String> = re.captures_iter(&string)
                    .map(|caps| {
                        let groups: Vec<String> = caps.iter()
                            .skip(1)
                            .map(|m| m.map(|m| format!("\"{}\"", m.as_str())).unwrap_or("NULL".to_string()))
                            .collect();
                        format!("{{{}}}", groups.join(","))
                    })
                    .collect();
                
                if all_matches.is_empty() {
                    Ok("".to_string())
                } else {
                    Ok(all_matches.join("|"))
                }
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    // regexp_split_to_array(string, pattern [, flags])
    conn.create_scalar_function("regexp_split_to_array", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
        let string: String = ctx.get(0)?;
        let pattern: String = ctx.get(1)?;
        
        match Regex::new(&pattern) {
            Ok(re) => {
                let parts: Vec<String> = re.split(&string)
                    .map(|s| format!("\"{}\"", s))
                    .collect();
                Ok(format!("{{{}}}", parts.join(",")))
            }
            Err(e) => Err(rusqlite::Error::UserFunctionError(
                format!("invalid regex: {}", e).into()
            )),
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_regex_functions(&conn).unwrap();
        conn
    }

    #[test]
    fn test_regexp_replace_basic() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_replace('foobarbaz', 'b..', 'X')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "fooXbaz");
    }

    #[test]
    fn test_regexp_replace_all() {
        let conn = setup_db();
        // Use 'g' flag for global replacement (all matches)
        let result: String = conn
            .query_row("SELECT regexp_replace('aaabbaa', 'b', 'X', 'g')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "aaaXXaa");
    }

    #[test]
    fn test_regexp_replace_case_insensitive() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_replace('FooBar', 'bar', 'baz', 'i')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "Foobaz");
    }

    #[test]
    fn test_regexp_substr_basic() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_substr('foobarbaz', 'b..')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "bar");
    }

    #[test]
    fn test_regexp_substr_no_match() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_substr('foobarbaz', 'xyz')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_regexp_instr_basic() {
        let conn = setup_db();
        let result: i64 = conn
            .query_row("SELECT regexp_instr('foobarbaz', 'bar')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, 4); // 1-indexed position
    }

    #[test]
    fn test_regexp_instr_no_match() {
        let conn = setup_db();
        let result: i64 = conn
            .query_row("SELECT regexp_instr('foobarbaz', 'xyz')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, 0); // 0 means not found
    }

    #[test]
    fn test_regexp_instr_with_occurrence() {
        let conn = setup_db();
        let result: i64 = conn
            .query_row("SELECT regexp_instr('ababab', 'ab', 1, 3)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, 5); // Third occurrence starts at position 5
    }

    #[test]
    fn test_regexp_match() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_match('foobarbequebaz', '(bar)(beque)')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{bar,beque}");
    }

    #[test]
    fn test_regexp_split_to_array() {
        let conn = setup_db();
        let result: String = conn
            .query_row("SELECT regexp_split_to_array('hello world test', '\\s+')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "{\"hello\",\"world\",\"test\"}");
    }
}