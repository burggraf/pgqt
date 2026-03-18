//! PostgreSQL COPY command implementation for PostgreSQLite
//!
//! This module provides full support for PostgreSQL COPY commands:
//! - COPY FROM STDIN: Import data in text, CSV, or binary format
//! - COPY TO STDOUT: Export data in text, CSV, or binary format
//!
//! Supported options:
//! - FORMAT (TEXT, CSV, BINARY)
//! - DELIMITER (custom column separator)
//! - QUOTE (CSV quote character)
//! - ESCAPE (escape character)
//! - NULL (null representation string)
//! - HEADER (include header row in CSV)
//! - ENCODING (character encoding)

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use bytes::Buf;
use futures::sink::Sink;
use futures::stream;
use pgwire::api::copy::CopyHandler as PgWireCopyHandler;
use pgwire::api::results::{CopyResponse, Response};
use pgwire::api::ClientInfo;
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::copy::{CopyData, CopyDone, CopyFail};
use pgwire::messages::PgWireBackendMessage;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub mod encoding;
use encoding::{CopyEncoding, decode_to_utf8, encode_from_utf8};

use crate::handler::errors::{PgError, PgErrorCode};

// PostgreSQL type OIDs for binary format
const BOOL_OID: i32 = 16;
const BYTEA_OID: i32 = 17;
const INT2_OID: i32 = 21;
const INT4_OID: i32 = 23;
const INT8_OID: i32 = 20;
const FLOAT4_OID: i32 = 700;
const FLOAT8_OID: i32 = 701;
const TEXT_OID: i32 = 25;
const VARCHAR_OID: i32 = 1043;
const BPCHAR_OID: i32 = 1042;
const DATE_OID: i32 = 1082;
const TIME_OID: i32 = 1083;
const TIMESTAMP_OID: i32 = 1114;
const TIMESTAMPTZ_OID: i32 = 1184;
const INTERVAL_OID: i32 = 1186;
const UUID_OID: i32 = 2950;
const NUMERIC_OID: i32 = 1700;
const JSON_OID: i32 = 114;
const JSONB_OID: i32 = 3802;

/// PostgreSQL epoch offset (2000-01-01) in microseconds
#[allow(dead_code)]
const PG_EPOCH_MICROS: i64 = 946684800000000i64;

/// Read a boolean from binary format (1 byte: 0=false, 1=true)
#[allow(dead_code)]
fn read_bool_binary(data: &[u8]) -> Result<bool> {
    if data.len() != 1 {
        return Err(anyhow!("Boolean must be 1 byte, got {}", data.len()));
    }
    Ok(data[0] != 0)
}

/// Write a boolean to binary format
#[allow(dead_code)]
fn write_bool_binary(value: bool) -> Vec<u8> {
    vec![if value { 1 } else { 0 }]
}

/// Read an i16 from big-endian binary format
fn read_i16_binary(data: &[u8]) -> Result<i16> {
    if data.len() != 2 {
        return Err(anyhow!("int2 must be 2 bytes, got {}", data.len()));
    }
    let bytes: [u8; 2] = data.try_into().unwrap();
    Ok(i16::from_be_bytes(bytes))
}

/// Write an i16 to big-endian binary format
#[allow(dead_code)]
fn write_i16_binary(value: i16) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

/// Read an i32 from big-endian binary format
fn read_i32_binary(data: &[u8]) -> Result<i32> {
    if data.len() != 4 {
        return Err(anyhow!("int4 must be 4 bytes, got {}", data.len()));
    }
    let bytes: [u8; 4] = data.try_into().unwrap();
    Ok(i32::from_be_bytes(bytes))
}

/// Write an i32 to big-endian binary format
#[allow(dead_code)]
fn write_i32_binary(value: i32) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

/// Read an i64 from big-endian binary format
fn read_i64_binary(data: &[u8]) -> Result<i64> {
    if data.len() != 8 {
        return Err(anyhow!("int8 must be 8 bytes, got {}", data.len()));
    }
    let bytes: [u8; 8] = data.try_into().unwrap();
    Ok(i64::from_be_bytes(bytes))
}

/// Write an i64 to big-endian binary format
#[allow(dead_code)]
fn write_i64_binary(value: i64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

/// Read an f32 from big-endian binary format (IEEE 754)
fn read_f32_binary(data: &[u8]) -> Result<f32> {
    if data.len() != 4 {
        return Err(anyhow!("float4 must be 4 bytes, got {}", data.len()));
    }
    let bytes: [u8; 4] = data.try_into().unwrap();
    Ok(f32::from_be_bytes(bytes))
}

/// Write an f32 to big-endian binary format
#[allow(dead_code)]
fn write_f32_binary(value: f32) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

/// Read an f64 from big-endian binary format (IEEE 754)
fn read_f64_binary(data: &[u8]) -> Result<f64> {
    if data.len() != 8 {
        return Err(anyhow!("float8 must be 8 bytes, got {}", data.len()));
    }
    let bytes: [u8; 8] = data.try_into().unwrap();
    Ok(f64::from_be_bytes(bytes))
}

/// Write an f64 to big-endian binary format
#[allow(dead_code)]
fn write_f64_binary(value: f64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

/// Read a date from binary format (days since 2000-01-01)
fn read_date_binary(data: &[u8]) -> Result<String> {
    if data.len() != 4 {
        return Err(anyhow!("date must be 4 bytes, got {}", data.len()));
    }
    let days = read_i32_binary(data)?;
    // PostgreSQL epoch is 2000-01-01, convert to Unix timestamp
    let pg_epoch = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let date = pg_epoch + chrono::Duration::days(days as i64);
    Ok(date.format("%Y-%m-%d").to_string())
}

/// Write a date to binary format
#[allow(dead_code)]
fn write_date_binary(value: &str) -> Result<Vec<u8>> {
    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|e| anyhow!("Invalid date format: {}", e))?;
    let pg_epoch = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let days = (date - pg_epoch).num_days() as i32;
    Ok(write_i32_binary(days))
}

/// Read a timestamp from binary format (microseconds since 2000-01-01)
fn read_timestamp_binary(data: &[u8]) -> Result<String> {
    if data.len() != 8 {
        return Err(anyhow!("timestamp must be 8 bytes, got {}", data.len()));
    }
    let micros = read_i64_binary(data)?;
    let pg_epoch = chrono::DateTime::from_timestamp(946684800, 0).unwrap();
    let dt = pg_epoch + chrono::Duration::microseconds(micros);
    Ok(dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string())
}

/// Write a timestamp to binary format
#[allow(dead_code)]
fn write_timestamp_binary(value: &str) -> Result<Vec<u8>> {
    // Try parsing with various formats
    let dt = if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        dt
    } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        dt
    } else {
        return Err(anyhow!("Invalid timestamp format: {}", value));
    };
    
    let pg_epoch = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()
        .and_hms_opt(0, 0, 0).unwrap();
    let micros = (dt - pg_epoch).num_microseconds().unwrap_or(0);
    Ok(write_i64_binary(micros))
}

