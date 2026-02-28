use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    AConst, AExpr, BoolExpr, ColumnDef, ColumnRef, Constraint, CreateStmt, FuncCall, Node,
    RangeVar, ResTarget, SelectStmt, TypeCast, TypeName, InsertStmt, UpdateStmt, DeleteStmt,
    JoinExpr, NullTest, SubLink, CaseExpr
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
            // Fallback: simple string replacement for basic cases
            TranspileResult {
                sql: sql.to_lowercase().replace("now()", "datetime('now')"),
                create_table_metadata: None,
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
            NodeEnum::InsertStmt(ref insert_stmt) => TranspileResult {
                sql: reconstruct_insert_stmt(insert_stmt),
                create_table_metadata: None,
            },
            NodeEnum::UpdateStmt(ref update_stmt) => TranspileResult {
                sql: reconstruct_update_stmt(update_stmt),
                create_table_metadata: None,
            },
            NodeEnum::DeleteStmt(ref delete_stmt) => TranspileResult {
                sql: reconstruct_delete_stmt(delete_stmt),
                create_table_metadata: None,
            },
            NodeEnum::VariableSetStmt(ref _set_stmt) => TranspileResult {
                sql: "select 1".to_string(), // Safely ignore SET for now
                create_table_metadata: None,
            },
            NodeEnum::VariableShowStmt(ref show_stmt) => TranspileResult {
                sql: format!("select current_setting('{}') as {}", show_stmt.name, show_stmt.name),
                create_table_metadata: None,
            },
            _ => TranspileResult {
                sql: node.deparse().unwrap_or_else(|_| "".to_string()).to_lowercase(),
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

    // Extract constraints
    let constraints = extract_constraints(&col_def.constraints);
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
        pg_query::protobuf::ConstrType::ConstrPrimary => Some("PRIMARY KEY".to_string()),
        pg_query::protobuf::ConstrType::ConstrUnique => Some("UNIQUE".to_string()),
        pg_query::protobuf::ConstrType::ConstrCheck => {
            if let Some(ref expr) = constraint.raw_expr {
                let expr_sql = reconstruct_node(expr);
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
fn reconstruct_select_stmt(stmt: &SelectStmt) -> String {
    let mut parts = Vec::new();

    // Check if this is a VALUES statement (used in INSERT)
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt);
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
            .map(reconstruct_node)
            .collect();
        parts.push(columns.join(", "));
    }

    // FROM clause
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt
            .from_clause
            .iter()
            .map(reconstruct_node)
            .collect();
        parts.push(tables.join(", "));
    }

    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause);
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
            .map(reconstruct_node)
            .collect();
        parts.push(groups.join(", "));
    }

    // HAVING clause
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause);
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
            .map(reconstruct_sort_by)
            .collect();
        parts.push(sorts.join(", "));
    }

    // LIMIT clause
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count);
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
        let offset_sql = reconstruct_node(limit_offset);
        if !offset_sql.is_empty() {
            parts.push("offset".to_string());
            parts.push(offset_sql);
        }
    }

    parts.join(" ")
}

/// Reconstruct a VALUES statement (used in INSERT)
fn reconstruct_values_stmt(stmt: &SelectStmt) -> String {
    let mut values_parts = Vec::new();
    
    for values_list in &stmt.values_lists {
        if let Some(ref inner) = values_list.node {
            if let NodeEnum::List(list) = inner {
                let values: Vec<String> = list
                    .items
                    .iter()
                    .map(reconstruct_node)
                    .collect();
                values_parts.push(format!("({})", values.join(", ")));
            }
        }
    }
    
    format!("values {}", values_parts.join(", "))
}

/// Reconstruct a SortBy node (ORDER BY)
fn reconstruct_sort_by(node: &Node) -> String {
    if let Some(ref inner) = node.node {
        if let NodeEnum::SortBy(sort_by) = inner {
            let expr_sql = sort_by
                .node
                .as_ref()
                .map(|n| reconstruct_node(n))
                .unwrap_or_default();
            
            let direction = match sort_by.sortby_dir() {
                pg_query::protobuf::SortByDir::SortbyAsc => " ASC",
                pg_query::protobuf::SortByDir::SortbyDesc => " DESC",
                _ => "",
            };
            
            return format!("{}{}", expr_sql, direction.to_lowercase());
        }
    }
    reconstruct_node(node)
}

