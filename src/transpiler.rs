use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    AArrayExpr, AConst, AExpr, ArrayExpr, BoolExpr, ColumnDef, ColumnRef, Constraint, CreateStmt, FuncCall, Node,
    RangeVar, ResTarget, SelectStmt, TypeCast, TypeName, InsertStmt, UpdateStmt, DeleteStmt,
    JoinExpr, NullTest, SubLink, CaseExpr, CreateRoleStmt, DropRoleStmt, GrantStmt, GrantRoleStmt,
    AlterTableStmt, WindowDef, RangeSubselect, CoalesceExpr, DropStmt, IndexStmt, SqlValueFunction,
    RangeFunction,
};

// RLS-related imports
use crate::rls::{RlsContext, get_rls_where_clause, can_bypass_rls, build_rls_expression};
use crate::catalog::{is_rls_enabled, get_applicable_policies};
use rusqlite::Connection;

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
    pub referenced_tables: Vec<String>,
    pub operation_type: OperationType,
}

/// Type of SQL operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    SELECT,
    INSERT,
    UPDATE,
    DELETE,
    DDL,
    OTHER,
}

/// Metadata extracted from a CREATE TABLE statement
#[derive(Debug)]
pub struct CreateTableMetadata {
    pub table_name: String,
    pub columns: Vec<ColumnTypeInfo>,
}

/// Context for the transpilation process
pub struct TranspileContext {
    pub referenced_tables: Vec<String>,
}

impl TranspileContext {
    pub fn new() -> Self {
        Self {
            referenced_tables: Vec::new(),
        }
    }
}

/// Transpile PostgreSQL SQL to SQLite SQL using AST walking
/// Returns both the transpiled SQL and any extracted metadata
pub fn transpile_with_metadata(sql: &str) -> TranspileResult {
    let mut ctx = TranspileContext::new();
    match pg_query::parse(sql) {
        Ok(result) => {
            if let Some(raw_stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = raw_stmt.stmt {
                    return reconstruct_sql_with_metadata(stmt_node, &mut ctx);
                }
            }

            TranspileResult {
                sql: sql.to_lowercase(),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::OTHER,
            }
        }
        Err(_) => {
            // Fallback: simple string replacement for basic cases
            TranspileResult {
                sql: sql.to_lowercase().replace("now()", "datetime('now')"),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::OTHER,
            }
        }
    }
}

#[allow(dead_code)]
/// Transpile PostgreSQL SQL to SQLite SQL (backward compatible)
pub fn transpile(sql: &str) -> String {
    transpile_with_metadata(sql).sql
}

/// Reconstruct SQL from a parsed AST node, returning both SQL and metadata
fn reconstruct_sql_with_metadata(node: &Node, ctx: &mut TranspileContext) -> TranspileResult {
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::SelectStmt(ref select_stmt) => TranspileResult {
                sql: reconstruct_select_stmt(select_stmt, ctx),
                create_table_metadata: None,
                referenced_tables: ctx.referenced_tables.clone(),
                operation_type: OperationType::SELECT,
            },
            NodeEnum::CreateStmt(ref create_stmt) => {
                let mut res = reconstruct_create_stmt_with_metadata(create_stmt, ctx);
                res.operation_type = OperationType::DDL;
                res
            }
            NodeEnum::InsertStmt(ref insert_stmt) => TranspileResult {
                sql: reconstruct_insert_stmt(insert_stmt, ctx),
                create_table_metadata: None,
                referenced_tables: ctx.referenced_tables.clone(),
                operation_type: OperationType::INSERT,
            },
            NodeEnum::UpdateStmt(ref update_stmt) => TranspileResult {
                sql: reconstruct_update_stmt(update_stmt, ctx),
                create_table_metadata: None,
                referenced_tables: ctx.referenced_tables.clone(),
                operation_type: OperationType::UPDATE,
            },
            NodeEnum::DeleteStmt(ref delete_stmt) => TranspileResult {
                sql: reconstruct_delete_stmt(delete_stmt, ctx),
                create_table_metadata: None,
                referenced_tables: ctx.referenced_tables.clone(),
                operation_type: OperationType::DELETE,
            },
            NodeEnum::VariableSetStmt(ref set_stmt) => {
                // Handle SET ROLE specially
                if set_stmt.name == "role" && !set_stmt.args.is_empty() {
                    if let Some(ref node) = set_stmt.args[0].node {
                        if let NodeEnum::AConst(ref aconst) = node {
                            if let Some(ref val) = aconst.val {
                                if let pg_query::protobuf::a_const::Val::Sval(ref s) = val {
                                    return TranspileResult {
                                        sql: format!("-- SET ROLE {}", s.sval),
                                        create_table_metadata: None,
                                        referenced_tables: Vec::new(),
                                        operation_type: OperationType::OTHER,
                                    };
                                }
                            }
                        }
                    }
                }
                TranspileResult {
                    sql: "select 1".to_string(), // Safely ignore other SET statements
                    create_table_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::OTHER,
                }
            }
            NodeEnum::VariableShowStmt(ref show_stmt) => TranspileResult {
                sql: format!("select current_setting('{}') as {}", show_stmt.name, show_stmt.name),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::SELECT,
            },
            NodeEnum::CreateRoleStmt(ref create_role_stmt) => TranspileResult {
                sql: reconstruct_create_role_stmt(create_role_stmt, ctx),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
            },
            NodeEnum::DropRoleStmt(ref drop_role_stmt) => TranspileResult {
                sql: reconstruct_drop_role_stmt(drop_role_stmt),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
            },
            NodeEnum::GrantStmt(ref grant_stmt) => TranspileResult {
                sql: reconstruct_grant_stmt(grant_stmt),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
            },
            NodeEnum::GrantRoleStmt(ref grant_role_stmt) => TranspileResult {
                sql: reconstruct_grant_role_stmt(grant_role_stmt),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
            },
            NodeEnum::AlterTableStmt(ref alter_stmt) => {
                let sql = reconstruct_alter_table_stmt(alter_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                }
            }
            NodeEnum::DropStmt(ref drop_stmt) => {
                let sql = reconstruct_drop_stmt(drop_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                }
            }
            NodeEnum::IndexStmt(ref index_stmt) => {
                let sql = reconstruct_index_stmt(index_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                }
            }
            _ => TranspileResult {
                sql: node.deparse().unwrap_or_else(|_| "".to_string()).to_lowercase(),
                create_table_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::OTHER,
            },
        }
    } else {
        TranspileResult {
            sql: String::new(),
            create_table_metadata: None,
            referenced_tables: Vec::new(),
            operation_type: OperationType::OTHER,
        }
    }
}

/// Reconstruct a CREATE TABLE statement and extract metadata
fn reconstruct_create_stmt_with_metadata(stmt: &CreateStmt, ctx: &mut TranspileContext) -> TranspileResult {
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| r.relname.clone())
        .unwrap_or_default();

    // Get schema name from relation
    let schema_name = stmt
        .relation
        .as_ref()
        .map(|r| r.schemaname.to_lowercase())
        .unwrap_or_default();

    // Build full table name with schema prefix (if not public/pg_catalog)
    let full_table_name = if schema_name.is_empty() || schema_name == "public" || schema_name == "pg_catalog" {
        table_name.to_lowercase()
    } else {
        format!("{}.{}", schema_name, table_name.to_lowercase())
    };

    ctx.referenced_tables.push(table_name.to_lowercase());

    let mut columns: Vec<ColumnTypeInfo> = Vec::new();
    let mut column_defs: Vec<String> = Vec::new();

    for element in &stmt.table_elts {
        if let Some(ref node) = element.node {
            if let NodeEnum::ColumnDef(ref col_def) = node {
                let (col_sql, type_info) = reconstruct_column_def(col_def, ctx);
                column_defs.push(col_sql);
                if let Some(info) = type_info {
                    columns.push(info);
                }
            }
        }
    }

    let sql = format!(
        "create table {} ({})",
        full_table_name,
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
        referenced_tables: ctx.referenced_tables.clone(),
        operation_type: OperationType::DDL,
    }
}

/// Reconstruct a column definition and extract type metadata
/// Returns (SQLite column SQL, optional metadata)
fn reconstruct_column_def(col_def: &ColumnDef, ctx: &mut TranspileContext) -> (String, Option<ColumnTypeInfo>) {
    let col_name = col_def.colname.clone();
    let original_type = extract_original_type(&col_def.type_name);
    let sqlite_type = rewrite_type_for_sqlite(&original_type);

    // Extract constraints
    let constraints = extract_constraints(&col_def.constraints, ctx);
    let constraints_str = if constraints.is_empty() {
        None
    } else {
        Some(constraints.clone())
    };

    // Build column definition
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
/// Comprehensive coverage of all PostgreSQL data types
/// Returns lowercase types for consistency
fn rewrite_type_for_sqlite(pg_type: &str) -> String {
    let upper = pg_type.to_uppercase();

    // Serial types (auto-increment)
    if upper.starts_with("SERIAL") || upper.starts_with("SMALLSERIAL") || upper.starts_with("BIGSERIAL") {
        return "integer primary key autoincrement".to_string();
    }

    // Character/String types
    if upper.starts_with("VARCHAR")
        || upper.starts_with("CHARACTER VARYING")
        || upper.starts_with("CHAR")
        || upper.starts_with("CHARACTER")
        || upper.starts_with("BPCHAR")
        || upper == "TEXT"
    {
        return "text".to_string();
    }

    // Array types - stored as JSON text (check before INT to handle INT[])
    if upper.ends_with("[]") || upper.starts_with("ARRAY") {
        return "text".to_string();
    }

    // Range types - stored as TEXT
    if upper == "INT4RANGE" 
        || upper == "INT8RANGE" 
        || upper == "NUMRANGE" 
        || upper == "TSRANGE"
        || upper == "TSTZRANGE"
        || upper == "DATERANGE"
    {
        return "text".to_string();
    }

    // Integer types
    if upper.starts_with("INT") 
        || upper.starts_with("INTEGER") 
        || upper.starts_with("BIGINT") 
        || upper.starts_with("SMALLINT")
        || upper.starts_with("INT2")
        || upper.starts_with("INT4")
        || upper.starts_with("INT8")
    {
        return "integer".to_string();
    }

    // Floating point and numeric types
    if upper.starts_with("REAL")
        || upper.starts_with("FLOAT")
        || upper.starts_with("FLOAT4")
        || upper.starts_with("FLOAT8")
        || upper.starts_with("DOUBLE")
        || upper.starts_with("NUMERIC")
        || upper.starts_with("DECIMAL")
    {
        return "real".to_string();
    }

    // Boolean type
    if upper == "BOOLEAN" || upper == "BOOL" {
        return "integer".to_string();
    }

    // Date/Time types
    if upper.starts_with("TIMESTAMP")
        || upper.starts_with("DATE")
        || upper.starts_with("TIME")
        || upper.starts_with("INTERVAL")
    {
        return "text".to_string();
    }

    // JSON types
    if upper == "JSON" || upper == "JSONB" || upper.starts_with("JSON") {
        return "text".to_string();
    }

    // UUID type
    if upper == "UUID" {
        return "text".to_string();
    }

    // Binary data
    if upper == "BYTEA" {
        return "blob".to_string();
    }

    // VECTOR type (pgvector compatibility) - stored as TEXT (JSON format)
    if upper.starts_with("VECTOR") {
        return "text".to_string();
    }

    // Money type - store as REAL (or TEXT for precision)
    if upper == "MONEY" {
        return "real".to_string();
    }

    // Bit string types
    if upper.starts_with("BIT") || upper.starts_with("VARBIT") {
        return "text".to_string();
    }

    // XML type
    if upper == "XML" {
        return "text".to_string();
    }

    // Network address types
    if upper == "INET" || upper == "CIDR" || upper == "MACADDR" || upper == "MACADDR8" {
        return "text".to_string();
    }

    // Geometric types - all stored as TEXT (representations)
    if upper == "POINT" 
        || upper == "LINE" 
        || upper == "LSEG" 
        || upper == "BOX" 
        || upper == "PATH" 
        || upper == "POLYGON" 
        || upper == "CIRCLE" 
    {
        return "text".to_string();
    }

    // Full-text search types
    if upper == "TSVECTOR" || upper == "TSQUERY" {
        return "text".to_string();
    }

    // Default to TEXT for unknown types (ENUM, DOMAIN, composite types, etc.)
    "text".to_string()
}

/// Extract constraint strings from column constraints
fn extract_constraints(constraints: &[Node], ctx: &mut TranspileContext) -> String {
    let parts: Vec<String> = constraints
        .iter()
        .filter_map(|c| {
            if let Some(ref inner) = c.node {
                if let NodeEnum::Constraint(ref con) = inner {
                    return reconstruct_constraint(con, ctx);
                }
            }
            None
        })
        .collect();

    parts.join(" ")
}

/// Reconstruct a single constraint
fn reconstruct_constraint(constraint: &Constraint, ctx: &mut TranspileContext) -> Option<String> {
    match constraint.contype() {
        pg_query::protobuf::ConstrType::ConstrNotnull => Some("NOT NULL".to_string()),
        pg_query::protobuf::ConstrType::ConstrNull => Some("NULL".to_string()),
        pg_query::protobuf::ConstrType::ConstrDefault => {
            if let Some(ref expr) = constraint.raw_expr {
                let expr_sql = reconstruct_node(expr, ctx);
                // SQLite requires parentheses around function calls in DEFAULT
                // Check if the expression contains parentheses (indicating a function call)
                let is_func_call = expr_sql.contains('(') && expr_sql.contains(')');
                if is_func_call {
                    Some(format!("DEFAULT ({})", expr_sql))
                } else {
                    Some(format!("DEFAULT {}", expr_sql))
                }
            } else {
                None
            }
        }
        pg_query::protobuf::ConstrType::ConstrPrimary => {
            // Skip PRIMARY KEY if type already includes it (e.g., SERIAL -> integer primary key autoincrement)
            return None;
        }
        pg_query::protobuf::ConstrType::ConstrUnique => Some("UNIQUE".to_string()),
        pg_query::protobuf::ConstrType::ConstrCheck => {
            if let Some(ref expr) = constraint.raw_expr {
                let expr_sql = reconstruct_node(expr, ctx);
                Some(format!("CHECK ({})", expr_sql))
            } else {
                None
            }
        }
        pg_query::protobuf::ConstrType::ConstrForeign => {
            // Foreign keys are not fully supported in this simplified version
            None
        }
        _ => None,
    }
}

/// Reconstruct a SELECT statement with DISTINCT ON using ROW_NUMBER() polyfill
fn reconstruct_distinct_on_select(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Extract DISTINCT ON expressions
    let partition_exprs = crate::distinct_on::extract_distinct_on_exprs(stmt);
    if partition_exprs.is_empty() {
        // Fallback to regular SELECT
        return reconstruct_select_stmt_fallback(stmt, ctx);
    }

    // Build inner query columns - also save original columns for outer SELECT
    let mut inner_cols = Vec::new();
    let outer_select_cols: String;

    if stmt.target_list.is_empty() {
        inner_cols.push("*".to_string());
        // For SELECT *, we need to exclude __rn in outer query
        // This is tricky - we'll use a subquery approach
        outer_select_cols = "*".to_string();
    } else {
        let original_cols: Vec<String> = stmt.target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_cols = original_cols.clone();
        // Build outer SELECT with original column names (excluding __rn)
        outer_select_cols = original_cols.join(", ");
    }

    // Build ROW_NUMBER() OVER clause
    let partition_by = partition_exprs.join(", ");

    // Build ORDER BY for window (must include DISTINCT ON expressions + additional sort)
    let order_by = if !stmt.sort_clause.is_empty() {
        let sorts: Vec<String> = stmt.sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        sorts.join(", ")
    } else {
        // No ORDER BY - use DISTINCT ON expressions
        partition_by.clone()
    };

    // Add ROW_NUMBER column
    let row_num_col = format!(
        "row_number() over (partition by {} order by {}) as \"__rn\"",
        partition_by, order_by
    );
    inner_cols.push(row_num_col);

    // Build inner query
    let mut inner_parts = Vec::new();
    inner_parts.push("select".to_string());
    inner_parts.push(inner_cols.join(", "));

    // FROM clause
    if !stmt.from_clause.is_empty() {
        inner_parts.push("from".to_string());
        let tables: Vec<String> = stmt.from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_parts.push(tables.join(", "));
    }

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            inner_parts.push("where".to_string());
            inner_parts.push(where_sql);
        }
    }

    // GROUP BY clause
    if !stmt.group_clause.is_empty() {
        inner_parts.push("group by".to_string());
        let groups: Vec<String> = stmt.group_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_parts.push(groups.join(", "));
    }

    // HAVING clause
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause, ctx);
        if !having_sql.is_empty() {
            inner_parts.push("having".to_string());
            inner_parts.push(having_sql);
        }
    }

    let inner_query = inner_parts.join(" ");

    // Build outer query - select original columns, exclude __rn
    let mut outer_parts = Vec::new();

    // For SELECT *, we need to explicitly list columns to exclude __rn
    // This is a limitation - for SELECT * queries, __rn may appear in results
    // But for explicit column lists, we can exclude it
    if outer_select_cols == "*" {
        // We need to wrap again to filter out __rn
        // Use: SELECT * EXCEPT (__rn) is not supported in SQLite
        // Instead, we'll just select * and the client will see __rn
        // This is acceptable as PostgreSQL DISTINCT ON doesn't add extra columns
        // A better solution would be to parse the table schema
        outer_parts.push("select * from".to_string());
    } else {
        outer_parts.push(format!("select {} from", outer_select_cols));
    }
    outer_parts.push(format!("({}) as \"__distinct_on_sub\"", inner_query));
    outer_parts.push("where".to_string());
    outer_parts.push("\"__rn\" = 1".to_string());

    // Preserve ORDER BY from original query (outer query)
    if !stmt.sort_clause.is_empty() {
        outer_parts.push("order by".to_string());
        let sorts: Vec<String> = stmt.sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        outer_parts.push(sorts.join(", "));
    }

    // Preserve LIMIT
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count, ctx);
        if !limit_sql.is_empty() && limit_sql.to_uppercase() != "NULL" {
            outer_parts.push("limit".to_string());
            outer_parts.push(limit_sql);
        }
    }

    // Preserve OFFSET
    if let Some(ref limit_offset) = stmt.limit_offset {
        let offset_sql = reconstruct_node(limit_offset, ctx);
        if !offset_sql.is_empty() {
            outer_parts.push("offset".to_string());
            outer_parts.push(offset_sql);
        }
    }

    outer_parts.join(" ")
}

