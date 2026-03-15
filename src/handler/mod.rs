//! Handler module for PGQT proxy server.
//!
//! This module contains the `SqliteHandler` struct which implements the PostgreSQL wire protocol
//! handler for translating PostgreSQL queries to SQLite.

use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use anyhow::Result;
use rusqlite::Connection;
use dashmap::DashMap;
use pgwire::api::portal::{Portal, Format};
use pgwire::api::stmt::{StoredStatement, QueryParser};
use pgwire::api::query::ExtendedQueryHandler;
use pgwire::api::results::{DescribePortalResponse, DescribeStatementResponse, FieldInfo, Response, DescribeResponse};
use pgwire::api::{ClientInfo, ClientPortalStore};
use pgwire::error::PgWireResult;
use pgwire::messages::PgWireBackendMessage;
use pgwire::messages::data::RowDescription;
use futures::Sink;
use async_trait::async_trait;

use crate::debug;
use crate::catalog::{init_catalog, init_system_views};
use crate::connection_pool::{ConnectionHandle, ConnectionPool};
use crate::schema::{SchemaManager, SearchPath};
use crate::copy;

// Thread-local storage for the current user during query execution
thread_local! {
    static CURRENT_USER: RefCell<String> = RefCell::new("postgres".to_string());
    static CURRENT_CLIENT_ID: RefCell<u32> = RefCell::new(0);
}

/// Set the current user for the current thread
pub fn set_current_user(user: &str) {
    CURRENT_USER.with(|u| *u.borrow_mut() = user.to_string());
}

/// Get the current user for the current thread
pub fn get_current_user() -> String {
    CURRENT_USER.with(|u| u.borrow().clone())
}

/// Set the current client ID for the current thread
pub fn set_current_client_id(cid: u32) {
    CURRENT_CLIENT_ID.with(|c| *c.borrow_mut() = cid);
}

/// Get the current client ID for the current thread
pub fn get_current_client_id() -> u32 {
    CURRENT_CLIENT_ID.with(|c| *c.borrow())
}

/// Helper function to parse a timestamp string into DateTime<Utc>
fn parse_datetime(ts: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, Utc, NaiveDateTime};
    
    // Try various formats
    DateTime::parse_from_rfc3339(ts).ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S%.f").ok()
                .map(|ndt| DateTime::from_naive_utc_and_offset(ndt, Utc))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()
                .map(|ndt| DateTime::from_naive_utc_and_offset(ndt, Utc))
        })
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(ts, "%Y-%m-%d").ok()
                .map(|d| DateTime::from_naive_utc_and_offset(d.and_hms_opt(0, 0, 0).unwrap(), Utc))
        })
}

/// Helper function to parse timezone offset string into minutes
/// Supports formats like: "+02", "-05:30", "+01:00", "UTC", "Europe/Prague"
fn parse_timezone_offset(tz_str: &str) -> i32 {
    use std::str::FromStr;
    
    let tz = tz_str.trim();
    
    // Handle special cases
    if tz.eq_ignore_ascii_case("utc") || tz.eq_ignore_ascii_case("gmt") {
        return 0;
    }
    
    // Handle common timezone abbreviations (simplified)
    let abbrev_offset = match tz.to_uppercase().as_str() {
        "EST" => -300,  // UTC-5
        "EDT" => -240,  // UTC-4
        "CST" => -360,  // UTC-6
        "CDT" => -300,  // UTC-5
        "MST" => -420,  // UTC-7
        "MDT" => -360,  // UTC-6
        "PST" => -480,  // UTC-8
        "PDT" => -420,  // UTC-7
        "IST" => 330,   // UTC+5:30
        "CET" => 60,    // UTC+1
        "CEST" => 120,  // UTC+2
        _ => i32::MAX,   // Not a recognized abbreviation
    };
    
    if abbrev_offset != i32::MAX {
        return abbrev_offset;
    }
    
    // Parse offset format: +HH, -HH, +HH:MM, -HH:MM
    if tz.starts_with('+') || tz.starts_with('-') {
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let rest = &tz[1..];
        
        // Try HH:MM format
        if let Some(colon_pos) = rest.find(':') {
            let hours = rest[..colon_pos].parse::<i32>().unwrap_or(0);
            let mins = rest[colon_pos + 1..].parse::<i32>().unwrap_or(0);
            return sign * (hours * 60 + mins);
        }
        
        // Try HH format (just hours)
        if let Ok(hours) = rest.parse::<i32>() {
            return sign * hours * 60;
        }
        
        // Try HHMM format (e.g., +0530 for IST)
        if rest.len() >= 2 {
            if let Ok(hours) = rest[..2].parse::<i32>() {
                let mins = if rest.len() > 2 {
                    rest[2..].parse::<i32>().unwrap_or(0)
                } else {
                    0
                };
                return sign * (hours * 60 + mins);
            }
        }
    }
    
    // Default to UTC for unrecognized timezones
    0
}

// Submodules
pub mod errors;
pub mod query;
pub mod rewriter;
pub mod transaction;
pub mod utils;

// Re-export commonly used items
pub use query::QueryExecution;
pub use utils::HandlerUtils;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Idle,           // Not in a transaction
    InTransaction,  // BEGIN called, no errors
    InError,        // Command failed inside transaction, must ROLLBACK
}

/// Session context for each client connection
#[derive(Debug, Clone)]
pub struct SessionContext {
    #[allow(dead_code)]
    pub authenticated_user: String,
    pub current_user: String,
    pub search_path: SearchPath,
    pub transaction_status: TransactionStatus,
    pub savepoints: Vec<String>,
    pub settings: std::collections::HashMap<String, String>,
}

impl SessionContext {
    pub fn new(authenticated_user: String) -> Self {
        Self {
            authenticated_user: authenticated_user.clone(),
            current_user: authenticated_user,
            search_path: SearchPath::default(),
            transaction_status: TransactionStatus::Idle,
            savepoints: Vec::new(),
            settings: std::collections::HashMap::new(),
        }
    }
}

/// PostgreSQL-to-SQLite proxy handler
#[derive(Clone)]
pub struct SqliteHandler {
    pub conn: Arc<Mutex<Connection>>,
    pub conn_pool: ConnectionPool,
    pub sessions: Arc<DashMap<u32, SessionContext>>,
    /// Per-client connections checked out from the pool
    pub client_connections: Arc<DashMap<u32, (Arc<Mutex<Connection>>, ConnectionHandle)>>,
    pub schema_manager: SchemaManager,
    pub copy_handler: copy::CopyHandler,
    pub functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>,
}

impl SqliteHandler {
    /// Create a new SqliteHandler with the given database path
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        init_catalog(&conn)?;
        init_system_views(&conn)?;

        let conn_arc = Arc::new(Mutex::new(conn));
        let copy_handler = crate::copy::CopyHandler::new(conn_arc.clone());

        // Create connection pool with default size of 10
        let conn_pool = ConnectionPool::new(std::path::Path::new(db_path), 10)?;

        let sessions = Arc::new(DashMap::new());

        let handler = Self {
            conn: conn_arc,
            conn_pool,
            sessions: sessions.clone(),
            client_connections: Arc::new(DashMap::new()),
            schema_manager: SchemaManager::new(std::path::Path::new(db_path)),
            copy_handler,
            functions: Arc::new(DashMap::new()),
        };

        // Register PostgreSQL-compatible functions
        Self::register_builtin_functions(&handler.conn.lock().unwrap(), handler.functions.clone(), sessions)?;

        // Register PL/pgSQL call wrappers
        handler.register_plpgsql_wrappers(&handler.conn.lock().unwrap())?;

