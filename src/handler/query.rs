//! Query execution module
//!
//! This module contains methods for executing SQL queries including:
//! - Main query execution dispatch
//! - SELECT statement execution
//! - DML statement execution (INSERT, UPDATE, DELETE)
//! - COPY statement handling

use std::sync::{Arc, Mutex};
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use dashmap::DashMap;
use futures::stream;

use crate::catalog::{store_table_metadata, store_relation_metadata, FunctionMetadata};
use crate::schema::{SchemaManager, SearchPath};
use crate::handler::{SessionContext, SqliteHandler};
use crate::handler::transaction::{handle_transaction_control, is_transaction_control};
use crate::handler::utils::HandlerUtils;
use crate::copy;
use pgwire::api::results::{DataRowEncoder, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::Type;

/// Execute a SELECT statement and return results
pub fn execute_select(
    conn: &Connection,
    sql: &str,
) -> Result<Vec<Response>> {
    let mut stmt = conn.prepare(sql)?;
    let col_count = stmt.column_count();

    let fields: Arc<Vec<FieldInfo>> = Arc::new(
        (0..col_count)
            .map(|i| {
                let col_name = stmt.column_name(i).unwrap_or("?column?").to_string();

                FieldInfo::new(col_name, None, None, Type::TEXT, pgwire::api::results::FieldFormat::Text)
            })
            .collect(),
    );

    let mut data_rows = Vec::new();
    let mut rows = stmt.query([])?;

    while let Some(row) = rows.next()? {
        let mut encoder = DataRowEncoder::new(fields.clone());

        for i in 0..col_count {
            let value: Option<String> = row.get::<_, Option<i64>>(i).ok()
                .map(|v| v.map(|x| x.to_string()))
                .or_else(|| row.get::<_, Option<f64>>(i).ok()
                    .map(|v| v.map(|x| x.to_string())))
                .or_else(|| row.get::<_, Option<String>>(i).ok())
                .flatten();
            match value {
                Some(v) => encoder.encode_field(&Some(v))?,
                None => encoder.encode_field(&None::<String>)?,
            }
        }

        data_rows.push(Ok(encoder.take_row()));
    }

    let row_stream = stream::iter(data_rows);

    Ok(vec![Response::Query(QueryResponse::new(
        fields,
        row_stream,
    ))])
}

/// Execute a non-SELECT statement (INSERT, UPDATE, DELETE, DDL)
pub fn execute_statement(
    conn: &Connection,
    sql: &str,
) -> Result<Vec<Response>> {
    println!("Executing statement: {}", sql);

    let statements: Vec<&str> = sql.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    let mut total_changes = 0;

    for stmt in statements {
        total_changes += conn.execute(stmt, [])?;
    }

    let upper_sql = sql.trim().to_uppercase();
    let tag = if upper_sql.starts_with("CREATE TABLE") {
        Tag::new("CREATE TABLE")
    } else if upper_sql.starts_with("INSERT") {
        Tag::new("INSERT 0").with_rows(total_changes)
    } else if upper_sql.starts_with("UPDATE") {
        Tag::new("UPDATE").with_rows(total_changes)
    } else if upper_sql.starts_with("DELETE") {
        Tag::new("DELETE").with_rows(total_changes)
    } else {
        Tag::new("OK")
    };

    Ok(vec![Response::Execution(tag)])
}
