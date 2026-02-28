use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    AConst, AExpr, BoolExpr, ColumnDef, ColumnRef, Constraint, CreateStmt, FuncCall, Node,
    RangeVar, ResTarget, SelectStmt, TypeCast, TypeName,
};

/// Metadata for a column extracted from a CREATE TABLE statement
#[derive(Debug, Clone)]
pub struct ColumnTypeInfo {
    pub column_name: String,
    pub original_type: String,
    pub constraints: Option<String>,
}

/// Result of transpiling a SQL statement
#[derive(Debug)]
pub struct TranspileResult {
    pub sql: String,
    pub create_table_metadata: Option<CreateTableMetadata>,
}

/// Metadata extracted from a CREATE TABLE statement
#[derive(Debug)]
pub struct CreateTableMetadata {
    pub table_name: String,
    pub columns: Vec<ColumnTypeInfo>,
}

/// Transpile PostgreSQL SQL to SQLite SQL using AST walking
/// Returns both the transpiled SQL and any extracted metadata
pub fn transpile_with_metadata(sql: &str) -> TranspileResult {
    match pg_query::parse(sql) {
        Ok(result) => {
            if let Some(raw_stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = raw_stmt.stmt {
                    return reconstruct_sql_with_metadata(stmt_node);
                }
            }

            TranspileResult {
                sql: sql.to_lowercase(),
                create_table_metadata: None,
            }
        }
        Err(_) => {
            // Fallback for parse errors: use original string-based transformation
            TranspileResult {
                sql: sql.to_lowercase().replace("now()", "datetime('now')"),
                create_table_metadata: None,
            }
        }
    }
}

/// Transpile PostgreSQL SQL to SQLite SQL (backward compatible)
pub fn transpile(sql: &str) -> String {
    transpile_with_metadata(sql).sql
}

/// Reconstruct SQL from a parsed AST node, returning both SQL and metadata
fn reconstruct_sql_with_metadata(node: &Node) -> TranspileResult {
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::SelectStmt(ref select_stmt) => TranspileResult {
                sql: reconstruct_select_stmt(select_stmt),
                create_table_metadata: None,
            },
            NodeEnum::CreateStmt(ref create_stmt) => {
                reconstruct_create_stmt_with_metadata(create_stmt)
            }
            _ => TranspileResult {
                sql: node.deparse().unwrap_or_else(|_| "".to_string()),
                create_table_metadata: None,
            },
        }
    } else {
        TranspileResult {
            sql: String::new(),
            create_table_metadata: None,
        }
    }
}

/// Reconstruct SQL from a parsed AST node (backward compatible)
fn reconstruct_sql(node: &Node) -> String {
    reconstruct_sql_with_metadata(node).sql
}

/// Reconstruct a CREATE TABLE statement and extract metadata
fn reconstruct_create_stmt_with_metadata(stmt: &CreateStmt) -> TranspileResult {
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| r.relname.clone())
        .unwrap_or_default();

    let mut columns: Vec<ColumnTypeInfo> = Vec::new();
    let mut column_defs: Vec<String> = Vec::new();

    for element in &stmt.table_elts {
        if let Some(ref node) = element.node {
            if let NodeEnum::ColumnDef(ref col_def) = node {
                let (col_sql, type_info) = reconstruct_column_def(col_def);
                column_defs.push(col_sql);
                if let Some(info) = type_info {
                    columns.push(info);
                }
            }
        }
    }

    let sql = format!(
        "create table {} ({})",
        table_name.to_lowercase(),
        column_defs.join(", ")
    );

    let metadata = if columns.is_empty() {
        None
    } else {
        Some(CreateTableMetadata {
            table_name: table_name.to_lowercase(),
            columns,
        })
    };

    TranspileResult {
        sql,
        create_table_metadata: metadata,
    }
}

/// Reconstruct a column definition and extract type metadata
/// Returns (SQLite column SQL, optional metadata)
fn reconstruct_column_def(col_def: &ColumnDef) -> (String, Option<ColumnTypeInfo>) {
    let col_name = col_def.colname.clone();
    let original_type = extract_original_type(&col_def.type_name);
    let sqlite_type = rewrite_type_for_sqlite(&original_type);

    // Extract constraints (NOT NULL, PRIMARY KEY, etc.)
    let constraints = extract_constraints(&col_def.constraints);
    let constraints_str = if constraints.is_empty() {
        None
    } else {
        Some(constraints.clone())
    };

    // Combine type and constraints for SQLite
    let mut parts = vec![col_name.to_lowercase(), sqlite_type];
    if !constraints.is_empty() {
        parts.push(constraints);
    }

    let metadata = ColumnTypeInfo {
        column_name: col_name.to_lowercase(),
        original_type: original_type.clone(),
        constraints: constraints_str,
    };

    (parts.join(" "), Some(metadata))
}

