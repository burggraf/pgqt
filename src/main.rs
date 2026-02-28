use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use futures::stream;
use pgwire::api::auth::noop::NoopStartupHandler;
use pgwire::api::query::{PlaceholderExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::results::{DataRowEncoder, FieldFormat, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::{ClientInfo, Type};
use pgwire::error::{ErrorInfo, PgWireResult};
use pgwire::tokio::process_socket;
use rusqlite::Connection;
use tokio::net::TcpListener;

mod catalog;
mod transpiler;

use catalog::{init_catalog, store_table_metadata};
use transpiler::transpile_with_metadata;

/// PostgreSQL-to-SQLite proxy handler
struct SqliteHandler {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteHandler {
    fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        init_catalog(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Execute a SQL query and return the results
    fn execute_query(&self, sql: &str) -> Result<Vec<Response>> {
        let transpile_result = transpile_with_metadata(sql);
        let sqlite_sql = &transpile_result.sql;

        let conn = self.conn.lock().unwrap();

        let is_select = sqlite_sql.trim().to_lowercase().starts_with("select");
        let is_create_table = sqlite_sql.trim().to_uppercase().starts_with("CREATE TABLE");

        if is_create_table {
            // For CREATE TABLE, we need to execute the DDL first, then store metadata
            // This avoids the "cannot start a transaction within a transaction" error
            // because SQLite starts an implicit transaction for CREATE TABLE
            let result = self.execute_statement(&conn, sqlite_sql)?;

            // Store metadata after CREATE TABLE completes
            if let Some(metadata) = transpile_result.create_table_metadata {
                let columns: Vec<(String, String, Option<String>)> = metadata
                    .columns
                    .into_iter()
                    .map(|c| (c.column_name, c.original_type, c.constraints))
                    .collect();

                store_table_metadata(&conn, &metadata.table_name, &columns)?;
            }

            Ok(result)
        } else if is_select {
            self.execute_select(&conn, sqlite_sql)
        } else {
            self.execute_statement(&conn, sqlite_sql)
        }
    }

    fn execute_select(&self, conn: &Connection, sql: &str) -> Result<Vec<Response>> {
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();

        let fields: Arc<Vec<FieldInfo>> = Arc::new(
            (0..col_count)
                .map(|i| {
                    let col_name = stmt.column_name(i).unwrap_or("?column?").to_string();

                    FieldInfo::new(col_name, None, None, Type::TEXT, FieldFormat::Text)
                })
                .collect(),
        );

        let mut data_rows = Vec::new();
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let mut encoder = DataRowEncoder::new(fields.clone());

            for i in 0..col_count {
                let value: Option<String> = row.get(i).ok();
                match value {
                    Some(v) => encoder.encode_field(&Some(v))?,
                    None => encoder.encode_field(&None::<String>)?,
                }
            }

            data_rows.push(encoder.finish());
        }

        let row_stream = stream::iter(data_rows);

        Ok(vec![Response::Query(QueryResponse::new(
            fields,
            row_stream,
        ))])
    }

    fn execute_statement(&self, conn: &Connection, sql: &str) -> Result<Vec<Response>> {
        println!("Executing statement: {}", sql);
        let changes = conn.execute(sql, [])?;

        let upper_sql = sql.trim().to_uppercase();
        let tag = if upper_sql.starts_with("CREATE TABLE") {
            Tag::new("CREATE TABLE")
        } else if upper_sql.starts_with("INSERT") {
            Tag::new("INSERT").with_oid(0).with_rows(changes)
        } else if upper_sql.starts_with("UPDATE") {
            Tag::new("UPDATE").with_rows(changes)
        } else if upper_sql.starts_with("DELETE") {
            Tag::new("DELETE").with_rows(changes)
        } else {
            Tag::new("OK")
        };

        Ok(vec![Response::Execution(tag)])
    }
}

#[async_trait]
impl SimpleQueryHandler for SqliteHandler {
    async fn do_query<'a, 'b: 'a, C>(&'b self, _client: &mut C, query: &'a str) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        println!("Received query: {}", query);
        match self.execute_query(query) {
            Ok(responses) => Ok(responses),
            Err(e) => {
                eprintln!("Error executing query: {}", e);
                Ok(vec![Response::Error(Box::new(ErrorInfo::new(
                    "ERROR".to_owned(),
                    "XX000".to_owned(),
                    e.to_string(),
                )))])
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let db_path = std::env::var("PG_LITE_DB").unwrap_or_else(|_| "test.db".to_string());
    let port = std::env::var("PG_LITE_PORT").unwrap_or_else(|_| "5432".to_string());
    let addr = format!("127.0.0.1:{}", port);

    let listener = TcpListener::bind(&addr).await?;
    println!("Server listening on {}", addr);

    let handler = Arc::new(SqliteHandler::new(&db_path)?);
    let startup_handler = Arc::new(NoopStartupHandler);
    let extended_handler = Arc::new(PlaceholderExtendedQueryHandler);

    loop {
        let (incoming_socket, client_addr) = listener.accept().await?;
        println!("New connection from {}", client_addr);

        let handler = handler.clone();
        let startup_handler = startup_handler.clone();
        let extended_handler = extended_handler.clone();

        tokio::spawn(async move {
            let _ = process_socket(
                incoming_socket,
                None,
                startup_handler,
                handler,
                extended_handler,
            )
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db_path(name: &str) -> String {
        let temp_dir = std::env::temp_dir();
        temp_dir.join(name).to_str().unwrap().to_string()
    }

    fn cleanup_db(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_handler_initializes_catalog() {
        let db_path = temp_db_path("test_pg_lite.db");
        cleanup_db(&db_path);

        let handler = SqliteHandler::new(&db_path).unwrap();

        let conn = handler.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='__pg_meta__'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
        cleanup_db(&db_path);
    }

    #[test]
    fn test_create_table_stores_metadata() {
        let db_path = temp_db_path("test_pg_lite_meta.db");
        cleanup_db(&db_path);

        let handler = SqliteHandler::new(&db_path).unwrap();

        let _ = handler.execute_query("CREATE TABLE test_table (id SERIAL, name VARCHAR(10), created_at TIMESTAMP WITH TIME ZONE)");

        let conn = handler.conn.lock().unwrap();
        let metadata = catalog::get_table_metadata(&conn, "test_table").unwrap();

        assert_eq!(metadata.len(), 3);

        let types: Vec<String> = metadata.iter().map(|m| m.original_type.clone()).collect();
        assert!(types.contains(&"SERIAL".to_string()));
        assert!(types.contains(&"VARCHAR(10)".to_string()));
        assert!(types.contains(&"TIMESTAMP WITH TIME ZONE".to_string()));

        cleanup_db(&db_path);
    }
}
