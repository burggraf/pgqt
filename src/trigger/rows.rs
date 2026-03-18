//! OLD/NEW row building for trigger execution
//!
//! This module provides functions to build row data for trigger functions:
//! - [`build_old_row`] - Build OLD row from SQLite for UPDATE/DELETE operations
//! - [`build_new_row`] - Build NEW row from INSERT/UPDATE statement values

use anyhow::{anyhow, Result};
use rusqlite::{Connection, types::Value};
use std::collections::HashMap;

/// Build OLD row data from SQLite for UPDATE/DELETE operations
///
/// This queries the database for the current row values before they are modified.
/// For UPDATE/DELETE triggers, we need to fetch the existing row data.
///
/// # Arguments
///
/// * `conn` - SQLite connection
/// * `table_name` - Name of the table
/// * `pk_columns` - Primary key column names
/// * `pk_values` - Primary key values to identify the row
///
/// # Returns
///
/// A HashMap containing column names and their values
#[allow(dead_code)]
pub fn build_old_row(
    conn: &Connection,
    table_name: &str,
    pk_columns: &[String],
    pk_values: &[Value],
) -> Result<HashMap<String, Value>> {
    if pk_columns.is_empty() {
        return Err(anyhow!("Cannot build OLD row without primary key columns"));
    }

    if pk_columns.len() != pk_values.len() {
        return Err(anyhow!(
            "Primary key column count ({}) does not match value count ({})",
            pk_columns.len(),
            pk_values.len()
        ));
    }

    // Build WHERE clause for primary key lookup
    let where_clauses: Vec<String> = pk_columns
        .iter()
        .map(|col| format!("{} = ?", col))
        .collect();
    let where_sql = where_clauses.join(" AND ");

    // Query to get all columns for the row
    let sql = format!("SELECT * FROM {} WHERE {}", table_name, where_sql);

    // Execute query with primary key values
    let mut stmt = conn.prepare_cached(&sql)?;
    let column_count = stmt.column_count();

    // Get column names
    let column_names: Vec<String> = (0..column_count)
        .map(|i| stmt.column_name(i).unwrap_or("").to_string())
        .collect();

    // Convert pk_values to references for rusqlite
    let param_refs: Vec<&dyn rusqlite::ToSql> = pk_values
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect();

    // Execute and fetch row
    let mut rows = stmt.query(&*param_refs)?;

    if let Some(row) = rows.next()? {
        let mut result = HashMap::new();
        for (i, col_name) in column_names.iter().enumerate() {
            let value: Value = row.get(i)?;
            result.insert(col_name.clone(), value);
        }
        Ok(result)
    } else {
        Err(anyhow!("Row not found in table {}", table_name))
    }
}

/// Build NEW row data from INSERT/UPDATE statement values
///
/// For INSERT operations, the NEW row comes from the VALUES clause.
/// For UPDATE operations, the NEW row comes from the SET clause combined
/// with the OLD row values for columns that are not being updated.
///
/// # Arguments
///
/// * `values` - Vector of (column_name, value) pairs
///
/// # Returns
///
/// A HashMap containing column names and their values
#[allow(dead_code)]
pub fn build_new_row(values: &[(String, Value)]) -> HashMap<String, Value> {
    values.iter().cloned().collect()
}

/// Build NEW row for INSERT statement
///
/// Parses the INSERT statement to extract column names and values.
/// This is a best-effort parsing - complex expressions may not be fully supported.
#[allow(dead_code)]
pub fn build_new_row_from_insert(
    _conn: &Connection,
    _table_name: &str,
    sql: &str,
) -> Result<HashMap<String, Value>> {
    // Parse the INSERT statement using pg_query
    let result = pg_query::parse(sql)?;

    if let Some(raw_stmt) = result.protobuf.stmts.first() {
        if let Some(ref stmt_node) = raw_stmt.stmt {
            if let Some(pg_query::protobuf::node::Node::InsertStmt(stmt)) = &stmt_node.node {
                return extract_insert_values(stmt);
            }
        }
    }

    Err(anyhow!("Failed to parse INSERT statement"))
}