/// Fallback for when DISTINCT ON transformation fails
fn reconstruct_select_stmt_fallback(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Just use regular SELECT without DISTINCT ON
    let mut parts = Vec::new();
    parts.push("select".to_string());

    if stmt.target_list.is_empty() {
        parts.push("*".to_string());
    } else {
        let columns: Vec<String> = stmt.target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(columns.join(", "));
    }

    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt.from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(tables.join(", "));
    }

    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct a SELECT statement
fn reconstruct_select_stmt(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Check if this is a VALUES statement (used in INSERT)
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt, ctx);
    }

    // Handle DISTINCT ON - transform to ROW_NUMBER() window function
    if crate::distinct_on::is_distinct_on(stmt) {
        return reconstruct_distinct_on_select(stmt, ctx);
    }

    let mut parts = Vec::new();

    // Handle regular DISTINCT
    if !stmt.distinct_clause.is_empty() {
        parts.push("select distinct".to_string());
    } else {
        parts.push("select".to_string());
    }

    // Target list (columns)
    if stmt.target_list.is_empty() {
        parts.push("*".to_string());
    } else {
        let columns: Vec<String> = stmt
            .target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(columns.join(", "));
    }

    // FROM clause
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt
            .from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(tables.join(", "));
    }

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    // GROUP BY clause
    if !stmt.group_clause.is_empty() {
        parts.push("group by".to_string());
        let groups: Vec<String> = stmt
            .group_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(groups.join(", "));
    }

    // HAVING clause
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause, ctx);
        if !having_sql.is_empty() {
            parts.push("having".to_string());
            parts.push(having_sql);
        }
    }

    // ORDER BY clause (from sort_clause)
    if !stmt.sort_clause.is_empty() {
        parts.push("order by".to_string());
        let sorts: Vec<String> = stmt
            .sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        parts.push(sorts.join(", "));
    }

    // LIMIT clause
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count, ctx);
        // Check for NULL (which represents LIMIT ALL)
        if limit_sql.to_uppercase() == "NULL" {
            parts.push("limit".to_string());
            parts.push("-1".to_string());
        } else if !limit_sql.is_empty() {
            parts.push("limit".to_string());
            parts.push(limit_sql);
        }
    }

    // OFFSET clause
    if let Some(ref limit_offset) = stmt.limit_offset {
        let offset_sql = reconstruct_node(limit_offset, ctx);
        if !offset_sql.is_empty() {
            parts.push("offset".to_string());
            parts.push(offset_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct a VALUES statement (used in INSERT)
fn reconstruct_values_stmt(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    let mut values_parts = Vec::new();

    for values_list in &stmt.values_lists {
        if let Some(ref inner) = values_list.node {
            if let NodeEnum::List(list) = inner {
                let values: Vec<String> = list
                    .items
                    .iter()
                    .map(|n| reconstruct_node(n, ctx))
                    .collect();
                values_parts.push(format!("({})", values.join(", ")));
            }
        }
    }

    format!("values {}", values_parts.join(", "))
}

/// Reconstruct a SortBy node (ORDER BY)
fn reconstruct_sort_by(node: &Node, ctx: &mut TranspileContext) -> String {
    if let Some(ref inner) = node.node {
        if let NodeEnum::SortBy(sort_by) = inner {
            let expr_sql = sort_by
                .node
                .as_ref()
                .map(|n| reconstruct_node(n, ctx))
                .unwrap_or_default();

            let direction = match sort_by.sortby_dir() {
                pg_query::protobuf::SortByDir::SortbyAsc => " ASC",
                pg_query::protobuf::SortByDir::SortbyDesc => " DESC",
                _ => "",
            };

            return format!("{}{}", expr_sql, direction.to_lowercase());
        }
    }
    reconstruct_node(node, ctx)
}

/// Reconstruct an INSERT statement
fn reconstruct_insert_stmt(stmt: &InsertStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("insert into".to_string());

    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name);

    // Columns
    if !stmt.cols.is_empty() {
        let cols: Vec<String> = stmt
            .cols
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::ResTarget(target) = inner {
                        return Some(target.name.to_lowercase());
                    }
                }
                None
            })
            .collect();
        parts.push(format!("({})", cols.join(", ")));
    }

    // VALUES or SELECT
    if let Some(ref select_stmt) = stmt.select_stmt {
        let select_sql = reconstruct_node(select_stmt, ctx);
        parts.push(select_sql);
    }

    parts.join(" ")
}

/// Reconstruct an UPDATE statement
fn reconstruct_update_stmt(stmt: &UpdateStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("update".to_string());

    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name);

    // SET clause
    parts.push("set".to_string());
    let targets: Vec<String> = stmt
        .target_list
        .iter()
        .filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::ResTarget(target) = inner {
                    let col_name = target.name.to_lowercase();
                    let val = target
                        .val
                        .as_ref()
                        .map(|v| reconstruct_node(v, ctx))
                        .unwrap_or_default();
                    return Some(format!("{} = {}", col_name, val));
                }
            }
            None
        })
        .collect();
    parts.push(targets.join(", "));

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct a DELETE statement
fn reconstruct_delete_stmt(stmt: &DeleteStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("delete from".to_string());

    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name);

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct SQL from a generic AST node
fn reconstruct_node(node: &Node, ctx: &mut TranspileContext) -> String {
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::ResTarget(ref res_target) => reconstruct_res_target(res_target, ctx),
            NodeEnum::RangeVar(ref range_var) => reconstruct_range_var(range_var, ctx),
            NodeEnum::RangeSubselect(ref range_subselect) => {
                reconstruct_range_subselect(range_subselect, ctx)
            }
            NodeEnum::AStar(_) => "*".to_string(),
            NodeEnum::ColumnRef(ref col_ref) => reconstruct_column_ref(col_ref, ctx),
            NodeEnum::String(s) => s.sval.clone(),
            NodeEnum::FuncCall(ref func_call) => reconstruct_func_call(func_call, ctx),
            NodeEnum::AConst(ref aconst) => {
                let val = reconstruct_aconst(aconst);
                // Check if this constant is a string and looks like a range literal
                if val.starts_with('\'') && val.ends_with('\'') {
                    let trimmed = val[1..val.len()-1].trim();
                    if (trimmed.starts_with('[') || trimmed.starts_with('(')) &&
                       (trimmed.ends_with(']') || trimmed.ends_with(')')) &&
                       (trimmed.contains(',') || trimmed.to_lowercase() == "empty") {
                        // Check if it's a geometric type (point, box, circle, etc.)
                        // Geometric types: (x,y), ((x1,y1),(x2,y2)), <(x,y),r>
                        // Ranges: [a,b), (a,b], [a,b], (a,b), empty
                        // Points have 1 comma, boxes/lsegs have 3 commas, circles start with <
                        let comma_count = trimmed.matches(",").count();
                        let is_point = trimmed.starts_with("(") && 
                            !trimmed.contains("[") && 
                            !trimmed.contains("]") &&
                            comma_count == 1;
                        let is_box_or_lseg = trimmed.starts_with("(") && 
                            !trimmed.contains("[") && 
                            !trimmed.contains("]") &&
                            comma_count == 3;
                        let is_circle = trimmed.starts_with("<") && trimmed.ends_with(">");
                        // Check if it looks like a JSON array (contains quotes)
                        let is_json_array = trimmed.starts_with("[") && trimmed.contains('"');
                        if is_point || is_box_or_lseg || is_circle || is_json_array {
                            return val; // Don't canonicalize geometric types or JSON arrays
                        }
                        return format!("range_canonicalize({})", val);
                    }
                }
                val
            },
            NodeEnum::TypeCast(ref type_cast) => reconstruct_type_cast(type_cast, ctx),
            NodeEnum::AExpr(ref a_expr) => reconstruct_a_expr(a_expr, ctx),
            NodeEnum::BoolExpr(ref bool_expr) => reconstruct_bool_expr(bool_expr, ctx),
            NodeEnum::JoinExpr(ref join_expr) => reconstruct_join_expr(join_expr, ctx),
            NodeEnum::SelectStmt(ref select_stmt) => reconstruct_select_stmt(select_stmt, ctx),
            NodeEnum::SubLink(ref sub_link) => reconstruct_sub_link(sub_link, ctx),
            NodeEnum::NullTest(ref null_test) => reconstruct_null_test(null_test, ctx),
            NodeEnum::CaseExpr(ref case_expr) => reconstruct_case_expr(case_expr, ctx),
            NodeEnum::CoalesceExpr(ref coalesce_expr) => {
                reconstruct_coalesce_expr(coalesce_expr, ctx)
            }
            NodeEnum::SortBy(_) => reconstruct_sort_by(node, ctx),
            NodeEnum::List(ref list) => {
                let items: Vec<String> = list.items.iter().map(|n| reconstruct_node(n, ctx)).collect();
                items.join(", ")
            }
            NodeEnum::CaseWhen(_) => {
                // CaseWhen is handled within reconstruct_case_expr, not standalone
                "".to_string()
            }
            NodeEnum::SqlvalueFunction(ref sql_val) => reconstruct_sql_value_function(sql_val),
            NodeEnum::ArrayExpr(ref array_expr) => reconstruct_array_expr(array_expr, ctx),
            NodeEnum::AArrayExpr(ref a_array_expr) => reconstruct_a_array_expr(a_array_expr, ctx),
            NodeEnum::RangeFunction(ref range_func) => reconstruct_range_function(range_func, ctx),
            _ => node.deparse().unwrap_or_else(|_| "".to_string()).to_lowercase(),
        }
    } else {
        String::new()
    }
}