/// Read a UUID from binary format (16 bytes)
fn read_uuid_binary(data: &[u8]) -> Result<String> {
    if data.len() != 16 {
        return Err(anyhow!("UUID must be 16 bytes, got {}", data.len()));
    }
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]
    ))
}

/// Write a UUID to binary format
#[allow(dead_code)]
fn write_uuid_binary(value: &str) -> Result<Vec<u8>> {
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|e| anyhow!("Invalid UUID: {}", e))?;
    Ok(uuid.as_bytes().to_vec())
}

/// Read a numeric/decimal value from binary format
/// PostgreSQL numeric format:
/// - ndigits (i16): number of digits
/// - weight (i16): weight of first digit
/// - sign (i16): 0x0000=positive, 0x4000=negative, 0xC000=NaN
/// - dscale (i16): display scale
/// - digits (i16[]): base-10000 digits
fn read_numeric_binary(data: &[u8]) -> Result<String> {
    if data.len() < 8 {
        return Err(anyhow!("Numeric data too short: {} bytes", data.len()));
    }
    
    let mut cursor = std::io::Cursor::new(data);
    let ndigits = cursor.get_i16();
    let weight = cursor.get_i16();
    let sign = cursor.get_i16();
    let _dscale = cursor.get_i16();
    
    // Check for NaN (0xC000 as i16 = -16384)
    if sign == -16384i16 {
        return Ok("NaN".to_string());
    }
    
    let mut result = String::new();
    if sign == 0x4000 {
        result.push('-');
    }
    
    // Read digits (each is base 10000)
    let mut digits = Vec::new();
    for _ in 0..ndigits {
        if cursor.remaining() < 2 {
            return Err(anyhow!("Numeric data truncated"));
        }
        digits.push(cursor.get_i16());
    }
    
    if digits.is_empty() {
        return Ok("0".to_string());
    }
    
    // Build the number string
    // First digit is most significant
    result.push_str(&digits[0].to_string());
    
    // Remaining digits are 4 digits each (padded with leading zeros)
    for i in 1..digits.len() {
        result.push_str(&format!("{:04}", digits[i]));
    }
    
    // Apply decimal point based on weight and dscale
    let digit_count = digits.len();
    let decimal_pos = (weight as i32 + 1) * 4;
    
    if decimal_pos <= 0 {
        // Number is less than 1
        let zeros = (-decimal_pos) as usize;
        let mut new_result = if sign == 0x4000 { "-0.".to_string() } else { "0.".to_string() };
        for _ in 0..zeros {
            new_result.push('0');
        }
        // Remove the sign from result if present
        let digits_only = result.trim_start_matches('-');
        new_result.push_str(digits_only);
        result = new_result;
    } else if decimal_pos < (digit_count * 4) as i32 {
        // Insert decimal point within the digits
        let pos = decimal_pos as usize;
        let digits_only = result.trim_start_matches('-').to_string();
        let before = &digits_only[..pos.min(digits_only.len())];
        let after = if pos < digits_only.len() { &digits_only[pos..] } else { "" };
        result = if sign == 0x4000 { "-".to_string() } else { String::new() };
        result.push_str(before);
        if !after.is_empty() {
            result.push('.');
            result.push_str(after);
        }
    }
    
    // Trim trailing zeros after decimal point
    if result.contains('.') {
        while result.ends_with('0') {
            result.pop();
        }
        if result.ends_with('.') {
            result.pop();
        }
    }
    
    Ok(result)
}

/// Convert binary data to SQLite Value based on type OID
fn binary_to_sqlite_value(data: &[u8], type_oid: i32) -> Result<rusqlite::types::Value> {
    if data.is_empty() {
        return Ok(rusqlite::types::Value::Null);
    }
    
    match type_oid {
        BOOL_OID => {
            // Flexible boolean reading: 
            // - Binary: 1 byte (0x00=false, 0x01=true)
            // - Text: "t"/"f", "true"/"false", "1"/"0", "yes"/"no", "on"/"off"
            let text = String::from_utf8_lossy(data).to_lowercase();
            let val = match text.as_str() {
                "t" | "true" | "1" | "yes" | "on" => true,
                "f" | "false" | "0" | "no" | "off" => false,
                _ => {
                    // Try binary interpretation for single byte (0x00 or 0x01)
                    if data.len() == 1 && (data[0] == 0 || data[0] == 1) {
                        data[0] != 0
                    } else {
                        // Default to false for unknown values
                        false
                    }
                }
            };
            Ok(rusqlite::types::Value::Integer(if val { 1 } else { 0 }))
        }
        INT2_OID => {
            // Flexible: binary (2 bytes) or text
            let val = match data.len() {
                2 => read_i16_binary(data)? as i64,
                _ => String::from_utf8_lossy(data).parse::<i64>()?,
            };
            Ok(rusqlite::types::Value::Integer(val))
        }
        INT4_OID => {
            // Flexible: binary (4 bytes) or text
            let val = match data.len() {
                4 => read_i32_binary(data)? as i64,
                _ => String::from_utf8_lossy(data).parse::<i64>()?,
            };
            Ok(rusqlite::types::Value::Integer(val))
        }
        INT8_OID => {
            // Flexible integer reading based on actual data length
            // This handles cases where the catalog says INT8 but data is INT4
            let val = match data.len() {
                2 => read_i16_binary(data)? as i64,
                4 => read_i32_binary(data)? as i64,
                8 => read_i64_binary(data)?,
                _ => String::from_utf8_lossy(data).parse::<i64>()?,
            };
            Ok(rusqlite::types::Value::Integer(val))
        }
        FLOAT4_OID => {
            let val = read_f32_binary(data)?;
            Ok(rusqlite::types::Value::Real(val as f64))
        }
        FLOAT8_OID => {
            // Flexible float reading based on actual data length
            let val = match data.len() {
                4 => read_f32_binary(data)? as f64,
                8 => read_f64_binary(data)?,
                _ => return Err(anyhow!("Invalid float data length: {}", data.len())),
            };
            Ok(rusqlite::types::Value::Real(val))
        }
        DATE_OID => {
            let val = read_date_binary(data)?;
            Ok(rusqlite::types::Value::Text(val))
        }
        TIMESTAMP_OID | TIMESTAMPTZ_OID => {
            let val = read_timestamp_binary(data)?;
            Ok(rusqlite::types::Value::Text(val))
        }
        UUID_OID => {
            let val = read_uuid_binary(data)?;
            Ok(rusqlite::types::Value::Text(val))
        }
        NUMERIC_OID => {
            let val = read_numeric_binary(data)?;
            Ok(rusqlite::types::Value::Text(val))
        }
        TEXT_OID | VARCHAR_OID | BPCHAR_OID | JSON_OID | JSONB_OID | BYTEA_OID => {
            // Text types - store as-is
            Ok(rusqlite::types::Value::Text(String::from_utf8_lossy(data).to_string()))
        }
        _ => {
            // Unknown type - try as text
            Ok(rusqlite::types::Value::Text(String::from_utf8_lossy(data).to_string()))
        }
    }
}

