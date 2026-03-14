//! SQL transpilation from PostgreSQL to SQLite
//!
//! This module provides functionality to parse PostgreSQL SQL statements
//! and transpile them into SQLite-compatible SQL. It handles:
//!
//! - DDL statements (CREATE TABLE, ALTER TABLE, DROP, etc.)
//! - DML statements (SELECT, INSERT, UPDATE, DELETE)
//! - Function calls and expressions
//! - Window functions
//! - Row-Level Security (RLS) policy injection
//!
//! ## Usage
//!
//! ```rust
//! use pgqt::transpiler::{transpile, transpile_with_metadata};
//!
//! // Simple transpilation
//! let sqlite_sql = transpile("SELECT * FROM users WHERE id = 1");
//!
//! // Transpilation with metadata extraction
//! let result = transpile_with_metadata("CREATE TABLE users (id SERIAL PRIMARY KEY)");
//! println!("SQL: {}", result.sql);
//! println!("Operation: {:?}", result.operation_type);
//! ```

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::Node;

#[allow(unused_imports)]
use crate::copy::{CopyStatement, CopyDirection, CopyOptions, CopyFormat};


// Submodules
pub mod registry;
pub mod context;
mod utils;
pub mod ddl;
pub mod dml;
pub mod expr;
pub mod func;
pub mod rls;
pub mod window;

// Re-exports from context
pub mod metadata;

// Re-export metadata types
#[allow(unused_imports)]
pub use metadata::{ColumnInfo, MetadataProvider, NoOpMetadataProvider};
pub use context::{
    OperationType, 
    TranspileContext, 
    TranspileResult
};

// Re-export public functions
pub use func::parse_create_function;
pub use ddl::{parse_create_trigger, parse_drop_trigger};

/// Extract column aliases from a SELECT statement's target list
fn extract_column_aliases_from_select(select_stmt: &pg_query::protobuf::SelectStmt) -> Vec<String> {
    use pg_query::protobuf::node::Node as NodeEnum;
    
    let mut aliases = Vec::new();
    
    for target in &select_stmt.target_list {
        if let Some(ref inner) = target.node {
            if let NodeEnum::ResTarget(res_target) = inner {
                if !res_target.name.is_empty() {
                    aliases.push(res_target.name.clone());
                } else {
                    aliases.push(String::new());
                }
            } else {
                aliases.push(String::new());
            }
        } else {
            aliases.push(String::new());
        }
    }
    
    aliases
}

/// Transpile PostgreSQL SQL to SQLite SQL using AST walking
/// Returns both the transpiled SQL and any extracted metadata
pub fn transpile_with_metadata(sql: &str) -> TranspileResult {
    let mut ctx = TranspileContext::new();
    transpile_with_context(sql, &mut ctx)
}

