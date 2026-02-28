use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    AConst, AExpr, BoolExpr, ColumnDef, ColumnRef, Constraint, CreateStmt, FuncCall, Node,
    RangeVar, ResTarget, SelectStmt, TypeCast, TypeName, InsertStmt, UpdateStmt, DeleteStmt,
    JoinExpr, NullTest, SubLink, CaseExpr, CreateRoleStmt, DropRoleStmt, GrantStmt, GrantRoleStmt,
    AlterTableStmt,
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

/// Reconstruct a SELECT statement
fn reconstruct_select_stmt(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    // Check if this is a VALUES statement (used in INSERT)
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt, ctx);
    }

    // Handle DISTINCT / DISTINCT ON
    if !stmt.distinct_clause.is_empty() {
        // Check if this is DISTINCT ON (has expressions in distinct_clause)
        let has_expressions = stmt.distinct_clause.iter().any(|n| {
            if let Some(ref inner) = n.node {
                matches!(inner, NodeEnum::ColumnRef(_) | NodeEnum::ResTarget(_))
            } else {
                false
            }
        });
        
        if has_expressions {
            // DISTINCT ON - for now just output DISTINCT (full ROW_NUMBER() rewrite is complex)
            parts.push("select distinct".to_string());
        } else {
            parts.push("select distinct".to_string());
        }
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
            NodeEnum::AStar(_) => "*".to_string(),
            NodeEnum::ColumnRef(ref col_ref) => reconstruct_column_ref(col_ref, ctx),
            NodeEnum::String(s) => s.sval.clone(),
            NodeEnum::FuncCall(ref func_call) => reconstruct_func_call(func_call, ctx),
            NodeEnum::AConst(ref aconst) => reconstruct_aconst(aconst),
            NodeEnum::TypeCast(ref type_cast) => reconstruct_type_cast(type_cast, ctx),
            NodeEnum::AExpr(ref a_expr) => reconstruct_a_expr(a_expr, ctx),
            NodeEnum::BoolExpr(ref bool_expr) => reconstruct_bool_expr(bool_expr, ctx),
            NodeEnum::JoinExpr(ref join_expr) => reconstruct_join_expr(join_expr, ctx),
            NodeEnum::SelectStmt(ref select_stmt) => reconstruct_select_stmt(select_stmt, ctx),
            NodeEnum::SubLink(ref sub_link) => reconstruct_sub_link(sub_link, ctx),
            NodeEnum::NullTest(ref null_test) => reconstruct_null_test(null_test, ctx),
            NodeEnum::CaseExpr(ref case_expr) => reconstruct_case_expr(case_expr, ctx),
            NodeEnum::List(ref list) => {
                let items: Vec<String> = list.items.iter().map(|n| reconstruct_node(n, ctx)).collect();
                items.join(", ")
            }
            NodeEnum::CaseWhen(_) => {
                // CaseWhen is handled within reconstruct_case_expr, not standalone
                "".to_string()
            }
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

/// Reconstruct an AExpr node (operators)
fn reconstruct_a_expr(a_expr: &AExpr, ctx: &mut TranspileContext) -> String {
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

    let args: Vec<String> = func_call
        .args
        .iter()
        .map(|n| reconstruct_node(n, ctx))
        .collect();

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
        _ => {
            // For unknown functions, return the full name if schema-qualified
            // but strip 'pg_catalog' if present as SQLite doesn't have it
            if func_parts.len() > 1 {
                if func_parts[0] == "pg_catalog" {
                    return format!("{}({})", func_name, args.join(", "));
                }
                return format!("{}({})", full_func_name, args.join(", "));
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

    format!("{}({})", sqlite_func, args.join(", "))
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
