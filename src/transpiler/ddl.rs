//! DDL (Data Definition Language) statement reconstruction
//!
//! This module handles the reconstruction of PostgreSQL DDL statements
//! into SQLite-compatible SQL, including CREATE TABLE, ALTER TABLE,
//! DROP, TRUNCATE, CREATE INDEX, and COPY statements.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    Node, CreateStmt, ColumnDef, Constraint, AlterTableStmt, DropStmt, TruncateStmt, 
    IndexStmt, CopyStmt
};
use super::context::{TranspileContext, TranspileResult, OperationType, CreateTableMetadata, ColumnTypeInfo};
use crate::transpiler::reconstruct_node;
use pg_query::protobuf::TypeName;

pub(crate) fn reconstruct_create_stmt_with_metadata(stmt: &CreateStmt, ctx: &mut TranspileContext) -> TranspileResult {
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
        copy_metadata: None,
        referenced_tables: ctx.referenced_tables.clone(),
        operation_type: OperationType::DDL,
        errors: Vec::new(),
    }
}

/// Reconstruct a column definition and extract type metadata
/// Returns (SQLite column SQL, optional metadata)
pub(crate) fn reconstruct_column_def(col_def: &ColumnDef, ctx: &mut TranspileContext) -> (String, Option<ColumnTypeInfo>) {
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
pub(crate) fn extract_original_type(type_name: &Option<TypeName>) -> String {
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
pub(crate) fn rewrite_type_for_sqlite(pg_type: &str) -> String {
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
pub(crate) fn extract_constraints(constraints: &[Node], ctx: &mut TranspileContext) -> String {
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
pub(crate) fn reconstruct_constraint(constraint: &Constraint, ctx: &mut TranspileContext) -> Option<String> {
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

pub(crate) fn reconstruct_alter_table_stmt(stmt: &AlterTableStmt, ctx: &mut TranspileContext) -> String {
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
pub(crate) fn reconstruct_drop_stmt(stmt: &DropStmt, ctx: &mut TranspileContext) -> String {
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

    // SQLite doesn't support multiple objects in a single DROP statement
    // Generate separate DROP statements for each object
    let if_exists = if stmt.missing_ok { " if exists " } else { " " };

    let drops: Vec<String> = object_names
        .iter()
        .map(|name| format!("drop {}{}{}", type_keyword, if_exists, name))
        .collect();

    drops.join("; ")
}

/// Reconstruct TRUNCATE statement for SQLite compatibility
/// SQLite doesn't support TRUNCATE, so we convert it to DELETE FROM statements
pub(crate) fn reconstruct_truncate_stmt(stmt: &TruncateStmt, ctx: &mut TranspileContext) -> String {
    let mut delete_statements: Vec<String> = Vec::new();

    for rel in &stmt.relations {
        if let Some(ref inner) = rel.node {
            if let NodeEnum::RangeVar(rv) = inner {
                let table_name = if rv.schemaname.is_empty() || rv.schemaname == "public" {
                    rv.relname.to_lowercase()
                } else {
                    format!("{}.{}", rv.schemaname.to_lowercase(), rv.relname.to_lowercase())
                };
                ctx.referenced_tables.push(rv.relname.to_lowercase());
                delete_statements.push(format!("delete from {}", table_name));
            }
        }
    }

    if delete_statements.is_empty() {
        return "-- TRUNCATE statement with no tables".to_string();
    }

    delete_statements.join("; ")
}

/// Reconstruct CREATE INDEX statement for SQLite compatibility
pub(crate) fn reconstruct_index_stmt(stmt: &IndexStmt, ctx: &mut TranspileContext) -> String {
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

/// Reconstruct a COPY statement
pub(crate) fn reconstruct_copy_stmt(stmt: &CopyStmt, ctx: &mut TranspileContext) -> Result<TranspileResult, anyhow::Error> {
    use crate::copy::{CopyStatement, CopyDirection, CopyOptions, CopyFormat};

    // Get table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| r.relname.clone())
        .unwrap_or_default();

    if !table_name.is_empty() {
        ctx.referenced_tables.push(table_name.to_lowercase());
    }

    // Determine direction
    let is_from = stmt.is_from;
    let direction = if is_from {
        CopyDirection::From
    } else {
        CopyDirection::To
    };

    // Extract column list
    let mut columns = Vec::new();
    for att_elem in &stmt.attlist {
        if let Some(ref node) = att_elem.node {
            if let NodeEnum::String(s) = node {
                columns.push(s.sval.clone());
            }
        }
    }

    // Parse options from options field
    let mut options = CopyOptions::default();
    for def_elem in &stmt.options {
        if let Some(ref node) = def_elem.node {
            if let NodeEnum::DefElem(def) = node {
                let def_name = def.defname.to_uppercase();
                match def_name.as_str() {
                    "FORMAT" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref inner) = arg.node {
                                if let NodeEnum::String(s) = inner {
                                    let format_str = s.sval.to_uppercase();
                                    match format_str.as_str() {
                                        "TEXT" => {
                                            options.format = CopyFormat::Text;
                                            options.delimiter = '\t';
                                            options.null_string = "\\N".to_string();
                                        }
                                        "CSV" => {
                                            options.format = CopyFormat::Csv;
                                            options.delimiter = ',';
                                            options.null_string = "".to_string();
                                        }
                                        "BINARY" => {
                                            options.format = CopyFormat::Binary;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    "DELIMITER" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref inner) = arg.node {
                                if let NodeEnum::String(s) = inner {
                                    let delim = &s.sval;
                                    if !delim.is_empty() {
                                        options.delimiter = delim.chars().next().unwrap_or('\t');
                                    }
                                }
                            }
                        }
                    }
                    "QUOTE" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref inner) = arg.node {
                                if let NodeEnum::String(s) = inner {
                                    let quote = &s.sval;
                                    if !quote.is_empty() {
                                        options.quote = quote.chars().next().unwrap_or('"');
                                    }
                                }
                            }
                        }
                    }
                    "ESCAPE" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref inner) = arg.node {
                                if let NodeEnum::String(s) = inner {
                                    let escape = &s.sval;
                                    if !escape.is_empty() {
                                        options.escape = escape.chars().next().unwrap_or('\\');
                                    }
                                }
                            }
                        }
                    }
                    "NULL" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref inner) = arg.node {
                                if let NodeEnum::String(s) = inner {
                                    options.null_string = s.sval.clone();
                                }
                            }
                        }
                    }
                    "HEADER" => {
                        options.header = true;
                    }
                    "ENCODING" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref inner) = arg.node {
                                if let NodeEnum::String(s) = inner {
                                    options.encoding = s.sval.clone();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Get query for COPY TO
    let query = if !is_from {
        stmt.query.as_ref().map(|q| {
            // Reconstruct the query
            reconstruct_node(q, ctx)
        })
    } else {
        None
    };

    // Build the COPY statement metadata
    let copy_stmt = CopyStatement {
        table_name: if table_name.is_empty() { None } else { Some(table_name) },
        columns,
        direction,
        options,
        query,
    };

    // Return a special marker SQL that the main handler will recognize
    let marker_sql = format!("-- COPY {:?} {:?}", direction, copy_stmt.table_name.as_deref().unwrap_or("QUERY"));

    Ok(TranspileResult {
        sql: marker_sql,
        create_table_metadata: None,
        copy_metadata: Some(copy_stmt),
        referenced_tables: ctx.referenced_tables.clone(),
        operation_type: OperationType::OTHER,
        errors: Vec::new(),
    })
}

/// Reconstruct an IndexElem (column in an index)
pub(crate) fn reconstruct_index_elem(elem: &pg_query::protobuf::IndexElem, ctx: &mut TranspileContext) -> String {
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