/// Reconstruct an INSERT statement
fn reconstruct_insert_stmt(stmt: &InsertStmt) -> String {
    let mut parts = Vec::new();
    
    parts.push("insert into".to_string());
    
    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            if r.schemaname.is_empty() || r.schemaname == "public" {
                r.relname.to_lowercase()
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), r.relname.to_lowercase())
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
        let select_sql = reconstruct_node(select_stmt);
        parts.push(select_sql);
    }
    
    parts.join(" ")
}

/// Reconstruct an UPDATE statement
fn reconstruct_update_stmt(stmt: &UpdateStmt) -> String {
    let mut parts = Vec::new();
    
    parts.push("update".to_string());
    
    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            if r.schemaname.is_empty() || r.schemaname == "public" {
                r.relname.to_lowercase()
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), r.relname.to_lowercase())
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
                        .map(|v| reconstruct_node(v))
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
        let where_sql = reconstruct_node(where_clause);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }
    
    parts.join(" ")
}

/// Reconstruct a DELETE statement
fn reconstruct_delete_stmt(stmt: &DeleteStmt) -> String {
    let mut parts = Vec::new();
    
    parts.push("delete from".to_string());
    
    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            if r.schemaname.is_empty() || r.schemaname == "public" {
                r.relname.to_lowercase()
            } else {
                format!("{}.{}", r.schemaname.to_lowercase(), r.relname.to_lowercase())
            }
        })
        .unwrap_or_default();
    parts.push(table_name);
    
    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
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
            NodeEnum::JoinExpr(ref join_expr) => reconstruct_join_expr(join_expr),
            NodeEnum::SelectStmt(ref select_stmt) => reconstruct_select_stmt(select_stmt),
            NodeEnum::SubLink(ref sub_link) => reconstruct_sub_link(sub_link),
            NodeEnum::NullTest(ref null_test) => reconstruct_null_test(null_test),
            NodeEnum::CaseExpr(ref case_expr) => reconstruct_case_expr(case_expr),
            NodeEnum::List(ref list) => {
                let items: Vec<String> = list.items.iter().map(reconstruct_node).collect();
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
fn reconstruct_join_expr(join_expr: &JoinExpr) -> String {
    let mut parts = Vec::new();
    
    // Left side
    if let Some(ref left) = join_expr.larg {
        parts.push(reconstruct_node(left));
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
        parts.push(reconstruct_node(right));
    }
    
    // ON clause
    if let Some(ref qual) = join_expr.quals {
        let qual_sql = reconstruct_node(qual);
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
fn reconstruct_sub_link(sub_link: &SubLink) -> String {
    let subquery = sub_link
        .subselect
        .as_ref()
        .map(|n| reconstruct_node(n))
        .unwrap_or_default();
    
    match sub_link.sub_link_type() {
        pg_query::protobuf::SubLinkType::ExistsSublink => format!("exists ({})", subquery),
        pg_query::protobuf::SubLinkType::AnySublink => {
            let test_expr = sub_link
                .testexpr
                .as_ref()
                .map(|n| reconstruct_node(n))
                .unwrap_or_default();
            format!("{} in ({})", test_expr, subquery)
        }
        pg_query::protobuf::SubLinkType::AllSublink => {
            let test_expr = sub_link
                .testexpr
                .as_ref()
                .map(|n| reconstruct_node(n))
                .unwrap_or_default();
            format!("{} in ({})", test_expr, subquery)
        }
        _ => format!("({})", subquery),
    }
}

/// Reconstruct a NullTest (IS NULL / IS NOT NULL)
fn reconstruct_null_test(null_test: &NullTest) -> String {
    let arg = null_test
        .arg
        .as_ref()
        .map(|n| reconstruct_node(n))
        .unwrap_or_default();
    
    match null_test.nulltesttype() {
        pg_query::protobuf::NullTestType::IsNull => format!("{} is null", arg),
        pg_query::protobuf::NullTestType::IsNotNull => format!("{} is not null", arg),
        _ => arg,
    }
}

/// Reconstruct a Case expression
fn reconstruct_case_expr(case_expr: &CaseExpr) -> String {
    let mut parts = Vec::new();
    parts.push("case".to_string());
    
    // CASE expression (if present) - this is the simple CASE form: CASE expr WHEN ...
    if let Some(ref arg) = case_expr.arg {
        parts.push(reconstruct_node(arg));
    }
    
    // WHEN clauses
    for when in &case_expr.args {
        if let Some(ref inner) = when.node {
            if let NodeEnum::CaseWhen(case_when) = inner {
                let when_expr = case_when.expr.as_ref().map(|n| reconstruct_node(n)).unwrap_or_default();
                let when_result = case_when.result.as_ref().map(|n| reconstruct_node(n)).unwrap_or_default();
                
                parts.push(format!("when {} then {}", when_expr, when_result));
            }
        }
    }
    
    // ELSE clause
    if let Some(ref default_result) = case_expr.defresult {
        let default_sql = reconstruct_node(default_result);
        parts.push(format!("else {}", default_sql));
    }
    
    parts.push("end".to_string());
    parts.join(" ")
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
    format!("cast({} as {})", arg_sql, sqlite_type.to_lowercase())
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
fn reconstruct_bool_expr(bool_expr: &BoolExpr) -> String {
    let op = match bool_expr.boolop() {
        pg_query::protobuf::BoolExprType::AndExpr => "AND",
        pg_query::protobuf::BoolExprType::OrExpr => "OR",
        pg_query::protobuf::BoolExprType::NotExpr => "NOT",
        _ => "AND",
    };

    let args: Vec<String> = bool_expr.args.iter().map(reconstruct_node).collect();

    if bool_expr.boolop() == pg_query::protobuf::BoolExprType::NotExpr {
        format!("NOT ({})", args.join(" "))
    } else {
        format!("({})", args.join(&format!(" {} ", op)))
    }
}

/// Reconstruct a ResTarget node (SELECT column or alias)
fn reconstruct_res_target(target: &ResTarget) -> String {
    let name = &target.name;
    if let Some(ref val) = target.val {
        let val_sql = reconstruct_node(val);
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
fn reconstruct_range_var(range_var: &RangeVar) -> String {
    let table_name = range_var.relname.to_lowercase();
    let schema_name = range_var.schemaname.to_lowercase();
    let alias = range_var.alias.as_ref().map(|a| a.aliasname.to_lowercase());

    // Map 'public' and 'pg_catalog' schema to no prefix (SQLite doesn't have schemas)
    // Other schemas are treated as attached databases
    let full_table = if schema_name.is_empty() || schema_name == "public" || schema_name == "pg_catalog" {
        format!("\"{}\"", table_name)
    } else {
        format!("\"{}\".\"{}\"", schema_name, table_name)
    };

    if let Some(a) = alias {
        if a != table_name && a != format!("{}.{}", schema_name, table_name) {
            format!("{} as \"{}\"", full_table, a)
        } else {
            full_table
        }
    } else {
        full_table
    }
}

/// Reconstruct a ColumnRef node
fn reconstruct_column_ref(col_ref: &ColumnRef) -> String {
    let fields: Vec<String> = col_ref
        .fields
        .iter()
        .filter_map(|f| {
            if let Some(ref inner) = f.node {
                match inner {
                    NodeEnum::String(s) => Some(format!("\"{}\"", s.sval.to_lowercase())),
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
fn reconstruct_func_call(func_call: &FuncCall) -> String {
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
        .map(reconstruct_node)
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

/// Debug function to print AST structure
#[allow(dead_code)]
pub fn debug_ast(sql: &str) {
    match pg_query::parse(sql) {
        Ok(result) => {
            println!("Parsed AST for: {}", sql);
            for (i, raw_stmt) in result.protobuf.stmts.iter().enumerate() {
                println!("Statement {}:", i);
                if let Some(ref stmt) = raw_stmt.stmt {
                    debug_node(stmt, 0);
                }
            }
        }
        Err(e) => println!("Parse error: {}", e),
    }
}

#[allow(dead_code)]
fn debug_node(node: &Node, indent: usize) {
    let prefix = "  ".repeat(indent);
    if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::SelectStmt(stmt) => {
                println!("{}SelectStmt:", prefix);
                println!("{}  target_list: {:?}", prefix, stmt.target_list.len());
                println!("{}  from_clause: {:?}", prefix, stmt.from_clause.len());
            }
            NodeEnum::ResTarget(target) => {
                println!("{}ResTarget: name='{}'", prefix, target.name);
            }
            NodeEnum::AStar(_) => {
                println!("{}AStar: *", prefix);
            }
            NodeEnum::RangeVar(range_var) => {
                println!("{}RangeVar: {}.{}", prefix, range_var.schemaname, range_var.relname);
            }
            NodeEnum::ColumnRef(_col_ref) => {
                println!("{}ColumnRef:", prefix);
            }
            NodeEnum::String(s) => {
                println!("{}String: '{}'", prefix, s.sval);
            }
            NodeEnum::CreateStmt(stmt) => {
                println!("{}CreateStmt:", prefix);
                if let Some(ref rel) = stmt.relation {
                    println!("{}  table: {}", prefix, rel.relname);
                }
                for element in &stmt.table_elts {
                    if let Some(ref node) = element.node {
                        if let NodeEnum::ColumnDef(col_def) = node {
                            println!("{}  column: {}", prefix, col_def.colname);
                            if let Some(ref type_name) = col_def.type_name {
                                let names: Vec<String> = type_name
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
                                println!("{}    type: {:?}", prefix, names);
                            }
                        }
                    }
                }
            }
            NodeEnum::ColumnDef(col_def) => {
                println!("{}ColumnDef: {}", prefix, col_def.colname);
            }
            _ => println!("{}Other: {:?}", prefix, inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_select_star() {
        let input = "SELECT * FROM users";
        debug_ast(input);
    }

    #[test]
    fn test_transpile_select_star() {
        let input = "SELECT * FROM users";
        let expected = "select * from users";
        let result = transpile(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_transpile_select_columns() {
        let input = "SELECT id, name FROM users";
        let expected = "select id, name from users";
        let result = transpile(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_transpile_limit_all() {
        let input = "SELECT * FROM users LIMIT ALL";
        let expected = "select * from users limit -1";
        let result = transpile(input);
        println!("LIMIT ALL test: input='{}', output='{}'", input, result);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_transpile_now() {
        let input = "SELECT now()";
        let result = transpile(input);
        assert!(result.contains("datetime('now')"));
    }

    #[test]
    fn test_transpile_case_expression() {
        let input = "SELECT CASE c.relkind WHEN 'r' THEN 'table' WHEN 'v' THEN 'view' END as type FROM pg_class c";
        debug_ast(input);
        let result = transpile(input);
        println!("CASE expression result: {}", result);
        assert!(result.contains("case"));
        assert!(result.contains("when"));
        assert!(result.contains("then"));
        assert!(result.contains("end"));
    }

    #[test]
    fn test_transpile_in_clause() {
        let input = "SELECT * FROM users WHERE id IN (1, 2, 3)";
        debug_ast(input);
        let result = transpile(input);
        println!("IN clause result: {}", result);
        assert!(result.contains("in"));
        assert!(result.contains("(1, 2, 3)"));
    }

    #[test]
    fn test_transpile_dt_query() {
        let input = r#"SELECT n.nspname as "Schema",
  c.relname as "Name",
  CASE c.relkind WHEN 'r' THEN 'table' WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized view' WHEN 'i' THEN 'index' WHEN 'S' THEN 'sequence' WHEN 't' THEN 'TOAST table' WHEN 'f' THEN 'foreign table' WHEN 'p' THEN 'partitioned table' WHEN 'I' THEN 'partitioned index' END as "Type",
  pg_catalog.pg_get_userbyid(c.relowner) as "Owner"
FROM pg_catalog.pg_class c
     LEFT JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r','p','')
      AND n.nspname <> 'pg_catalog'
      AND n.nspname !~ '^pg_toast'
      AND n.nspname <> 'information_schema'
  AND pg_catalog.pg_table_is_visible(c.oid)
ORDER BY 1,2"#;
        debug_ast(input);
        let result = transpile(input);
        println!("\\dt query result: {}", result);
        // Should use NOT regexp function call (with quotes)
        assert!(result.contains("NOT regexp('^pg_toast', \"n\".\"nspname\")"));
    }

    #[test]
    fn test_transpile_schema_mapping_public() {
        let input = "SELECT * FROM public.users";
        let result = transpile(input);
        // public schema should be stripped
        assert!(!result.contains("public."));
        assert!(result.contains("users"));
    }

    #[test]
    fn test_transpile_schema_mapping_attached_db() {
        let input = "SELECT * FROM inventory.products";
        let result = transpile(input);
        // Non-public schemas should be kept as database.table
        assert!(result.contains("inventory.products"));
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
        
        assert!(result.sql.contains("create table test_table"));
        assert!(result.sql.contains("integer primary key autoincrement"));
        assert!(result.sql.contains("text"));
        
        let metadata = result.create_table_metadata.expect("Should have metadata");
        assert_eq!(metadata.table_name, "test_table");
        
        let id_col = metadata
            .columns
            .iter()
            .find(|c| c.column_name == "id")
            .expect("Should have id column");
        assert!(id_col.original_type.contains("SERIAL"));
    }

    #[test]
    fn test_create_table_timestamp() {
        let input = "CREATE TABLE events (id SERIAL, created_at TIMESTAMP WITH TIME ZONE)";
        let result = transpile_with_metadata(input);
        
        assert!(result.sql.contains("create table events"));
        assert!(result.sql.contains("text")); // TIMESTAMP maps to TEXT in SQLite
        
        let metadata = result.create_table_metadata.expect("Should have metadata");
        let ts_col = metadata
            .columns
            .iter()
            .find(|c| c.column_name == "created_at")
            .expect("Should have created_at column");
        assert!(ts_col.original_type.contains("TIMESTAMP"));
    }

    #[test]
    fn test_rewrite_types() {
        assert_eq!(rewrite_type_for_sqlite("SERIAL"), "integer primary key autoincrement");
        assert_eq!(rewrite_type_for_sqlite("VARCHAR(10)"), "text");
        assert_eq!(rewrite_type_for_sqlite("INTEGER"), "integer");
        assert_eq!(rewrite_type_for_sqlite("BOOLEAN"), "integer");
        assert_eq!(rewrite_type_for_sqlite("TIMESTAMP"), "text");
        assert_eq!(rewrite_type_for_sqlite("JSONB"), "text");
        assert_eq!(rewrite_type_for_sqlite("UUID"), "text");
        assert_eq!(rewrite_type_for_sqlite("BYTEA"), "blob");
        
        // Additional types
        assert_eq!(rewrite_type_for_sqlite("MONEY"), "real");
        assert_eq!(rewrite_type_for_sqlite("INET"), "text");
        assert_eq!(rewrite_type_for_sqlite("POINT"), "text");
        assert_eq!(rewrite_type_for_sqlite("INT[]"), "text");
        assert_eq!(rewrite_type_for_sqlite("TSVECTOR"), "text");
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    fn test_debug_distinct() {
        let input = "SELECT DISTINCT status FROM orders";
        debug_ast(input);
        let result = transpile(input);
        println!("Transpiled: {}", result);
        assert!(result.contains("distinct"));
    }

    #[test]
    fn test_debug_cast() {
        let input = "SELECT '1'::int";
        debug_ast(input);
        let result = transpile(input);
        println!("Transpiled: {}", result);
        assert!(result.contains("cast"));
    }
}

#[cfg(test)]
mod operator_tests {
    use super::*;

    #[test]
    fn test_transpile_operators() {
        let input = "SELECT id FROM users WHERE name ~~ 'alice%' AND id > 10";
        let result = transpile(input);
        println!("Transpiled: {}", result);
        assert!(result.contains("like"));
        assert!(result.contains("id > 10"));
    }

    #[test]
    fn test_transpile_cast() {
        let input = "SELECT '1'::int";
        let result = transpile(input);
        println!("Transpiled: {}", result);
        assert!(result.contains("cast("));
        assert!(result.contains("as integer"));
    }
}

#[cfg(test)]
mod statement_tests {
    use super::*;

    #[test]
    fn test_transpile_insert() {
        let input = "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')";
        debug_ast(input);
        let result = transpile(input);
        println!("Transpiled INSERT: {}", result);
        assert!(result.contains("insert into"));
        assert!(result.contains("users"));
        assert!(result.contains("values"));
    }

    #[test]
    fn test_transpile_update() {
        let input = "UPDATE users SET name = 'Bob' WHERE id = 1";
        let result = transpile(input);
        println!("Transpiled UPDATE: {}", result);
        assert!(result.contains("update"));
        assert!(result.contains("set"));
        assert!(result.contains("where"));
    }

    #[test]
    fn test_transpile_delete() {
        let input = "DELETE FROM users WHERE id = 1";
        let result = transpile(input);
        println!("Transpiled DELETE: {}", result);
        assert!(result.contains("delete from"));
        assert!(result.contains("where"));
    }

    #[test]
    fn test_transpile_order_by() {
        let input = "SELECT * FROM users ORDER BY name ASC, id DESC";
        let result = transpile(input);
        println!("Transpiled ORDER BY: {}", result);
        assert!(result.contains("order by"));
        assert!(result.contains("name asc"));
        assert!(result.contains("id desc"));
    }

    #[test]
    fn test_transpile_group_by() {
        let input = "SELECT status, COUNT(*) FROM orders GROUP BY status";
        let result = transpile(input);
        println!("Transpiled GROUP BY: {}", result);
        assert!(result.contains("group by"));
        assert!(result.contains("status"));
    }

    #[test]
    fn test_transpile_join() {
        let input = "SELECT u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id";
        let result = transpile(input);
        println!("Transpiled JOIN: {}", result);
        assert!(result.contains("join"));
        assert!(result.contains("on"));
    }

    #[test]
    fn test_transpile_subquery() {
        let input = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)";
        let result = transpile(input);
        println!("Transpiled subquery: {}", result);
        assert!(result.contains("in"));
        assert!(result.contains("select"));
    }

    #[test]
    fn test_transpile_offset() {
        let input = "SELECT * FROM users LIMIT 10 OFFSET 20";
        let result = transpile(input);
        println!("Transpiled OFFSET: {}", result);
        assert!(result.contains("limit 10"));
        assert!(result.contains("offset 20"));
    }

    #[test]
    fn test_transpile_distinct() {
        let input = "SELECT DISTINCT status FROM orders";
        let result = transpile(input);
        println!("Transpiled DISTINCT: {}", result);
        assert!(result.contains("distinct"));
    }

    #[test]
    fn test_transpile_like_operators() {
        let input = "SELECT * FROM users WHERE name ~~ 'Alice%'";
        let result = transpile(input);
        println!("Transpiled LIKE: {}", result);
        assert!(result.contains("like"));
    }

    #[test]
    fn test_transpile_not_like_operators() {
        let input = "SELECT * FROM users WHERE name !~~ 'Alice%'";
        let result = transpile(input);
        println!("Transpiled NOT LIKE: {}", result);
        assert!(result.contains("not like"));
    }

    #[test]
    fn test_create_table_all_types() {
        let input = r#"CREATE TABLE test (
            id SERIAL,
            name VARCHAR(100),
            email TEXT,
            age INTEGER,
            score REAL,
            is_active BOOLEAN,
            created_at TIMESTAMP WITH TIME ZONE,
            data JSONB,
            uuid UUID,
            content BYTEA,
            price MONEY,
            flags BIT(8),
            ip_addr INET,
            location POINT,
            tags TEXT[],
            search_doc TSVECTOR
        )"#;
        
        let result = transpile_with_metadata(input);
        println!("Transpiled CREATE TABLE: {}", result.sql);
        
        assert!(result.sql.contains("create table test"));
        assert!(result.sql.contains("integer primary key autoincrement"));
        assert!(result.sql.contains("text"));
        assert!(result.sql.contains("integer"));
        assert!(result.sql.contains("real"));
        
        let metadata = result.create_table_metadata.expect("Should have metadata");
        assert_eq!(metadata.columns.len(), 16);
    }
}