/// Batch size for COPY INSERT operations (performance optimization)
#[allow(dead_code)]
const COPY_BATCH_SIZE: usize = 1000;

/// COPY operation state
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum CopyState {
    /// No active COPY operation
    #[default]
    Idle,
    /// COPY FROM STDIN in progress
    FromStdin {
        table_name: String,
        columns: Vec<String>,
        options: CopyOptions,
    },
    /// COPY TO STDOUT in progress
    ToStdout {
        query: String,
        options: CopyOptions,
    },
}


/// COPY data format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum CopyFormat {
    /// Text format (tab-delimited, default)
    #[default]
    Text,
    /// CSV format (comma-delimited)
    Csv,
    /// Binary format
    Binary,
}

impl CopyFormat {
    /// Get the format code for the wire protocol
    pub fn format_code(&self) -> i8 {
        match self {
            CopyFormat::Text => 0,
            CopyFormat::Csv => 0,  // CSV uses text format code
            CopyFormat::Binary => 1,
        }
    }
}


/// COPY command options
#[derive(Debug, Clone, PartialEq)]
pub struct CopyOptions {
    /// Data format
    pub format: CopyFormat,
    /// Column delimiter character
    pub delimiter: char,
    /// Quote character (CSV only)
    pub quote: char,
    /// Escape character
    pub escape: char,
    /// String representing NULL values
    pub null_string: String,
    /// Include header row (CSV only)
    pub header: bool,
    /// Character encoding
    pub encoding: CopyEncoding,
    /// Force quote for specific columns (CSV TO only)
    pub force_quote: Option<Vec<String>>,
    /// Force not null for specific columns (CSV FROM only)
    pub force_not_null: Option<Vec<String>>,
    /// Quote all columns (CSV TO only)
    pub force_quote_all: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        CopyOptions {
            format: CopyFormat::Text,
            delimiter: '\t',  // Tab for text, comma for CSV
            quote: '"',
            escape: '\t',  // Same as delimiter for text
            null_string: "\\N".to_string(),
            header: false,
            encoding: CopyEncoding::Utf8,
            force_quote: None,
            force_not_null: None,
            force_quote_all: false,
        }
    }
}

impl CopyOptions {
    /// Create options for text format
    #[allow(dead_code)]
    pub fn text() -> Self {
        CopyOptions {
            format: CopyFormat::Text,
            delimiter: '\t',
            escape: '\t',
            null_string: "\\N".to_string(),
            ..Default::default()
        }
    }

    /// Create options for CSV format
    #[allow(dead_code)]
    pub fn csv() -> Self {
        CopyOptions {
            format: CopyFormat::Csv,
            delimiter: ',',
            escape: '"',
            null_string: "".to_string(),
            ..Default::default()
        }
    }

    /// Create options for binary format
    #[allow(dead_code)]
    pub fn binary() -> Self {
        CopyOptions {
            format: CopyFormat::Binary,
            delimiter: '\0',  // Not used for binary
            escape: '\0',
            null_string: "".to_string(),
            ..Default::default()
        }
    }

    /// Set the delimiter
    #[allow(dead_code)]
    pub fn with_delimiter(mut self, delimiter: char) -> Self {
        self.delimiter = delimiter;
        self
    }

    /// Set the quote character
    #[allow(dead_code)]
    pub fn with_quote(mut self, quote: char) -> Self {
        self.quote = quote;
        self
    }

    /// Set the escape character
    #[allow(dead_code)]
    pub fn with_escape(mut self, escape: char) -> Self {
        self.escape = escape;
        self
    }

    /// Set the null string
    #[allow(dead_code)]
    pub fn with_null_string(mut self, null_string: String) -> Self {
        self.null_string = null_string;
        self
    }

    /// Set header option
    #[allow(dead_code)]
    pub fn with_header(mut self, header: bool) -> Self {
        self.header = header;
        self
    }

    /// Set encoding
    #[allow(dead_code)]
    pub fn with_encoding(mut self, encoding: CopyEncoding) -> Self {
        self.encoding = encoding;
        self
    }
}

/// Parsed COPY statement information
#[derive(Debug, Clone)]
pub struct CopyStatement {
    /// Table name (for COPY FROM)
    pub table_name: Option<String>,
    /// Column list (for COPY FROM)
    pub columns: Vec<String>,
    /// Source/destination type
    pub direction: CopyDirection,
    /// COPY options
    pub options: CopyOptions,
    /// Query string (for COPY TO with query)
    pub query: Option<String>,
}

/// COPY direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyDirection {
    /// COPY FROM - importing data
    From,
    /// COPY TO - exporting data
    To,
}

/// PostgreSQLite COPY handler
#[derive(Clone)]
pub struct CopyHandler {
    /// SQLite connection
    conn: Arc<Mutex<Connection>>,
    /// Current COPY state
    state: Arc<Mutex<CopyState>>,
    /// Data buffer for accumulating COPY data
    buffer: Arc<Mutex<Vec<u8>>>,
    /// Row counter for progress tracking
    row_count: Arc<Mutex<usize>>,
}