/// Transpile with a specific context (useful for function inlining)
pub fn transpile_with_context(sql: &str, ctx: &mut TranspileContext) -> TranspileResult {
    match pg_query::parse(sql) {
        Ok(result) => {
            if let Some(raw_stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = raw_stmt.stmt {
                    return reconstruct_sql_with_metadata(stmt_node, ctx);
                }
            }

            TranspileResult {
                sql: sql.to_lowercase(),
                create_table_metadata: None,
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::OTHER,
                errors: ctx.errors.clone(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            }
        }
        Err(_) => {
            // Fallback: basic normalization
            TranspileResult {
                sql: sql.to_lowercase().replace("now()", "datetime('now')"),
                create_table_metadata: None,
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::OTHER,
                errors: ctx.errors.clone(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
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
    let mut result = if let Some(ref inner) = node.node {
        match inner {
            NodeEnum::SelectStmt(ref select_stmt) => {
                let sql = dml::reconstruct_select_stmt(select_stmt, ctx);
                let column_aliases = extract_column_aliases_from_select(select_stmt);
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::SELECT,
                    errors: Vec::new(),
                    column_aliases,
                    column_types: Vec::new(),
                }
            }
            NodeEnum::DefineStmt(ref define_stmt) => TranspileResult {
                sql: ddl::reconstruct_define_stmt(define_stmt, ctx),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::CreateEnumStmt(ref create_enum_stmt) => TranspileResult {
                sql: ddl::reconstruct_create_enum_stmt(create_enum_stmt, ctx),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::CreateStmt(ref create_stmt) => {
                let mut res = ddl::reconstruct_create_stmt_with_metadata(create_stmt, ctx);
                res.operation_type = OperationType::DDL;
                res
            }
            NodeEnum::InsertStmt(ref insert_stmt) => TranspileResult {
                sql: dml::reconstruct_insert_stmt(insert_stmt, ctx),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: ctx.referenced_tables.clone(),
                operation_type: OperationType::INSERT,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::UpdateStmt(ref update_stmt) => TranspileResult {
                sql: dml::reconstruct_update_stmt(update_stmt, ctx),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: ctx.referenced_tables.clone(),
                operation_type: OperationType::UPDATE,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::DeleteStmt(ref delete_stmt) => TranspileResult {
                sql: dml::reconstruct_delete_stmt(delete_stmt, ctx),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: ctx.referenced_tables.clone(),
                operation_type: OperationType::DELETE,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::VariableSetStmt(ref set_stmt) => {
                // Handle SET ROLE
                if set_stmt.name == "role" && !set_stmt.args.is_empty() {
                    if let Some(ref node) = set_stmt.args[0].node {
                        if let NodeEnum::AConst(ref aconst) = node {
                            if let Some(ref val) = aconst.val {
                                if let pg_query::protobuf::a_const::Val::Sval(ref s) = val {
                                    return TranspileResult {
                                        sql: format!("-- SET ROLE {}", s.sval),
                                        create_table_metadata: None, 
                                        copy_metadata: None,
                                        referenced_tables: Vec::new(),
                                        operation_type: OperationType::OTHER,
                                        errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                                    };
                                }
                            }
                        }
                    }
                }
                TranspileResult {
                    sql: "select 1".to_string(), 
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::OTHER,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::VariableShowStmt(ref show_stmt) => {
                let name = show_stmt.name.to_lowercase();
                let sql = if name == "all" {
                    // SHOW ALL returns all settings as name/value pairs
                    // Format matches PostgreSQL's SHOW ALL output
                    r#"select 'search_path' as name, '"$user", public' as setting, 'Sets the schema search order for names that are not schema-qualified.' as description
                        union all select 'server_version', '15.0', 'Shows the server version.'
                        union all select 'server_version_num', '150000', 'Shows the server version as an integer.'
                        union all select 'timezone', 'UTC', 'Sets the time zone for displaying and interpreting time stamps.'
                        union all select 'transaction_isolation', 'read committed', 'Sets the current transaction isolation level.'
                        union all select 'default_transaction_read_only', 'off', 'Sets the default read-only status of new transactions.'
                        union all select 'statement_timeout', '0', 'Sets the maximum allowed duration of any statement.'
                        union all select 'client_encoding', 'UTF8', 'Sets the client-side encoding (character set).'
                        union all select 'application_name', '', 'Sets the application name to be reported in statistics and logs.'
                        union all select 'DateStyle', 'ISO, MDY', 'Sets the display format for date and time values.'
                        union all select 'standard_conforming_strings', 'on', 'Causes ''...'' strings to treat backslashes literally.'"#.to_string()
                } else {
                    format!("select current_setting('{}') as {}", show_stmt.name, show_stmt.name)
                };
                TranspileResult {
                    sql,
                    create_table_metadata: None,
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::SELECT,
                    errors: Vec::new(),
                    column_aliases: Vec::new(),
                    column_types: Vec::new(),
                }
            }
            NodeEnum::CreateRoleStmt(ref create_role_stmt) => TranspileResult {
                sql: rls::reconstruct_create_role_stmt(create_role_stmt, ctx),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::AlterRoleStmt(ref alter_role_stmt) => TranspileResult {
                sql: rls::reconstruct_alter_role_stmt(alter_role_stmt, ctx),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::DropRoleStmt(ref drop_role_stmt) => TranspileResult {
                sql: rls::reconstruct_drop_role_stmt(drop_role_stmt),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::GrantStmt(ref grant_stmt) => TranspileResult {
                sql: rls::reconstruct_grant_stmt(grant_stmt),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::GrantRoleStmt(ref grant_role_stmt) => TranspileResult {
                sql: rls::reconstruct_grant_role_stmt(grant_role_stmt),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::AlterTableStmt(ref alter_stmt) => {
                let sql = ddl::reconstruct_alter_table_stmt(alter_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::DropStmt(ref drop_stmt) => {
                let sql = ddl::reconstruct_drop_stmt(drop_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::IndexStmt(ref index_stmt) => {
                let sql = ddl::reconstruct_index_stmt(index_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::CopyStmt(ref copy_stmt) => {
                match ddl::reconstruct_copy_stmt(copy_stmt, ctx) {
                    Ok(result) => result,
                    Err(e) => TranspileResult {
                        sql: format!("-- COPY ERROR: {}", e),
                        create_table_metadata: None, 
                        copy_metadata: None,
                        referenced_tables: Vec::new(),
                        operation_type: OperationType::OTHER,
                        errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                    }
                }
            }
            NodeEnum::TruncateStmt(ref truncate_stmt) => {
                let sql = ddl::reconstruct_truncate_stmt(truncate_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::ViewStmt(ref view_stmt) => {
                let sql = ddl::reconstruct_view_stmt(view_stmt, ctx);
                TranspileResult {
                    sql,
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: ctx.referenced_tables.clone(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::VacuumStmt(ref _vacuum_stmt) => {
                TranspileResult {
                    sql: "VACUUM".to_string(),
                    create_table_metadata: None,
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::OTHER,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::CommentStmt(ref comment_stmt) => {
                let (obj_type, obj_name) = ddl::extract_comment_object_info(comment_stmt);
                TranspileResult {
                    sql: format!("select __pg_comment_on('{}', '{}', '{}')", obj_type, obj_name.replace('\'', "''"), comment_stmt.comment.replace('\'', "''")),
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::SELECT,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::AlterDefaultPrivilegesStmt(ref stmt) => {
                TranspileResult {
                    sql: rls::reconstruct_alter_default_privileges_stmt(stmt),
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::AlterOwnerStmt(ref stmt) => TranspileResult {
                sql: rls::reconstruct_alter_owner_stmt(stmt),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::DDL,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
            NodeEnum::CreatePolicyStmt(_) => {
                TranspileResult {
                    sql: format!("-- CREATE POLICY IGNORED"),
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::CreateTrigStmt(ref trig_stmt) => {
                // Parse the trigger to get its name and table
                let trigger_name = trig_stmt.trigname.clone();
                let table_name = trig_stmt.relation.as_ref()
                    .map(|r| r.relname.clone())
                    .unwrap_or_default();
                
                TranspileResult {
                    sql: format!("-- CREATE TRIGGER: {} ON {}", trigger_name, table_name),
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::DDL,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            NodeEnum::DoStmt(ref do_stmt) => {
                let code = ddl::extract_do_block_code(do_stmt).unwrap_or_else(|| "BEGIN END;".to_string());
                TranspileResult {
                    sql: format!("select __pg_do_block('{}')", code.replace('\'', "''")),
                    create_table_metadata: None, 
                    copy_metadata: None,
                    referenced_tables: Vec::new(),
                    operation_type: OperationType::SELECT,
                    errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
                }
            }
            _ => TranspileResult {
                sql: node.deparse().unwrap_or_else(|_| "".to_string()).to_lowercase(),
                create_table_metadata: None, 
                copy_metadata: None,
                referenced_tables: Vec::new(),
                operation_type: OperationType::OTHER,
                errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
            },
        }
    } else {
        TranspileResult {
            sql: String::new(),
            create_table_metadata: None, 
            copy_metadata: None,
            referenced_tables: Vec::new(),
            operation_type: OperationType::OTHER,
            errors: Vec::new(),
                column_aliases: Vec::new(),
                column_types: Vec::new(),
        }
    };

    result.errors.extend(ctx.errors.clone());
    result
}

// Re-export reconstruct_node from expr module for use by other modules
pub(crate) use expr::reconstruct_node;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_table_cascade() {
        // CASCADE should be stripped for SQLite compatibility
        let result = transpile_with_metadata("DROP TABLE IF EXISTS test_jsonb CASCADE");
        assert_eq!(result.sql, "drop table if exists test_jsonb");
        assert_eq!(result.operation_type, OperationType::DDL);
    }

    #[test]
    fn test_drop_table_restrict() {
        // RESTRICT should be stripped for SQLite compatibility
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
        assert_eq!(result.sql, "drop table table1; drop table table2");
    }

    #[test]
    fn test_drop_multiple_tables_if_exists() {
        let result = transpile_with_metadata("DROP TABLE IF EXISTS table1, table2");
        assert_eq!(result.sql, "drop table if exists table1; drop table if exists table2");
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
        // IF NOT EXISTS should be preserved
        let result = transpile_with_metadata("CREATE TABLE IF NOT EXISTS my_table (id INTEGER PRIMARY KEY)");
        assert!(result.sql.contains("create table"));
        assert!(result.sql.contains("my_table"));
    }

    #[test]
    fn test_insert_with_array_expr() {
        let sql = "INSERT INTO test_jsonb(name, tags, props) VALUES ('Alice', ARRAY['dev', 'remote'], '{\"age\": 30}')";
        let result = transpile_with_metadata(sql);

        // Should contain the INSERT statement
        assert!(result.sql.contains("insert into test_jsonb"));
        // Array should be converted to JSON format
        assert!(!result.sql.contains(", ,"), "Array should not be empty: {}", result.sql);
        // Check for JSON array format
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

        // Should contain the INSERT statement
        assert!(result.sql.contains("insert into test_jsonb"));
        // Arrays should be converted to JSON format
        assert!(!result.sql.contains(", ,"), "Arrays should not be empty: {}", result.sql);
        // Check for JSON array format
        assert!(result.sql.contains("'[\"dev\",\"remote\"]'") || result.sql.contains("'[\"qa\",\"onsite\"]'"),
                "Arrays should be converted to JSON: {}", result.sql);
    }

    #[test]
    fn test_jsonb_key_exists_operator() {
        // PostgreSQL ? operator (key exists)
        let result = transpile_with_metadata("SELECT id, name FROM test_jsonb WHERE props ? 'team'");
        // Should use json_type for ? operator
        assert!(result.sql.contains("json_type"), "Should use json_type for ? operator: {}", result.sql);
        assert!(result.sql.contains("IS NOT NULL"), "Should check IS NOT NULL: {}", result.sql);
    }

    #[test]
    fn test_jsonb_any_key_exists_operator() {
        // PostgreSQL ?| operator (any key exists)
        let result = transpile_with_metadata("SELECT id, name FROM test_jsonb WHERE props ?| ARRAY['skills', 'hobbies']");
        // Should use EXISTS for ?| operator
        assert!(result.sql.contains("EXISTS"), "Should use EXISTS for ?| operator: {}", result.sql);
        assert!(result.sql.contains("json_each"), "Should use json_each for ?| operator: {}", result.sql);
    }

    #[test]
    fn test_jsonb_all_keys_exist_operator() {
        // PostgreSQL ?& operator (all keys exist)
        let result = transpile_with_metadata("SELECT id, name FROM test_jsonb WHERE props ?& ARRAY['skills', 'hobbies']");
        // Should use NOT EXISTS for ?& operator
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

        assert!(result.sql.contains("json_each"), "Should use json_each: {}", result.sql);
        assert!(!result.sql.to_uppercase().contains("LATERAL"), "Should not contain LATERAL keyword: {}", result.sql);
        assert!(result.errors.is_empty(), "Should have no errors for function lateral: {:?}", result.errors);
    }

    #[test]
    fn test_lateral_subquery_error() {
        let result = transpile_with_metadata("SELECT * FROM (SELECT 1 as x) a, LATERAL (SELECT a.x + 1 as y) b");
        assert!(!result.errors.is_empty(), "Should have errors for lateral subquery");
        assert!(result.errors[0].contains("LATERAL subqueries are not yet supported"), "Error message should be correct: {}", result.errors[0]);
    }

    #[test]
    fn test_jsonb_remove_array() {
        let result = transpile_with_metadata("SELECT props - ARRAY['age', 'active'] AS reduced FROM test_jsonb");
        println!("Transpiled remove array: {}", result.sql);

        assert!(result.sql.contains("json_remove"), "Should use json_remove: {}", result.sql);
    }

    #[test]
    fn test_offset_without_limit() {
        // SQLite requires LIMIT when using OFFSET
        let result = transpile_with_metadata("SELECT 1 OFFSET 0");
        println!("Transpiled OFFSET without LIMIT: {}", result.sql);
        assert!(result.sql.contains("limit"), "Should add LIMIT when OFFSET is present: {}", result.sql);
        assert!(result.sql.contains("offset"), "Should contain OFFSET: {}", result.sql);
    }

    #[test]
    fn test_subquery_with_offset() {
        // Test that subquery with OFFSET works correctly
        let result = transpile_with_metadata("SELECT foo FROM (SELECT 1 OFFSET 0) AS foo");
        println!("Transpiled subquery with OFFSET: {}", result.sql);
        assert!(result.sql.contains("limit"), "Should add LIMIT in subquery: {}", result.sql);
        // The column should be accessible as 'foo' from the outer query
        // The transpiled SQL should be: select foo from (select 1 limit -1 offset 0) as foo
        assert!(result.sql.contains("as foo"), "Should have alias 'foo': {}", result.sql);
    }

    #[test]
    fn test_range_contains_single_value() {
        // Test that r @> '15' uses range_contains, not array_contains
        let result = transpile_with_metadata("SELECT id FROM test_ranges WHERE r @> '15'");
        println!("Transpiled range contains: {}", result.sql);
        assert!(result.sql.contains("range_contains"), "Should use range_contains for single value: {}", result.sql);
        assert!(!result.sql.contains("array_contains"), "Should NOT use array_contains: {}", result.sql);
    }

    #[test]
    fn test_update_row_constructor() {
        let sql = "UPDATE t SET (a, b) = (1, 2)";
        let result = transpile_with_metadata(sql);
        assert_eq!(result.sql, "update t set a = 1, b = 2");
    }
}