/// Extract the original PostgreSQL type name from TypeName
/// Maps internal PostgreSQL type names back to user-facing names
fn extract_original_type(type_name: &Option<TypeName>) -> String {
    if let Some(tn) = type_name {
        let names: Vec<String> = tn
            .names
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::String(s) = inner {
                        return Some(s.sval.clone());
                    }
                }
                None
            })
            .collect();

        if names.is_empty() {
            return "TEXT".to_string();
        }

        let base_type = names.last().unwrap().to_uppercase();

        // Map internal PostgreSQL type names to user-facing names
        let mapped_type = match base_type.as_str() {
            "TIMESTAMPTZ" => "TIMESTAMP WITH TIME ZONE",
            "TIMESTAMP" => "TIMESTAMP WITHOUT TIME ZONE",
            "TIMETZ" => "TIME WITH TIME ZONE",
            "TIME" => "TIME WITHOUT TIME ZONE",
            "VARBIT" => "BIT VARYING",
            "BPCHAR" => "CHARACTER",
            "VARCHAR" => "VARCHAR",
            "CHAR" => "CHARACTER",
            "INT8" => "BIGINT",
            "INT4" => "INTEGER",
            "INT2" => "SMALLINT",
            "FLOAT4" => "REAL",
            "FLOAT8" => "DOUBLE PRECISION",
            "BOOL" => "BOOLEAN",
            _ => &base_type,
        };

        if tn.typmods.is_empty() {
            mapped_type.to_string()
        } else {
            let mods: Vec<String> = tn
                .typmods
                .iter()
                .filter_map(|m| {
                    if let Some(ref inner) = m.node {
                        if let NodeEnum::AConst(ref aconst) = inner {
                            if let Some(ref val) = aconst.val {
                                if let pg_query::protobuf::a_const::Val::Ival(i) = val {
                                    return Some(i.ival.to_string());
                                }
                            }
                        }
                    }
                    None
                })
                .collect();

            if mods.is_empty() {
                mapped_type.to_string()
            } else {
                format!("{}({})", mapped_type, mods.join(", "))
            }
        }
    } else {
        "TEXT".to_string()
    }
}

/// Rewrite PostgreSQL types to SQLite-compatible types
fn rewrite_type_for_sqlite(pg_type: &str) -> String {
    let upper = pg_type.to_uppercase();

    // SERIAL types -> INTEGER PRIMARY KEY AUTOINCREMENT
    if upper.starts_with("SERIAL") {
        return "INTEGER PRIMARY KEY AUTOINCREMENT".to_string();
    }
    if upper.starts_with("BIGSERIAL") {
        return "INTEGER PRIMARY KEY AUTOINCREMENT".to_string();
    }
    if upper.starts_with("SMALLSERIAL") {
        return "INTEGER PRIMARY KEY AUTOINCREMENT".to_string();
    }

    // Character types -> TEXT
    if upper.starts_with("VARCHAR")
        || upper.starts_with("CHARACTER VARYING")
        || upper.starts_with("CHAR")
        || upper.starts_with("CHARACTER")
        || upper == "TEXT"
    {
        return "TEXT".to_string();
    }

    // Numeric types
    if upper.starts_with("INT") || upper.starts_with("INTEGER") || upper.starts_with("BIGINT") || upper.starts_with("SMALLINT")
    {
        return "INTEGER".to_string();
    }

    if upper.starts_with("REAL")
        || upper.starts_with("FLOAT")
        || upper.starts_with("DOUBLE")
        || upper.starts_with("NUMERIC")
        || upper.starts_with("DECIMAL")
    {
        return "REAL".to_string();
    }

    // Boolean -> INTEGER (SQLite doesn't have native boolean)
    if upper == "BOOLEAN" || upper == "BOOL" {
        return "INTEGER".to_string();
    }

    // Date/Time types -> TEXT
    if upper.starts_with("TIMESTAMP")
        || upper.starts_with("DATE")
        || upper.starts_with("TIME")
    {
        return "TEXT".to_string();
    }

    // JSON/JSONB -> TEXT
    if upper == "JSON" || upper == "JSONB" {
        return "TEXT".to_string();
    }

    // UUID -> TEXT
    if upper == "UUID" {
        return "TEXT".to_string();
    }

    // BYTEA -> BLOB
    if upper == "BYTEA" {
        return "BLOB".to_string();
    }

    // Default fallback
    "TEXT".to_string()
}