        Ok(handler)
    }

    /// Register PL/pgSQL call wrappers
    pub fn register_plpgsql_wrappers(&self, conn: &Connection) -> Result<()> {
        use rusqlite::functions::FunctionFlags;

        // pgqt_plpgsql_call_scalar - Execute PL/pgSQL function and return scalar
        let functions_cache = self.functions().clone();
        conn.create_scalar_function("pgqt_plpgsql_call_scalar", -1, FunctionFlags::SQLITE_UTF8, move |ctx| {
            let func_name: String = ctx.get(0)?;
            let mut args = Vec::new();
            for i in 1..ctx.len() {
                args.push(ctx.get::<rusqlite::types::Value>(i)?);
            }

            // Look up function metadata from the cache
            let metadata = functions_cache.get(&func_name)
                .ok_or_else(|| rusqlite::Error::UserFunctionError(format!("Function {} not found", func_name).into()))?;

            // Create a new Lua runtime for this call
            let runtime = crate::plpgsql::PlPgSqlRuntime::new()
                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

            let temp_conn = Connection::open_in_memory()?; 
            
            // Reconstruct the CREATE FUNCTION statement to parse the PL/pgSQL
            let mut arg_defs = Vec::new();
            for (i, typ) in metadata.arg_types.iter().enumerate() {
                let name = if i < metadata.arg_names.len() && !metadata.arg_names[i].is_empty() {
                    metadata.arg_names[i].clone()
                } else {
                    format!("arg{}", i + 1)
                };
                arg_defs.push(format!("{} {}", name, typ));
            }
            let args_signature = arg_defs.join(", ");

            let create_sql = if metadata.function_body.to_uppercase().contains("BEGIN") {
                format!("CREATE FUNCTION {}({}) RETURNS {} AS $${}$$ LANGUAGE plpgsql;", 
                    func_name, args_signature, metadata.return_type, metadata.function_body)
            } else {
                format!("CREATE FUNCTION {}({}) RETURNS {} AS $$BEGIN {} END;$$ LANGUAGE plpgsql;", 
                    func_name, args_signature, metadata.return_type, metadata.function_body)
            };
            
            let parsed_func = crate::plpgsql::parse_plpgsql_function(&create_sql)
                .map_err(|e| {
                    eprintln!("Failed to parse PL/pgSQL: {}. SQL: {}", e, create_sql);
                    rusqlite::Error::UserFunctionError(e.into())
                })?;
            let lua_code = crate::plpgsql::transpile_to_lua(&parsed_func)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

            let result = runtime.execute_function(&temp_conn, &lua_code, &args)
                .map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;

            Ok(result)
        })?;

        Ok(())
    }

    /// Register built-in PostgreSQL-compatible functions with SQLite
    pub fn register_builtin_functions(conn: &Connection, _functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>, sessions: Arc<DashMap<u32, SessionContext>>) -> Result<()> {
        use rusqlite::functions::FunctionFlags;
        // Register current_user function that returns the session user
        conn.create_scalar_function("pgqt_current_user", 0, FunctionFlags::SQLITE_UTF8, |_ctx| {
            Ok(get_current_user())
        })?;


        // pg_get_userbyid - returns username for OID
        conn.create_scalar_function("pg_get_userbyid", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok("postgres".to_string())
        })?;

        // pg_table_is_visible - checks if table is visible in search path
        conn.create_scalar_function("pg_table_is_visible", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        // pg_type_is_visible - checks if type is visible
        conn.create_scalar_function("pg_type_is_visible", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        // pg_function_is_visible - checks if function is visible
        conn.create_scalar_function("pg_function_is_visible", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let _oid: i64 = ctx.get(0)?;
            Ok(true)
        })?;

        
        // pg_get_function_result - returns return type based on OID
        conn.create_scalar_function("pg_get_function_result", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let oid: i64 = ctx.get(0)?;
            // Built-in function OIDs
            let result = match oid {
                10001 => "timestamp with time zone", // now
                10002 => "timestamp with time zone", // current_timestamp
                10003 => "date",                      // current_date
                10004 => "time with time zone",       // current_time
                _ => "integer",                       // default for user functions
            };
            Ok(result.to_string())
        })?;

        // pg_get_function_identity_arguments - returns argument signature
        conn.create_scalar_function("pg_get_function_identity_arguments", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("".to_string())
        })?;

        // pg_get_function_arguments - returns formatted argument list
        let _functions_args = _functions.clone();
        conn.create_scalar_function("pg_get_function_arguments", 1, FunctionFlags::SQLITE_UTF8, move |ctx| {
            let _oid: i64 = ctx.get(0)?;
            
            // Look up function by OID in functions cache
            // We need to find the function name first or iterate the cache.
            // Since the cache is by name, let's find the name from the catalog if not in cache by OID.
            
            // For now, let's just return a placeholder or do a quick query.
            Ok("".to_string())
        })?;
        // repeat(text, int) - repeats text N times
        conn.create_scalar_function("repeat", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let s: String = ctx.get(0)?;
            let n: i64 = ctx.get(1)?;
            if n <= 0 {
                Ok("".to_string())
            } else {
                Ok(s.repeat(n as usize))
            }
        })?;

        // power(a, b) - mathematical power function (a^b)
        // PostgreSQL: power(2, 3) => 8.0
        conn.create_scalar_function("power", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let base: f64 = ctx.get(0)?;
            let exp: f64 = ctx.get(1)?;
            Ok(base.powf(exp))
        })?;

        // split_part(string, delimiter, index) - split string by delimiter and return nth part
        // PostgreSQL: split_part('abc~def~ghi', '~', 2) => 'def'
        // Negative index counts from end: -1 => last part
        conn.create_scalar_function("split_part", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let string: String = ctx.get(0)?;
            let delimiter: String = ctx.get(1)?;
            let index: i64 = ctx.get(2)?;
            
            let parts: Vec<&str> = string.split(&delimiter).collect();
            
            if index > 0 {
                // Positive index: 1-indexed from start
                let idx = (index - 1) as usize;
                Ok(parts.get(idx).map(|s| s.to_string()).unwrap_or_default())
            } else if index < 0 {
                // Negative index: count from end (-1 = last)
                let idx = (parts.len() as i64 + index) as usize;
                Ok(parts.get(idx).map(|s| s.to_string()).unwrap_or_default())
            } else {
                // Index 0 returns empty string
                Ok(String::new())
            }
        })?;

        // date_trunc(field, timestamp) - truncate timestamp to specified precision
        // PostgreSQL: date_trunc('year', '2024-03-15 10:30:45'::timestamp) => '2024-01-01 00:00:00'
        // Supported fields: millennium, century, decade, year, quarter, month, week, day, hour, minute, second
        conn.create_scalar_function("date_trunc", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, Datelike, Timelike, NaiveDateTime};
            
            let field: String = ctx.get::<String>(0)?.to_lowercase();
            let ts: String = ctx.get(1)?;
            
            // Parse the timestamp
            let dt = ts.parse::<DateTime<Utc>>().or_else(|_| {
                NaiveDateTime::parse_from_str(&ts, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            }).or_else(|_| {
                NaiveDateTime::parse_from_str(&ts, "%Y-%m-%d")
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            });
            
            match dt {
                Ok(dt) => {
                    let truncated = match field.as_str() {
                        "millennium" => {
                            let m = (dt.year() - 1) / 1000 + 1;
                            dt.with_year((m - 1) * 1000 + 1)
                                .and_then(|d| d.with_month(1))
                                .and_then(|d| d.with_day(1))
                                .and_then(|d| d.with_hour(0))
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "century" => {
                            let c = (dt.year() - 1) / 100 + 1;
                            dt.with_year((c - 1) * 100 + 1)
                                .and_then(|d| d.with_month(1))
                                .and_then(|d| d.with_day(1))
                                .and_then(|d| d.with_hour(0))
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "decade" => {
                            dt.with_year((dt.year() / 10) * 10)
                                .and_then(|d| d.with_month(1))
                                .and_then(|d| d.with_day(1))
                                .and_then(|d| d.with_hour(0))
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "year" => {
                            dt.with_month(1)
                                .and_then(|d| d.with_day(1))
                                .and_then(|d| d.with_hour(0))
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "quarter" => {
                            let q_month = ((dt.month() - 1) / 3) * 3 + 1;
                            dt.with_month(q_month)
                                .and_then(|d| d.with_day(1))
                                .and_then(|d| d.with_hour(0))
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "month" => {
                            dt.with_day(1)
                                .and_then(|d| d.with_hour(0))
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "week" => {
                            // Truncate to Monday of the week
                            let days_from_monday = dt.weekday().num_days_from_monday() as i64;
                            let monday = dt - chrono::Duration::days(days_from_monday);
                            monday.with_hour(0)
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "day" => {
                            dt.with_hour(0)
                                .and_then(|d| d.with_minute(0))
                                .and_then(|d| d.with_second(0))
                        }
                        "hour" => {
                            dt.with_minute(0)
                                .and_then(|d| d.with_second(0))
                        }
                        "minute" => {
                            dt.with_second(0)
                        }
                        "second" => Some(dt),
                        _ => Some(dt),
                    };
                    
                    match truncated {
                        Some(t) => Ok(t.format("%Y-%m-%d %H:%M:%S").to_string()),
                        None => Ok(ts), // Return original if truncation fails
                    }
                }
                Err(_) => Ok(ts), // Return original if parsing fails
            }
        })?;

        // date_part(field, source) - extract field from timestamp as double precision
        // PostgreSQL: date_part('year', '2024-03-15 10:30:45'::timestamp) => 2024
        // Supported fields: year, month, day, hour, minute, second, quarter, week, dow, doy, epoch, etc.
        conn.create_scalar_function("date_part", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, Datelike, Timelike, NaiveDateTime};
            
            let field: String = ctx.get::<String>(0)?.to_lowercase();
            let source: String = ctx.get(1)?;
            
            // Parse the timestamp
            let dt = source.parse::<DateTime<Utc>>().or_else(|_| {
                NaiveDateTime::parse_from_str(&source, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            }).or_else(|_| {
                NaiveDateTime::parse_from_str(&source, "%Y-%m-%d")
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            });
            
            match dt {
                Ok(dt) => {
                    let result = match field.as_str() {
                        "year" | "years" => dt.year() as f64,
                        "month" | "months" => dt.month() as f64,
                        "day" | "days" => dt.day() as f64,
                        "hour" | "hours" => dt.hour() as f64,
                        "minute" | "minutes" => dt.minute() as f64,
                        "second" | "seconds" => dt.second() as f64 + dt.timestamp_subsec_millis() as f64 / 1000.0,
                        "millisecond" | "milliseconds" => dt.timestamp_subsec_millis() as f64,
                        "microsecond" | "microseconds" => (dt.timestamp_subsec_micros() % 1000) as f64,
                        "quarter" | "quarters" => ((dt.month() - 1) / 3 + 1) as f64,
                        "week" | "weeks" => dt.iso_week().week() as f64,
                        "dow" | "dayofweek" => dt.weekday().num_days_from_sunday() as f64, // 0 = Sunday, 6 = Saturday
                        "isodow" => dt.weekday().number_from_monday() as f64, // 1 = Monday, 7 = Sunday
                        "doy" | "dayofyear" => dt.ordinal() as f64,
                        "epoch" => dt.timestamp() as f64, // Seconds since Unix epoch
                        "decade" | "decades" => (dt.year() / 10) as f64,
                        "century" | "centuries" => ((dt.year() - 1) / 100 + 1) as f64,
                        "millennium" | "millennia" => ((dt.year() - 1) / 1000 + 1) as f64,
                        "julian" => {
                            // Julian day number
                            let duration_since_epoch = dt.timestamp() as f64;
                            duration_since_epoch / 86400.0 + 2440587.5
                        }
                        _ => 0.0, // Unknown field
                    };
                    Ok(result)
                }
                Err(_) => Ok(0.0), // Return 0 if parsing fails
            }
        })?;

        // format_type - formats type OID to type name
        conn.create_scalar_function("format_type", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let type_oid: i64 = ctx.get(0)?;
            let _typemod: Option<i64> = ctx.get(1)?;

            let type_name = match type_oid {
                16 => "boolean",
                20 => "bigint",
                21 => "smallint",
                23 => "integer",
                25 => "text",
                700 => "real",
                701 => "double precision",
                1043 => "character varying",
                1114 => "timestamp without time zone",
                1184 => "timestamp with time zone",
                _ => "text",
            };
            Ok(type_name.to_string())
        })?;

        // to_char - format a value according to a format string
        // Basic implementation supporting common numeric formats
        conn.create_scalar_function("to_char", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: f64 = ctx.get(0)?;
            let format: String = ctx.get(1)?;
            
            // Map PostgreSQL format patterns to Rust format strings
            // FM9999.99 -> removes leading/trailing spaces
            // 9999.99 -> standard numeric format
            let format = format.trim_start_matches("FM");
            
            if format.contains('.') {
                // Determine decimal places from format
                let decimal_places = format.split('.').nth(1).map(|s| s.len()).unwrap_or(2);
                let format_str = format!("%.{}f", decimal_places);
                Ok(format_str.replace("{}", &format!("{:.1$}", value, decimal_places)))
            } else {
                // Integer format
                Ok(format!("{:.0}", value))
            }
        })?;

        // to_timestamp(unix_epoch) - convert Unix epoch to timestamp
        // PostgreSQL: to_timestamp(946684800) => '2000-01-01 00:00:00+00'
        conn.create_scalar_function("to_timestamp", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, NaiveDateTime};
            
            // Handle both integer and float inputs
            let epoch: f64 = match ctx.get_raw(0) {
                rusqlite::types::ValueRef::Integer(i) => i as f64,
                rusqlite::types::ValueRef::Real(f) => f,
                rusqlite::types::ValueRef::Text(t) => {
                    let s = std::str::from_utf8(t).unwrap_or("0");
                    s.parse().unwrap_or(0.0)
                }
                _ => 0.0,
            };
            
            // Handle special values
            if epoch.is_infinite() {
                if epoch.is_sign_positive() {
                    return Ok("infinity".to_string());
                } else {
                    return Ok("-infinity".to_string());
                }
            }
            
            if epoch.is_nan() {
                return Ok("NaN".to_string());
            }
            
            let secs = epoch as i64;
            let nanos = ((epoch - secs as f64) * 1_000_000_000.0) as u32;
            
            match DateTime::from_timestamp(secs, nanos) {
                Some(dt) => Ok(dt.format("%Y-%m-%d %H:%M:%S%.f%:z").to_string()),
                None => {
                    // Out of range - return approximate date
                    if epoch > 0.0 {
                        Ok("294276-12-31 23:59:59+00".to_string())
                    } else {
                        Ok("4714-11-24 00:00:00+00 BC".to_string())
                    }
                }
            }
        })?;

        // make_timestamptz(year, month, day, hour, min, sec [, timezone]) - create timestamp with time zone
        // PostgreSQL: make_timestamptz(2014, 12, 28, 6, 30, 45.887) => '2014-12-28 06:30:45.887+00'
        conn.create_scalar_function("make_timestamptz", 6, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, NaiveDate, NaiveTime, NaiveDateTime, FixedOffset, TimeZone};
            
            let year: i32 = ctx.get(0)?;
            let month: u32 = ctx.get(1)?;
            let day: u32 = ctx.get(2)?;
            let hour: u32 = ctx.get(3)?;
            let min: u32 = ctx.get(4)?;
            let sec: f64 = ctx.get(5)?;
            
            let sec_int = sec as u32;
            let micros = ((sec - sec_int as f64) * 1_000_000.0) as u32;
            
            match NaiveDate::from_ymd_opt(year, month, day)
                .and_then(|d| NaiveTime::from_hms_micro_opt(hour, min, sec_int, micros)
                    .map(|t| NaiveDateTime::new(d, t))) {
                Some(naive) => {
                    let dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
                    Ok(dt.format("%Y-%m-%d %H:%M:%S%.f%:z").to_string())
                }
                None => Ok(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}+00", 
                    year, month, day, hour, min, sec_int, micros))
            }
        })?;

        // make_timestamptz with timezone argument (7 args)
        conn.create_scalar_function("make_timestamptz", 7, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, NaiveDate, NaiveTime, NaiveDateTime, FixedOffset, TimeZone};
            
            let year: i32 = ctx.get(0)?;
            let month: u32 = ctx.get(1)?;
            let day: u32 = ctx.get(2)?;
            let hour: u32 = ctx.get(3)?;
            let min: u32 = ctx.get(4)?;
            let sec: f64 = ctx.get(5)?;
            let tz_str: String = ctx.get(6)?;
            
            let sec_int = sec as u32;
            let micros = ((sec - sec_int as f64) * 1_000_000.0) as u32;
            
            // Parse timezone offset (e.g., "+02", "-05:30", "Europe/Prague")
            let offset_minutes = parse_timezone_offset(&tz_str);
            let offset = FixedOffset::east_opt(offset_minutes * 60).unwrap_or(FixedOffset::east_opt(0).unwrap());
            
            match NaiveDate::from_ymd_opt(year, month, day)
                .and_then(|d| NaiveTime::from_hms_micro_opt(hour, min, sec_int, micros)
                    .map(|t| NaiveDateTime::new(d, t))) {
                Some(naive) => {
                    let dt = offset.from_local_datetime(&naive).single()
                        .unwrap_or_else(|| offset.from_utc_datetime(&naive));
                    Ok(dt.format("%Y-%m-%d %H:%M:%S%.f%:z").to_string())
                }
                None => Ok(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}+00", 
                    year, month, day, hour, min, sec_int, micros))
            }
        })?;

        // age(timestamp [, reference]) - calculate age between timestamps
        // PostgreSQL: age('2001-10-19 10:23:54', '2000-01-01') => '1 year 9 mons 18 days 10:23:54'
        conn.create_scalar_function("age", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, Datelike, Timelike, NaiveDateTime};
            
            let ts: String = ctx.get(0)?;
            let dt = parse_datetime(&ts).unwrap_or_else(|| Utc::now());
            let now = Utc::now();
            
            let years = now.year() - dt.year();
            let months = now.month() as i32 - dt.month() as i32;
            let days = now.day() as i32 - dt.day() as i32;
            let hours = now.hour() as i32 - dt.hour() as i32;
            let mins = now.minute() as i32 - dt.minute() as i32;
            let secs = now.second() as i32 - dt.second() as i32;
            
            let mut result = String::new();
            if years != 0 {
                result.push_str(&format!("{} year{} ", years.abs(), if years.abs() == 1 { "" } else { "s" }));
            }
            if months != 0 {
                result.push_str(&format!("{} mon{} ", months.abs(), if months.abs() == 1 { "" } else { "s" }));
            }
            if days != 0 {
                result.push_str(&format!("{} day{} ", days.abs(), if days.abs() == 1 { "" } else { "s" }));
            }
            result.push_str(&format!("{:02}:{:02}:{:02}", hours.abs(), mins.abs(), secs.abs()));
            
            Ok(result)
        })?;

        conn.create_scalar_function("age", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, Datelike, Timelike};
            
            let ts1: String = ctx.get(0)?;
            let ts2: String = ctx.get(1)?;
            
            let dt1 = parse_datetime(&ts1).unwrap_or_else(|| Utc::now());
            let dt2 = parse_datetime(&ts2).unwrap_or_else(|| Utc::now());
            
            let years = dt1.year() - dt2.year();
            let months = dt1.month() as i32 - dt2.month() as i32;
            let days = dt1.day() as i32 - dt2.day() as i32;
            let hours = dt1.hour() as i32 - dt2.hour() as i32;
            let mins = dt1.minute() as i32 - dt2.minute() as i32;
            let secs = dt1.second() as i32 - dt2.second() as i32;
            
            let mut result = String::new();
            if years != 0 {
                result.push_str(&format!("{} year{} ", years.abs(), if years.abs() == 1 { "" } else { "s" }));
            }
            if months != 0 {
                result.push_str(&format!("{} mon{} ", months.abs(), if months.abs() == 1 { "" } else { "s" }));
            }
            if days != 0 {
                result.push_str(&format!("{} day{} ", days.abs(), if days.abs() == 1 { "" } else { "s" }));
            }
            result.push_str(&format!("{:02}:{:02}:{:02}", hours.abs(), mins.abs(), secs.abs()));
            
            Ok(result)
        })?;

        // version - returns PostgreSQL version string
        conn.create_scalar_function("version", 0, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("PostgreSQL 15.0 (pgqt)".to_string())
        })?;

        // set_config(name, value, is_local) - returns new value
        let sessions_set = sessions.clone();
        conn.create_scalar_function("set_config", 3, FunctionFlags::SQLITE_UTF8, move |ctx| {
            let name: String = ctx.get(0)?;
            let value: String = ctx.get(1)?;
            let _is_local: bool = ctx.get(2)?;
            
            // In a UDF, we don't know the client_id directly.
            // As a workaround, we can use a thread-local client_id if we set it during query execution.
            // For now, let's update ALL sessions or just assume client 0 for testing if we can't get ID.
            // Better: update the session of the current thread if we can track it.
            
            // Let's use a thread-local for CLIENT_ID
            let cid = get_current_client_id();
            if let Some(mut session) = sessions_set.get_mut(&cid) {
                session.settings.insert(name.to_lowercase(), value.clone());
            }
            
            Ok(value)
        })?;

        // __pg_do_block(code) - executes PL/pgSQL code block
        conn.create_scalar_function("__pg_do_block", 1, FunctionFlags::SQLITE_UTF8, move |ctx| {
            let code: String = ctx.get(0)?;
            
            // Reconstruct a dummy CREATE FUNCTION to parse the block
            let dummy_sql = format!("CREATE FUNCTION __pg_do_block() RETURNS void AS $${}$$ LANGUAGE plpgsql;", code);
            
            let parsed_func = match crate::plpgsql::parse_plpgsql_function(&dummy_sql) {
                Ok(f) => f,
                Err(e) => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            };
            
            let lua_code = match crate::plpgsql::transpile_to_lua(&parsed_func) {
                Ok(l) => l,
                Err(e) => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            };
            
            let runtime = match crate::plpgsql::PlPgSqlRuntime::new() {
                Ok(r) => r,
                Err(e) => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            };
            
            // Note: In a real UDF we might not have the connection, but mlua runtime might need it for nested queries
            // For DO blocks, we don't pass arguments
            // We need a way to get the current connection inside the UDF if it wants to run SQL
            // For now, execute without connection-dependent SQL support inside DO blocks if needed,
            // or use a thread-local connection if available.
            
            // Since we're inside a rusqlite callback, we can't easily use the connection to run more queries
            // unless we use the raw handle. mlua runtime currently doesn't support this easily.
            
            let _result = match runtime.execute_function_no_conn(&lua_code, &[]) {
                Ok(r) => r,
                Err(e) => return Err(rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))),
            };
            
            Ok(1i64) // Return success
        })?;

        // __pg_comment_on(obj_type, obj_name, comment) - stores a comment in pg_description
        conn.create_scalar_function("__pg_comment_on", 3, FunctionFlags::SQLITE_UTF8, |ctx| {
            let _obj_type: String = ctx.get(0)?;
            let _obj_name: String = ctx.get(1)?;
            let _comment: String = ctx.get(2)?;
            
            // We need a connection to store the comment. 
            // Workaround: use a temporary connection or thread-local.
            // Since this is called via SELECT __pg_comment_on(...), we can't easily use the active connection
            // from within the UDF in rusqlite.
            
            Ok(1i64)
        })?;

        // __pg_create_enum(type_name, label, sort_order)
        conn.create_scalar_function("__pg_create_enum", 3, FunctionFlags::SQLITE_UTF8, |ctx| {
            let _type_name: String = ctx.get(0)?;
            let _label: String = ctx.get(1)?;
            let _sort_order: f64 = ctx.get(2)?;
            Ok(1i64) 
        })?;

        // current_schema - returns current schema name
        conn.create_scalar_function("current_schema", 0, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("public".to_string())
        })?;

        // current_schemas - returns array of schema names
        conn.create_scalar_function("current_schemas", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("{public}".to_string())
        })?;

        // current_setting - returns setting value
        let sessions_get = sessions.clone();
        conn.create_scalar_function("current_setting", 1, FunctionFlags::SQLITE_UTF8, move |ctx| {
            let name: String = ctx.get(0)?;
            let cid = get_current_client_id();
            if let Some(session) = sessions_get.get(&cid) {
                if let Some(val) = session.settings.get(&name.to_lowercase()) {
                    return Ok(val.clone());
                }
            }
            
            match name.as_str() {
                "server_version_num" => Ok("150000".to_string()),
                "server_version" => Ok("15.0".to_string()),
                "standard_conforming_strings" => Ok("on".to_string()),
                "client_encoding" => Ok("UTF8".to_string()),
                "search_path" => Ok("\"$user\", public".to_string()),
                "timezone" => Ok("UTC".to_string()),
                "TimeZone" => Ok("UTC".to_string()),
                "transaction_isolation" => Ok("read committed".to_string()),
                "transaction_isolation_level" => Ok("read committed".to_string()),
                "default_transaction_read_only" => Ok("off".to_string()),
                "statement_timeout" => Ok("0".to_string()),
                "application_name" => Ok("".to_string()),
                "DateStyle" => Ok("ISO, MDY".to_string()),
                "datestyle" => Ok("ISO, MDY".to_string()),
                _ => Ok("".to_string()),
            }
        })?;

        // pg_encoding_to_char - returns encoding name
        conn.create_scalar_function("pg_encoding_to_char", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let encoding: i64 = ctx.get(0)?;
            match encoding {
                0 => Ok("SQL_ASCII".to_string()),
                6 => Ok("UTF8".to_string()),
                _ => Ok("UTF8".to_string()),
            }
        })?;

        // array_to_string - converts array to string with delimiter
        conn.create_scalar_function("array_to_string", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let sep: String = ctx.get(1)?;
            crate::array::array_to_string_fn(&arr, &sep, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_to_string", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let sep: String = ctx.get(1)?;
            let null_str: String = ctx.get(2)?;
            crate::array::array_to_string_fn(&arr, &sep, Some(&null_str))
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // array_length - returns array length
        conn.create_scalar_function("array_length", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let dim: i32 = ctx.get(1)?;
            crate::array::array_length_fn(&arr, dim)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // cardinality - returns total number of elements
        conn.create_scalar_function("cardinality", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::array_cardinality(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // to_tsvector - creates full-text search vector
        conn.create_scalar_function("to_tsvector", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            Ok(crate::fts::to_tsvector_impl(&config, &text))
        })?;

        // to_tsquery - creates full-text search query
        conn.create_scalar_function("to_tsquery", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let config: String = ctx.get(0)?;
            let query: String = ctx.get(1)?;
            Ok(crate::fts::to_tsquery_impl(&config, &query))
        })?;

        // ts_rank - ranks document against query
        conn.create_scalar_function("ts_rank", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::ts_rank_impl(&tsvector, &tsquery))
        })?;

        // tsmatch - full-text search match operator
        conn.create_scalar_function("tsmatch", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let tsvector: String = ctx.get(0)?;
            let tsquery: String = ctx.get(1)?;
            Ok(crate::fts::tsvector_matches_tsquery(&tsvector, &tsquery))
        })?;

        // l2_distance - L2/Euclidean distance between two vectors
        conn.create_scalar_function("l2_distance", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l2_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // inner_product - inner/dot product of two vectors
        conn.create_scalar_function("inner_product", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::inner_product(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // cosine_distance - cosine distance between two vectors
        conn.create_scalar_function("cosine_distance", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::cosine_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Array functions
        conn.create_scalar_function("array_overlap", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::operators::array_overlap(&left, &right)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_contains", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::operators::array_contains(&left, &right)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_contained", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::operators::array_contained(&left, &right)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_append", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_append(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_prepend", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let elem: String = ctx.get(0)?;
            let arr: String = ctx.get(1)?;
            crate::array::functions::array_prepend(&elem, &arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_cat", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let left: String = ctx.get(0)?;
            let right: String = ctx.get(1)?;
            crate::array::functions::array_cat(&left, &right)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_remove", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_remove(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_replace", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let old: String = ctx.get(1)?;
            let new: String = ctx.get(2)?;
            crate::array::functions::array_replace(&arr, &old, &new)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_position", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_position_fn(&arr, &elem, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_positions", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::array::functions::array_positions_fn(&arr, &elem)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_ndims", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::functions::array_ndims_fn(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_dims", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            crate::array::functions::array_dims_fn(&arr)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("trim_array", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let arr: String = ctx.get(0)?;
            let n: i32 = ctx.get(1)?;
            crate::array::functions::trim_array_fn(&arr, n)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("array_fill", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: String = ctx.get(0)?;
            let dimensions: String = ctx.get(1)?;
            crate::array::functions::array_fill_fn(&value, &dimensions, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // string_to_array - splits string into array
        conn.create_scalar_function("string_to_array", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let text: String = ctx.get(0)?;
            let delimiter: String = ctx.get(1)?;
            crate::array::functions::string_to_array_fn(&text, &delimiter, None)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Geo functions
        conn.create_scalar_function("geo_distance", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let p1: String = ctx.get(0)?;
            let p2: String = ctx.get(1)?;
            crate::geo::point_distance(&p1, &p2)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_overlaps", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_overlaps(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_contains", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_contains(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_contained", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_contains(&b2, &b1)  // reverse order for contained
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_left", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_left(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_right", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_right(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_below", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_below(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("geo_above", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let b1: String = ctx.get(0)?;
            let b2: String = ctx.get(1)?;
            crate::geo::box_above(&b1, &b2)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // Range functions
        use crate::range::RangeType;

        conn.create_scalar_function("range_contains", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let range: String = ctx.get(0)?;
            let elem: String = ctx.get(1)?;
            crate::range::range_contains_elem(&range, &elem, RangeType::Int4)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        // int4range with 2 args (default bounds [))
        conn.create_scalar_function("int4range", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low = ctx.get_raw(0).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(0).unwrap().to_string());
            let high = ctx.get_raw(1).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(1).unwrap().to_string());
            let rv = crate::range::parse_range(&format!("[{},{})", low, high), RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        // int4range with 3 args (custom bounds)
        conn.create_scalar_function("int4range", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low = ctx.get_raw(0).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(0).unwrap().to_string());
            let high = ctx.get_raw(1).as_str().map(|s| s.to_string()).unwrap_or_else(|_| ctx.get::<i64>(1).unwrap().to_string());
            let bounds: String = ctx.get(2)?;
            let rv = crate::range::parse_range(&format!("{}{},{}{}", &bounds[0..1], low, high, &bounds[1..2]), RangeType::Int4)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        // daterange with 2 args (default bounds [))
        conn.create_scalar_function("daterange", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low: String = ctx.get(0)?;
            let high: String = ctx.get(1)?;
            let rv = crate::range::parse_range(&format!("[{},{})", low, high), RangeType::Date)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        // daterange with 3 args (custom bounds)
        conn.create_scalar_function("daterange", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let low: String = ctx.get(0)?;
            let high: String = ctx.get(1)?;
            let bounds: String = ctx.get(2)?;
            let rv = crate::range::parse_range(&format!("{}{},{}{}", &bounds[0..1], low, high, &bounds[1..2]), RangeType::Date)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(rv.to_postgres_string())
        })?;

        conn.create_scalar_function("isempty", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let range: String = ctx.get(0)?;
            crate::range::isempty(&range, RangeType::Int4)
                .map(|b| if b { 1i64 } else { 0i64 })
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("lower", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let val: String = ctx.get(0)?;
            match crate::range::lower(&val, RangeType::Int4) {
                Ok(opt) => Ok(opt.unwrap_or_default()),
                Err(_) => Ok(val.to_lowercase()),
            }
        })?;

        conn.create_scalar_function("upper", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let val: String = ctx.get(0)?;
            match crate::range::upper(&val, RangeType::Int4) {
                Ok(opt) => Ok(opt.unwrap_or_default()),
                Err(_) => Ok(val.to_uppercase()),
            }
        })?;

        // regexp - pattern matching function (used by ~ operator)
        conn.create_scalar_function("regexp", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            let regex = regex::Regex::new(&pattern)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(if regex.is_match(&text) { 1i64 } else { 0i64 })
        })?;

        // regexpi - case-insensitive pattern matching (used by ~* operator)        
        conn.create_scalar_function("regexpi", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            let regex = regex::Regex::new(&format!("(?i){}", pattern))
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(if regex.is_match(&text) { 1i64 } else { 0i64 })
        })?;

        // Register statistical aggregate functions
        crate::stats::register_statistical_functions(conn)?;

        // Register regex functions
        crate::regex_funcs::register_regex_functions(conn)?;

        // Register array_agg aggregate function
        crate::array_agg::register_array_agg(conn)?;

        // random - returns float between 0 and 1
        conn.create_scalar_function("random", 0, FunctionFlags::SQLITE_UTF8, |_ctx| {
            let r = unsafe { libc::rand() } as f64 / libc::RAND_MAX as f64;
            Ok(r)
        })?;

        // pg_typeof - returns type name of the value
        conn.create_scalar_function("pg_typeof", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let val = ctx.get_raw(0);
            match val {
                rusqlite::types::ValueRef::Null => Ok("unknown".to_string()),
                rusqlite::types::ValueRef::Integer(_) => Ok("integer".to_string()),
                rusqlite::types::ValueRef::Real(_) => Ok("double precision".to_string()),
                rusqlite::types::ValueRef::Text(_) => Ok("text".to_string()),
                rusqlite::types::ValueRef::Blob(_) => Ok("bytea".to_string()),
            }
        })?;

        // pg_get_viewdef - stub for view definition
        conn.create_scalar_function("pg_get_viewdef", -1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |_ctx| {
            Ok("SELECT 1".to_string())
        })?;

        // obj_description(oid, catalog) - returns comment for object
        conn.create_scalar_function("obj_description", 2, FunctionFlags::SQLITE_UTF8, |_ctx| {
            // This is tricky because we need to query __pg_description__
            // but we are inside a UDF callback.
            // For now, return NULL and let the test use pg_description directly if it wants,
            // or we implement it properly later.
            Ok(Option::<String>::None)
        })?;

        // date_bin(stride, source, origin) - bin timestamp into intervals
        // PostgreSQL: date_bin('15 minutes', '2024-03-15 10:23:45', '2000-01-01') => '2024-03-15 10:15:00'
        conn.create_scalar_function("date_bin", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::{DateTime, Utc, NaiveDateTime, Duration};

            let stride: String = ctx.get(0)?;
            let source: String = ctx.get(1)?;
            let origin: String = ctx.get(2)?;

            // Parse interval string (e.g., "15 minutes", "1 hour", "2 days")
            let stride_parts: Vec<&str> = stride.trim().split_whitespace().collect();
            if stride_parts.len() != 2 {
                return Ok(source); // Return original if parsing fails
            }

            let stride_num: i64 = match stride_parts[0].parse() {
                Ok(n) => n,
                Err(_) => return Ok(source),
            };

            let stride_seconds = match stride_parts[1].to_lowercase().as_str() {
                "microsecond" | "microseconds" => stride_num / 1_000_000,
                "millisecond" | "milliseconds" => stride_num / 1000,
                "second" | "seconds" => stride_num,
                "minute" | "minutes" => stride_num * 60,
                "hour" | "hours" => stride_num * 3600,
                "day" | "days" => stride_num * 86400,
                "week" | "weeks" => stride_num * 86400 * 7,
                _ => return Ok(source), // Unsupported unit
            };

            if stride_seconds <= 0 {
                return Ok(source);
            }

            // Parse timestamps
            let source_dt = source.parse::<DateTime<Utc>>().or_else(|_| {
                NaiveDateTime::parse_from_str(&source, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            }).or_else(|_| {
                // Date-only format - treat as midnight UTC
                chrono::NaiveDate::parse_from_str(&source, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            });

            let origin_dt = origin.parse::<DateTime<Utc>>().or_else(|_| {
                NaiveDateTime::parse_from_str(&origin, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            }).or_else(|_| {
                // Date-only format - treat as midnight UTC
                chrono::NaiveDate::parse_from_str(&origin, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                    .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            });

            match (source_dt, origin_dt) {
                (Ok(source_dt), Ok(origin_dt)) => {
                    let diff = source_dt.signed_duration_since(origin_dt);
                    let diff_seconds = diff.num_seconds();
                    let bin_number = diff_seconds / stride_seconds;
                    let binned = origin_dt + Duration::seconds(bin_number * stride_seconds);
                    Ok(binned.format("%Y-%m-%d %H:%M:%S").to_string())
                }
                _ => Ok(source), // Return original if parsing fails
            }
        })?;

        // to_date(text, format) - convert string to date
        // PostgreSQL: to_date('2024-03-15', 'YYYY-MM-DD') => '2024-03-15'
        conn.create_scalar_function("to_date", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use chrono::NaiveDate;

            let text: String = ctx.get(0)?;
            let format: String = ctx.get(1)?;

            // Map PostgreSQL format patterns to chrono format
            let chrono_format = format
                .replace("YYYY", "%Y")
                .replace("YY", "%y")
                .replace("MM", "%m")
                .replace("DD", "%d")
                .replace("HH24", "%H")
                .replace("HH12", "%I")
                .replace("MI", "%M")
                .replace("SS", "%S")
                .replace("Mon", "%b")
                .replace("Month", "%B");

            match NaiveDate::parse_from_str(&text, &chrono_format) {
                Ok(date) => Ok(date.format("%Y-%m-%d").to_string()),
                Err(_) => Ok(String::new()), // Return empty on parse failure
            }
        })?;

        // reverse(string) - reverse a string
        // PostgreSQL: reverse('abcde') => 'edcba'
        conn.create_scalar_function("reverse", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let s: String = ctx.get(0)?;
            Ok(s.chars().rev().collect::<String>())
        })?;

        // left(string, n) - get first n characters (or all but last |n| if n is negative)
        // PostgreSQL: left('hello', 2) => 'he', left('hello', -2) => 'hel'
        conn.create_scalar_function("left", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let s: String = ctx.get(0)?;
            let n: i64 = ctx.get(1)?;

            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;

            let result: String = if n >= 0 {
                // First n characters
                let end = n.min(len) as usize;
                chars[..end].iter().collect::<String>()
            } else {
                // All but last |n| characters
                let end = (len + n).max(0) as usize;
                chars[..end].iter().collect::<String>()
            };

            Ok(result)
        })?;

        // right(string, n) - get last n characters (or all but first |n| if n is negative)
        // PostgreSQL: right('hello', 2) => 'lo', right('hello', -2) => 'llo'
        conn.create_scalar_function("right", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let s: String = ctx.get(0)?;
            let n: i64 = ctx.get(1)?;

            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;

            let result: String = if n >= 0 {
                // Last n characters
                let start = (len - n.min(len)).max(0) as usize;
                chars[start..].iter().collect::<String>()
            } else {
                // All but first |n| characters
                let start = (-n).min(len) as usize;
                chars[start..].iter().collect::<String>()
            };

            Ok(result)
        })?;

        // concat(...) - concatenate all arguments, treating NULL as empty string
        // PostgreSQL: concat('a', 'b', 'c') => 'abc', concat('a', NULL, 'c') => 'ac'
        conn.create_scalar_function("concat", -1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let mut result = String::new();

            for i in 0..ctx.len() {
                // Try to get as string, skip if NULL
                if let Ok(s) = ctx.get::<String>(i) {
                    result.push_str(&s);
                } else if let Ok(n) = ctx.get::<i64>(i) {
                    result.push_str(&n.to_string());
                } else if let Ok(n) = ctx.get::<f64>(i) {
                    result.push_str(&n.to_string());
                }
                // NULL values are silently skipped
            }

            Ok(result)
        })?;

        // chr(code) - Convert ASCII/Unicode code to character
        // PostgreSQL: chr(65) => 'A', chr(945) => 'α'
        conn.create_scalar_function("chr", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let code: i32 = ctx.get(0)?;
            match std::char::from_u32(code as u32) {
                Some(c) => Ok(c.to_string()),
                None => Ok(String::new()),
            }
        })?;

        // lpad(string, length [, fill]) - Left pad string to specified length
        // PostgreSQL: lpad('hi', 5) => '   hi', lpad('hi', 5, 'x') => 'xxxhi'
        conn.create_scalar_function("lpad", -1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let string: String = ctx.get(0)?;
            let length: i64 = ctx.get(1)?;
            let fill: String = if ctx.len() > 2 {
                ctx.get(2)?
            } else {
                " ".to_string()
            };
            
            let str_len = string.chars().count() as i64;
            if str_len >= length {
                // Truncate if longer
                Ok(string.chars().take(length as usize).collect())
            } else {
                let pad_len = (length - str_len) as usize;
                let fill_chars: Vec<char> = fill.chars().collect();
                
                // Handle empty fill string - just return original string
                if fill_chars.is_empty() {
                    return Ok(string);
                }
                
                let mut result = String::new();
                
                // Build padding
                for i in 0..pad_len {
                    result.push(fill_chars[i % fill_chars.len()]);
                }
                result.push_str(&string);
                Ok(result)
            }
        })?;

        // rpad(string, length [, fill]) - Right pad string to specified length
        // PostgreSQL: rpad('hi', 5) => 'hi   ', rpad('hi', 5, 'x') => 'hixxx'
        conn.create_scalar_function("rpad", -1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let string: String = ctx.get(0)?;
            let length: i64 = ctx.get(1)?;
            let fill: String = if ctx.len() > 2 {
                ctx.get(2)?
            } else {
                " ".to_string()
            };
            
            let str_len = string.chars().count() as i64;
            if str_len >= length {
                // Truncate if longer
                Ok(string.chars().take(length as usize).collect())
            } else {
                let pad_len = (length - str_len) as usize;
                let fill_chars: Vec<char> = fill.chars().collect();
                
                // Handle empty fill string - just return original string
                if fill_chars.is_empty() {
                    return Ok(string);
                }
                
                let mut result = string;
                
                // Build padding
                for i in 0..pad_len {
                    result.push(fill_chars[i % fill_chars.len()]);
                }
                Ok(result)
            }
        })?;

        // translate(string, from_chars, to_chars) - Replace characters
        // PostgreSQL: translate('hello', 'l', 'L') => 'heLLo', translate('12345', '143', 'ax') => 'a2x5'
        conn.create_scalar_function("translate", 3, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let string: String = ctx.get(0)?;
            let from: String = ctx.get(1)?;
            let to: String = ctx.get(2)?;
            
            let mut result = String::new();
            for ch in string.chars() {
                if let Some(pos) = from.find(ch) {
                    // Character found in 'from', replace with corresponding 'to' char
                    if let Some(replacement) = to.chars().nth(pos) {
                        result.push(replacement);
                    }
                    // If no corresponding char in 'to', character is dropped
                } else {
                    // Character not in 'from', keep it
                    result.push(ch);
                }
            }
            Ok(result)
        })?;

        // format(format_string, ...) - Simple string formatting with %s placeholder
        // PostgreSQL: format('Hello %s', 'World') => 'Hello World'
        // Supports: %s (string), %% (literal %)
        conn.create_scalar_function("format", -1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            if ctx.len() < 1 {
                return Ok(String::new());
            }
            
            let format_str: String = ctx.get(0)?;
            let mut result = format_str;
            let mut arg_idx = 1;
            let mut search_pos = 0;
            
            // Simple placeholder replacement
            while let Some(pos) = result[search_pos..].find('%') {
                let abs_pos = search_pos + pos;
                if abs_pos + 1 >= result.len() {
                    break;
                }
                
                let placeholder = &result[abs_pos..abs_pos+2];
                let replacement = match placeholder {
                    "%s" => {
                        if arg_idx < ctx.len() {
                            let arg: String = ctx.get(arg_idx)?;
                            arg_idx += 1;
                            arg
                        } else {
                            // Not enough arguments - leave placeholder and move past it
                            search_pos = abs_pos + 2;
                            continue;
                        }
                    }
                    "%%" => "%".to_string(),
                    _ => {
                        // Unknown placeholder, skip it
                        search_pos = abs_pos + 2;
                        continue;
                    }
                };
                
                result.replace_range(abs_pos..abs_pos+2, &replacement);
                search_pos = abs_pos + replacement.len();
            }
            
            Ok(result)
        })?;

        // UUID Functions
        
        // like_escape(pattern, escape_char) - Escape special LIKE pattern characters
        // PostgreSQL internal function used by ORMs like Hibernate
        // Escapes % and _ characters in the pattern using the specified escape character
        conn.create_scalar_function("like_escape", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let pattern: String = ctx.get(0)?;
            let escape_char: String = ctx.get(1)?;
            
            if escape_char.is_empty() {
                return Ok(pattern);
            }
            
            let escape = escape_char.chars().next().unwrap_or('\\');
            let mut result = String::new();
            
            for ch in pattern.chars() {
                if ch == '%' || ch == '_' || ch == escape {
                    result.push(escape);
                }
                result.push(ch);
            }
            
            Ok(result)
        })?;

        // to_number(text, format) - Convert formatted string to numeric
        // PostgreSQL: to_number('12,454.8-', '99G999D9S') => -12454.8
        // Supports format patterns: 9, 0, ., ,, D, G, S, MI, PR, FM
        conn.create_scalar_function("to_number", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let text: String = ctx.get(0)?;
            let format: String = ctx.get(1)?;
            
            // Parse the format string
            let format_upper = format.to_uppercase();
            let is_fm = format_upper.starts_with("FM");
            let format_clean = if is_fm { &format_upper[2..] } else { &format_upper };
            
            // Determine decimal point and group separator from format
            let uses_dot_decimal = format_clean.contains('.');
            let uses_comma_group = format_clean.contains(',');
            let uses_d_decimal = format_clean.contains('D');
            let uses_g_group = format_clean.contains('G');
            
            // Determine sign handling
            let has_mi = format_clean.contains("MI");
            let has_pr = format_clean.contains("PR");
            let has_s = format_clean.contains('S');
            
            // Clean up the input text
            let mut cleaned = text.trim().to_string();
            
            // Handle PR (angle brackets for negative)
            let is_negative = if has_pr && cleaned.starts_with('<') && cleaned.ends_with('>') {
                cleaned = cleaned[1..cleaned.len()-1].to_string();
                true
            } else if has_mi && cleaned.ends_with('-') {
                cleaned = cleaned[..cleaned.len()-1].to_string();
                true
            } else if cleaned.ends_with('-') {
                cleaned = cleaned[..cleaned.len()-1].to_string();
                true
            } else if cleaned.starts_with('-') {
                cleaned = cleaned[1..].to_string();
                true
            } else if cleaned.starts_with('+') {
                cleaned = cleaned[1..].to_string();
                false
            } else {
                false
            };
            
            // Remove currency symbols and other non-numeric prefixes
            // Skip leading non-digit characters (except . and ,)
            while !cleaned.is_empty() && !cleaned.chars().next().unwrap().is_ascii_digit() 
                  && cleaned.chars().next().unwrap() != '.' && cleaned.chars().next().unwrap() != ',' {
                cleaned.remove(0);
            }
            
            // Remove group separators
            if uses_g_group || uses_comma_group {
                cleaned = cleaned.replace(',', "");
            }
            
            // Normalize decimal point
            if uses_d_decimal {
                // D is locale-dependent, but we'll assume '.' for simplicity
                // In a full implementation, this would use the current locale
            }
            
            // Parse the number
            let result: f64 = match cleaned.parse::<f64>() {
                Ok(n) => {
                    if is_negative { -n } else { n }
                }
                Err(_) => 0.0,
            };
            
            Ok(result)
        })?;

        // uuidv4() - Generate a random UUID (version 4)
        // Alias for gen_random_uuid()
        conn.create_scalar_function("uuidv4", 0, FunctionFlags::SQLITE_UTF8, |_ctx| {
            use uuid::Uuid;
            Ok(Uuid::new_v4().to_string())
        })?;
        
        // gen_random_uuid() - Generate a random UUID (version 4)
        conn.create_scalar_function("gen_random_uuid", 0, FunctionFlags::SQLITE_UTF8, |_ctx| {
            use uuid::Uuid;
            Ok(Uuid::new_v4().to_string())
        })?;
        
        // uuidv7() - Generate a time-ordered UUID (version 7)
        conn.create_scalar_function("uuidv7", 0, FunctionFlags::SQLITE_UTF8, |_ctx| {
            use uuid::Uuid;
            Ok(Uuid::now_v7().to_string())
        })?;
        
        // uuid_extract_version(uuid) - Extract version from UUID
        // Returns the version number (1, 4, 7, etc.)
        conn.create_scalar_function("uuid_extract_version", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use uuid::Uuid;
            let uuid_str: String = ctx.get(0)?;
            match Uuid::parse_str(&uuid_str) {
                Ok(uuid) => Ok(uuid.get_version_num() as i64),
                Err(_) => Ok(0i64),
            }
        })?;
        
        // uuid_extract_timestamp(uuid) - Extract timestamp from v1 or v7 UUID
        // Returns timestamp as string in ISO format
        conn.create_scalar_function("uuid_extract_timestamp", 1, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            use uuid::Uuid;
            use chrono::{DateTime, Utc};
            
            let uuid_str: String = ctx.get(0)?;
            match Uuid::parse_str(&uuid_str) {
                Ok(uuid) => {
                    match uuid.get_version_num() {
                        1 => {
                            // UUID v1 has embedded timestamp
                            let (secs, nanos) = uuid_to_timestamp_v1(&uuid);
                            match DateTime::from_timestamp(secs, nanos) {
                                Some(dt) => Ok(dt.format("%Y-%m-%d %H:%M:%S%.f").to_string()),
                                None => Ok(String::new()),
                            }
                        }
                        7 => {
                            // UUID v7 has Unix timestamp in milliseconds
                            let millis = uuid_to_timestamp_v7(&uuid);
                            let secs = (millis / 1000) as i64;
                            let nanos = ((millis % 1000) * 1_000_000) as u32;
                            match DateTime::from_timestamp(secs, nanos) {
                                Some(dt) => Ok(dt.format("%Y-%m-%d %H:%M:%S%.f").to_string()),
                                None => Ok(String::new()),
                            }
                        }
                        _ => Ok(String::new()),
                    }
                }
                Err(_) => Ok(String::new()),
            }
        })?;

        // pg_input_is_valid(value, type) - Check if value is valid for the specified type
        // Returns 1 (true) or 0 (false)
        conn.create_scalar_function("pg_input_is_valid", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: String = ctx.get(0)?;
            let type_name: String = ctx.get(1)?;
            
            let valid = match type_name.to_lowercase().as_str() {
                "varchar" | "character varying" => {
                    // Extract length if specified (e.g., varchar(4))
                    if let Some(start) = type_name.find('(') {
                        if let Some(end) = type_name.find(')') {
                            if let Ok(max_len) = type_name[start+1..end].parse::<usize>() {
                                value.len() <= max_len
                            } else {
                                true
                            }
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                }
                "int" | "integer" | "bigint" | "smallint" => {
                    value.parse::<i64>().is_ok()
                }
                "numeric" | "decimal" | "real" | "double" => {
                    value.parse::<f64>().is_ok()
                }
                "uuid" => {
                    use uuid::Uuid;
                    Uuid::parse_str(&value).is_ok()
                }
                _ => true, // Unknown types are considered valid
            };
            
            Ok(if valid { 1i64 } else { 0i64 })
        })?;

        // pg_input_error_info(value, type) - Returns error information for invalid values
        // Returns error message string or empty string if valid
        conn.create_scalar_function("pg_input_error_info", 2, FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC, |ctx| {
            let value: String = ctx.get(0)?;
            let type_name: String = ctx.get(1)?;
            
            let error_msg = match type_name.to_lowercase().as_str() {
                "varchar" | "character varying" => {
                    if let Some(start) = type_name.find('(') {
                        if let Some(end) = type_name.find(')') {
                            if let Ok(max_len) = type_name[start+1..end].parse::<usize>() {
                                if value.len() > max_len {
                                    Some(format!("value too long for type {} ({})", type_name, max_len))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                "int" | "integer" => {
                    if value.parse::<i64>().is_err() {
                        Some(format!("invalid input syntax for type integer: \"{}\"", value))
                    } else {
                        None
                    }
                }
                "uuid" => {
                    use uuid::Uuid;
                    if Uuid::parse_str(&value).is_err() {
                        Some(format!("invalid input syntax for type uuid: \"{}\"", value))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            
            Ok(error_msg.unwrap_or_default())
        })?;

        Ok(())
    }

    /// Get or checkout a per-session connection for a client
    /// 
    /// If the client already has a connection checked out, returns a clone of the Arc.
    /// Otherwise, checks out a new connection from the pool.
    #[allow(dead_code)]
    pub fn get_session_connection(&self, client_id: u32) -> Result<Arc<Mutex<Connection>>> {
        // Check if client already has a connection
        if let Some(entry) = self.client_connections.get(&client_id) {
            let (conn, _handle) = entry.value();
            return Ok(conn.clone());
        }

        // Checkout a new connection from the pool
        let (conn, handle) = self.conn_pool.checkout(client_id)?;
        self.client_connections.insert(client_id, (conn.clone(), handle));
        Ok(conn)
    }

    /// Return a per-session connection to the pool
    /// 
    /// Called when a client disconnects or when explicitly returning the connection.
    #[allow(dead_code)]
    pub fn return_session_connection(&self, client_id: u32) {
        if let Some((_, (conn, handle))) = self.client_connections.remove(&client_id) {
            // The handle will be dropped, which marks the connection as returned in the pool
            // Then we return the actual connection
            drop(handle);
            self.conn_pool.return_connection(conn);
        }
    }

    /// Get the shared legacy connection (for backwards compatibility during migration)
    #[allow(dead_code)]
    pub fn get_shared_connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }
}

use crate::transpiler::metadata::{ColumnInfo, MetadataProvider};

// Implement HandlerUtils trait for SqliteHandler
impl HandlerUtils for SqliteHandler {
    fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    fn sessions(&self) -> &Arc<DashMap<u32, SessionContext>> {
        &self.sessions
    }

    fn schema_manager(&self) -> &SchemaManager {
        &self.schema_manager
    }

    fn functions(&self) -> &Arc<DashMap<String, crate::catalog::FunctionMetadata>> {
        &self.functions
    }

    fn get_session_connection(&self, client_id: u32) -> Result<Arc<Mutex<Connection>>> {
        // Check if client already has a connection
        if let Some(entry) = self.client_connections.get(&client_id) {
            let (conn, _handle) = entry.value();
            return Ok(conn.clone());
        }

        // Checkout a new connection from the pool
        let (conn, handle) = self.conn_pool.checkout(client_id)?;
        self.client_connections.insert(client_id, (conn.clone(), handle));
        Ok(conn)
    }
}

impl MetadataProvider for SqliteHandler {
    fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>> {
        let conn = self.conn.lock().unwrap();
        
        match crate::catalog::get_table_columns_with_defaults(&conn, table_name) {
            Ok(metadata) => {
                let columns: Vec<ColumnInfo> = metadata
                    .into_iter()
                    .map(|m| {
                        let default_expr = m.constraints.as_ref()
                            .and_then(|c| crate::catalog::extract_default_from_constraints(c));
                        
                        ColumnInfo {
                            name: m.column_name,
                            original_type: m.original_type,
                            default_expr,
                            is_nullable: m.constraints.as_ref()
                                .map(|c| !c.to_uppercase().contains("NOT NULL"))
                                .unwrap_or(true),
                            type_oid: None,
                        }
                    })
                    .collect();
                
                if columns.is_empty() {
                    None
                } else {
                    Some(columns)
                }
            }
            Err(_) => None,
        }
    }
    
    fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        
        match crate::catalog::get_column_metadata(&conn, table_name, column_name) {
            Ok(Some(metadata)) => {
                metadata.constraints
                    .and_then(|c| crate::catalog::extract_default_from_constraints(&c))
            }
            _ => None,
        }
    }

    /// Check if a type is an enum and return its values
    fn get_enum_labels(&self, type_name: &str) -> Option<Vec<String>> {
        // Return None for common built-in types to avoid unnecessary catalog lookups
        let upper = type_name.to_uppercase();
        match upper.as_str() {
            "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "SERIAL" | "BIGSERIAL" | "TEXT" | "VARCHAR" | "BOOLEAN" | "BOOL" | "JSON" | "JSONB" | "REAL" | "DOUBLE PRECISION" | "TIMESTAMP" | "TIMESTAMPTZ" | "DATE" | "TIME" | "TIMETZ" | "UUID" | "BYTEA" | "NUMERIC" | "DECIMAL" | "INT4" | "INT8" | "INT2" | "FLOAT4" | "FLOAT8" | "OID" | "NAME" | "CHAR" | "BPCHAR" | "CHARACTER" | "BIT" | "VARBIT" | "REGCLASS" | "REGPROC" | "REGTYPE" | "BOX" | "POINT" | "LSEG" | "PATH" | "POLYGON" | "CIRCLE" | "CIDR" | "INET" | "MACADDR" | "MACADDR8" | "MONEY" | "TSVECTOR" | "TSQUERY" | "XML" => return None,
            _ => {}
        }
        if upper.contains("ARRAY") || upper.ends_with("[]") {
            return None;
        }

        let conn = self.conn.lock().unwrap();
        // Check if it's a known enum in the catalog
        if let Ok(true) = crate::catalog::is_enum_type(&conn, type_name) {
            let labels = crate::catalog::get_enum_values(&conn, type_name).unwrap_or_default();
            if !labels.is_empty() {
                return Some(labels);
            }
        }
        // Also try lowercase
        if let Ok(true) = crate::catalog::is_enum_type(&conn, &type_name.to_lowercase()) {
            let labels = crate::catalog::get_enum_values(&conn, &type_name.to_lowercase()).unwrap_or_default();
            if !labels.is_empty() {
                return Some(labels);
            }
        }
        None
    }
}

// Implement QueryExecution trait for SqliteHandler
impl QueryExecution for SqliteHandler {
    fn copy_handler(&self) -> &copy::CopyHandler {
        &self.copy_handler
    }
    
    fn as_metadata_provider(&self) -> Arc<dyn crate::transpiler::metadata::MetadataProvider> {
        Arc::new(self.clone())
    }
}

pub struct SqliteQueryParser;

#[async_trait]
impl QueryParser for SqliteQueryParser {
    type Statement = String;

    async fn parse_sql<C>(&self, _client: &C, sql: &str, _types: &[Option<pgwire::api::Type>]) -> PgWireResult<Self::Statement>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        Ok(sql.to_string())
    }

    fn get_parameter_types(&self, _stmt: &Self::Statement) -> PgWireResult<Vec<pgwire::api::Type>> {
        Ok(vec![])
    }

    fn get_result_schema(&self, _stmt: &Self::Statement, _format: Option<&Format>) -> PgWireResult<Vec<FieldInfo>> {
        Ok(vec![])
    }
}

#[async_trait]
impl ExtendedQueryHandler for SqliteHandler {
    type Statement = String;
    type QueryParser = SqliteQueryParser;

    fn query_parser(&self) -> Arc<Self::QueryParser> {
        Arc::new(SqliteQueryParser)
    }

    async fn do_query<C>(
        &self,
        client: &mut C,
        portal: &Portal<Self::Statement>,
        _max_rows: usize,
    ) -> PgWireResult<Response>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        pgwire::error::PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        // Get unique client identifier from PostgreSQL PID
        let (pid, _) = client.pid_and_secret_key();
        let client_id = pid as u32;

        let query = &portal.statement.statement;
        let params = &portal.parameters;
        
        debug!("Extended query from client {}: {}", client_id, query);
        
        // Convert params to Option<String> for execute_query_params
        let mut param_strings = Vec::new();
        for (i, param) in params.iter().enumerate() {
            if let Some(bytes) = param {
                if portal.parameter_format.is_binary(i) {
                    // Get type from statement if available
                    let pg_type = portal.statement.parameter_types.get(i)
                        .and_then(|t| t.as_ref())
                        .unwrap_or(&pgwire::api::Type::UNKNOWN);
                    
                    debug!("Parameter {} is binary, type: {:?}", i, pg_type);
                    
                    match *pg_type {
                        pgwire::api::Type::INT4 | pgwire::api::Type::OID | pgwire::api::Type::REGCLASS | pgwire::api::Type::INT2 => {
                            if bytes.len() == 4 {
                                let b: [u8; 4] = bytes.as_ref().try_into().unwrap_or([0; 4]);
                                let val = i32::from_be_bytes(b);
                                param_strings.push(Some(val.to_string()));
                                continue;
                            } else if bytes.len() == 2 {
                                let b: [u8; 2] = bytes.as_ref().try_into().unwrap_or([0; 2]);
                                let val = i16::from_be_bytes(b);
                                param_strings.push(Some(val.to_string()));
                                continue;
                            }
                        }
                        pgwire::api::Type::INT8 => {
                            if bytes.len() == 8 {
                                let b: [u8; 8] = bytes.as_ref().try_into().unwrap_or([0; 8]);
                                let val = i64::from_be_bytes(b);
                                param_strings.push(Some(val.to_string()));
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                param_strings.push(Some(String::from_utf8_lossy(bytes).to_string()));
            } else {
                param_strings.push(None);
            }
        }

        match self.execute_query_params(client_id, query, &param_strings) {
            Ok(mut responses) => {
                if let Some(resp) = responses.pop() {
                    // Force RowDescription for SELECTs if not already sent by client Describe
                    if let Response::Query(ref query_resp) = resp {
                        let fields = query_resp.row_schema();
                        let _row_desc = RowDescription::new(fields.iter().map(|f| {
                            pgwire::messages::data::FieldDescription::new(
                                f.name().to_string(),
                                f.table_id().unwrap_or(0),
                                f.column_id().unwrap_or(0),
                                f.datatype().oid(),
                                0,
                                0,
                                f.format().value(),
                            )
                        }).collect());
                        // println!("DEBUG: Sending forced RowDescription");
                        // client.send(PgWireBackendMessage::RowDescription(row_desc)).await?;
                    }
                    Ok(resp)
                } else {
                    Ok(Response::Execution(pgwire::api::results::Tag::new("OK")))
                }
            }
            Err(e) => {
                eprintln!("Error executing extended query: {}", e);
                let pg_err = crate::handler::errors::PgError::from_anyhow(e);
                Ok(Response::Error(Box::new(pg_err.into_error_info())))
            }
        }
    }

    async fn do_describe_statement<C>(
        &self,
        _client: &mut C,
        statement: &StoredStatement<Self::Statement>,
    ) -> PgWireResult<DescribeStatementResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        pgwire::error::PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let query = &statement.statement;
        debug!("Describe statement: {}", query);
        let _result = crate::transpiler::transpile_with_metadata(query);
        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
        ctx.set_metadata_provider(self.as_metadata_provider());
        let transpile_result = crate::transpiler::transpile_with_context(query, &mut ctx);
        
        let conn = self.conn().lock().unwrap();
        if let Ok(stmt) = conn.prepare(&transpile_result.sql) {
            let fields = self.build_field_info(&stmt, &transpile_result.referenced_tables, &conn, &transpile_result.column_aliases, &transpile_result.column_types)
                .unwrap_or_default();
            
            // For parameters, we don't know the types yet easily, so return UNKNOWN or derived from statement.parameter_types
            let param_types = statement.parameter_types.iter().map(|t| t.clone().unwrap_or(pgwire::api::Type::UNKNOWN)).collect();
            
            debug!("Returning {} fields for Describe statement", fields.len());
            return Ok(DescribeStatementResponse::new(param_types, fields));
        }
        
        Ok(DescribeStatementResponse::no_data())
    }

    async fn do_describe_portal<C>(
        &self,
        _client: &mut C,
        portal: &Portal<Self::Statement>,
    ) -> PgWireResult<DescribePortalResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        pgwire::error::PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let query = &portal.statement.statement;
        debug!("Describe portal: {}", query);
        let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
        ctx.set_metadata_provider(self.as_metadata_provider());
        let transpile_result = crate::transpiler::transpile_with_context(query, &mut ctx);
        
        let conn = self.conn().lock().unwrap();
        if let Ok(stmt) = conn.prepare(&transpile_result.sql) {
            let fields = self.build_field_info(&stmt, &transpile_result.referenced_tables, &conn, &transpile_result.column_aliases, &transpile_result.column_types)
                .unwrap_or_default();
            debug!("Returning {} fields for Describe portal", fields.len());
            return Ok(DescribePortalResponse::new(fields));
        }
        
        Ok(DescribePortalResponse::new(vec![]))
    }
}

// UUID Helper Functions

/// Extract timestamp from UUID v1
/// Returns (seconds, nanoseconds) since Unix epoch
fn uuid_to_timestamp_v1(uuid: &uuid::Uuid) -> (i64, u32) {
    let bytes = uuid.as_bytes();
    
    // UUID v1 timestamp is stored in a special format
    // bytes 0-3: low time
    // bytes 4-5: mid time  
    // bytes 6-7: high time (with version in top 4 bits)
    let low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64;
    let mid = u16::from_le_bytes([bytes[4], bytes[5]]) as u64;
    let high = (u16::from_le_bytes([bytes[6], bytes[7]]) & 0x0FFF) as u64;
    
    // Combine to get 60-bit timestamp (100-nanosecond intervals since Oct 15, 1582)
    let timestamp_100ns = ((high as u64) << 48) | ((mid as u64) << 32) | (low as u64);
    
    // Convert to Unix epoch (Oct 15, 1582 to Jan 1, 1970 is 122192928000000000 100ns intervals)
    let unix_timestamp_100ns = timestamp_100ns.saturating_sub(0x01B21DD213814000);
    
    // Convert to seconds and nanoseconds
    let secs = (unix_timestamp_100ns / 10_000_000) as i64;
    let nanos = ((unix_timestamp_100ns % 10_000_000) * 100) as u32;
    
    (secs, nanos)
}

/// Extract timestamp from UUID v7
/// Returns milliseconds since Unix epoch
fn uuid_to_timestamp_v7(uuid: &uuid::Uuid) -> u64 {
    let bytes = uuid.as_bytes();
    
    // UUID v7 stores Unix timestamp in milliseconds in the first 48 bits (big-endian)
    ((bytes[0] as u64) << 40) |
    ((bytes[1] as u64) << 32) |
    ((bytes[2] as u64) << 24) |
    ((bytes[3] as u64) << 16) |
    ((bytes[4] as u64) << 8) |
    (bytes[5] as u64)
}