/// Extract values from an INSERT statement
#[allow(dead_code)]
fn extract_insert_values(
    stmt: &pg_query::protobuf::InsertStmt,
) -> Result<HashMap<String, Value>> {
    use pg_query::protobuf::node::Node as NodeEnum;

    let mut result = HashMap::new();

    // Get column names from the insert statement
    let column_names: Vec<String> = stmt
        .cols
        .iter()
        .filter_map(|n| {
            if let Some(NodeEnum::ResTarget(rt)) = n.node.as_ref() {
                Some(rt.name.clone())
            } else {
                None
            }
        })
        .collect();

    // Get values from the select statement (VALUES clause)
    if let Some(select_stmt) = &stmt.select_stmt {
        if let Some(NodeEnum::SelectStmt(select)) = select_stmt.node.as_ref() {
            // Get the first values list (for single-row INSERT)
            if let Some(values_list) = select.values_lists.first() {
                if let Some(NodeEnum::List(list)) = values_list.node.as_ref() {
                    for (i, item) in list.items.iter().enumerate() {
                        if i >= column_names.len() {
                            break;
                        }

                        let value = extract_value_from_node(item)?;
                        result.insert(column_names[i].clone(), value);
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Extract a Value from a pg_query Node
fn extract_value_from_node(node: &pg_query::protobuf::Node) -> Result<Value> {
    use pg_query::protobuf::node::Node as NodeEnum;

    if let Some(inner) = &node.node {
        match inner {
            NodeEnum::AConst(aconst) => {
                if let Some(val) = &aconst.val {
                    use pg_query::protobuf::a_const::Val;
                    match val {
                        Val::Ival(i) => Ok(Value::Integer(i.ival as i64)),
                        Val::Fval(f) => Ok(Value::Real(f.fval.parse()?)),
                        Val::Sval(s) => Ok(Value::Text(s.sval.clone())),
                        Val::Boolval(b) => Ok(Value::Integer(if b.boolval { 1 } else { 0 })),
                        _ => Ok(Value::Null),
                    }
                } else {
                    Ok(Value::Null)
                }
            }
            NodeEnum::TypeCast(typecast) => {
                // Handle type casts like '2024-01-01'::date
                if let Some(arg) = &typecast.arg {
                    extract_value_from_node(arg)
                } else {
                    Ok(Value::Null)
                }
            }
            NodeEnum::FuncCall(func_call) => {
                // Handle function calls like NOW()
                let func_name = func_call
                    .funcname
                    .iter()
                    .filter_map(|n| {
                        if let Some(NodeEnum::String(s)) = n.node.as_ref() {
                            Some(s.sval.to_uppercase())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(".");

                match func_name.as_str() {
                    "NOW" | "CURRENT_TIMESTAMP" => {
                        Ok(Value::Text(chrono::Local::now().to_rfc3339()))
                    }
                    "CURRENT_DATE" => {
                        Ok(Value::Text(chrono::Local::now().format("%Y-%m-%d").to_string()))
                    }
                    _ => {
                        // For other functions, we can't evaluate them here
                        // Return NULL and let the database handle it
                        Ok(Value::Null)
                    }
                }
            }
            _ => Ok(Value::Null),
        }
    } else {
        Ok(Value::Null)
    }
}

/// Get primary key columns for a table
pub fn get_primary_key_columns(
    conn: &Connection,
    table_name: &str,
) -> Result<Vec<String>> {
    // Query SQLite pragma to get primary key columns
    let sql = format!("PRAGMA table_info({})", table_name);
    let mut stmt = conn.prepare_cached(&sql)?;

    let rows = stmt.query_map([], |row| {
        let name: String = row.get(1)?;
        let pk: i32 = row.get(5)?;
        Ok((name, pk))
    })?;

    let mut pk_columns = Vec::new();
    for row in rows {
        let (name, pk) = row?;
        if pk > 0 {
            pk_columns.push((pk, name));
        }
    }

    // Sort by pk order
    pk_columns.sort_by_key(|(pk, _)| *pk);

    Ok(pk_columns.into_iter().map(|(_, name)| name).collect())
}

/// Build NEW row for UPDATE statement
///
/// For UPDATE operations, the NEW row comes from the SET clause.
/// Note: This only extracts the modified columns. To get a complete NEW row,
/// you would need to merge these with the OLD row values for unmodified columns.
#[allow(dead_code)]
pub fn build_new_row_from_update(
    conn: &Connection,
    table_name: &str,
    sql: &str,
) -> Result<HashMap<String, Value>> {
    // Parse the UPDATE statement using pg_query
    let result = pg_query::parse(sql)?;

    if let Some(raw_stmt) = result.protobuf.stmts.first() {
        if let Some(ref stmt_node) = raw_stmt.stmt {
            if let Some(pg_query::protobuf::node::Node::UpdateStmt(stmt)) = &stmt_node.node {
                return extract_update_values(conn, table_name, stmt);
            }
        }
    }

    Err(anyhow!("Failed to parse UPDATE statement"))
}

/// Extract values from an UPDATE statement
fn extract_update_values(
    _conn: &Connection,
    _table_name: &str,
    stmt: &pg_query::protobuf::UpdateStmt,
) -> Result<HashMap<String, Value>> {
    use pg_query::protobuf::node::Node as NodeEnum;

    let mut result = HashMap::new();

    // Get the target list (SET clause assignments)
    for target in &stmt.target_list {
        if let Some(NodeEnum::ResTarget(rt)) = target.node.as_ref() {
            let col_name = &rt.name;
            
            // Get the value from the expression
            if let Some(val_node) = &rt.val {
                let value = extract_value_from_node(val_node)?;
                result.insert(col_name.clone(), value);
            }
        }
    }

    Ok(result)
}

/// Build OLD row by looking up the row that matches the WHERE clause of an UPDATE/DELETE
///
/// This is a simplified implementation that tries to extract the primary key from the WHERE clause.
/// For complex WHERE clauses, it may not work correctly.
#[allow(dead_code)]
pub fn build_old_row_from_where(
    conn: &Connection,
    table_name: &str,
    where_clause: &str,
    params: &[Value],
) -> Result<HashMap<String, Value>> {
    // First, try to get the primary key columns
    let pk_columns = get_primary_key_columns(conn, table_name)?;

    if pk_columns.is_empty() {
        // No primary key - we need to fetch based on the WHERE clause
        // This is less efficient but necessary
        let sql = format!("SELECT * FROM {} WHERE {} LIMIT 1", table_name, where_clause);
        let mut stmt = conn.prepare_cached(&sql)?;

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect();

        let param_refs: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        let mut rows = stmt.query(&*param_refs)?;

        if let Some(row) = rows.next()? {
            let mut result = HashMap::new();
            for (i, col_name) in column_names.iter().enumerate() {
                let value: Value = row.get(i)?;
                result.insert(col_name.clone(), value);
            }
            Ok(result)
        } else {
            Err(anyhow!("No row found matching WHERE clause"))
        }
    } else {
        // Try to extract primary key values from the WHERE clause
        // This is a simplified parser - it looks for patterns like "id = ?" or "id = 1"
        let sql = format!("SELECT * FROM {} WHERE {} LIMIT 1", table_name, where_clause);
        let mut stmt = conn.prepare_cached(&sql)?;

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("").to_string())
            .collect();

        let param_refs: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        let mut rows = stmt.query(&*param_refs)?;

        if let Some(row) = rows.next()? {
            let mut result = HashMap::new();
            for (i, col_name) in column_names.iter().enumerate() {
                let value: Value = row.get(i)?;
                result.insert(col_name.clone(), value);
            }
            Ok(result)
        } else {
            Err(anyhow!("No row found matching WHERE clause"))
        }
    }
}

/// Extract OLD row from an UPDATE/DELETE statement
///
/// Parses the SQL to extract the WHERE clause and fetches the matching row.
/// This is a best-effort implementation - complex WHERE clauses may not be fully supported.
#[allow(dead_code)]
pub fn extract_old_row_from_dml(
    conn: &Connection,
    table_name: &str,
    sql: &str,
) -> Result<HashMap<String, Value>> {
    // Parse the SQL to extract the WHERE clause
    let result = pg_query::parse(sql)?;

    if let Some(raw_stmt) = result.protobuf.stmts.first() {
        if let Some(ref stmt_node) = raw_stmt.stmt {
            // Handle UPDATE statements
            if let Some(pg_query::protobuf::node::Node::UpdateStmt(stmt)) = &stmt_node.node {
                if let Some(where_clause) = &stmt.where_clause {
                    // Try to deparse the WHERE clause
                    // We need to reconstruct a minimal parse result for deparse
                    let where_sql = deparse_where_clause(where_clause)?;
                    return build_old_row_from_where(conn, table_name, &where_sql, &[]);
                } else {
                    // No WHERE clause - fetch any row (usually first)
                    return build_old_row_from_where(conn, table_name, "1=1", &[]);
                }
            }
            // Handle DELETE statements
            else if let Some(pg_query::protobuf::node::Node::DeleteStmt(stmt)) = &stmt_node.node {
                if let Some(where_clause) = &stmt.where_clause {
                    let where_sql = deparse_where_clause(where_clause)?;
                    return build_old_row_from_where(conn, table_name, &where_sql, &[]);
                } else {
                    // No WHERE clause
                    return build_old_row_from_where(conn, table_name, "1=1", &[]);
                }
            }
        }
    }

    Err(anyhow!("Could not extract WHERE clause from DML statement"))
}

/// Deparse a WHERE clause node back to SQL
fn deparse_where_clause(where_clause: &pg_query::protobuf::Node) -> Result<String> {
    // pg_query::deparse can deparse a Node back to SQL, but it requires
    // the node to be wrapped in a RawStmt or similar in some versions.
    // However, let's try a direct approach by looking at the node type.
    
    use pg_query::protobuf::node::Node as NodeEnum;
    
    match &where_clause.node {
        Some(NodeEnum::AExpr(expr)) => {
            // Simple expressions like "id = 1"
            let left = if let Some(ref lexpr) = expr.lexpr {
                deparse_node(lexpr)?
            } else {
                "".to_string()
            };
            
            let right = if let Some(ref rexpr) = expr.rexpr {
                deparse_node(rexpr)?
            } else {
                "".to_string()
            };
            
            let op = match expr.name.first()
                .and_then(|n| if let Some(NodeEnum::String(ref s)) = n.node { Some(s.sval.clone()) } else { None }) {
                Some(s) => s.clone(),
                None => "=".to_string()
            };
                
            Ok(format!("{} {} {}", left, op, right))
        }
        _ => {
            // For more complex expressions, we really need a proper deparser.
            // Let's try to use pg_query's deparse if we can.
            // Since we don't have a full deparser for individual nodes easily,
            // we'll fall back to a placeholder for very complex cases.
            Err(anyhow!("Complex WHERE clause deparsing not yet fully implemented"))
        }
    }
}

/// Helper to deparse simple nodes
fn deparse_node(node: &pg_query::protobuf::Node) -> Result<String> {
    use pg_query::protobuf::node::Node as NodeEnum;
    
    match &node.node {
        Some(NodeEnum::ColumnRef(cref)) => {
            if let Some(NodeEnum::String(ref s)) = cref.fields.last().and_then(|f| f.node.as_ref()) {
                Ok(s.sval.clone())
            } else {
                Ok("?".to_string())
            }
        }
        Some(NodeEnum::AConst(aconst)) => {
            use pg_query::protobuf::a_const::Val;
            match &aconst.val {
                Some(Val::Ival(i)) => Ok(i.ival.to_string()),
                Some(Val::Fval(f)) => Ok(f.fval.clone()),
                Some(Val::Sval(s)) => Ok(format!("'{}'", s.sval.replace('\'', "''"))),
                Some(Val::Boolval(b)) => Ok(if b.boolval { "true" } else { "false" }.to_string()),
                _ => Ok("NULL".to_string()),
            }
        }
        Some(NodeEnum::AExpr(expr)) => {
            let left = if let Some(ref lexpr) = expr.lexpr {
                deparse_node(lexpr)?
            } else {
                "".to_string()
            };
            
            let right = if let Some(ref rexpr) = expr.rexpr {
                deparse_node(rexpr)?
            } else {
                "".to_string()
            };
            
            let op = match expr.name.first()
                .and_then(|n| if let Some(NodeEnum::String(ref s)) = n.node { Some(s.sval.clone()) } else { None }) {
                Some(s) => s.clone(),
                None => "=".to_string()
            };
                
            Ok(format!("{} {} {}", left, op, right))
        }
        _ => Ok("?".to_string()),
    }
}

/// Extract expressions from an UPDATE statement
pub fn extract_update_expressions(
    sql: &str,
) -> Result<HashMap<String, String>> {
    use pg_query::protobuf::node::Node as NodeEnum;

    let result = pg_query::parse(sql)?;
    let mut exprs = HashMap::new();

    if let Some(raw_stmt) = result.protobuf.stmts.first() {
        if let Some(ref stmt_node) = raw_stmt.stmt {
            if let Some(NodeEnum::UpdateStmt(stmt)) = &stmt_node.node {
                for target in &stmt.target_list {
                    if let Some(NodeEnum::ResTarget(rt)) = target.node.as_ref() {
                        let col_name = &rt.name;
                        if let Some(val_node) = &rt.val {
                            let expr = deparse_node(val_node)?;
                            exprs.insert(col_name.clone(), expr);
                        }
                    }
                }
            }
        }
    }

    Ok(exprs)
}

/// Check if an update expression is a simple literal value or a complex expression
/// Returns true if the expression is complex (contains operators, column references, etc.)
/// Returns false if the expression is a simple literal (number, string, etc.)
pub fn is_complex_expression(expr: &str) -> bool {
    // Simple literals don't contain these characters
    let complex_chars = ['+', '-', '*', '/', '(', ')'];
    
    // Check for complex single-character indicators
    for indicator in &complex_chars {
        if expr.contains(*indicator) {
            return true;
        }
    }
    
    // Check for multi-character operators
    if expr.contains("||") || expr.contains("::") {
        return true;
    }
    
    // Check for column references (alphanumeric that could be column names)
    // If it contains spaces but isn't quoted, it's likely complex
    if expr.contains(' ') && !expr.starts_with('\'') && !expr.starts_with('"') {
        return true;
    }
    
    false
}

/// Convert a rusqlite Row to a HashMap of column names and values
pub fn row_to_map(row: &rusqlite::Row) -> Result<HashMap<String, Value>> {
    let mut result = HashMap::new();
    let column_count = row.as_ref().column_count();
    for i in 0..column_count {
        let name = row.as_ref().column_name(i)?.to_string();
        let value: Value = row.get(i)?;
        result.insert(name, value);
    }
    Ok(result)
}

/// Fetch the row that was just inserted
pub fn fetch_inserted_row(
    conn: &Connection,
    table_name: &str,
) -> Result<HashMap<String, Value>> {
    // Try to get by rowid first as it's most reliable in SQLite
    let sql = format!("SELECT * FROM {} WHERE rowid = last_insert_rowid()", table_name);
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        return row_to_map(row);
    }
    
    // Fallback to PK
    let pk_columns = get_primary_key_columns(conn, table_name)?;
    if pk_columns.is_empty() {
        return Err(anyhow!("Could not fetch inserted row for {}", table_name));
    }
    
    let pk = &pk_columns[0];
    let sql = format!("SELECT * FROM {} WHERE {} = last_insert_rowid()", table_name, pk);
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        return row_to_map(row);
    }
    
    Err(anyhow!("Could not fetch inserted row"))
}

/// Extract OLD row data from a DML statement (if possible)
#[allow(dead_code)]
pub fn extract_old_row_from_dml_simple(
    _sql: &str,
) -> Option<HashMap<String, Value>> {
    None // Placeholder
}

/// Extract the NEW row data from an INSERT statement
///
/// Parses INSERT statements to extract column names and values.
/// Supports simple INSERT VALUES statements.
///
/// # Arguments
/// * `sql` - The INSERT SQL statement
///
/// # Returns
/// A HashMap containing column names and their values, or None if parsing fails
pub fn extract_inserted_row(sql: &str) -> Option<HashMap<String, Value>> {
    use pg_query::protobuf::node::Node as NodeEnum;

    let result = pg_query::parse(sql).ok()?;

    if let Some(raw_stmt) = result.protobuf.stmts.first() {
        if let Some(ref stmt_node) = raw_stmt.stmt {
            if let Some(NodeEnum::InsertStmt(stmt)) = &stmt_node.node {
                return extract_insert_data(stmt);
            }
        }
    }

    None
}

/// Extract column names and values from an InsertStmt
fn extract_insert_data(stmt: &pg_query::protobuf::InsertStmt) -> Option<HashMap<String, Value>> {
    use pg_query::protobuf::node::Node as NodeEnum;

    let mut result = HashMap::new();

    // Get column names from the insert statement
    let column_names: Vec<String> = stmt
        .cols
        .iter()
        .filter_map(|col| {
            if let Some(NodeEnum::ResTarget(rt)) = col.node.as_ref() {
                Some(rt.name.clone())
            } else {
                None
            }
        })
        .collect();

    // Get values from the VALUES clause
    if let Some(ref select_stmt) = stmt.select_stmt {
        if let Some(NodeEnum::SelectStmt(select)) = select_stmt.node.as_ref() {
            // Get the first set of values (for simple INSERT VALUES)
            if let Some(values_list_node) = select.values_lists.first() {
                if let Some(ref node_inner) = values_list_node.node {
                    if let NodeEnum::List(list) = node_inner {
                        for (i, value_node) in list.items.iter().enumerate() {
                            if i < column_names.len() {
                                if let Ok(val) = extract_value_from_node(value_node) {
                                    result.insert(column_names[i].clone(), val);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@example.com')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (2, 'Bob', 'bob@example.com')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_build_old_row() {
        let conn = setup_test_db();

        let pk_columns = vec!["id".to_string()];
        let pk_values = vec![Value::Integer(1)];

        let old_row = build_old_row(&conn, "users", &pk_columns, &pk_values).unwrap();

        assert_eq!(old_row.get("id"), Some(&Value::Integer(1)));
        assert_eq!(
            old_row.get("name"),
            Some(&Value::Text("Alice".to_string()))
        );
        assert_eq!(
            old_row.get("email"),
            Some(&Value::Text("alice@example.com".to_string()))
        );
    }

    #[test]
    fn test_build_new_row() {
        let values = vec![
            ("name".to_string(), Value::Text("Charlie".to_string())),
            ("email".to_string(), Value::Text("charlie@example.com".to_string())),
        ];

        let new_row = build_new_row(&values);

        assert_eq!(
            new_row.get("name"),
            Some(&Value::Text("Charlie".to_string()))
        );
        assert_eq!(
            new_row.get("email"),
            Some(&Value::Text("charlie@example.com".to_string()))
        );
    }

    #[test]
    fn test_get_primary_key_columns() {
        let conn = setup_test_db();

        let pk_columns = get_primary_key_columns(&conn, "users").unwrap();
        assert_eq!(pk_columns, vec!["id"]);
    }

    #[test]
    fn test_build_old_row_not_found() {
        let conn = setup_test_db();

        let pk_columns = vec!["id".to_string()];
        let pk_values = vec![Value::Integer(999)];

        let result = build_old_row(&conn, "users", &pk_columns, &pk_values);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_old_row_composite_pk() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE order_items (order_id INTEGER, item_id INTEGER, quantity INTEGER, PRIMARY KEY (order_id, item_id))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO order_items (order_id, item_id, quantity) VALUES (1, 1, 5)",
            [],
        )
        .unwrap();

        let pk_columns = vec!["order_id".to_string(), "item_id".to_string()];
        let pk_values = vec![Value::Integer(1), Value::Integer(1)];

        let old_row = build_old_row(&conn, "order_items", &pk_columns, &pk_values).unwrap();

        assert_eq!(old_row.get("order_id"), Some(&Value::Integer(1)));
        assert_eq!(old_row.get("item_id"), Some(&Value::Integer(1)));
        assert_eq!(old_row.get("quantity"), Some(&Value::Integer(5)));
    }

    #[test]
    fn test_build_new_row_from_insert() {
        let conn = setup_test_db();

        let sql = "INSERT INTO users (id, name, email) VALUES (3, 'Charlie', 'charlie@example.com')";
        let new_row = build_new_row_from_insert(&conn, "users", sql).unwrap();

        assert_eq!(new_row.get("id"), Some(&Value::Integer(3)));
        assert_eq!(new_row.get("name"), Some(&Value::Text("Charlie".to_string())));
        assert_eq!(new_row.get("email"), Some(&Value::Text("charlie@example.com".to_string())));
    }

    #[test]
    fn test_build_new_row_from_update() {
        let conn = setup_test_db();

        let sql = "UPDATE users SET name = 'Alice Updated' WHERE id = 1";
        let new_row = build_new_row_from_update(&conn, "users", sql).unwrap();

        // Should have the updated name
        assert_eq!(new_row.get("name"), Some(&Value::Text("Alice Updated".to_string())));
        // Note: This only extracts SET clause values, not the full row
    }
}