/// Extract constraint strings from column constraints
fn extract_constraints(constraints: &[Node]) -> String {
    let parts: Vec<String> = constraints
        .iter()
        .filter_map(|c| {
            if let Some(ref inner) = c.node {
                if let NodeEnum::Constraint(ref con) = inner {
                    return reconstruct_constraint(con);
                }
            }
            None
        })
        .collect();

    parts.join(" ")
}

/// Reconstruct a single constraint
fn reconstruct_constraint(constraint: &Constraint) -> Option<String> {
    match constraint.contype() {
        pg_query::protobuf::ConstrType::ConstrNotnull => Some("NOT NULL".to_string()),
        pg_query::protobuf::ConstrType::ConstrNull => Some("NULL".to_string()),
        pg_query::protobuf::ConstrType::ConstrDefault => {
            if let Some(ref expr) = constraint.raw_expr {
                let expr_sql = reconstruct_node(expr);
                Some(format!("DEFAULT {}", expr_sql))
            } else {
                None
            }
        }
        pg_query::protobuf::ConstrType::ConstrPrimary => Some("PRIMARY KEY".to_string()),
        pg_query::protobuf::ConstrType::ConstrUnique => Some("UNIQUE".to_string()),
        pg_query::protobuf::ConstrType::ConstrCheck => {
            if let Some(ref expr) = constraint.raw_expr {
                let expr_sql = reconstruct_node(expr);
                Some(format!("CHECK ({}", expr_sql))
            } else {
                None
            }
        }
        pg_query::protobuf::ConstrType::ConstrForeign => {
            // Foreign key constraints are more complex, skip for now
            None
        }
        _ => None,
    }
}

/// Reconstruct a SELECT statement
fn reconstruct_select_stmt(stmt: &SelectStmt) -> String {
    let mut parts = Vec::new();

    parts.push("select".to_string());

    if stmt.target_list.is_empty() {
        parts.push("*".to_string());
    } else {
        let columns: Vec<String> = stmt
            .target_list
            .iter()
            .map(reconstruct_node)
            .collect::<Vec<_>>()
            .join(", ")
            .split(", ")
            .map(|s| s.to_string())
            .collect();
        parts.push(columns.join(", "));
    }

    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt
            .from_clause
            .iter()
            .map(reconstruct_node)
            .collect::<Vec<_>>()
            .join(", ")
            .split(", ")
            .map(|s| s.to_string())
            .collect();
        parts.push(tables.join(", "));
    }

    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    if let Some(ref limit_count) = stmt.limit_count {
        if is_limit_all(limit_count) {
            parts.push("limit".to_string());
            parts.push("-1".to_string());
        } else {
            let limit_sql = reconstruct_node(limit_count);
            if !limit_sql.is_empty() {
                parts.push("limit".to_string());
                parts.push(limit_sql);
            }
        }
    }

    parts.join(" ")
}

/// Reconstruct SQL from a generic AST node
fn reconstruct_node(node: &Node) -> String {
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::ResTarget(ref res_target) => reconstruct_res_target(res_target),
            NodeEnum::RangeVar(ref range_var) => reconstruct_range_var(range_var),
            NodeEnum::AStar(_) => "*".to_string(),
            NodeEnum::ColumnRef(ref col_ref) => reconstruct_column_ref(col_ref),
            NodeEnum::String(s) => s.sval.clone(),
            NodeEnum::FuncCall(ref func_call) => reconstruct_func_call(func_call),
            NodeEnum::AConst(ref aconst) => reconstruct_aconst(aconst),
            NodeEnum::TypeCast(ref type_cast) => reconstruct_type_cast(type_cast),
            NodeEnum::AExpr(ref a_expr) => reconstruct_a_expr(a_expr),
            NodeEnum::BoolExpr(ref bool_expr) => reconstruct_bool_expr(bool_expr),
            _ => node.deparse().unwrap_or_else(|_| "".to_string()),
        }
    } else {
        String::new()
    }
}