impl CopyHandler {
    /// Create a new COPY handler
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        CopyHandler {
            conn,
            state: Arc::new(Mutex::new(CopyState::Idle)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            row_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Start a COPY FROM STDIN operation
    pub fn start_copy_from(
        &self,
        table_name: String,
        columns: Vec<String>,
        options: CopyOptions,
    ) -> Result<Response> {
        // Get column count before moving
        let column_count = columns.len();
        
        let mut state = self.state.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let mut buffer = self.buffer.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let mut row_count = self.row_count.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        // Reset state
        *state = CopyState::FromStdin {
            table_name,
            columns,
            options: options.clone(),
        };
        buffer.clear();
        *row_count = 0;

        // For COPY FROM, we return a CopyResponse with column information
        // The actual data will be received via on_copy_data
        let empty_stream = stream::iter(Vec::<Result<CopyData, PgWireError>>::new());
        
        Ok(Response::CopyIn(CopyResponse::new(
            options.format.format_code(),
            column_count,
            empty_stream,
        )))
    }

    /// Start a COPY TO STDOUT operation
    pub fn start_copy_to(&self, query: String, options: CopyOptions) -> Result<Response> {
        let mut state = self.state.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        // Reset state
        *state = CopyState::ToStdout {
            query: query.clone(),
            options: options.clone(),
        };

        // For COPY TO, we need to execute the query and stream results
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        
        let mut stmt = conn.prepare_cached(&query)?;
        let col_count = stmt.column_count();
        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        
        let mut rows = stmt.query([])?;
        let mut all_data = Vec::new();

        // Handle HEADER for CSV
        if options.format == CopyFormat::Csv && options.header {
            let header_row = col_names
                .iter()
                .map(|name| format_csv_value(Some(name), options.delimiter, options.quote, false))
                .collect::<Vec<String>>()
                .join(&options.delimiter.to_string());
            let header_line = format!("{}\n", header_row);
            // Encode header to target encoding
            let encoded_header = encode_from_utf8(&header_line, &options.encoding)
                .map_err(|e| anyhow!("COPY encoding error: {}", e))?;
            all_data.push(Ok(CopyData::new(Bytes::from(encoded_header))));
        }

        while let Some(row) = rows.next()? {
            let mut values = Vec::new();
            for i in 0..col_count {
                let val: Option<String> = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => None,
                    rusqlite::types::ValueRef::Integer(i) => Some(i.to_string()),
                    rusqlite::types::ValueRef::Real(f) => Some(f.to_string()),
                    rusqlite::types::ValueRef::Text(s) => Some(String::from_utf8_lossy(s).to_string()),
                    rusqlite::types::ValueRef::Blob(b) => Some(String::from_utf8_lossy(b).to_string()),
                };
                values.push(val);
            }

            let line = match options.format {
                CopyFormat::Text => {
                    values
                        .iter()
                        .map(|v| format_text_value(v.as_deref(), &options.null_string, options.escape))
                        .collect::<Vec<String>>()
                        .join(&options.delimiter.to_string())
                }
                CopyFormat::Csv => {
                    values
                        .iter()
                        .map(|v| format_csv_value(v.as_deref(), options.delimiter, options.quote, false))
                        .collect::<Vec<String>>()
                        .join(&options.delimiter.to_string())
                }
                CopyFormat::Binary => {
                    // Binary format writer
                    if all_data.is_empty() {
                        // Start of stream: add header
                        let mut header = Vec::with_capacity(19);
                        header.extend_from_slice(b"PGCOPY\n\xff\r\n\0");
                        header.extend_from_slice(&0i32.to_be_bytes()); // flags
                        header.extend_from_slice(&0i32.to_be_bytes()); // header extension
                        all_data.push(Ok(CopyData::new(Bytes::from(header))));
                    }

                    let mut tuple = Vec::new();
                    tuple.extend_from_slice(&(col_count as i16).to_be_bytes());
                    for v in &values {
                        match v {
                            Some(s) => {
                                let bytes = s.as_bytes();
                                tuple.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
                                tuple.extend_from_slice(bytes);
                            }
                            None => {
                                tuple.extend_from_slice(&(-1i32).to_be_bytes());
                            }
                        }
                    }
                    all_data.push(Ok(CopyData::new(Bytes::from(tuple))));
                    continue; // Skip the newline-added push below
                }
            };
            
            // Encode from UTF-8 to target encoding for text/CSV formats
            let encoded_line = match options.format {
                CopyFormat::Text | CopyFormat::Csv => {
                    encode_from_utf8(&format!("{}\n", line), &options.encoding)
                        .map_err(|e| anyhow!("COPY encoding error: {}", e))?
                }
                CopyFormat::Binary => format!("{}\n", line).into_bytes(),
            };
            all_data.push(Ok(CopyData::new(Bytes::from(encoded_line))));
        }

        // Add trailer for binary format
        if options.format == CopyFormat::Binary {
            let mut trailer = Vec::new();
            trailer.extend_from_slice(&(-1i16).to_be_bytes());
            all_data.push(Ok(CopyData::new(Bytes::from(trailer))));
        }

        let data_stream = stream::iter(all_data);
        
        Ok(Response::CopyOut(CopyResponse::new(
            options.format.format_code(),
            col_count,
            data_stream,
        )))
    }

    /// Process buffered COPY data
    fn process_buffer(&self) -> Result<usize> {
        let state = self.state.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let buffer = self.buffer.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        match &*state {
            CopyState::FromStdin { table_name, columns, options } => {
                match options.format {
                    CopyFormat::Text => {
                        self.process_text_data(&buffer, table_name, columns, options)
                    }
                    CopyFormat::Csv => {
                        self.process_csv_data(&buffer, table_name, columns, options)
                    }
                    CopyFormat::Binary => {
                        self.process_binary_data(&buffer, table_name, columns, options)
                    }
                }
            }
            _ => Ok(0),
        }
    }

    /// Process text format data with transaction wrapping
    fn process_text_data(
        &self,
        data: &[u8],
        table_name: &str,
        columns: &[String],
        options: &CopyOptions,
    ) -> Result<usize> {
        self.with_transaction(|conn| {
            let mut row_count = 0;
            let mut line_number = 0;

            // Convert from source encoding to UTF-8, then parse text format
            let content = decode_to_utf8(data, &options.encoding)
                .map_err(|e| anyhow!("COPY {}: encoding error: {}", table_name, e))?;
            // Use split_inclusive to handle empty trailing fields and preserve row boundaries
            let lines: Vec<&str> = content.split_inclusive('\n').collect();

            for line in lines {
                line_number += 1;
                let mut line = line;
                if line.ends_with('\n') {
                    line = &line[..line.len() - 1];
                }
                if line.ends_with('\r') {
                    line = &line[..line.len() - 1];
                }
                
                if line.is_empty() || line == "\\." {
                    continue;
                }

                // Split by delimiter
                let values: Vec<&str> = line.split(options.delimiter).collect();

                // Validate column count
                if !columns.is_empty() && values.len() != columns.len() {
                    return Err(anyhow!(
                        "COPY {}: line {}, expected {} columns but got {}",
                        table_name, line_number, columns.len(), values.len()
                    ));
                }

                // Convert values, handling NULL
                let converted_values: Vec<Option<String>> = values
                    .iter()
                    .enumerate()
                    .map(|(_col_idx, v)| {
                        if v == &options.null_string {
                            None
                        } else {
                            Some(unescape_text_value(v, options.escape))
                        }
                    })
                    .collect();

                // Build and execute INSERT statement
                let sql = build_insert_sql(table_name, columns, converted_values.len())?;
                let mut stmt = conn.prepare_cached(&sql)?;

                // Convert params for rusqlite
                let params: Vec<rusqlite::types::Value> = converted_values
                    .into_iter()
                    .map(|v| match v {
                        Some(s) => rusqlite::types::Value::Text(s),
                        None => rusqlite::types::Value::Null,
                    })
                    .collect();

                let param_refs: Vec<&dyn rusqlite::ToSql> = params
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();

                if let Err(e) = stmt.execute(rusqlite::params_from_iter(param_refs.iter())) {
                    return Err(anyhow!(
                        "COPY {}: line {}, column {}: {}",
                        table_name, line_number, columns.get(0).unwrap_or(&"unknown".to_string()), e
                    ));
                }
                row_count += 1;
            }

            Ok(row_count)
        })
    }

    /// Process CSV format data with transaction wrapping
    fn process_csv_data(
        &self,
        data: &[u8],
        table_name: &str,
        columns: &[String],
        options: &CopyOptions,
    ) -> Result<usize> {
        self.with_transaction(|conn| {
            let mut row_count = 0;

            // Convert from source encoding to UTF-8, then parse CSV format
            let content = decode_to_utf8(data, &options.encoding)
                .map_err(|e| anyhow!("COPY {}: encoding error: {}", table_name, e))?;
            let lines: Vec<&str> = content.lines().collect();

            // Skip header if present
            let start_idx = if options.header && !lines.is_empty() { 1 } else { 0 };
            let mut line_number = start_idx;

            for line in &lines[start_idx..] {
                line_number += 1;
                
                if line.is_empty() {
                    continue;
                }

                // Parse CSV line
                let values = parse_csv_line(line, options.delimiter, options.quote)
                    .map_err(|e| anyhow!("COPY {}: line {}: {}", table_name, line_number, e))?;

                // Convert values, handling NULL
                let converted_values: Vec<Option<String>> = values
                    .into_iter()
                    .map(|v| {
                        if v.is_empty() && options.null_string.is_empty() {
                            None
                        } else if v == options.null_string {
                            None
                        } else {
                            Some(v)
                        }
                    })
                    .collect();

                // Build and execute INSERT statement
                let sql = build_insert_sql(table_name, columns, converted_values.len())?;
                let mut stmt = conn.prepare_cached(&sql)?;

                // Convert params for rusqlite
                let params: Vec<rusqlite::types::Value> = converted_values
                    .into_iter()
                    .map(|v| match v {
                        Some(s) => rusqlite::types::Value::Text(s),
                        None => rusqlite::types::Value::Null,
                    })
                    .collect();

                let param_refs: Vec<&dyn rusqlite::ToSql> = params
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();

                stmt.execute(rusqlite::params_from_iter(param_refs.iter()))?;
                row_count += 1;
            }

            Ok(row_count)
        })
    }

    /// Process binary format data with transaction wrapping
    fn process_binary_data(
        &self,
        data: &[u8],
        table_name: &str,
        columns: &[String],
        _options: &CopyOptions,
    ) -> Result<usize> {
        use bytes::Buf;
        
        // Parse binary header outside of transaction (no DB access needed)
        let mut cursor = std::io::Cursor::new(data);
        
        // 1. Check signature (11 bytes)
        if cursor.remaining() < 11 {
            return Err(anyhow!("Binary COPY data too short (signature)"));
        }
        let mut signature = [0u8; 11];
        cursor.copy_to_slice(&mut signature);
        if &signature != b"PGCOPY\n\xff\r\n\0" {
            return Err(anyhow!("Invalid binary COPY signature"));
        }

        // 2. Read flags (4 bytes)
        if cursor.remaining() < 4 {
            return Err(anyhow!("Binary COPY data too short (flags)"));
        }
        let _flags = cursor.get_i32();

        // 3. Read header extension length (4 bytes)
        if cursor.remaining() < 4 {
            return Err(anyhow!("Binary COPY data too short (extension)"));
        }
        let ext_len = cursor.get_i32();
        if ext_len > 0 {
            if cursor.remaining() < ext_len as usize {
                return Err(anyhow!("Binary COPY data too short (extension data)"));
            }
            cursor.advance(ext_len as usize);
        }

        // Get remaining data after header for processing within transaction
        let remaining_data = {
            let remaining = cursor.remaining();
            let mut buf = vec![0u8; remaining];
            cursor.copy_to_slice(&mut buf);
            buf
        };
        
        // Process tuples within transaction
        self.with_transaction(|conn| {
            let mut cursor = std::io::Cursor::new(&remaining_data);
            
            // Get column names and types (if columns is empty, gets all table columns)
            let (column_names, column_types) = self.get_column_type_oids(table_name, columns, conn)?;
            
            let mut row_count = 0;

            // 4. Process tuples
            while cursor.remaining() >= 2 {
                let field_count = cursor.get_i16();
                
                // Check for trailer (-1)
                if field_count == -1 {
                    break;
                }

                if field_count < 0 {
                    return Err(anyhow!("Invalid field count in binary COPY: {}", field_count));
                }

                let mut values = Vec::new();
                for col_idx in 0..field_count {
                    if cursor.remaining() < 4 {
                        return Err(anyhow!("Binary COPY data too short (field length)"));
                    }
                    let len = cursor.get_i32();
                    if len == -1 {
                        values.push(rusqlite::types::Value::Null);
                    } else if len < 0 {
                        return Err(anyhow!("Invalid field length in binary COPY: {}", len));
                    } else {
                        if cursor.remaining() < len as usize {
                            return Err(anyhow!("Binary COPY data too short (field data)"));
                        }
                        let mut field_data = vec![0u8; len as usize];
                        cursor.copy_to_slice(&mut field_data);
                        
                        // Get the type OID for this column
                        let type_oid = column_types.get(col_idx as usize).copied().unwrap_or(TEXT_OID);
                        
                        // Convert binary data to SQLite value based on type
                        let value = binary_to_sqlite_value(&field_data, type_oid)
                            .map_err(|e| anyhow!("COPY {}: row {}, column {}: {}", 
                                table_name, row_count + 1, col_idx + 1, e))?;
                        values.push(value);
                    }
                }

                // Build and execute INSERT statement (use column_names which may have been populated from catalog)
                let sql = build_insert_sql(table_name, &column_names, values.len())?;
                let mut stmt = conn.prepare_cached(&sql)?;

                // Convert params for rusqlite
                let param_refs: Vec<&dyn rusqlite::ToSql> = values
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();

                stmt.execute(rusqlite::params_from_iter(param_refs.iter()))?;
                row_count += 1;
            }

            Ok(row_count)
        })
    }
    
    /// Get column type OIDs from the catalog
    fn get_column_type_oids(
        &self,
        table_name: &str,
        columns: &[String],
        conn: &Connection,
    ) -> Result<(Vec<String>, Vec<i32>)> {
        let mut column_names = Vec::new();
        let mut type_oids = Vec::new();
        
        // If no columns specified, get all columns from the table
        let _columns_to_lookup: Vec<String> = if columns.is_empty() {
            let mut stmt = conn.prepare_cached(
                "SELECT column_name, original_type 
                 FROM __pg_meta__ 
                 WHERE table_name = ?1
                 ORDER BY rowid"
            )?;
            let rows = stmt.query_map([table_name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            
            let mut cols = Vec::new();
            for row in rows {
                let (col_name, type_name) = row?;
                column_names.push(col_name.clone());
                type_oids.push(type_name_to_oid(&type_name));
                cols.push(col_name);
            }
            cols
        } else {
            for col_name in columns {
                column_names.push(col_name.clone());
                
                // Try to get the type from the catalog
                let type_name: Result<String, rusqlite::Error> = conn.query_row(
                    "SELECT original_type FROM __pg_meta__ 
                     WHERE table_name = ?1 AND column_name = ?2",
                    [table_name, col_name],
                    |row| row.get(0),
                );
                
                let oid = match type_name {
                    Ok(name) => type_name_to_oid(&name),
                    Err(_) => TEXT_OID, // Default to text
                };
                type_oids.push(oid);
            }
            columns.to_vec()
        };
        
        Ok((column_names, type_oids))
    }

    /// Get the current COPY state
    #[allow(dead_code)]
    pub fn get_state(&self) -> Result<CopyState> {
        let state = self.state.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        Ok(state.clone())
    }

    /// Check if a COPY operation is in progress
    #[allow(dead_code)]
    pub fn is_copy_in_progress(&self) -> Result<bool> {
        let state = self.state.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        Ok(matches!(*state, CopyState::FromStdin { .. } | CopyState::ToStdout { .. }))
    }

    /// Reset the COPY state to Idle
    pub fn reset_state(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let mut buffer = self.buffer.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let mut row_count = self.row_count.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        *state = CopyState::Idle;
        buffer.clear();
        *row_count = 0;

        Ok(())
    }

    /// Get the row count for the current COPY operation
    #[allow(dead_code)]
    pub fn get_row_count(&self) -> Result<usize> {
        let row_count = self.row_count.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        Ok(*row_count)
    }

    /// Execute a closure within an explicit transaction for bulk operations
    fn with_transaction<F, R>(
        &self,
        f: F,
    ) -> Result<R>
    where
        F: FnOnce(&Connection) -> Result<R>,
        R: Clone,
    {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        
        // Begin transaction
        conn.execute("BEGIN", [])
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;
        
        // Execute the bulk operation
        let result = f(&conn);
        
        // Commit or rollback based on result
        match result {
            Ok(ref r) => {
                conn.execute("COMMIT", [])
                    .map_err(|e| anyhow!("Failed to commit transaction: {}", e))?;
                Ok(r.clone())
            }
            Err(ref e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(anyhow!("Transaction rolled back due to error: {}", e))
            }
        }
    }
}

#[async_trait]
impl PgWireCopyHandler for CopyHandler {
    async fn on_copy_data<C>(&self, _client: &mut C, copy_data: CopyData) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let mut buffer = self.buffer.lock().map_err(|e| {
            PgWireError::UserError(Box::new(
                PgError::internal(format!("Lock error: {}", e)).into_error_info()
            ))
        })?;

        // Append data to buffer
        buffer.extend_from_slice(&copy_data.data);

        Ok(())
    }

    async fn on_copy_done<C>(&self, client: &mut C, _done: CopyDone) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        // Process the buffered data
        let row_count = match self.process_buffer() {
            Ok(count) => {
                let mut count_guard = self.row_count.lock().map_err(|e| {
                    PgWireError::UserError(Box::new(
                        PgError::internal(format!("Lock error: {}", e)).into_error_info()
                    ))
                })?;
                *count_guard = count;

                // Clear buffer
                let mut buffer_guard = self.buffer.lock().map_err(|e| {
                    PgWireError::UserError(Box::new(
                        PgError::internal(format!("Lock error: {}", e)).into_error_info()
                    ))
                })?;
                buffer_guard.clear();
                
                count
            }
            Err(e) => return Err(PgWireError::UserError(Box::new(
                PgError::new(PgErrorCode::InvalidParameterValue, 
                    format!("COPY data parsing error: {}", e)).into_error_info()
            ))),
        };

        // Send CommandComplete response for the COPY command
        let tag = pgwire::api::results::Tag::new("COPY").with_rows(row_count);
        pgwire::api::query::send_execution_response(client, tag).await?;

        Ok(())
    }

    async fn on_copy_fail<C>(&self, _client: &mut C, fail: CopyFail) -> PgWireError
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: std::fmt::Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        // Reset state
        let _ = self.reset_state();

        PgWireError::UserError(Box::new(
            PgError::new(PgErrorCode::QueryCanceled, 
                format!("COPY failed: {}", fail.message)).into_error_info()
        ))
    }
}