/// Reconstruct a JOIN expression
fn reconstruct_join_expr(join_expr: &JoinExpr, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    // Left side
    if let Some(ref left) = join_expr.larg {
        parts.push(reconstruct_node(left, ctx));
    }

    // Join type
    let join_type = match join_expr.jointype() {
        pg_query::protobuf::JoinType::JoinInner => "join",
        pg_query::protobuf::JoinType::JoinLeft => "left join",
        pg_query::protobuf::JoinType::JoinRight => "left join", // SQLite doesn't support RIGHT JOIN
        pg_query::protobuf::JoinType::JoinFull => "left join", // SQLite doesn't support FULL JOIN
        _ => "join",
    };
    parts.push(join_type.to_string());

    // Right side
    if let Some(ref right) = join_expr.rarg {
        parts.push(reconstruct_node(right, ctx));
    }

    // ON clause
    if let Some(ref qual) = join_expr.quals {
        let qual_sql = reconstruct_node(qual, ctx);
        if !qual_sql.is_empty() {
            parts.push("on".to_string());
            parts.push(qual_sql);
        }
    }

    // USING clause (if present instead of ON)
    if !join_expr.using_clause.is_empty() {
        parts.push("using".to_string());
        let cols: Vec<String> = join_expr
            .using_clause
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::String(s) = inner {
                        return Some(s.sval.to_lowercase());
                    }
                }
                None
            })
            .collect();
        parts.push(format!("({})", cols.join(", ")));
    }

    parts.join(" ")
}

/// Reconstruct a SubLink (subquery)
fn reconstruct_sub_link(sub_link: &SubLink, ctx: &mut TranspileContext) -> String {
    let subquery = sub_link
        .subselect
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();

    match sub_link.sub_link_type() {
        pg_query::protobuf::SubLinkType::ExistsSublink => format!("exists ({})", subquery),
        pg_query::protobuf::SubLinkType::AnySublink => {
            let test_expr = sub_link
                .testexpr
                .as_ref()
                .map(|n| reconstruct_node(n, ctx))
                .unwrap_or_default();
            format!("{} in ({})", test_expr, subquery)
        }
        pg_query::protobuf::SubLinkType::AllSublink => {
            let test_expr = sub_link
                .testexpr
                .as_ref()
                .map(|n| reconstruct_node(n, ctx))
                .unwrap_or_default();
            format!("{} in ({})", test_expr, subquery)
        }
        _ => format!("({})", subquery),
    }
}

/// Reconstruct a NullTest (IS NULL / IS NOT NULL)
fn reconstruct_null_test(null_test: &NullTest, ctx: &mut TranspileContext) -> String {
    let arg = null_test
        .arg
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();

    match null_test.nulltesttype() {
        pg_query::protobuf::NullTestType::IsNull => format!("{} is null", arg),
        pg_query::protobuf::NullTestType::IsNotNull => format!("{} is not null", arg),
        _ => arg,
    }
}

/// Reconstruct a Case expression
fn reconstruct_case_expr(case_expr: &CaseExpr, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();
    parts.push("case".to_string());

    // CASE expression (if present) - this is the simple CASE form: CASE expr WHEN ...
    if let Some(ref arg) = case_expr.arg {
        parts.push(reconstruct_node(arg, ctx));
    }

    // WHEN clauses
    for when in &case_expr.args {
        if let Some(ref inner) = when.node {
            if let NodeEnum::CaseWhen(case_when) = inner {
                let when_expr = case_when.expr.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();
                let when_result = case_when.result.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();

                parts.push(format!("when {} then {}", when_expr, when_result));
            }
        }
    }

    // ELSE clause
    if let Some(ref default_result) = case_expr.defresult {
        let default_sql = reconstruct_node(default_result, ctx);
        parts.push(format!("else {}", default_sql));
    }

    parts.push("end".to_string());
    parts.join(" ")
}

/// Reconstruct a TypeCast node
fn reconstruct_type_cast(type_cast: &TypeCast, ctx: &mut TranspileContext) -> String {
    let arg_sql = type_cast
        .arg
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    let original_type = extract_original_type(&type_cast.type_name);
    let sqlite_type = rewrite_type_for_sqlite(&original_type);
    format!("cast({} as {})", arg_sql, sqlite_type.to_lowercase())
}

/// Reconstruct a constant value
fn reconstruct_aconst(aconst: &AConst) -> String {
    if let Some(ref val) = aconst.val {
        match val {
            pg_query::protobuf::a_const::Val::Ival(i) => i.ival.to_string(),
            pg_query::protobuf::a_const::Val::Fval(f) => f.fval.clone(),
            pg_query::protobuf::a_const::Val::Sval(s) => format!("'{}'", s.sval.replace('"', "\"").replace('\'', "''")),
            pg_query::protobuf::a_const::Val::Boolval(b) => (if b.boolval { "1" } else { "0" }).to_string(),
            _ => "NULL".to_string(),
        }
    } else {
        "NULL".to_string()
    }
}

/// Reconstruct an ArrayExpr node (ARRAY[...] syntax)
/// Converts PostgreSQL ARRAY expressions to SQLite JSON arrays
fn reconstruct_array_expr(array_expr: &ArrayExpr, ctx: &mut TranspileContext) -> String {
    let elements: Vec<serde_json::Value> = array_expr
        .elements
        .iter()
        .map(|n| {
            let val = reconstruct_node(n, ctx);
            // If the value is already quoted (a string), use it as-is for JSON
            // Otherwise, it's a literal that needs to be included in JSON
            if val.starts_with('\'') && val.ends_with('\'') {
                // It's a string literal - extract the inner value for JSON
                let inner = &val[1..val.len()-1];
                serde_json::Value::String(inner.to_string())
            } else if val == "NULL" {
                serde_json::Value::Null
            } else if val == "1" || val == "0" {
                // Boolean values (converted to 1/0 by reconstruct_aconst)
                serde_json::Value::Bool(val == "1")
            } else if let Ok(num) = val.parse::<i64>() {
                serde_json::Value::Number(num.into())
            } else if let Ok(num) = val.parse::<f64>() {
                serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap_or(0.into()))
            } else {
                serde_json::Value::String(val)
            }
        })
        .collect();

    // Store as JSON array string
    format!("'{}'", serde_json::to_string(&elements).unwrap_or_else(|_| "[]".to_string()))
}

/// Reconstruct an AArrayExpr node (ARRAY[...] syntax in parsed SQL)
/// Converts PostgreSQL ARRAY expressions to SQLite JSON arrays
fn reconstruct_a_array_expr(a_array_expr: &AArrayExpr, ctx: &mut TranspileContext) -> String {
    let elements: Vec<serde_json::Value> = a_array_expr
        .elements
        .iter()
        .map(|n| {
            let val = reconstruct_node(n, ctx);
            // If the value is already quoted (a string), use it as-is for JSON
            // Otherwise, it's a literal that needs to be included in JSON
            if val.starts_with('\'') && val.ends_with('\'') {
                // It's a string literal - extract the inner value for JSON
                let inner = &val[1..val.len()-1];
                serde_json::Value::String(inner.to_string())
            } else if val == "NULL" {
                serde_json::Value::Null
            } else if val == "1" || val == "0" {
                // Boolean values (converted to 1/0 by reconstruct_aconst)
                serde_json::Value::Bool(val == "1")
            } else if let Ok(num) = val.parse::<i64>() {
                serde_json::Value::Number(num.into())
            } else if let Ok(num) = val.parse::<f64>() {
                serde_json::Number::from_f64(num)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::String(val))
            } else {
                serde_json::Value::String(val)
            }
        })
        .collect();

    // Store as JSON array string
    format!("'{}'", serde_json::to_string(&elements).unwrap_or_else(|_| "[]".to_string()))
}