/// Reconstruct a TypeCast node
fn reconstruct_type_cast(type_cast: &TypeCast) -> String {
    let arg_sql = type_cast
        .arg
        .as_ref()
        .map(|n| reconstruct_node(n))
        .unwrap_or_default();
    let original_type = extract_original_type(&type_cast.type_name);
    let sqlite_type = rewrite_type_for_sqlite(&original_type);
    format!("CAST({} AS {})", arg_sql, sqlite_type)
}

/// Reconstruct an AExpr node (operators)
fn reconstruct_a_expr(a_expr: &AExpr) -> String {
    let lexpr_sql = a_expr
        .lexpr
        .as_ref()
        .map(|n| reconstruct_node(n))
        .unwrap_or_default();
    let rexpr_sql = a_expr
        .rexpr
        .as_ref()
        .map(|n| reconstruct_node(n))
        .unwrap_or_default();
    let op_name = a_expr
        .name
        .first()
        .and_then(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::String(s) = inner {
                    return Some(s.sval.clone());
                }
            }
            None
        })
        .unwrap_or_else(|| "".to_string());

    // Basic operator mapping (Postgres specific ones can be added here)
    let sqlite_op = match op_name.as_str() {
        "~~" => "LIKE",
        "~~*" => "LIKE", // SQLite LIKE is case-insensitive anyway by default for ASCII
        "!~~" => "NOT LIKE",
        _ => &op_name,
    };

    format!("{} {} {}", lexpr_sql, sqlite_op, rexpr_sql)
}

/// Reconstruct a BoolExpr node (AND, OR, NOT)
fn reconstruct_bool_expr(bool_expr: &BoolExpr) -> String {
    let op = match bool_expr.boolop() {
        pg_query::protobuf::BoolExprType::AndExpr => "AND",
        pg_query::protobuf::BoolExprType::OrExpr => "OR",
        pg_query::protobuf::BoolExprType::NotExpr => "NOT",
        _ => "AND", // Default
    };

    let args: Vec<String> = bool_expr.args.iter().map(reconstruct_node).collect();

    if bool_expr.boolop() == pg_query::protobuf::BoolExprType::NotExpr {
        format!("NOT ({})", args.join(" "))
    } else {
        format!("({})", args.join(&format!(" {} ", op)))
    }
}

/// Reconstruct a ResTarget (column/expression with optional alias)
fn reconstruct_res_target(target: &ResTarget) -> String {
    if let Some(ref val) = target.val {
        let val_sql = reconstruct_node(val);
        if target.name.is_empty() {
            val_sql
        } else {
            format!("{} as {}", val_sql, target.name.to_lowercase())
        }
    } else {
        String::new()
    }
}

/// Reconstruct a RangeVar (table reference)
fn reconstruct_range_var(range_var: &RangeVar) -> String {
    let table_name = range_var.relname.to_lowercase();
    let schema_name = range_var.schemaname.to_lowercase();

    if schema_name.is_empty() || schema_name == "public" {
        table_name
    } else {
        format!("{}.{}", schema_name, table_name)
    }
}