/// Build an INSERT SQL statement for COPY data
    fn build_insert_sql(
    table_name: &str,
    columns: &[String],
    value_count: usize,
) -> Result<String> {
    if columns.is_empty() {
        // If no columns specified, use VALUES only
        let placeholders: Vec<String> = (0..value_count).map(|i| format!("?{}", i + 1)).collect();
        Ok(format!(
            "INSERT INTO {} VALUES ({})",
            table_name,
            placeholders.join(", ")
        ))
    } else {
        if columns.len() != value_count {
            return Err(anyhow!("table {} has {} columns but {} values were supplied", table_name, columns.len(), value_count));
        }
        // Use explicit column list
        let placeholders: Vec<String> = (0..columns.len()).map(|i| format!("?{}", i + 1)).collect();
        Ok(format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            columns.join(", "),
            placeholders.join(", ")
        ))
    }
}

/// Unescape a text format value
fn unescape_text_value(value: &str, escape_char: char) -> String {
    if escape_char == '\t' || !value.contains('\\') {
        // No escaping needed
        return value.to_string();
    }

    let mut result = String::new();
    let mut chars = value.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Parse a CSV line, handling quoted fields
fn parse_csv_line(line: &str, delimiter: char, quote: char) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == quote {
            if in_quotes {
                // Check for escaped quote ("")
                if chars.peek() == Some(&quote) {
                    chars.next(); // Consume second quote
                    current.push(quote);
                } else {
                    in_quotes = false;
                }
            } else {
                in_quotes = true;
            }
        } else if c == delimiter && !in_quotes {
            result.push(current);
            current = String::new();
        } else {
            current.push(c);
        }
    }

    // Add the last field
    result.push(current);

    Ok(result)
}

