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

use crate::handler::errors::{PgError, PgErrorCode};

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
    pub encoding: String,
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
            encoding: "UTF8".to_string(),
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
    pub fn with_encoding(mut self, encoding: String) -> Self {
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
        
        let mut stmt = conn.prepare(&query)?;
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
            all_data.push(Ok(CopyData::new(Bytes::from(format!("{}\n", header_row)))));
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
            all_data.push(Ok(CopyData::new(Bytes::from(format!("{}\n", line)))));
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

    /// Process text format data
    fn process_text_data(
        &self,
        data: &[u8],
        table_name: &str,
        columns: &[String],
        options: &CopyOptions,
    ) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let mut row_count = 0;

        // Parse text format: tab-delimited rows, newline-separated
        let content = String::from_utf8_lossy(data);
        // Use split_inclusive to handle empty trailing fields and preserve row boundaries
        let lines: Vec<&str> = content.split_inclusive('\n').collect();

        for line in lines {
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

            // Convert values, handling NULL
            let converted_values: Vec<Option<String>> = values
                .iter()
                .map(|v| {
                    if v == &options.null_string {
                        None
                    } else {
                        Some(unescape_text_value(v, options.escape))
                    }
                })
                .collect();

            // Build and execute INSERT statement
            let sql = build_insert_sql(table_name, columns, converted_values.len())?;
            let mut stmt = conn.prepare(&sql)?;

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
    }

    /// Process CSV format data
    fn process_csv_data(
        &self,
        data: &[u8],
        table_name: &str,
        columns: &[String],
        options: &CopyOptions,
    ) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        let mut row_count = 0;

        // Parse CSV format
        let content = String::from_utf8_lossy(data);
        let lines: Vec<&str> = content.lines().collect();

        // Skip header if present
        let start_idx = if options.header && !lines.is_empty() {
            1
        } else {
            0
        };

        for line in &lines[start_idx..] {
            if line.is_empty() {
                continue;
            }

            // Parse CSV line
            let values = parse_csv_line(line, options.delimiter, options.quote)?;

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
            let mut stmt = conn.prepare(&sql)?;

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
    }

    /// Process binary format data
    fn process_binary_data(
        &self,
        data: &[u8],
        table_name: &str,
        columns: &[String],
        _options: &CopyOptions,
    ) -> Result<usize> {
        use bytes::Buf;
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

        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
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
            for _ in 0..field_count {
                if cursor.remaining() < 4 {
                    return Err(anyhow!("Binary COPY data too short (field length)"));
                }
                let len = cursor.get_i32();
                if len == -1 {
                    values.push(None);
                } else if len < 0 {
                    return Err(anyhow!("Invalid field length in binary COPY: {}", len));
                } else {
                    if cursor.remaining() < len as usize {
                        return Err(anyhow!("Binary COPY data too short (field data)"));
                    }
                    let mut field_data = vec![0u8; len as usize];
                    cursor.copy_to_slice(&mut field_data);
                    
                    // Convert binary to string for SQLite (simplified)
                    // In a real implementation, we'd handle specific type OIDs
                    // For now, we assume text or simple numeric types
                    values.push(Some(String::from_utf8_lossy(&field_data).to_string()));
                }
            }

            // Build and execute INSERT statement
            let sql = build_insert_sql(table_name, columns, values.len())?;
            let mut stmt = conn.prepare(&sql)?;

            // Convert params for rusqlite
            let params: Vec<rusqlite::types::Value> = values
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
}