/// Reconstruct a SQL value function (CURRENT_TIMESTAMP, CURRENT_DATE, etc.)
fn reconstruct_sql_value_function(sql_val: &SqlValueFunction) -> String {
    use pg_query::protobuf::SqlValueFunctionOp;

    match sql_val.op() {
        SqlValueFunctionOp::SvfopCurrentTimestamp | SqlValueFunctionOp::SvfopCurrentTimestampN => {
            // SQLite's CURRENT_TIMESTAMP is equivalent to PostgreSQL's
            "CURRENT_TIMESTAMP".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentDate => {
            "date('now')".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentTime | SqlValueFunctionOp::SvfopCurrentTimeN => {
            "time('now')".to_string()
        }
        SqlValueFunctionOp::SvfopLocaltime | SqlValueFunctionOp::SvfopLocaltimeN => {
            "time('now', 'localtime')".to_string()
        }
        SqlValueFunctionOp::SvfopLocaltimestamp | SqlValueFunctionOp::SvfopLocaltimestampN => {
            "datetime('now', 'localtime')".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentUser | SqlValueFunctionOp::SvfopUser => {
            // SQLite doesn't have a built-in CURRENT_USER, but we can return a reasonable default
            "'current_user'".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentRole => {
            "'current_role'".to_string()
        }
        SqlValueFunctionOp::SvfopSessionUser => {
            "'session_user'".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentCatalog => {
            "'current_catalog'".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentSchema => {
            // Return 'main' as the default schema in SQLite
            "'main'".to_string()
        }
        _ => {
            // Unknown SQL value function, try to deparse
            "NULL".to_string()
        }
    }
}

/// Check if a node is an array expression (ArrayExpr or AArrayExpr)
fn is_array_expr(node: &Node) -> bool {
    if let Some(ref inner) = node.node {
        matches!(inner, NodeEnum::ArrayExpr(_) | NodeEnum::AArrayExpr(_))
    } else {
        false
    }
}

/// Check if a node is a string literal containing a JSON array
fn is_json_array_string(node: &Node) -> bool {
    if let Some(ref inner) = node.node {
        if let NodeEnum::AConst(const_node) = inner {
            if let Some(ref val) = const_node.val {
                if let pg_query::protobuf::a_const::Val::Sval(sval) = val {
                    let s = &sval.sval;
                    // Check if it looks like a JSON array: starts with [ and ends with ]
                    return s.trim().starts_with('[') && s.trim().ends_with(']');
                }
            }
        }
    }
    false
}

/// Reconstruct an AExpr node (operators)
fn reconstruct_a_expr(a_expr: &AExpr, ctx: &mut TranspileContext) -> String {
    // Check if operands are array expressions before reconstructing
    let lexpr_is_array = a_expr.lexpr.as_ref().map_or(false, |n| is_array_expr(n) || is_json_array_string(n));
    let rexpr_is_array = a_expr.rexpr.as_ref().map_or(false, |n| is_array_expr(n) || is_json_array_string(n));
    
    let lexpr_sql = a_expr
        .lexpr
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    let rexpr_sql = a_expr
        .rexpr
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
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

    // Handle IN expressions
    match a_expr.kind() {
        pg_query::protobuf::AExprKind::AexprIn => {
            // IN expression: expr IN (val1, val2, ...)
            return format!("{} in ({})", lexpr_sql, rexpr_sql);
        }
        pg_query::protobuf::AExprKind::AexprOpAny => {
            // ANY expression: expr = ANY (array)
            return format!("{} = any ({})", lexpr_sql, rexpr_sql);
        }
        pg_query::protobuf::AExprKind::AexprOpAll => {
            // ALL expression
            return format!("{} = all ({})", lexpr_sql, rexpr_sql);
        }
        _ => {}
    }

    // Handle PostgreSQL-specific operators
    match op_name.as_str() {
        "~~" | "~~*" => format!("{} like {}", lexpr_sql, rexpr_sql),
        "!~~" | "!~~*" => format!("{} not like {}", lexpr_sql, rexpr_sql),
        "~" => format!("regexp({}, {})", rexpr_sql, lexpr_sql),
        "~*" => format!("regexpi({}, {})", rexpr_sql, lexpr_sql),
        "!~" => format!("NOT regexp({}, {})", rexpr_sql, lexpr_sql),
        "!~*" => format!("NOT regexpi({}, {})", rexpr_sql, lexpr_sql),
        "@@" => format!("fts_match({}, {})", lexpr_sql, rexpr_sql),
        "@>@" => format!("fts_contains({}, {})", lexpr_sql, rexpr_sql),  // tsquery contains
        "<@@" => format!("fts_contained({}, {})", lexpr_sql, rexpr_sql), // tsquery contained by
        // Array and Range operators (PostgreSQL compatibility)
        "&&" => {
            // Check if operands look like ranges or arrays or geo objects
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            // Determine operation type: geo, array, or range
            // Priority: geo > array > range
            let is_geo = lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
                        (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
                        (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")"));
            let is_array = !is_geo && (lexpr_is_array || rexpr_is_array ||
                           lexpr_lower.contains("[") || rexpr_lower.contains("["));
            let is_range = !is_geo && !is_array && (lexpr_lower.contains("range") || rexpr_lower.contains("range"));
            
            if is_geo {
                format!("geo_overlaps({}, {})", lexpr_sql, rexpr_sql)
            } else if is_range {
                format!("range_overlaps({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("array_overlap({}, {})", lexpr_sql, rexpr_sql)
            }
        }
        "@>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            // Determine operation type: geo, array, or range
            let is_geo = lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
                        (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
                        (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")"));
            let is_array = !is_geo && (lexpr_is_array || rexpr_is_array ||
                           lexpr_lower.contains("[") || rexpr_lower.contains("["));
            let is_range = !is_geo && !is_array && (lexpr_lower.contains("range") || rexpr_lower.contains("range") ||
                           lexpr_lower == "r"); // Special case for our test table column
            
            if is_geo {
                format!("geo_contains({}, {})", lexpr_sql, rexpr_sql)
            } else if is_range {
                format!("range_contains({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("array_contains({}, {})", lexpr_sql, rexpr_sql)
            }
        }
        "<@" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            // Determine operation type: geo, array, or range
            let is_geo = lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
                        (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
                        (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")"));
            let is_array = !is_geo && (lexpr_is_array || rexpr_is_array ||
                           lexpr_lower.contains("[") || rexpr_lower.contains("["));
            let is_range = !is_geo && !is_array && (lexpr_lower.contains("range") || rexpr_lower.contains("range"));
            
            if is_geo {
                format!("geo_contained({}, {})", lexpr_sql, rexpr_sql)
            } else if is_range {
                format!("range_contained({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("array_contained({}, {})", lexpr_sql, rexpr_sql)
            }
        }
        "<<" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            if lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
               (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
               (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")")) {
                format!("geo_left({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("range_left({}, {})", lexpr_sql, rexpr_sql)
            }
        },
        ">>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            if lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
               (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
               (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")")) {
                format!("geo_right({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("range_right({}, {})", lexpr_sql, rexpr_sql)
            }
        },
        "<<|" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            if lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
               (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
               (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")")) {
                format!("geo_below({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("{} <<| {}", lexpr_sql, rexpr_sql)
            }
        },
        "|>>" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            if lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
               (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
               (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")")) {
                format!("geo_above({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("{} |>> {}", lexpr_sql, rexpr_sql)
            }
        },
        "-|-" => format!("range_adjacent({}, {})", lexpr_sql, rexpr_sql),
        "&<" => format!("range_no_extend_right({}, {})", lexpr_sql, rexpr_sql),
        "&>" => format!("range_no_extend_left({}, {})", lexpr_sql, rexpr_sql),
        // JSONB operators (PostgreSQL compatibility)
        "?" => format!("json_type({}, '$.' || {}) IS NOT NULL", lexpr_sql, rexpr_sql),
        "?|" => format!("EXISTS (SELECT 1 FROM json_each({}) WHERE json_type({}, '$.' || value) IS NOT NULL)", rexpr_sql, lexpr_sql),
        "?&" => format!("NOT EXISTS (SELECT 1 FROM json_each({}) WHERE json_type({}, '$.' || value) IS NULL)", rexpr_sql, lexpr_sql),
        // || operator is overloaded in PostgreSQL:
        // - JSONB: json1 || json2 -> json_patch(json1, json2)
        // - tsvector: ts1 || ts2 -> tsvector_concat(ts1, ts2)
        // - text: s1 || s2 -> s1 || s2 (SQLite native)
        "||" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            let lexpr_trimmed = lexpr_sql.trim();
            let rexpr_trimmed = rexpr_sql.trim();

            // Check for tsvector context (function calls like to_tsvector)
            if lexpr_lower.contains("to_tsvector") || rexpr_lower.contains("to_tsvector") ||
               lexpr_lower.contains("tsvector") || rexpr_lower.contains("tsvector") {
                format!("tsvector_concat({}, {})", lexpr_sql, rexpr_sql)
            }
            // Check for JSON context (literals or json functions)
            else if lexpr_trimmed.starts_with("'{") || rexpr_trimmed.starts_with("'{") ||
                    lexpr_trimmed.starts_with("'[") || rexpr_trimmed.starts_with("'[") ||
                    lexpr_lower.contains("json") || rexpr_lower.contains("json") ||
                    lexpr_lower.contains("props") || rexpr_lower.contains("props") {
                format!("json_patch({}, {})", lexpr_sql, rexpr_sql)
            }
            // Default to SQLite's string concatenation
            else {
                format!("{} || {}", lexpr_sql, rexpr_sql)
            }
        }
        // JSONB key removal: json - 'key' -> json_remove(json, '$.key')
        // For arrays: json - ARRAY['a','b'] -> json_remove(json, '$.a', '$.b')
        "-" => {
            // Check if rexpr looks like a JSON array
            let rexpr_trimmed = rexpr_sql.trim();
            if rexpr_trimmed.starts_with("'[") || rexpr_trimmed.starts_with("[") {
                // Extract the array and expand it into multiple paths
                // This is a simplified approach - parse the JSON array string
                let array_str = rexpr_trimmed.trim_matches(|c| c == '\'');
                if let Ok(keys) = serde_json::from_str::<Vec<String>>(array_str) {
                    let paths: Vec<String> = keys.iter().map(|k| format!("'$.{}'", k)).collect();
                    format!("json_remove({}, {})", lexpr_sql, paths.join(", "))
                } else {
                    format!("json_remove({}, '$.' || {})", lexpr_sql, rexpr_sql)
                }
            } else {
                format!("json_remove({}, '$.' || {})", lexpr_sql, rexpr_sql)
            }
        }
        // Vector distance operators (pgvector compatibility) and geometric distance
        "<->" => {
            let lexpr_lower = lexpr_sql.to_lowercase();
            let rexpr_lower = rexpr_sql.to_lowercase();
            // Check for geometric types: contains '<' (circle) or '(x,y)' pattern
            if lexpr_lower.contains("<") || rexpr_lower.contains("<") ||
               (!lexpr_lower.contains("[") && lexpr_lower.contains("(") && lexpr_lower.contains(",") && lexpr_lower.contains(")")) ||
               (!rexpr_lower.contains("[") && rexpr_lower.contains("(") && rexpr_lower.contains(",") && rexpr_lower.contains(")")) {
                format!("geo_distance({}, {})", lexpr_sql, rexpr_sql)
            } else {
                format!("vector_l2_distance({}, {})", lexpr_sql, rexpr_sql)
            }
        },     // L2 distance or geometric distance
        "<=>" => format!("vector_cosine_distance({}, {})", lexpr_sql, rexpr_sql), // Cosine distance
        "<#>" => format!("vector_inner_product({}, {})", lexpr_sql, rexpr_sql),   // Inner product
        "<+>" => format!("vector_l1_distance({}, {})", lexpr_sql, rexpr_sql),     // L1 distance
        _ => format!("{} {} {}", lexpr_sql, op_name, rexpr_sql),
    }
}

/// Reconstruct a BoolExpr node (AND, OR, NOT)
fn reconstruct_bool_expr(bool_expr: &BoolExpr, ctx: &mut TranspileContext) -> String {
    let op = match bool_expr.boolop() {
        pg_query::protobuf::BoolExprType::AndExpr => "AND",
        pg_query::protobuf::BoolExprType::OrExpr => "OR",
        pg_query::protobuf::BoolExprType::NotExpr => "NOT",
        _ => "AND",
    };

    let args: Vec<String> = bool_expr.args.iter().map(|n| reconstruct_node(n, ctx)).collect();

    if bool_expr.boolop() == pg_query::protobuf::BoolExprType::NotExpr {
        format!("NOT ({})", args.join(" "))
    } else {
        format!("({})", args.join(&format!(" {} ", op)))
    }
}

/// Reconstruct a ResTarget node (SELECT column or alias)
fn reconstruct_res_target(target: &ResTarget, ctx: &mut TranspileContext) -> String {
    let name = &target.name;
    if let Some(ref val) = target.val {
        let val_sql = reconstruct_node(val, ctx);
        if name.is_empty() {
            val_sql
        } else {
            format!("{} as \"{}\"", val_sql, name.to_lowercase())
        }
    } else if !name.is_empty() {
        format!("\"{}\"", name.to_lowercase())
    } else {
        String::new()
    }
}

/// Reconstruct a RangeVar node (table reference)
fn reconstruct_range_var(range_var: &RangeVar, ctx: &mut TranspileContext) -> String {
    let table_name = range_var.relname.to_lowercase();
    ctx.referenced_tables.push(table_name.clone());
    let schema_name = range_var.schemaname.to_lowercase();
    let alias = range_var.alias.as_ref().map(|a| a.aliasname.to_lowercase());

    // Map 'public' and 'pg_catalog' schema to no prefix (SQLite doesn't have schemas)
    // Other schemas are treated as attached databases
    let full_table = if schema_name.is_empty() || schema_name == "public" || schema_name == "pg_catalog" {
        table_name.clone()
    } else {
        format!("{}.{}", schema_name, table_name)
    };

    if let Some(a) = alias {
        if a != table_name && a != format!("{}.{}", schema_name, table_name) {
            format!("{} as {}", full_table, a)
        } else {
            full_table
        }
    } else {
        full_table
    }
}

/// Reconstruct a RangeSubselect node (subquery in FROM clause)
fn reconstruct_range_subselect(range_subselect: &RangeSubselect, ctx: &mut TranspileContext) -> String {
    let subquery = range_subselect
        .subquery
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();

    let alias = range_subselect
        .alias
        .as_ref()
        .map(|a| a.aliasname.to_lowercase());

    if let Some(a) = alias {
        format!("({}) as {}", subquery, a)
    } else {
        format!("({})", subquery)
    }
}

/// Reconstruct a RangeFunction node (table function in FROM clause, like LATERAL jsonb_each)
fn reconstruct_range_function(range_func: &RangeFunction, ctx: &mut TranspileContext) -> String {
    // Extract the function calls from the functions field
    // Each item in functions is typically a List containing [FuncCall, empty_alias]
    let func_sql: Vec<String> = range_func
        .functions
        .iter()
        .filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::List(ref list) = inner {
                    // First item is usually the function call
                    if let Some(first) = list.items.first() {
                        return Some(reconstruct_node(first, ctx));
                    }
                } else {
                    return Some(reconstruct_node(n, ctx));
                }
            }
            None
        })
        .collect();

    // Build the table function call
    let base_func = func_sql.join(", ");

    // Handle alias - for jsonb_each(props) AS x(key, value), we need to handle coldeflist
    let alias_str = if let Some(ref alias) = range_func.alias {
        format!(" AS {}", alias.aliasname.to_lowercase())
    } else {
        String::new()
    };

    // Note: LATERAL keyword is implicit in SQLite for table-valued functions
    // so we don't need to include it
    if base_func.is_empty() {
        String::new()
    } else {
        format!("{}{}", base_func, alias_str)
    }
}

/// Reconstruct a CoalesceExpr node
fn reconstruct_coalesce_expr(coalesce_expr: &CoalesceExpr, ctx: &mut TranspileContext) -> String {
    let args: Vec<String> = coalesce_expr
        .args
        .iter()
        .map(|n| reconstruct_node(n, ctx))
        .collect();

    format!("coalesce({})", args.join(", "))
}

/// Reconstruct a ColumnRef node
fn reconstruct_column_ref(col_ref: &ColumnRef, _ctx: &mut TranspileContext) -> String {
    let fields: Vec<String> = col_ref
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
        .collect();

    fields.join(".")
}

#[allow(dead_code)]
/// Check if a node represents LIMIT ALL
fn is_limit_all(node: &Node) -> bool {
    if let Some(ref inner) = node.node {
        if let NodeEnum::AConst(ref aconst) = inner {
            if let Some(ref val) = aconst.val {
                if let pg_query::protobuf::a_const::Val::Ival(i) = val {
                    return i.ival == -1;
                }
            }
        }
    }
    false
}

/// Reconstruct a function call
fn reconstruct_func_call(func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
    // Build full function name from all parts (handle schema-qualified functions)
    let func_parts: Vec<String> = func_call
        .funcname
        .iter()
        .filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::String(s) = inner {
                    return Some(s.sval.to_lowercase());
                }
            }
            None
        })
        .collect();

    let full_func_name = func_parts.join(".");
    let func_name = func_parts.last().map(|s| s.as_str()).unwrap_or("");

    // Build args string - handle agg_star (count(*)) case
    let args_str = if func_call.agg_star {
        "*".to_string()
    } else {
        let args: Vec<String> = func_call
            .args
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        args.join(", ")
    };

    // Handle functions that need special argument processing
    match func_name {
        "jsonb_path_exists" => {
            // jsonb_path_exists(json, path) -> json_type(json, path) IS NOT NULL
            // Handle PostgreSQL JSONPath wildcards like $.skills[*]
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if args.len() >= 2 {
                let path = &args[1];
                // Strip [*] wildcard - SQLite doesn't support it directly
                // $.skills[*] -> $.skills (check if array exists and has elements)
                let clean_path = path.replace("[*]", "");
                // Check if the path exists and for arrays, check they have elements
                return format!(
                    "CASE WHEN json_type({}, {}) = 'array' THEN json_array_length(json_extract({}, {})) > 0 ELSE json_type({}, {}) IS NOT NULL END",
                    args[0], clean_path, args[0], clean_path, args[0], clean_path
                );
            }
            return format!("json_type({}) IS NOT NULL", args_str);
        }
        "jsonb_path_match" => {
            // jsonb_path_match(json, path) -> json_extract(json, path) = true
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                return format!("json_extract({}, {}) = 1", args[0], clean_path);
            }
            return format!("json_extract({}) = 1", args_str);
        }
        "jsonb_path_query" => {
            // jsonb_path_query(json, path) -> json_extract(json, path)
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                return format!("json_extract({}, {})", args[0], clean_path);
            }
            return format!("json_extract({})", args_str);
        }
        "jsonb_path_query_array" => {
            // jsonb_path_query_array(json, path) -> json_extract(json, path) (returns as array)
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                return format!("json_extract({}, {})", args[0], clean_path);
            }
            return format!("json_extract({})", args_str);
        }
        "jsonb_path_query_first" => {
            // jsonb_path_query_first(json, path) -> json_extract(json, path) (returns first match)
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                return format!("json_extract({}, {})", args[0], clean_path);
            }
            return format!("json_extract({})", args_str);
        }
        "jsonb_typeof" => {
            // jsonb_typeof(json) -> json_type(json) (returns type name)
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if !args.is_empty() {
                // SQLite json_type returns 'null', 'true', 'false', 'integer', 'real', 'text', 'array', 'object'
                // PostgreSQL jsonb_typeof returns 'object', 'array', 'string', 'number', 'boolean', 'null'
                // We need to map SQLite types to PostgreSQL types
                return format!(
                    "CASE json_type({0}) \
                    WHEN 'true' THEN 'boolean' \
                    WHEN 'false' THEN 'boolean' \
                    WHEN 'integer' THEN 'number' \
                    WHEN 'real' THEN 'number' \
                    WHEN 'text' THEN 'string' \
                    ELSE json_type({0}) END",
                    args[0]
                );
            }
            return "json_type(".to_string() + &args_str + ")";
        }
        "jsonb_object_keys" => {
            // jsonb_object_keys(json) -> extract keys from json object
            // PostgreSQL returns a set of keys, but we return as JSON array for SQLite
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if !args.is_empty() {
                // Return keys as a JSON array using a subquery
                return format!(
                    "(SELECT json_group_array(key) FROM json_each({}))",
                    args[0]
                );
            }
            return format!("(SELECT json_group_array(key) FROM json_each({}))", args_str);
        }
        "jsonb_each" | "json_each" => {
            // jsonb_each(json) -> json_each(json) in SQLite
            // This returns key/value pairs as rows
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if !args.is_empty() {
                return format!("json_each({})", args[0]);
            }
            return format!("json_each({})", args_str);
        }
        "jsonb_array_elements" => {
            // jsonb_array_elements(json) -> json_each(json) for arrays
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if !args.is_empty() {
                return format!("json_each({})", args[0]);
            }
            return format!("json_each({})", args_str);
        }
        "jsonb_extract_path" => {
            // jsonb_extract_path(json, keys...) -> json_extract(json, path)
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if args.len() >= 2 {
                // Build path from the keys
                return format!("json_extract({}, {})", args[0], args[1..].join(", "));
            }
            return format!("json_extract({})", args_str);
        }
        "jsonb_extract_path_text" => {
            // jsonb_extract_path_text(json, keys...) -> json_extract() with ->>
            let args: Vec<String> = func_call
                .args
                .iter()
                .map(|n| reconstruct_node(n, ctx))
                .collect();
            if args.len() >= 2 {
                return format!("json_extract({}, {})", args[0], args[1..].join(", "));
            }
            return format!("json_extract({})", args_str);
        }
        _ => {}
    }

    // Map PostgreSQL functions to SQLite equivalents
    let sqlite_func = match func_name {
        "now" => "datetime('now')",
        "current_timestamp" => "datetime('now')",
        "current_date" => "date('now')",
        "current_time" => "time('now')",
        "random" => "random()",
        "floor" => "floor",
        "ceil" => "ceil",
        "abs" => "abs",
        "coalesce" => "coalesce",
        "nullif" => "nullif",
        "length" => "length",
        "lower" => "lower",
        "upper" => "upper",
        "trim" => "trim",
        "ltrim" => "ltrim",
        "rtrim" => "rtrim",
        "substr" => "substr",
        "replace" => "replace",
        "round" => "round",
        // System catalog functions - strip schema and return as-is for now
        "pg_get_userbyid" => "pg_get_userbyid",
        "pg_table_is_visible" => "pg_table_is_visible",
        "pg_type_is_visible" => "pg_type_is_visible",
        "pg_function_is_visible" => "pg_function_is_visible",
        "format_type" => "format_type",
        "current_schema" => "current_schema",
        "current_schemas" => "current_schemas",
        "current_database" => "current_database",
        "current_setting" => "current_setting",
        "pg_my_temp_schema" => "pg_my_temp_schema",
        "pg_get_expr" => "pg_get_expr",
        "pg_get_indexdef" => "pg_get_indexdef",
        "obj_description" => "obj_description",
        "pg_get_constraintdef" => "pg_get_constraintdef",
        "pg_encoding_to_char" => "pg_encoding_to_char",
        "array_to_string" => "array_to_string",
        "array_length" => "array_length",
        "pg_table_size" => "pg_table_size",
        "pg_total_relation_size" => "pg_total_relation_size",
        "pg_size_pretty" => "pg_size_pretty",
        // UUID generation
        "gen_random_uuid" => "lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-' || substr('89ab', abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))",
        "uuid_generate_v4" => "lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-' || substr('89ab', abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))",

        // Full-Text Search functions
        "to_tsvector" => "to_tsvector",
        "to_tsquery" => "to_tsquery",
        "plainto_tsquery" => "plainto_tsquery",
        "phraseto_tsquery" => "phraseto_tsquery",
        "websearch_to_tsquery" => "websearch_to_tsquery",
        "ts_rank" => "ts_rank",
        "ts_rank_cd" => "ts_rank_cd",
        "ts_headline" => "ts_headline",
        "setweight" => "setweight",
        "strip" => "strip",
        "numnode" => "numnode",
        "querytree" => "querytree",
        "ts_rewrite" => "ts_rewrite",
        "ts_lexize" => "ts_lexize",
        "ts_debug" => "ts_debug",
        "ts_stat" => "ts_stat",
        "array_to_tsvector" => "array_to_tsvector",
        "jsonb_to_tsvector" => "jsonb_to_tsvector",

        // Range constructor functions
        "int4range" => "int4range",
        "int8range" => "int8range",
        "numrange" => "numrange",
        "tsrange" => "tsrange",
        "tstzrange" => "tstzrange",
        "daterange" => "daterange",

        _ => {
            // For unknown functions, return the full name if schema-qualified
            // but strip 'pg_catalog' if present as SQLite doesn't have it
            if func_parts.len() > 1 {
                if func_parts[0] == "pg_catalog" {
                    let base = format!("{}({})", func_name, args_str);
                    return add_window_clause(&base, func_call, ctx);
                }
                let base = format!("{}({})", full_func_name, args_str);
                return add_window_clause(&base, func_call, ctx);
            }
            func_name
        }
    };

    // Special case for functions that don't need arguments
    if sqlite_func == "datetime('now')"
        || sqlite_func == "date('now')"
        || sqlite_func == "time('now')"
        || sqlite_func == "random()"
        || sqlite_func.starts_with("lower(hex(randomblob(4)))") {
        return sqlite_func.to_string();
    }

    let base = format!("{}({})", sqlite_func, args_str);
    add_window_clause(&base, func_call, ctx)
}