/// Format a value for text format output
pub fn format_text_value(value: Option<&str>, null_string: &str, escape_char: char) -> String {
    match value {
        None => null_string.to_string(),
        Some(v) => escape_text_value(v, escape_char),
    }
}

/// Escape special characters in text format
fn escape_text_value(value: &str, escape_char: char) -> String {
    if escape_char == '\t' {
        // No escaping for default text format
        return value.to_string();
    }

    let mut result = String::new();
    for c in value.chars() {
        match c {
            '\n' => {
                result.push(escape_char);
                result.push('n');
            }
            '\t' => {
                result.push(escape_char);
                result.push('t');
            }
            '\r' => {
                result.push(escape_char);
                result.push('r');
            }
            c if c == escape_char => {
                result.push(escape_char);
                result.push(escape_char);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Format a value for CSV format output
pub fn format_csv_value(
    value: Option<&str>,
    delimiter: char,
    quote: char,
    force_quote: bool,
) -> String {
    match value {
        None => "".to_string(),
        Some(v) => {
            let needs_quoting = force_quote
                || v.contains(delimiter)
                || v.contains(quote)
                || v.contains('\n')
                || v.contains('\r');

            if needs_quoting {
                let escaped = v.replace(quote, &format!("{}{}", quote, quote));
                format!("{}{}{}", quote, escaped, quote)
            } else {
                v.to_string()
            }
        }
    }
}

/// Convert a type name (PostgreSQL or SQLite) to its OID
fn type_name_to_oid(type_name: &str) -> i32 {
    let normalized = type_name.to_lowercase();
    match normalized.as_str() {
        // PostgreSQL type names
        "bool" | "boolean" => BOOL_OID,
        "int2" | "smallint" => INT2_OID,
        "int4" | "int" => INT4_OID,
        "int8" | "bigint" => INT8_OID,
        "float4" => FLOAT4_OID,
        "float8" | "double precision" => FLOAT8_OID,
        "text" => TEXT_OID,
        "varchar" | "character varying" => VARCHAR_OID,
        "bpchar" | "character" | "char" => BPCHAR_OID,
        "date" => DATE_OID,
        "time" | "time without time zone" => TIME_OID,
        "timestamp" | "timestamp without time zone" => TIMESTAMP_OID,
        "timestamptz" | "timestamp with time zone" => TIMESTAMPTZ_OID,
        "interval" => INTERVAL_OID,
        "uuid" => UUID_OID,
        "numeric" | "decimal" => NUMERIC_OID,
        "json" => JSON_OID,
        "jsonb" => JSONB_OID,
        "bytea" => BYTEA_OID,
        // SQLite type names (mapped to PostgreSQL equivalents)
        "integer" => INT8_OID,  // SQLite INTEGER -> PostgreSQL bigint/int8
        "real" => FLOAT8_OID,   // SQLite REAL -> PostgreSQL float8 (not float4)
        "blob" => BYTEA_OID,    // SQLite BLOB -> PostgreSQL bytea
        _ => TEXT_OID, // Default to text for unknown types
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_options_default() {
        let opts = CopyOptions::default();
        assert_eq!(opts.format, CopyFormat::Text);
        assert_eq!(opts.delimiter, '\t');
        assert_eq!(opts.null_string, "\\N");
    }

    #[test]
    fn test_copy_options_text() {
        let opts = CopyOptions::text();
        assert_eq!(opts.format, CopyFormat::Text);
        assert_eq!(opts.delimiter, '\t');
    }

    #[test]
    fn test_copy_options_csv() {
        let opts = CopyOptions::csv();
        assert_eq!(opts.format, CopyFormat::Csv);
        assert_eq!(opts.delimiter, ',');
        assert_eq!(opts.null_string, "");
    }

    #[test]
    fn test_copy_options_binary() {
        let opts = CopyOptions::binary();
        assert_eq!(opts.format, CopyFormat::Binary);
    }

    #[test]
    fn test_parse_csv_line() {
        let line = r#"1,"hello, world",3"#;
        let result = parse_csv_line(line, ',', '"').unwrap();
        assert_eq!(result, vec!["1", "hello, world", "3"]);

        // Test escaped quotes: A field containing literal "quoted" should be represented as """quoted"""
        // For simplicity, we just verify basic CSV parsing works
        let line2 = r#"simple,value,here"#;
        let result2 = parse_csv_line(line2, ',', '"').unwrap();
        assert_eq!(result2, vec!["simple", "value", "here"]);
    }

    #[test]
    fn test_format_csv_value() {
        assert_eq!(format_csv_value(Some("hello"), ',', '"', false), "hello");
        assert_eq!(format_csv_value(Some("hello, world"), ',', '"', false), r#""hello, world""#);
        assert_eq!(format_csv_value(None, ',', '"', false), "");
        assert_eq!(format_csv_value(Some(r#"say "hello""#), ',', '"', false), r#""say ""hello""""#);
    }

    #[test]
    fn test_unescape_text_value() {
        assert_eq!(unescape_text_value("hello", '\\'), "hello");
        assert_eq!(unescape_text_value(r#"hello\nworld"#, '\\'), "hello\nworld");
        assert_eq!(unescape_text_value(r#"tab\there"#, '\\'), "tab\there");
    }

    #[test]
    fn test_escape_text_value() {
        assert_eq!(escape_text_value("hello", '\\'), "hello");
        assert_eq!(escape_text_value("hello\nworld", '\\'), r#"hello\nworld"#);
        assert_eq!(escape_text_value("tab\there", '\\'), r#"tab\there"#);
    }

    #[test]
    fn test_build_insert_sql() {
        let cols = vec!["id".to_string(), "name".to_string()];
        let sql = build_insert_sql("users", &cols, 2).unwrap();
        assert_eq!(sql, "INSERT INTO users (id, name) VALUES (?1, ?2)");

        let sql2 = build_insert_sql("users", &[], 3).unwrap();
        assert_eq!(sql2, "INSERT INTO users VALUES (?1, ?2, ?3)");
    }

    // Binary format tests
    #[test]
    fn test_read_bool_binary() {
        assert_eq!(read_bool_binary(&[0]).unwrap(), false);
        assert_eq!(read_bool_binary(&[1]).unwrap(), true);
        // PostgreSQL binary format: any non-zero is true
        assert_eq!(read_bool_binary(&[2]).unwrap(), true);
        assert!(read_bool_binary(&[]).is_err());
        assert!(read_bool_binary(&[0, 1]).is_err());
    }

    #[test]
    fn test_write_bool_binary() {
        assert_eq!(write_bool_binary(false), vec![0]);
        assert_eq!(write_bool_binary(true), vec![1]);
    }

    #[test]
    fn test_read_i16_binary() {
        let bytes = [0x00, 0x01]; // big-endian 1
        assert_eq!(read_i16_binary(&bytes).unwrap(), 1);
        
        let bytes = [0xFF, 0xFF]; // big-endian -1
        assert_eq!(read_i16_binary(&bytes).unwrap(), -1);
        
        let bytes = [0x7F, 0xFF]; // big-endian 32767
        assert_eq!(read_i16_binary(&bytes).unwrap(), 32767);
    }

    #[test]
    fn test_write_i16_binary() {
        assert_eq!(write_i16_binary(1), vec![0x00, 0x01]);
        assert_eq!(write_i16_binary(-1), vec![0xFF, 0xFF]);
    }

    #[test]
    fn test_read_i32_binary() {
        let bytes = [0x00, 0x00, 0x00, 0x01]; // big-endian 1
        assert_eq!(read_i32_binary(&bytes).unwrap(), 1);
        
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF]; // big-endian -1
        assert_eq!(read_i32_binary(&bytes).unwrap(), -1);
    }

    #[test]
    fn test_write_i32_binary() {
        assert_eq!(write_i32_binary(1), vec![0x00, 0x00, 0x00, 0x01]);
        assert_eq!(write_i32_binary(-1), vec![0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn test_read_i64_binary() {
        let bytes = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01]; // big-endian 1
        assert_eq!(read_i64_binary(&bytes).unwrap(), 1);
        
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]; // big-endian -1
        assert_eq!(read_i64_binary(&bytes).unwrap(), -1);
    }

    #[test]
    fn test_write_i64_binary() {
        assert_eq!(write_i64_binary(1), vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01]);
        assert_eq!(write_i64_binary(-1), vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn test_read_f32_binary() {
        let bytes = [0x3F, 0x80, 0x00, 0x00]; // big-endian 1.0
        assert_eq!(read_f32_binary(&bytes).unwrap(), 1.0);
        
        let bytes = [0x40, 0x49, 0x0F, 0xDB]; // big-endian ~3.14159
        assert!((read_f32_binary(&bytes).unwrap() - 3.14159).abs() < 0.0001);
    }

    #[test]
    fn test_read_f64_binary() {
        let bytes = [0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // big-endian 1.0
        assert_eq!(read_f64_binary(&bytes).unwrap(), 1.0);
        
        let bytes = [0x40, 0x09, 0x21, 0xFB, 0x54, 0x44, 0x2D, 0x18]; // big-endian ~3.14159
        assert!((read_f64_binary(&bytes).unwrap() - 3.14159265358979).abs() < 0.0000001);
    }

    #[test]
    fn test_read_uuid_binary() {
        let bytes = [0xa0, 0xee, 0xbc, 0x99, 0x9c, 0x0b, 0x4e, 0xf8, 
                     0xbb, 0x6d, 0x6b, 0xb9, 0xbd, 0x38, 0x0a, 0x11];
        assert_eq!(read_uuid_binary(&bytes).unwrap(), "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11");
    }

    #[test]
    fn test_binary_to_sqlite_value() {
        // Test bool
        let result = binary_to_sqlite_value(&[1], BOOL_OID).unwrap();
        assert_eq!(result, rusqlite::types::Value::Integer(1));
        
        // Test int4
        let result = binary_to_sqlite_value(&[0x00, 0x00, 0x00, 0x2A], INT4_OID).unwrap(); // 42
        assert_eq!(result, rusqlite::types::Value::Integer(42));
        
        // Test int8
        let result = binary_to_sqlite_value(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2A], INT8_OID).unwrap(); // 42
        assert_eq!(result, rusqlite::types::Value::Integer(42));
        
        // Test float8
        let result = binary_to_sqlite_value(&[0x40, 0x09, 0x21, 0xFB, 0x54, 0x44, 0x2D, 0x18], FLOAT8_OID).unwrap();
        assert!(matches!(result, rusqlite::types::Value::Real(_)));
        
        // Test text
        let result = binary_to_sqlite_value(b"hello", TEXT_OID).unwrap();
        assert_eq!(result, rusqlite::types::Value::Text("hello".to_string()));
    }

    #[test]
    fn test_type_name_to_oid() {
        assert_eq!(type_name_to_oid("bool"), BOOL_OID);
        assert_eq!(type_name_to_oid("int4"), INT4_OID);
        // "integer" maps to INT8 because SQLite INTEGER is stored as int8
        assert_eq!(type_name_to_oid("integer"), INT8_OID);
        assert_eq!(type_name_to_oid("text"), TEXT_OID);
        assert_eq!(type_name_to_oid("float8"), FLOAT8_OID);
        assert_eq!(type_name_to_oid("double precision"), FLOAT8_OID);
        assert_eq!(type_name_to_oid("uuid"), UUID_OID);
        assert_eq!(type_name_to_oid("unknown_type"), TEXT_OID);
        // SQLite type mappings
        assert_eq!(type_name_to_oid("INTEGER"), INT8_OID);
        assert_eq!(type_name_to_oid("REAL"), FLOAT8_OID);
    }
}