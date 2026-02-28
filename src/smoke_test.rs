use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use rusqlite::{params, Connection};

fn main() -> anyhow::Result<()> {
    // 1. Test SQL Parser (Postgres Dialect)
    let sql = "SELECT now(), name FROM users WHERE id = 1";
    let dialect = PostgreSqlDialect {};
    let ast = Parser::parse_sql(&dialect, sql)?;
    println!("Successfully parsed Postgres SQL: {:?}", ast);

    // 2. Test SQLite (In-Memory)
    let conn = Connection::open_in_memory()?;
    conn.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    )?;
    conn.execute(
        "INSERT INTO users (name) VALUES (?)",
        params!["test_user"],
    )?;

    let name: String = conn.query_row(
        "SELECT name FROM users WHERE id = 1",
        [],
        |row| row.get(0),
    )?;

    println!("Successfully queried SQLite: {}", name);
    Ok(())
}