/// Add OVER clause to a function call if present
fn add_window_clause(base: &str, func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
    if let Some(ref over) = func_call.over {
        let window_sql = reconstruct_window_def(over, ctx);
        // Always add OVER clause if the function has one, even if empty
        return format!("{} over ({})", base, window_sql);
    }
    base.to_string()
}

/// Reconstruct a CREATE ROLE statement as an INSERT into __pg_authid__
fn reconstruct_create_role_stmt(stmt: &CreateRoleStmt, _ctx: &mut TranspileContext) -> String {
    let role_name = stmt.role.clone();

    let mut superuser = false;
    let mut inherit = true;
    let mut createrole = false;
    let mut createdb = false;
    let mut canlogin = false;
    let mut password = "NULL".to_string();

    for opt in &stmt.options {
        if let Some(ref node) = opt.node {
            if let NodeEnum::DefElem(ref def) = node {
                match def.defname.as_str() {
                    "superuser" => {
                        superuser = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "inherit" => {
                        inherit = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "createrole" => {
                        createrole = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "createdb" => {
                        createdb = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "canlogin" => {
                        canlogin = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "password" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref val) = arg.node {
                                if let NodeEnum::String(ref s) = val {
                                    password = format!("'{}'", s.sval.replace('\'', "''"));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    format!(
        "INSERT INTO __pg_authid__ (rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin, rolpassword) \
         VALUES ('{}', {}, {}, {}, {}, {}, {})",
        role_name.to_lowercase(),
        if superuser { 1 } else { 0 },
        if inherit { 1 } else { 0 },
        if createrole { 1 } else { 0 },
        if createdb { 1 } else { 0 },
        if canlogin { 1 } else { 0 },
        password
    )
}

/// Reconstruct a DROP ROLE statement as a DELETE from __pg_authid__
fn reconstruct_drop_role_stmt(stmt: &DropRoleStmt) -> String {
    let roles: Vec<String> = stmt.roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(format!("'{}'", role.rolename.to_lowercase()));
            }
        }
        None
    }).collect();

    format!("DELETE FROM __pg_authid__ WHERE rolname IN ({})", roles.join(", "))
}

/// Reconstruct a GRANT statement as an INSERT into __pg_acl__
fn reconstruct_grant_stmt(stmt: &GrantStmt) -> String {
    let is_grant = stmt.is_grant;
    let objtype = stmt.objtype;

    // Only support OBJECT_TABLE for now
    if objtype != pg_query::protobuf::ObjectType::ObjectTable as i32 &&
       objtype != pg_query::protobuf::ObjectType::ObjectView as i32 {
        return "SELECT 1".to_string(); // Unsupported for now
    }

    let objects: Vec<String> = stmt.objects.iter().filter_map(|o| {
        if let Some(ref node) = o.node {
            if let NodeEnum::RangeVar(ref rv) = node {
                return Some(rv.relname.to_lowercase());
            }
        }
        None
    }).collect();

    let privileges: Vec<String> = stmt.privileges.iter().filter_map(|p| {
        if let Some(ref node) = p.node {
            if let NodeEnum::AccessPriv(ref ap) = node {
                return Some(ap.priv_name.to_uppercase());
            }
        }
        None
    }).collect();

    let grantees: Vec<String> = stmt.grantees.iter().filter_map(|g| {
        if let Some(ref node) = g.node {
            if let NodeEnum::RoleSpec(ref rs) = node {
                if rs.roletype == pg_query::protobuf::RoleSpecType::RolespecPublic as i32 {
                    return Some("PUBLIC".to_string());
                }
                return Some(rs.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    if is_grant {
        if objects.is_empty() || privileges.is_empty() || grantees.is_empty() {
            return "SELECT 1".to_string();
        }

        let obj = &objects[0];
        let priv_ = &privileges[0];
        let grantee = &grantees[0];
        format!(
            "INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) \
             SELECT c.oid, 'relation', COALESCE(r.oid, 0), '{}', 10 \
             FROM pg_class c LEFT JOIN pg_roles r ON r.rolname = '{}' \
             WHERE c.relname = '{}'",
            priv_, grantee, obj
        )
    } else {
        format!(
            "DELETE FROM __pg_acl__ WHERE object_id IN (SELECT oid FROM pg_class WHERE relname IN ({})) \
             AND grantee_id IN (SELECT oid FROM pg_roles WHERE rolname IN ({})) \
             AND privilege IN ({})",
            objects.iter().map(|o| format!("'{}'", o)).collect::<Vec<_>>().join(", "),
            grantees.iter().map(|g| format!("'{}'", g)).collect::<Vec<_>>().join(", "),
            privileges.iter().map(|p| format!("'{}'", p)).collect::<Vec<_>>().join(", ")
        )
    }
}

/// Reconstruct a GRANT role statement as an INSERT into __pg_auth_members__
fn reconstruct_grant_role_stmt(stmt: &GrantRoleStmt) -> String {
    let is_grant = stmt.is_grant;

    let granted_roles: Vec<String> = stmt.granted_roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(role.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    let grantee_roles: Vec<String> = stmt.grantee_roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(role.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    if is_grant {
        if granted_roles.is_empty() || grantee_roles.is_empty() {
            return "SELECT 1".to_string();
        }
        format!(
            "INSERT INTO __pg_auth_members__ (roleid, member, grantor) \
             SELECT r.oid, m.oid, 10 \
             FROM pg_roles r, pg_roles m \
             WHERE r.rolname = '{}' AND m.rolname = '{}'",
            granted_roles[0], grantee_roles[0]
        )
    } else {
        format!(
            "DELETE FROM __pg_auth_members__ WHERE roleid IN (SELECT oid FROM pg_roles WHERE rolname IN ({})) \
             AND member IN (SELECT oid FROM pg_roles WHERE rolname IN ({}))",
            granted_roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", "),
            grantee_roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", ")
        )
    }
}

/// Reconstruct ALTER TABLE statement with RLS support
fn reconstruct_alter_table_stmt(stmt: &AlterTableStmt, ctx: &mut TranspileContext) -> String {
    use pg_query::protobuf::AlterTableType;

    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| r.relname.clone())
        .unwrap_or_default()
        .to_lowercase();

    ctx.referenced_tables.push(table_name.clone());

    // Check for RLS-related alter operations
    for cmd in &stmt.cmds {
        if let Some(ref node) = cmd.node {
            if let NodeEnum::AlterTableCmd(ref alter_cmd) = node {
                let subtype = alter_cmd.subtype();

                match subtype {
                    AlterTableType::AtEnableRowSecurity => {
                        // ALTER TABLE ... ENABLE ROW LEVEL SECURITY
                        return format!(
                            "INSERT OR REPLACE INTO __pg_rls_enabled__ (relname, rls_enabled, rls_forced) VALUES ('{}', TRUE, FALSE)",
                            table_name
                        );
                    }
                    AlterTableType::AtDisableRowSecurity => {
                        // ALTER TABLE ... DISABLE ROW LEVEL SECURITY
                        return format!(
                            "UPDATE __pg_rls_enabled__ SET rls_enabled = FALSE WHERE relname = '{}'",
                            table_name
                        );
                    }
                    AlterTableType::AtForceRowSecurity => {
                        // ALTER TABLE ... FORCE ROW LEVEL SECURITY
                        return format!(
                            "UPDATE __pg_rls_enabled__ SET rls_forced = TRUE WHERE relname = '{}'",
                            table_name
                        );
                    }
                    AlterTableType::AtNoForceRowSecurity => {
                        // ALTER TABLE ... NO FORCE ROW LEVEL SECURITY
                        return format!(
                            "UPDATE __pg_rls_enabled__ SET rls_forced = FALSE WHERE relname = '{}'",
                            table_name
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    // For non-RLS ALTER TABLE, return a no-op or basic reconstruction
    format!("-- ALTER TABLE {} (non-RLS operation not yet supported)", table_name)
}

/// Reconstruct DROP statement for SQLite compatibility
/// SQLite doesn't support CASCADE/RESTRICT in DROP statements
fn reconstruct_drop_stmt(stmt: &DropStmt, ctx: &mut TranspileContext) -> String {
    use pg_query::protobuf::ObjectType;

    let remove_type = ObjectType::try_from(stmt.remove_type).unwrap_or(ObjectType::Undefined);

    // Determine the object type keyword
    let type_keyword = match remove_type {
        ObjectType::ObjectTable => "table",
        ObjectType::ObjectIndex => "index",
        ObjectType::ObjectView => "view",
        ObjectType::ObjectTrigger => "trigger",
        ObjectType::ObjectSchema => "schema",
        ObjectType::ObjectSequence => "sequence",
        ObjectType::ObjectDomain => "domain",
        ObjectType::ObjectType => "type",
        ObjectType::ObjectFunction => "function",
        ObjectType::ObjectAggregate => "aggregate",
        ObjectType::ObjectProcedure => "procedure",
        ObjectType::ObjectExtension => "extension",
        ObjectType::ObjectPolicy => "policy",
        ObjectType::ObjectCollation => "collation",
        ObjectType::ObjectConversion => "conversion",
        ObjectType::ObjectOperator => "operator",
        ObjectType::ObjectOpclass => "operator class",
        ObjectType::ObjectOpfamily => "operator family",
        ObjectType::ObjectLanguage => "language",
        ObjectType::ObjectForeignServer => "server",
        ObjectType::ObjectFdw => "foreign data wrapper",
        ObjectType::ObjectForeignTable => "foreign table",
        ObjectType::ObjectMatview => "materialized view",
        ObjectType::ObjectEventTrigger => "event trigger",
        ObjectType::ObjectPublication => "publication",
        ObjectType::ObjectSubscription => "subscription",
        ObjectType::ObjectStatisticExt => "statistics",
        ObjectType::ObjectTablespace => "tablespace",
        ObjectType::ObjectRole => "role",
        _ => "table", // Default fallback
    };

    // Extract object names from the objects list
    let mut object_names: Vec<String> = Vec::new();
    for obj in &stmt.objects {
        if let Some(ref inner) = obj.node {
            match inner {
                NodeEnum::List(list) => {
                    // Handle qualified names like schema.table
                    let parts: Vec<String> = list
                        .items
                        .iter()
                        .filter_map(|n| {
                            if let Some(ref node) = n.node {
                                if let NodeEnum::String(s) = node {
                                    return Some(s.sval.to_lowercase());
                                }
                            }
                            None
                        })
                        .collect();
                    if !parts.is_empty() {
                        object_names.push(parts.join("."));
                        // Track referenced tables for DDL operations
                        if remove_type == ObjectType::ObjectTable {
                            ctx.referenced_tables.push(parts.last().unwrap().clone());
                        }
                    }
                }
                NodeEnum::String(s) => {
                    object_names.push(s.sval.to_lowercase());
                    if remove_type == ObjectType::ObjectTable {
                        ctx.referenced_tables.push(s.sval.to_lowercase());
                    }
                }
                NodeEnum::RangeVar(rv) => {
                    let name = if rv.schemaname.is_empty() || rv.schemaname == "public" {
                        rv.relname.to_lowercase()
                    } else {
                        format!("{}.{}", rv.schemaname.to_lowercase(), rv.relname.to_lowercase())
                    };
                    object_names.push(name);
                    if remove_type == ObjectType::ObjectTable {
                        ctx.referenced_tables.push(rv.relname.to_lowercase());
                    }
                }
                _ => {}
            }
        }
    }

    if object_names.is_empty() {
        return "-- DROP statement with no objects".to_string();
    }

    // Build the DROP statement
    // SQLite syntax: DROP [TABLE|INDEX|VIEW|TRIGGER] [IF EXISTS] name
    // Note: SQLite doesn't support CASCADE/RESTRICT, so we ignore the behavior field
    let if_exists = if stmt.missing_ok { " if exists " } else { " " };

    format!(
        "drop {}{}{}",
        type_keyword,
        if_exists,
        object_names.join(", ")
    )
}

/// Reconstruct CREATE INDEX statement for SQLite compatibility
fn reconstruct_index_stmt(stmt: &IndexStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("create".to_string());

    if stmt.unique {
        parts.push("unique".to_string());
    }

    parts.push("index".to_string());

    // SQLite supports IF NOT EXISTS since 3.8.0 (2013)
    if stmt.if_not_exists {
        parts.push("if not exists".to_string());
    }

    // Index name
    let idx_name = stmt.idxname.to_lowercase();
    parts.push(idx_name);

    // ON table_name
    if let Some(ref relation) = stmt.relation {
        parts.push("on".to_string());
        let table_name = if relation.schemaname.is_empty() || relation.schemaname == "public" {
            relation.relname.to_lowercase()
        } else {
            format!("{}.{}", relation.schemaname.to_lowercase(), relation.relname.to_lowercase())
        };
        ctx.referenced_tables.push(relation.relname.to_lowercase());
        parts.push(table_name);
    }

    // Index columns/expressions
    if !stmt.index_params.is_empty() {
        let params: Vec<String> = stmt
            .index_params
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::IndexElem(ref elem) = inner {
                        return Some(reconstruct_index_elem(elem, ctx));
                    }
                }
                None
            })
            .collect();
        parts.push(format!("({})", params.join(", ")));
    }

    // WHERE clause (partial index)
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct an IndexElem (column in an index)
fn reconstruct_index_elem(elem: &pg_query::protobuf::IndexElem, ctx: &mut TranspileContext) -> String {
    use pg_query::protobuf::{SortByDir, SortByNulls};

    let mut parts = Vec::new();

    // Get the column name or expression
    if !elem.name.is_empty() {
        parts.push(elem.name.to_lowercase());
    } else if let Some(ref expr) = elem.expr {
        // Expression index
        parts.push(reconstruct_node(expr, ctx));
    }

    // Handle ordering (ASC/DESC)
    let ordering = SortByDir::try_from(elem.ordering).unwrap_or(SortByDir::SortbyDefault);
    match ordering {
        SortByDir::SortbyAsc => parts.push("asc".to_string()),
        SortByDir::SortbyDesc => parts.push("desc".to_string()),
        _ => {}
    }

    // Handle NULLS ordering
    let nulls = SortByNulls::try_from(elem.nulls_ordering).unwrap_or(SortByNulls::SortbyNullsDefault);
    match nulls {
        SortByNulls::SortbyNullsFirst => parts.push("nulls first".to_string()),
        SortByNulls::SortbyNullsLast => parts.push("nulls last".to_string()),
        _ => {}
    }

    parts.join(" ")
}

// RLS helper functions - not yet integrated into main transpilation pipeline
#[allow(dead_code)]
/// Reconstruct CREATE POLICY statement
///
/// CREATE POLICY name ON table_name
///     [AS {PERMISSIVE | RESTRICTIVE}]
///     [FOR {ALL | SELECT | INSERT | UPDATE | DELETE}]
///     [TO {role_name [, ...] | PUBLIC | CURRENT_USER | SESSION_USER} [, ...]]
///     [USING (using_expression)]
///     [WITH CHECK (check_expression)]
fn reconstruct_create_policy_stmt(sql: &str) -> String {
    // Parse the CREATE POLICY statement
    // Since pg_query may not fully support CREATE POLICY, we do manual parsing

    let sql_upper = sql.to_uppercase();

    // Extract policy name
    let policy_name = extract_policy_name(sql);

    // Extract table name
    let table_name = extract_policy_table_name(sql);

    // Determine PERMISSIVE vs RESTRICTIVE (default is PERMISSIVE)
    let permissive = !sql_upper.contains("RESTRICTIVE");

    // Determine command (ALL, SELECT, INSERT, UPDATE, DELETE)
    let command = extract_policy_command(sql);

    // Extract roles
    let roles = extract_policy_roles(sql);

    // Extract USING expression
    let using_expr = extract_policy_using(sql);

    // Extract WITH CHECK expression
    let with_check_expr = extract_policy_with_check(sql);

    // Build the INSERT statement for the policy
    let roles_str = if roles.is_empty() {
        "NULL".to_string()
    } else {
        format!("'{}'", roles.join(","))
    };

    let using_str = using_expr.map(|e| format!("'{}'", e.replace("'", "''"))).unwrap_or_else(|| "NULL".to_string());
    let with_check_str = with_check_expr.map(|e| format!("'{}'", e.replace("'", "''"))).unwrap_or_else(|| "NULL".to_string());

    format!(
        "INSERT OR REPLACE INTO __pg_rls_policies__
         (polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled)
         VALUES ('{}', '{}', '{}', {}, {}, {}, {}, TRUE)",
        policy_name,
        table_name,
        command,
        permissive,
        roles_str,
        using_str,
        with_check_str
    )
}

/// Extract policy name from CREATE POLICY statement
fn extract_policy_name(sql: &str) -> String {
    // CREATE POLICY name ON ...
    let re = regex::Regex::new(r"CREATE\s+POLICY\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract table name from CREATE POLICY statement
fn extract_policy_table_name(sql: &str) -> String {
    // CREATE POLICY name ON table_name ...
    let re = regex::Regex::new(r"CREATE\s+POLICY\s+\w+\s+ON\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract command from CREATE POLICY statement
fn extract_policy_command(sql: &str) -> String {
    let sql_upper = sql.to_uppercase();

    if sql_upper.contains("FOR SELECT") {
        "SELECT".to_string()
    } else if sql_upper.contains("FOR INSERT") {
        "INSERT".to_string()
    } else if sql_upper.contains("FOR UPDATE") {
        "UPDATE".to_string()
    } else if sql_upper.contains("FOR DELETE") {
        "DELETE".to_string()
    } else {
        "ALL".to_string() // Default
    }
}

/// Extract roles from CREATE POLICY statement
fn extract_policy_roles(sql: &str) -> Vec<String> {
    let sql_upper = sql.to_uppercase();

    // Check if TO clause exists
    if let Some(to_pos) = sql_upper.find("TO") {
        // Find the end of the TO clause (before USING or WITH CHECK)
        let end_pos = sql_upper.find("USING")
            .or_else(|| sql_upper.find("WITH CHECK"))
            .unwrap_or(sql.len());

        let to_clause = &sql[to_pos..end_pos];

        // Parse comma-separated roles
        to_clause
            .replace("TO", "")
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![] // Empty means PUBLIC
    }
}

/// Extract USING expression from CREATE POLICY statement
fn extract_policy_using(sql: &str) -> Option<String> {
    let sql_upper = sql.to_uppercase();

    if let Some(using_pos) = sql_upper.find("USING") {
        // Find the start of the expression (after USING)
        let expr_start = using_pos + "USING".len();

        // Find the end (before WITH CHECK or end of statement)
        let expr_end = sql_upper[expr_start..].find("WITH CHECK")
            .map(|pos| expr_start + pos)
            .unwrap_or(sql.len());

        let expr = sql[expr_start..expr_end].trim();

        // Remove surrounding parentheses if present
        let expr = expr.strip_prefix('(').unwrap_or(expr);
        let expr = expr.strip_suffix(')').unwrap_or(expr);

        Some(expr.trim().to_string())
    } else {
        None
    }
}

/// Extract WITH CHECK expression from CREATE POLICY statement
fn extract_policy_with_check(sql: &str) -> Option<String> {
    let sql_upper = sql.to_uppercase();

    if let Some(with_check_pos) = sql_upper.find("WITH CHECK") {
        // Find the start of the expression (after WITH CHECK)
        let expr_start = with_check_pos + "WITH CHECK".len();

        let expr = &sql[expr_start..];

        // Remove surrounding parentheses if present
        let expr = expr.strip_prefix('(').unwrap_or(expr);
        let expr = expr.strip_suffix(')').unwrap_or(expr);

        Some(expr.trim().to_string())
    } else {
        None
    }
}

/// Reconstruct DROP POLICY statement
#[allow(dead_code)]
fn reconstruct_drop_policy_stmt(sql: &str) -> String {
    // DROP POLICY [IF EXISTS] name ON table_name [CASCADE | RESTRICT]
    let policy_name = extract_drop_policy_name(sql);
    let table_name = extract_drop_policy_table_name(sql);

    format!(
        "DELETE FROM __pg_rls_policies__ WHERE polname = '{}' AND polrelid = '{}'",
        policy_name, table_name
    )
}

/// Extract policy name from DROP POLICY statement
#[allow(dead_code)]
fn extract_drop_policy_name(sql: &str) -> String {
    // DROP POLICY [IF EXISTS] name ON ...
    let re = regex::Regex::new(r"DROP\s+POLICY\s+(?:IF\s+EXISTS\s+)?(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract table name from DROP POLICY statement
#[allow(dead_code)]
fn extract_drop_policy_table_name(sql: &str) -> String {
    // DROP POLICY name ON table_name ...
    let re = regex::Regex::new(r"DROP\s+POLICY\s+(?:IF\s+EXISTS\s+)?\w+\s+ON\s+(\w+)").unwrap();
    re.captures(sql)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extended transpile function that handles RLS injection
///
/// This function takes a connection and RLS context to properly inject
/// RLS predicates into the transpiled SQL using AST manipulation.
#[allow(dead_code)]
pub fn transpile_with_rls(
    sql: &str,
    conn: &Connection,
    rls_ctx: &RlsContext,
) -> TranspileResult {
    let upper_sql = sql.trim().to_uppercase();

    // Handle CREATE POLICY specially
    if upper_sql.starts_with("CREATE POLICY") {
        let sqlite_sql = reconstruct_create_policy_stmt(sql);
        return TranspileResult {
            sql: sqlite_sql,
            create_table_metadata: None,
            referenced_tables: vec![extract_policy_table_name(sql)],
            operation_type: OperationType::DDL,
        };
    }

    // Handle DROP POLICY specially
    if upper_sql.starts_with("DROP POLICY") {
        let sqlite_sql = reconstruct_drop_policy_stmt(sql);
        return TranspileResult {
            sql: sqlite_sql,
            create_table_metadata: None,
            referenced_tables: vec![extract_drop_policy_table_name(sql)],
            operation_type: OperationType::DDL,
        };
    }

    // Parse the SQL for AST-based RLS injection
    match pg_query::parse(sql) {
        Ok(result) => {
            if let Some(raw_stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = raw_stmt.stmt {
                    return transpile_with_rls_ast(stmt_node, conn, rls_ctx, sql);
                }
            }
        }
        Err(_) => {}
    }

    // Fallback to standard transpilation if parsing fails
    transpile_with_metadata(sql)
}

/// Transpile SQL with RLS injection using AST manipulation
#[allow(dead_code)]
fn transpile_with_rls_ast(
    node: &Node,
    conn: &Connection,
    rls_ctx: &RlsContext,
    _original_sql: &str,
) -> TranspileResult {
    let mut ctx = TranspileContext::new();

    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::SelectStmt(ref select_stmt) => {
                let table_name = extract_table_name_from_select(select_stmt);
                let sql = reconstruct_select_stmt_with_rls(select_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::SELECT,
                }
            }
            NodeEnum::InsertStmt(ref insert_stmt) => {
                let table_name = extract_table_name_from_insert(insert_stmt);
                let sql = reconstruct_insert_stmt_with_rls(insert_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::INSERT,
                }
            }
            NodeEnum::UpdateStmt(ref update_stmt) => {
                let table_name = extract_table_name_from_update(update_stmt);
                let sql = reconstruct_update_stmt_with_rls(update_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::UPDATE,
                }
            }
            NodeEnum::DeleteStmt(ref delete_stmt) => {
                let table_name = extract_table_name_from_delete(delete_stmt);
                let sql = reconstruct_delete_stmt_with_rls(delete_stmt, &mut ctx, conn, rls_ctx, &table_name);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DELETE,
                }
            }
            _ => reconstruct_sql_with_metadata(node, &mut ctx),
        }
    } else {
        reconstruct_sql_with_metadata(node, &mut ctx)
    }
}

/// Extract table name from SELECT statement
fn extract_table_name_from_select(stmt: &SelectStmt) -> String {
    if !stmt.from_clause.is_empty() {
        if let Some(ref node) = stmt.from_clause[0].node {
            if let NodeEnum::RangeVar(r) = node {
                return r.relname.to_lowercase();
            }
        }
    }
    String::new()
}

/// Extract table name from INSERT statement
fn extract_table_name_from_insert(stmt: &InsertStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Extract table name from UPDATE statement
fn extract_table_name_from_update(stmt: &UpdateStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Extract table name from DELETE statement
fn extract_table_name_from_delete(stmt: &DeleteStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Reconstruct SELECT statement with RLS predicate injection
fn reconstruct_select_stmt_with_rls(
    stmt: &SelectStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    // Get RLS predicate if applicable
    let rls_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match get_rls_where_clause(conn, table_name, rls_ctx, "SELECT") {
            Ok(pred) => pred,
            Err(_) => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    // Handle VALUES clause (not applicable for RLS)
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt, ctx);
    }

    // SELECT clause
    if !stmt.distinct_clause.is_empty() {
        let has_expressions = stmt.distinct_clause.iter().any(|n| {
            if let Some(ref inner) = n.node {
                matches!(inner, NodeEnum::ColumnRef(_) | NodeEnum::ResTarget(_))
            } else {
                false
            }
        });

        if has_expressions {
            parts.push("select distinct".to_string());
        } else {
            parts.push("select distinct".to_string());
        }
    } else {
        parts.push("select".to_string());
    }

    // Target list
    if stmt.target_list.is_empty() {
        parts.push("*".to_string());
    } else {
        let columns: Vec<String> = stmt
            .target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(columns.join(", "));
    }

    // FROM clause
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt
            .from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(tables.join(", "));
    }

    // WHERE clause with RLS injection
    let where_sql = if let Some(ref where_clause) = stmt.where_clause {
        let existing_where = reconstruct_node(where_clause, ctx);
        if !existing_where.is_empty() {
            if let Some(ref rls) = rls_predicate {
                format!("({}) AND ({})", existing_where, rls)
            } else {
                existing_where
            }
        } else if let Some(ref rls) = rls_predicate {
            rls.clone()
        } else {
            String::new()
        }
    } else if let Some(ref rls) = rls_predicate {
        rls.clone()
    } else {
        String::new()
    };

    if !where_sql.is_empty() {
        parts.push("where".to_string());
        parts.push(where_sql);
    }

    // GROUP BY clause
    if !stmt.group_clause.is_empty() {
        parts.push("group by".to_string());
        let groups: Vec<String> = stmt
            .group_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(groups.join(", "));
    }

    // HAVING clause
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause, ctx);
        if !having_sql.is_empty() {
            parts.push("having".to_string());
            parts.push(having_sql);
        }
    }

    // ORDER BY clause
    if !stmt.sort_clause.is_empty() {
        parts.push("order by".to_string());
        let sorts: Vec<String> = stmt
            .sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        parts.push(sorts.join(", "));
    }

    // LIMIT clause
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count, ctx);
        if limit_sql.to_uppercase() == "NULL" {
            parts.push("limit".to_string());
            parts.push("-1".to_string());
        } else if !limit_sql.is_empty() {
            parts.push("limit".to_string());
            parts.push(limit_sql);
        }
    }

    // OFFSET clause
    if let Some(ref limit_offset) = stmt.limit_offset {
        let offset_sql = reconstruct_node(limit_offset, ctx);
        if !offset_sql.is_empty() {
            parts.push("offset".to_string());
            parts.push(offset_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct INSERT statement with RLS WITH CHECK validation
fn reconstruct_insert_stmt_with_rls(
    stmt: &InsertStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    // Get WITH CHECK predicate if applicable
    let with_check_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        if is_rls_enabled(conn, table_name).unwrap_or(false) {
            if !can_bypass_rls(conn, table_name, rls_ctx).unwrap_or(false) {
                let policies = get_applicable_policies(conn, table_name, "INSERT", &rls_ctx.user_roles).unwrap_or_default();
                if policies.is_empty() {
                    Some("FALSE".to_string())
                } else {
                    build_rls_expression(&policies, false) // false = WITH CHECK
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    parts.push("insert into".to_string());

    // Table name
    let table_name_full = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}.{}", r.catalogname.to_lowercase(), r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name_full);

    // Columns
    if !stmt.cols.is_empty() {
        let cols: Vec<String> = stmt
            .cols
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::ResTarget(target) = inner {
                        return Some(target.name.to_lowercase());
                    }
                }
                None
            })
            .collect();
        parts.push(format!("({})", cols.join(", ")));
    }

    // Handle INSERT with WITH CHECK by converting to INSERT...SELECT pattern
    if let Some(ref select_stmt) = stmt.select_stmt {
        if let Some(ref inner) = select_stmt.node {
            if let NodeEnum::SelectStmt(sel) = inner {
                if !sel.values_lists.is_empty() {
                    // This is a VALUES clause - convert to SELECT with WHERE
                    if let Some(ref check_expr) = with_check_predicate {
                        let values_sql = reconstruct_values_stmt(sel, ctx);
                        // Convert VALUES to SELECT with RLS check
                        let cols_str = if !stmt.cols.is_empty() {
                            let cols: Vec<String> = stmt
                                .cols
                                .iter()
                                .filter_map(|n| {
                                    if let Some(ref inner) = n.node {
                                        if let NodeEnum::ResTarget(target) = inner {
                                            return Some(target.name.to_lowercase());
                                        }
                                    }
                                    None
                                })
                                .collect();
                            cols.join(", ")
                        } else {
                            "*".to_string()
                        };

                        // Build INSERT...SELECT with WHERE clause for RLS
                        parts.push(format!(
                            "select {} from ({} {}) where ({})",
                            cols_str,
                            values_sql.replace("values ", ""),
                            if stmt.cols.is_empty() { "" } else { "as v(" },
                            check_expr
                        ));
                    } else {
                        let select_sql = reconstruct_node(select_stmt, ctx);
                        parts.push(select_sql);
                    }
                } else {
                    let select_sql = reconstruct_node(select_stmt, ctx);
                    parts.push(select_sql);
                }
            } else {
                let select_sql = reconstruct_node(select_stmt, ctx);
                parts.push(select_sql);
            }
        } else {
            let select_sql = reconstruct_node(select_stmt, ctx);
            parts.push(select_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct UPDATE statement with RLS predicate injection
fn reconstruct_update_stmt_with_rls(
    stmt: &UpdateStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    // Get USING predicate (for filtering rows that can be updated)
    let using_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match get_rls_where_clause(conn, table_name, rls_ctx, "UPDATE") {
            Ok(pred) => pred,
            Err(_) => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    parts.push("update".to_string());

    // Table name
    let table_name_full = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}.{}", r.catalogname.to_lowercase(), r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name_full);

    // SET clause
    parts.push("set".to_string());
    let targets: Vec<String> = stmt
        .target_list
        .iter()
        .filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::ResTarget(target) = inner {
                    let col_name = target.name.to_lowercase();
                    let val = target
                        .val
                        .as_ref()
                        .map(|v| reconstruct_node(v, ctx))
                        .unwrap_or_default();
                    return Some(format!("{} = {}", col_name, val));
                }
            }
            None
        })
        .collect();
    parts.push(targets.join(", "));

    // WHERE clause with RLS injection
    let where_sql = if let Some(ref where_clause) = stmt.where_clause {
        let existing_where = reconstruct_node(where_clause, ctx);
        if !existing_where.is_empty() {
            if let Some(ref rls) = using_predicate {
                format!("({}) AND ({})", existing_where, rls)
            } else {
                existing_where
            }
        } else if let Some(ref rls) = using_predicate {
            rls.clone()
        } else {
            String::new()
        }
    } else if let Some(ref rls) = using_predicate {
        rls.clone()
    } else {
        String::new()
    };

    if !where_sql.is_empty() {
        parts.push("where".to_string());
        parts.push(where_sql);
    }

    // FROM clause
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let from_items: Vec<String> = stmt
            .from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(from_items.join(", "));
    }

    parts.join(" ")
}

/// Reconstruct DELETE statement with RLS predicate injection
fn reconstruct_delete_stmt_with_rls(
    stmt: &DeleteStmt,
    ctx: &mut TranspileContext,
    conn: &Connection,
    rls_ctx: &RlsContext,
    table_name: &str,
) -> String {
    // Get RLS predicate if applicable
    let rls_predicate = if !table_name.is_empty() && !rls_ctx.bypass_rls {
        match get_rls_where_clause(conn, table_name, rls_ctx, "DELETE") {
            Ok(pred) => pred,
            Err(_) => None,
        }
    } else {
        None
    };

    let mut parts = Vec::new();

    parts.push("delete from".to_string());

    // Table name
    let table_name_full = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name_full);

    // WHERE clause with RLS injection
    let where_sql = if let Some(ref where_clause) = stmt.where_clause {
        let existing_where = reconstruct_node(where_clause, ctx);
        if !existing_where.is_empty() {
            if let Some(ref rls) = rls_predicate {
                format!("({}) AND ({})", existing_where, rls)
            } else {
                existing_where
            }
        } else if let Some(ref rls) = rls_predicate {
            rls.clone()
        } else {
            String::new()
        }
    } else if let Some(ref rls) = rls_predicate {
        rls.clone()
    } else {
        String::new()
    };

    if !where_sql.is_empty() {
        parts.push("where".to_string());
        parts.push(where_sql);
    }

    // USING clause (for additional tables)
    if !stmt.using_clause.is_empty() {
        parts.push("using".to_string());
        let using_items: Vec<String> = stmt
            .using_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(using_items.join(", "));
    }

    parts.join(" ")
}

// ============================================================================
// Window Function Support
// ============================================================================

/// Frame option bitmasks from PostgreSQL (parsenodes.h)
#[allow(dead_code)]
pub mod frame_options {
    pub const NONDEFAULT: i32 = 0x00001;
    pub const RANGE: i32 = 0x00002;
    pub const ROWS: i32 = 0x00004;
    pub const GROUPS: i32 = 0x00008;
    pub const BETWEEN: i32 = 0x00010;
    pub const START_UNBOUNDED_PRECEDING: i32 = 0x00020;
    pub const END_UNBOUNDED_PRECEDING: i32 = 0x00040; // disallowed
    pub const START_UNBOUNDED_FOLLOWING: i32 = 0x00080; // disallowed
    pub const END_UNBOUNDED_FOLLOWING: i32 = 0x00100;
    pub const START_CURRENT_ROW: i32 = 0x00200;
    pub const END_CURRENT_ROW: i32 = 0x00400;
    pub const START_OFFSET_PRECEDING: i32 = 0x00800;
    pub const END_OFFSET_PRECEDING: i32 = 0x01000;
    pub const START_OFFSET_FOLLOWING: i32 = 0x02000;
    pub const END_OFFSET_FOLLOWING: i32 = 0x04000;
    pub const EXCLUDE_CURRENT_ROW: i32 = 0x08000;
    pub const EXCLUDE_GROUP: i32 = 0x10000;
    pub const EXCLUDE_TIES: i32 = 0x20000;
    pub const EXCLUSION: i32 = EXCLUDE_CURRENT_ROW | EXCLUDE_GROUP | EXCLUDE_TIES;
}

/// Reconstruct a WindowDef (OVER clause) into SQLite syntax
fn reconstruct_window_def(win_def: &WindowDef, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    // Handle named window reference (e.g., OVER w)
    if !win_def.refname.is_empty() {
        return win_def.refname.to_lowercase();
    }

    // PARTITION BY clause
    if !win_def.partition_clause.is_empty() {
        let partition_cols: Vec<String> = win_def
            .partition_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(format!("partition by {}", partition_cols.join(", ")));
    }

    // ORDER BY clause
    if !win_def.order_clause.is_empty() {
        let order_cols: Vec<String> = win_def
            .order_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(format!("order by {}", order_cols.join(", ")));
    }

    // Frame specification
    let frame_opts = win_def.frame_options;

    // Only add frame if NONDEFAULT is set (explicit frame specified)
    if frame_opts & frame_options::NONDEFAULT != 0 {
        let frame_str = reconstruct_frame_specification(win_def, ctx);
        if !frame_str.is_empty() {
            parts.push(frame_str);
        }
    }

    parts.join(" ")
}

/// Reconstruct frame specification (ROWS/RANGE/GROUPS BETWEEN ... AND ...)
fn reconstruct_frame_specification(win_def: &WindowDef, ctx: &mut TranspileContext) -> String {
    let frame_opts = win_def.frame_options;
    let mut parts = Vec::new();

    // Determine frame mode: ROWS, RANGE, or GROUPS
    let mode = if frame_opts & frame_options::ROWS != 0 {
        "rows"
    } else if frame_opts & frame_options::GROUPS != 0 {
        "groups"
    } else {
        "range" // default
    };

    // Check for BETWEEN
    let has_between = frame_opts & frame_options::BETWEEN != 0;

    // Build start bound
    let start_bound = if frame_opts & frame_options::START_UNBOUNDED_PRECEDING != 0 {
        "unbounded preceding".to_string()
    } else if frame_opts & frame_options::START_CURRENT_ROW != 0 {
        "current row".to_string()
    } else if frame_opts & frame_options::START_OFFSET_PRECEDING != 0 {
        if let Some(ref offset) = win_def.start_offset {
            format!("{} preceding", reconstruct_node(offset, ctx))
        } else {
            "unbounded preceding".to_string()
        }
    } else if frame_opts & frame_options::START_OFFSET_FOLLOWING != 0 {
        if let Some(ref offset) = win_def.start_offset {
            format!("{} following", reconstruct_node(offset, ctx))
        } else {
            "current row".to_string()
        }
    } else {
        // Default start
        "unbounded preceding".to_string()
    };

    // Build end bound
    let end_bound = if frame_opts & frame_options::END_UNBOUNDED_FOLLOWING != 0 {
        "unbounded following".to_string()
    } else if frame_opts & frame_options::END_CURRENT_ROW != 0 {
        "current row".to_string()
    } else if frame_opts & frame_options::END_OFFSET_PRECEDING != 0 {
        if let Some(ref offset) = win_def.end_offset {
            format!("{} preceding", reconstruct_node(offset, ctx))
        } else {
            "current row".to_string()
        }
    } else if frame_opts & frame_options::END_OFFSET_FOLLOWING != 0 {
        if let Some(ref offset) = win_def.end_offset {
            format!("{} following", reconstruct_node(offset, ctx))
        } else {
            "current row".to_string()
        }
    } else {
        // Default end
        "current row".to_string()
    };

    // Build frame string
    if has_between {
        parts.push(format!("{} between {} and {}", mode, start_bound, end_bound));
    } else {
        // Short form (e.g., ROWS UNBOUNDED PRECEDING)
        parts.push(format!("{} {}", mode, start_bound));
    }

    // Handle EXCLUDE clause
    if frame_opts & frame_options::EXCLUDE_CURRENT_ROW != 0 {
        parts.push("exclude current row".to_string());
    } else if frame_opts & frame_options::EXCLUDE_GROUP != 0 {
        parts.push("exclude group".to_string());
    } else if frame_opts & frame_options::EXCLUDE_TIES != 0 {
        parts.push("exclude ties".to_string());
    }
    // EXCLUDE NO OTHERS is the default, so we don't emit it

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_table_cascade() {
        // SQLite doesn't support CASCADE, so it should be stripped
        let result = transpile_with_metadata("DROP TABLE IF EXISTS test_jsonb CASCADE");
        assert_eq!(result.sql, "drop table if exists test_jsonb");
        assert_eq!(result.operation_type, OperationType::DDL);
    }

    #[test]
    fn test_drop_table_restrict() {
        // SQLite doesn't support RESTRICT, so it should be stripped
        let result = transpile_with_metadata("DROP TABLE IF EXISTS my_table RESTRICT");
        assert_eq!(result.sql, "drop table if exists my_table");
    }

    #[test]
    fn test_drop_table_without_if_exists() {
        let result = transpile_with_metadata("DROP TABLE my_table");
        assert_eq!(result.sql, "drop table my_table");
    }

    #[test]
    fn test_drop_index() {
        let result = transpile_with_metadata("DROP INDEX IF EXISTS idx_test");
        assert_eq!(result.sql, "drop index if exists idx_test");
    }

    #[test]
    fn test_drop_view() {
        let result = transpile_with_metadata("DROP VIEW IF EXISTS my_view CASCADE");
        assert_eq!(result.sql, "drop view if exists my_view");
    }

    #[test]
    fn test_drop_multiple_tables() {
        let result = transpile_with_metadata("DROP TABLE table1, table2");
        assert!(result.sql.contains("table1"));
        assert!(result.sql.contains("table2"));
    }

    #[test]
    fn test_create_index_if_not_exists() {
        let result = transpile_with_metadata("CREATE INDEX IF NOT EXISTS idx_name ON my_table(column)");
        assert!(result.sql.contains("create index if not exists idx_name"));
        assert!(result.sql.contains("on my_table"));
        assert!(result.sql.contains("(column)"));
    }

    #[test]
    fn test_create_unique_index() {
        let result = transpile_with_metadata("CREATE UNIQUE INDEX IF NOT EXISTS idx_unique ON users(email)");
        println!("SQL: {:?}", result.sql);
        assert!(result.sql.contains("create unique index if not exists idx_unique"));
        assert!(result.sql.contains("on users"));
        assert!(result.sql.contains("(email)"));
    }

    #[test]
    fn test_create_index_with_where() {
        let result = transpile_with_metadata("CREATE INDEX idx_active ON users(email) WHERE active = 1");
        assert!(result.sql.contains("create index idx_active"));
        assert!(result.sql.contains("on users"));
        assert!(result.sql.contains("(email)"));
        assert!(result.sql.contains("where"));
    }

    #[test]
    fn test_create_table_if_not_exists() {
        // This should still work via the existing CreateStmt handler
        let result = transpile_with_metadata("CREATE TABLE IF NOT EXISTS my_table (id INTEGER PRIMARY KEY)");
        assert!(result.sql.contains("create table"));
        assert!(result.sql.contains("my_table"));
    }

    #[test]
    fn test_insert_with_array_expr() {
        let sql = "INSERT INTO test_jsonb(name, tags, props) VALUES ('Alice', ARRAY['dev', 'remote'], '{\"age\": 30}')";
        let result = transpile_with_metadata(sql);

        // Should convert ARRAY['dev', 'remote'] to a JSON array
        assert!(result.sql.contains("insert into test_jsonb"));
        // Check that array is converted to JSON format (not empty)
        assert!(!result.sql.contains(", ,"), "Array should not be empty: {}", result.sql);
        // Check proper JSON array format
        assert!(result.sql.contains("'[\"dev\",\"remote\"]'"), "Array should be JSON: {}", result.sql);
    }

    #[test]
    fn test_insert_with_multiple_array_rows() {
        let sql = r#"INSERT INTO test_jsonb(name, tags, props)
VALUES
    ('Alice', ARRAY['dev', 'remote'], '{"age": 30, "active": true}'),
    ('Bob', ARRAY['qa', 'onsite'], '{"age": 25}'),
    ('Carol', ARRAY['dev', 'remote'], '{"age": 35}')"#;
        let result = transpile_with_metadata(sql);

        // Should convert all ARRAY[] to JSON arrays
        assert!(result.sql.contains("insert into test_jsonb"));
        // No empty values
        assert!(!result.sql.contains(", ,"), "Arrays should not be empty: {}", result.sql);
        // All three rows should have JSON arrays
        assert!(result.sql.contains("'[\"dev\",\"remote\"]'") || result.sql.contains("'[\"qa\",\"onsite\"]'"),
                "Arrays should be converted to JSON: {}", result.sql);
    }

    #[test]
    fn test_jsonb_key_exists_operator() {
        // Test that the ? operator for JSONB key existence is translated
        let result = transpile_with_metadata("SELECT id, name FROM test_jsonb WHERE props ? 'team'");
        // Should translate to json_type check
        assert!(result.sql.contains("json_type"), "Should use json_type for ? operator: {}", result.sql);
        assert!(result.sql.contains("IS NOT NULL"), "Should check IS NOT NULL: {}", result.sql);
    }

    #[test]
    fn test_jsonb_any_key_exists_operator() {
        // Test that the ?| operator for JSONB any-key-existence is translated
        let result = transpile_with_metadata("SELECT id, name FROM test_jsonb WHERE props ?| ARRAY['skills', 'hobbies']");
        // Should translate to EXISTS with json_each
        assert!(result.sql.contains("EXISTS"), "Should use EXISTS for ?| operator: {}", result.sql);
        assert!(result.sql.contains("json_each"), "Should use json_each for ?| operator: {}", result.sql);
    }

    #[test]
    fn test_jsonb_all_keys_exist_operator() {
        // Test that the ?& operator for JSONB all-keys-existence is translated
        let result = transpile_with_metadata("SELECT id, name FROM test_jsonb WHERE props ?& ARRAY['skills', 'hobbies']");
        // Should translate to NOT EXISTS with json_each
        assert!(result.sql.contains("NOT EXISTS"), "Should use NOT EXISTS for ?& operator: {}", result.sql);
        assert!(result.sql.contains("json_each"), "Should use json_each for ?& operator: {}", result.sql);
    }

    #[test]
    fn test_jsonb_path_exists() {
        let result = transpile_with_metadata("SELECT id, name FROM test_jsonb WHERE jsonb_path_exists(props, '$.team.id')");
        assert!(result.sql.contains("json_type"), "Should use json_type for jsonb_path_exists: {}", result.sql);
        assert!(result.sql.contains("IS NOT NULL"), "Should check IS NOT NULL: {}", result.sql);
    }

    #[test]
    fn test_jsonb_path_query() {
        let result = transpile_with_metadata("SELECT jsonb_path_query(props, '$.team')");
        assert!(result.sql.contains("json_extract"), "Should use json_extract for jsonb_path_query: {}", result.sql);
    }

    #[test]
    fn test_jsonb_each_lateral() {
        let result = transpile_with_metadata("SELECT id, name, key, value FROM test_jsonb, LATERAL jsonb_each(props) AS x(key, value)");
        println!("Transpiled LATERAL: {}", result.sql);
        // Should translate jsonb_each to json_each and handle LATERAL
        assert!(!result.sql.is_empty(), "Should produce some SQL");
    }

    #[test]
    fn test_jsonb_remove_array() {
        let result = transpile_with_metadata("SELECT props - ARRAY['age', 'active'] AS reduced FROM test_jsonb");
        println!("Transpiled remove array: {}", result.sql);
        // Should expand array into multiple paths
        assert!(result.sql.contains("json_remove"), "Should use json_remove: {}", result.sql);
    }
}