/// Reconstruct a ColumnRef
fn reconstruct_column_ref(col_ref: &ColumnRef) -> String {
    col_ref
        .fields
        .iter()
        .filter_map(|f| {
            if let Some(ref inner) = f.node {
                match inner {
                    NodeEnum::String(s) => Some(s.sval.to_lowercase()),
                    NodeEnum::AStar(_) => Some("*".to_string()),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Check if a limit_count node represents LIMIT ALL
fn is_limit_all(node: &Node) -> bool {
    if let Some(ref inner) = node.node {
        if let NodeEnum::AConst(ref aconst) = inner {
            return aconst.isnull;
        }
    }
    false
}

/// Reconstruct a function call
fn reconstruct_func_call(func_call: &FuncCall) -> String {
    let func_name = func_call
        .funcname
        .first()
        .and_then(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::String(s) = inner {
                    return Some(s.sval.to_lowercase());
                }
            }
            None
        })
        .unwrap_or_default();

    if func_name == "now" && func_call.args.is_empty() {
        return "datetime('now')".to_string();
    }

    let args: Vec<String> = func_call.args.iter().map(reconstruct_node).collect();
    format!("{}({})", func_name, args.join(", "))
}

/// Reconstruct an AConst node
fn reconstruct_aconst(aconst: &AConst) -> String {
    if let Some(ref val) = aconst.val {
        match val {
            pg_query::protobuf::a_const::Val::Ival(i) => i.ival.to_string(),
            pg_query::protobuf::a_const::Val::Fval(f) => f.fval.clone(),
            pg_query::protobuf::a_const::Val::Sval(s) => format!("'{}'", s.sval),
            pg_query::protobuf::a_const::Val::Bsval(b) => format!("'{}'", b.bsval),
            _ => String::new(),
        }
    } else {
        String::new()
    }
}

pub fn debug_ast(sql: &str) {
    match pg_query::parse(sql) {
        Ok(result) => {
            println!("Parsed SQL: {}", sql);
            if let Some(raw_stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = raw_stmt.stmt {
                    debug_node(stmt_node, 0);
                }
            }
        }
        Err(e) => {
            println!("Parse error: {:?}", e);
        }
    }
}

fn debug_node(node: &Node, indent: usize) {
    let prefix = "  ".repeat(indent);
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::SelectStmt(stmt) => {
                println!("{}SelectStmt:", prefix);
                println!("{}  target_list ({} items):", prefix, stmt.target_list.len());
                for (i, target) in stmt.target_list.iter().enumerate() {
                    println!("{}    [{}]:", prefix, i);
                    debug_node(target, indent + 3);
                }
                println!("{}  from_clause ({} items):", prefix, stmt.from_clause.len());
                for (i, from) in stmt.from_clause.iter().enumerate() {
                    println!("{}    [{}]:", prefix, i);
                    debug_node(from, indent + 3);
                }
                println!("{}  limit_option: {}", prefix, stmt.limit_option);
                if let Some(ref limit) = stmt.limit_count {
                    println!("{}  limit_count:", prefix);
                    debug_node(limit, indent + 2);
                }
            }
            NodeEnum::ResTarget(target) => {
                println!("{}ResTarget:", prefix);
                println!("{}  name: {:?}", prefix, target.name);
                if let Some(ref val) = target.val {
                    println!("{}  val:", prefix);
                    debug_node(val, indent + 2);
                } else {
                    println!("{}  val: None", prefix);
                }
            }
            NodeEnum::AStar(_) => {
                println!("{}AStar", prefix);
            }
            NodeEnum::RangeVar(range_var) => {
                println!("{}RangeVar: {}", prefix, range_var.relname);
            }
            NodeEnum::ColumnRef(col_ref) => {
                println!("{}ColumnRef:", prefix);
                for (i, field) in col_ref.fields.iter().enumerate() {
                    println!("{}  field[{}]:", prefix, i);
                    debug_node(field, indent + 2);
                }
            }
            NodeEnum::String(s) => {
                println!("{}String: {:?}", prefix, s.sval);
            }
            NodeEnum::CreateStmt(stmt) => {
                println!("{}CreateStmt:", prefix);
                if let Some(ref rel) = stmt.relation {
                    println!("{}  table: {}", prefix, rel.relname);
                }
                println!("{}  columns ({} items):", prefix, stmt.table_elts.len());
                for (i, col) in stmt.table_elts.iter().enumerate() {
                    println!("{}    [{}]:", prefix, i);
                    debug_node(col, indent + 3);
                }
            }
            NodeEnum::ColumnDef(col_def) => {
                println!("{}ColumnDef: {}", prefix, col_def.colname);
                if let Some(ref type_name) = col_def.type_name {
                    let names: Vec<String> = type_name
                        .names
                        .iter()
                        .filter_map(|n| {
                            n.node.as_ref().map(|inner| {
                                if let NodeEnum::String(s) = inner {
                                    s.sval.clone()
                                } else {
                                    String::new()
                                }
                            })
                        })
                        .collect();
                    println!("{}  type: {:?}", prefix, names);
                }
            }
            _ => {
                println!("{}Other: {:?}", prefix, inner);
            }
        }
    } else {
        println!("{}Empty Node", prefix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_select_star() {
        debug_ast("SELECT * FROM users");
    }

    #[test]
    fn test_transpile_select_star() {
        let input = "SELECT * FROM users";
        let expected = "select * from users";
        assert_eq!(transpile(input), expected);
    }

    #[test]
    fn test_transpile_select_columns() {
        let input = "SELECT id, name FROM users";
        let expected = "select id, name from users";
        assert_eq!(transpile(input), expected);
    }

    #[test]
    fn test_transpile_limit_all() {
        let input = "SELECT * FROM users LIMIT ALL";
        let expected = "select * from users limit -1";
        assert_eq!(transpile(input), expected);
    }

    #[test]
    fn test_transpile_now() {
        let input = "SELECT now()";
        let result = transpile(input);
        assert_eq!(result, "select datetime('now')");
    }

    #[test]
    fn test_transpile_schema_mapping_public() {
        let input = "SELECT * FROM public.users";
        let result = transpile(input);
        assert_eq!(result, "select * from users");
    }

    #[test]
    fn test_transpile_schema_mapping_attached_db() {
        let input = "SELECT * FROM inventory.products";
        let result = transpile(input);
        assert_eq!(result, "select * from inventory.products");
    }

    #[test]
    fn test_transpile_unqualified_table() {
        let input = "SELECT * FROM users";
        let result = transpile(input);
        assert_eq!(result, "select * from users");
    }

    #[test]
    fn test_create_table_transpile() {
        let input = "CREATE TABLE test_table (id SERIAL, name VARCHAR(10))";
        let result = transpile_with_metadata(input);

        println!("Input: {}", input);
        println!("Output SQL: {}", result.sql);
        
        assert!(result.sql.contains("create table test_table"), "Missing 'create table test_table' in: {}", result.sql);
        assert!(result.sql.contains("INTEGER PRIMARY KEY AUTOINCREMENT"), "Missing 'INTEGER PRIMARY KEY AUTOINCREMENT' in: {}", result.sql);
        assert!(result.sql.contains("TEXT"), "Missing 'TEXT' in: {}", result.sql);

        let metadata = result.create_table_metadata.expect("Should have metadata");
        assert_eq!(metadata.table_name, "test_table");
        assert_eq!(metadata.columns.len(), 2);

        let id_col = metadata
            .columns
            .iter()
            .find(|c| c.column_name == "id")
            .expect("Should have id column");
        assert_eq!(id_col.original_type, "SERIAL");
    }

    #[test]
    fn test_create_table_timestamp() {
        let input = "CREATE TABLE events (id SERIAL, created_at TIMESTAMP WITH TIME ZONE)";
        let result = transpile_with_metadata(input);

        let metadata = result.create_table_metadata.expect("Should have metadata");
        let ts_col = metadata
            .columns
            .iter()
            .find(|c| c.column_name == "created_at")
            .expect("Should have created_at column");
        assert_eq!(ts_col.original_type, "TIMESTAMP WITH TIME ZONE");
    }

    #[test]
    fn test_rewrite_types() {
        assert_eq!(rewrite_type_for_sqlite("SERIAL"), "INTEGER PRIMARY KEY AUTOINCREMENT");
        assert_eq!(rewrite_type_for_sqlite("VARCHAR(10)"), "TEXT");
        assert_eq!(rewrite_type_for_sqlite("TEXT"), "TEXT");
        assert_eq!(rewrite_type_for_sqlite("INTEGER"), "INTEGER");
        assert_eq!(rewrite_type_for_sqlite("BOOLEAN"), "INTEGER");
        assert_eq!(rewrite_type_for_sqlite("TIMESTAMP WITH TIME ZONE"), "TEXT");
        assert_eq!(rewrite_type_for_sqlite("UUID"), "TEXT");
        assert_eq!(rewrite_type_for_sqlite("JSONB"), "TEXT");
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    fn test_debug_distinct() {
        println!("--- SELECT DISTINCT ---");
        debug_ast("SELECT DISTINCT id FROM users");
        
        println!("\n--- SELECT DISTINCT ON ---");
        debug_ast("SELECT DISTINCT ON (id) id, name FROM users ORDER BY id, name");
    }

    #[test]
    fn test_debug_cast() {
        println!("--- CAST :: ---");
        debug_ast("SELECT '1'::int, 1.2::float, name::text FROM users");
    }
}

#[cfg(test)]
mod operator_tests {
    use super::*;

    #[test]
    fn test_transpile_cast() {
        let input = "SELECT '1'::int";
        let result = transpile(input);
        assert_eq!(result, "select CAST('1' AS INTEGER)");
    }

    #[test]
    fn test_transpile_operators() {
        let input = "SELECT id FROM users WHERE name ~~ 'alice%' AND id > 10";
        let result = transpile(input);
        assert!(result.contains("LIKE 'alice%'"));
        assert!(result.contains("AND"));
        assert!(result.contains("id > 10"));
    }
}
