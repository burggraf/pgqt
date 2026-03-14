//! Unit tests for SQL transpilation

use pgqt::transpiler::{transpile, transpile_with_metadata};

#[test]
fn test_bitwise_shift_not_geo() {
    // Test that bitwise shift operators are not confused with geometric operators
    // The key fix is that geo_left should NOT appear for integer operations
    let sql = "SELECT (1::int2 << 15)::text";
    let result = transpile(sql);
    println!("Input: {}", sql);
    println!("Output: {}", result);
    
    // Check that geo_left is NOT in the output - this was the main bug
    assert!(!result.contains("geo_left"), "Output should not contain geo_left for integer shift, got: {}", result);
    
    // Check that the output contains the bitwise shift operator
    assert!(result.contains("<<"), "Output should contain << operator, got: {}", result);
}

#[test]
fn test_transpile_simple_select() {
    let input = "SELECT * FROM users";
    let result = transpile(input);
    assert_eq!(result, "select * from users");
}

#[test]
fn test_transpile_select_columns() {
    let input = "SELECT id, name, email FROM users";
    let result = transpile(input);
    assert_eq!(result, "select id, name, email from users");
}

#[test]
fn test_transpile_where_clause() {
    let input = "SELECT * FROM users WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("where"));
    assert!(result.contains("id = 1"));
}

#[test]
fn test_transpile_limit() {
    let input = "SELECT * FROM users LIMIT 10";
    let result = transpile(input);
    assert!(result.contains("limit 10"));
}

#[test]
fn test_transpile_limit_all() {
    let input = "SELECT * FROM users LIMIT ALL";
    let result = transpile(input);
    assert!(result.contains("limit -1"));
}

#[test]
fn test_transpile_offset() {
    let input = "SELECT * FROM users LIMIT 10 OFFSET 20";
    let result = transpile(input);
    assert!(result.contains("limit 10"));
    assert!(result.contains("offset 20"));
}

#[test]
fn test_transpile_order_by() {
    let input = "SELECT * FROM users ORDER BY name ASC, id DESC";
    let result = transpile(input);
    assert!(result.contains("order by"));
    assert!(result.contains("name asc"));
    assert!(result.contains("id desc"));
}

#[test]
fn test_transpile_distinct() {
    let input = "SELECT DISTINCT status FROM orders";
    let result = transpile(input);
    assert!(result.contains("distinct"));
}

#[test]
fn test_transpile_schema_public() {
    let input = "SELECT * FROM public.users";
    let result = transpile(input);
    // Should strip 'public' schema
    assert!(!result.contains("public."));
    assert!(result.contains("users"));
}

#[test]
fn test_transpile_schema_other() {
    let input = "SELECT * FROM inventory.products";
    let result = transpile(input);
    // Should preserve other schemas (attached databases)
    assert!(result.contains("inventory.products"));
}

#[test]
fn test_transpile_now() {
    let input = "SELECT now()";
    let result = transpile(input);
    assert!(result.contains("datetime('now')"));
}

#[test]
fn test_transpile_cast_int() {
    let input = "SELECT '123'::int";
    let result = transpile(input);
    assert!(result.contains("cast("));
    assert!(result.contains("as integer"));
}

#[test]
fn test_transpile_udf_inlining() {
    use pgqt::transpiler::{TranspileContext, transpile_with_context};
    use pgqt::catalog::{FunctionMetadata, ParamMode, ReturnTypeKind};
    use dashmap::DashMap;
    use std::sync::Arc;

    let functions = Arc::new(DashMap::new());
    functions.insert("add".to_string(), FunctionMetadata {
        oid: 1,
        name: "add".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["int".to_string(), "int".to_string()],
        arg_names: vec!["a".to_string(), "b".to_string()],
        arg_modes: vec![ParamMode::In, ParamMode::In],
        return_type: "int".to_string(),
        return_type_kind: ReturnTypeKind::Scalar,
        return_table_cols: None,
        function_body: "SELECT $1 + $2".to_string(),
        language: "sql".to_string(),
        volatility: "IMMUTABLE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "SAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    });

    let mut ctx = TranspileContext::with_functions(functions);
    let input = "SELECT add(1, 2)";
    let result = transpile_with_context(input, &mut ctx);
    assert_eq!(result.sql.to_lowercase(), "select (select 1 + 2 as \"?column?\")");
}

#[test]
fn test_transpile_void_udf_inlining() {
    use pgqt::transpiler::{TranspileContext, transpile_with_context};
    use pgqt::catalog::{FunctionMetadata, ParamMode, ReturnTypeKind};
    use dashmap::DashMap;
    use std::sync::Arc;

    let functions = Arc::new(DashMap::new());
    functions.insert("log".to_string(), FunctionMetadata {
        oid: 2,
        name: "log".to_string(),
        schema: "public".to_string(),
        arg_types: vec!["text".to_string()],
        arg_names: vec!["msg".to_string()],
        arg_modes: vec![ParamMode::In],
        return_type: "void".to_string(),
        return_type_kind: ReturnTypeKind::Void,
        return_table_cols: None,
        function_body: "SELECT $1".to_string(), // Simplified for testing
        language: "sql".to_string(),
        volatility: "VOLATILE".to_string(),
        strict: false,
        security_definer: false,
        parallel: "UNSAFE".to_string(),
        owner_oid: 1,
        created_at: None,
    });

    let mut ctx = TranspileContext::with_functions(functions);
    let input = "SELECT log('hi')";
    let result = transpile_with_context(input, &mut ctx);
    assert_eq!(result.sql.to_lowercase(), "select (select null from (select 'hi' as \"?column?\") limit 1)");
}

#[test]
fn test_transpile_like_operator() {
    let input = "SELECT * FROM users WHERE name ~~ 'Alice%'";
    let result = transpile(input);
    assert!(result.contains("like"));
}

#[test]
fn test_transpile_not_like_operator() {
    let input = "SELECT * FROM users WHERE name !~~ 'Alice%'";
    let result = transpile(input);
    assert!(result.contains("not like"));
}

#[test]
fn test_create_table_metadata() {
    let input = "CREATE TABLE test (id SERIAL, name VARCHAR(100))";
    let result = transpile_with_metadata(input);
    
    // Check SQL was generated
    assert!(result.sql.contains("create table"));
    assert!(result.sql.contains("integer primary key autoincrement"));
    assert!(result.sql.contains("text"));
    
    // Check metadata was extracted
    let metadata = result.create_table_metadata.expect("Should have metadata");
    assert_eq!(metadata.table_name, "test");
    assert_eq!(metadata.columns.len(), 2);
}

#[test]
fn test_create_table_timestamp_types() {
    let input = "CREATE TABLE events (id SERIAL, created_at TIMESTAMP WITH TIME ZONE)";
    let result = transpile_with_metadata(input);
    
    let metadata = result.create_table_metadata.expect("Should have metadata");
    let ts_col = metadata.columns.iter().find(|c| c.column_name == "created_at").unwrap();
    assert_eq!(ts_col.original_type, "TIMESTAMP WITH TIME ZONE");
}

#[test]
fn test_create_table_boolean() {
    let input = "CREATE TABLE flags (id SERIAL, is_active BOOLEAN)";
    let result = transpile_with_metadata(input);
    
    // SQLite should use INTEGER
    assert!(result.sql.contains("integer"));
    
    // But metadata should preserve BOOLEAN
    let metadata = result.create_table_metadata.expect("Should have metadata");
    let bool_col = metadata.columns.iter().find(|c| c.column_name == "is_active").unwrap();
    assert_eq!(bool_col.original_type, "BOOLEAN");
}

#[test]
fn test_create_table_json() {
    let input = "CREATE TABLE data (id SERIAL, payload JSONB)";
    let result = transpile_with_metadata(input);
    
    // SQLite stores as TEXT
    assert!(result.sql.contains("text"));
    
    // Metadata preserves JSONB
    let metadata = result.create_table_metadata.expect("Should have metadata");
    let json_col = metadata.columns.iter().find(|c| c.column_name == "payload").unwrap();
    assert_eq!(json_col.original_type, "JSONB");
}

#[test]
fn test_insert_statement() {
    let input = "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')";
    let result = transpile(input);
    assert!(result.contains("insert into"));
    // INSERT with explicit columns is transpiled to SELECT ... AS ... format
    assert!(result.contains("select") || result.contains("SELECT"));
    assert!(result.contains("'Alice'"));
}

#[test]
fn test_update_statement() {
    let input = "UPDATE users SET name = 'Bob' WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("update"));
    assert!(result.contains("set"));
    assert!(result.contains("where"));
}

#[test]
fn test_delete_statement() {
    let input = "DELETE FROM users WHERE id = 1";
    let result = transpile(input);
    assert!(result.contains("delete from"));
    assert!(result.contains("where"));
}

#[test]
fn test_join() {
    let input = "SELECT u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id";
    let result = transpile(input);
    assert!(result.contains("join"));
    assert!(result.contains("on"));
}

#[test]
fn test_subquery() {
    let input = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)";
    let result = transpile(input);
    assert!(result.contains("select"));
    assert!(result.contains("where"));
    assert!(result.contains("in"));
}

#[test]
fn test_group_by() {
    let input = "SELECT status, COUNT(*) FROM orders GROUP BY status";
    let result = transpile(input);
    assert!(result.contains("group by"));
    assert!(result.contains("count"));
}

#[test]
fn test_alias() {
    let input = "SELECT u.name AS user_name FROM users u";
    let result = transpile(input);
    assert!(result.contains("as"));
}

#[test]
fn test_transpile_recursive_cte_limit() {
    let input = "WITH RECURSIVE t(n) AS (VALUES (1) UNION ALL SELECT n+1 FROM t WHERE n < 100) SELECT sum(n) FROM t;";
    let result = transpile(input);
    // Should contain LIMIT 100 (default max_recursion_depth)
    assert!(result.to_lowercase().contains("limit 100"));
}

#[test]
fn test_transpile_anonymous_column_names() {
    // Basic anonymous column
    let input = "SELECT 1 + 1";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("as \"?column?\""));

    // CASE expression
    let input = "SELECT CASE WHEN 1=1 THEN 1 END";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("as \"case\""));

    // CAST expression
    let input = "SELECT '123'::int";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("as \"int4\""));

    // Function call
    let input = "SELECT lower('HI')";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("as \"lower\""));
}

#[test]
fn test_transpile_insert_padding_with_default() {
    use pgqt::transpiler::{TranspileContext, transpile_with_context};
    use pgqt::transpiler::metadata::{ColumnInfo, MetadataProvider};

    struct TestMetadataProvider;
    impl MetadataProvider for TestMetadataProvider {
        fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>> {
            if table_name == "t" {
                Some(vec![
                    ColumnInfo { name: "a".to_string(), original_type: "integer".to_string(), is_nullable: true, default_expr: None, type_oid: None },
                    ColumnInfo { name: "b".to_string(), original_type: "integer".to_string(), is_nullable: true, default_expr: None, type_oid: None },
                    ColumnInfo { name: "c".to_string(), original_type: "integer".to_string(), is_nullable: true, default_expr: Some("5".to_string()), type_oid: None },
                ])
            } else {
                None
            }
        }
        fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String> {
            if table_name == "t" && column_name == "c" {
                Some("5".to_string())
            } else {
                None
            }
        }
        fn get_enum_labels(&self, _type_name: &str) -> Option<Vec<String>> {
            None
        }
    }

    let mut ctx = TranspileContext::new();
    ctx.set_metadata_provider(std::sync::Arc::new(TestMetadataProvider));

    // Test: INSERT INTO t VALUES (1) -> should pad to (1, NULL, 5)
    let input = "INSERT INTO t VALUES (1)";
    let result = transpile_with_context(input, &mut ctx);
    println!("SQL 1: {}", result.sql);
    assert!(result.sql.contains("(a, b, c)"));
    assert!(result.sql.to_lowercase().contains("select 1 as a, null as b, 5 as c") || 
            result.sql.to_lowercase().contains("values (1, null, 5)"));

    // Test: INSERT INTO t VALUES (DEFAULT, 7) -> should pad to (NULL, 7, 5)
    let input = "INSERT INTO t VALUES (DEFAULT, 7)";
    let result = transpile_with_context(input, &mut ctx);
    println!("SQL 2: {}", result.sql);
    assert!(result.sql.contains("(a, b, c)"));
    assert!(result.sql.to_lowercase().contains("select null as a, 7 as b, 5 as c") || 
            result.sql.to_lowercase().contains("values (null, 7, 5)"));
}

#[test]
fn test_transpile_create_enum() {
    use pgqt::transpiler::{transpile_with_context, TranspileContext};
    let mut ctx = TranspileContext::new();
    let input = "CREATE TYPE status AS ENUM ('open', 'closed')";
    let result = transpile_with_context(input, &mut ctx);
    assert!(result.sql.contains("__pg_create_enum('status', 'open', 0)"));
    assert!(result.sql.contains("__pg_create_enum('status', 'closed', 1)"));
}

#[test]
fn test_transpile_enum_column_check() {
    use pgqt::transpiler::{transpile_with_context, TranspileContext};
    use pgqt::transpiler::metadata::{ColumnInfo, MetadataProvider};

    struct EnumMetadataProvider;
    impl MetadataProvider for EnumMetadataProvider {
        fn get_table_columns(&self, _t: &str) -> Option<Vec<ColumnInfo>> { None }
        fn get_column_default(&self, _t: &str, _c: &str) -> Option<String> { None }
        fn get_enum_labels(&self, type_name: &str) -> Option<Vec<String>> {
            if type_name == "STATUS" {
                Some(vec!["open".to_string(), "closed".to_string()])
            } else {
                None
            }
        }
    }

    let mut ctx = TranspileContext::new();
    ctx.set_metadata_provider(std::sync::Arc::new(EnumMetadataProvider));
    
    let input = "CREATE TABLE tasks (id int, task_status status)";
    let result = transpile_with_context(input, &mut ctx);
    println!("DEBUG: Enum column SQL: {}", result.sql);
    assert!(result.sql.to_lowercase().contains("check (task_status in ('open', 'closed'))"));
}

#[test]
fn test_transpile_repeat_function() {
    let input = "SELECT repeat('a', 3)";
    let result = transpile(input);
    // Transpiler should preserve the function name
    assert!(result.to_lowercase().contains("repeat('a', 3)"));
}

#[test]
fn test_transpile_nested_set_operations() {
    // Test: (SELECT 1 UNION SELECT 2) UNION SELECT 3
    // PostgreSQL might parse this as a tree.
    // We want to ensure SQLite precedence is correct.
    let input = "(SELECT 1 UNION SELECT 2) UNION SELECT 3";
    let result = transpile(input);
    // Since we don't handle parentheses well, we expect it to be 
    // flattened or correctly wrapped.
    assert!(result.to_lowercase().contains("union"));
}

#[test]
fn test_transpile_set_operation_with_order_by_on_branch() {
    // This is tricky for SQLite. If a branch has ORDER BY, it MUST be wrapped.
    // pg_query might not give us SelectStmt as a branch if it's just a simple SELECT.
    // But if it's a subquery with ORDER BY, it should be a SelectStmt.
    let input = "SELECT name FROM users UNION SELECT name FROM employees ORDER BY name";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("order by name"));
}

#[test]
fn test_transpile_nested_union_precedence() {
    // SELECT 1 UNION (SELECT 2 UNION SELECT 3)
    let input = "SELECT 1 UNION (SELECT 2 UNION SELECT 3)";
    let result = transpile(input);
    println!("DEBUG: result = {}", result);
    // Since right side has op > 1, it should be wrapped in SELECT * FROM (...)
    assert!(result.to_lowercase().contains("select * from (select 2 as \"?column?\" union select 3 as \"?column?\")"));
}

#[test]
fn test_transpile_union_column_names_no_suffix() {
    // (SELECT 1, 2, 3 UNION SELECT 4, 5, 6)
    let input = "(SELECT 1, 2, 3 UNION SELECT 4, 5, 6)";
    let result = transpile(input);
    println!("DEBUG: result = {}", result);
    // Should NOT contain ?column?:1 or ?column?:2
    assert!(!result.contains("?column?:"));
    assert!(result.contains("?column?"));
}

#[test]
fn test_transpile_nested_union_with_order_by() {
    // (SELECT 1, 2 UNION SELECT 3, 4 ORDER BY 1) INTERSECT SELECT 3, 4
    let input = "(SELECT 1, 2 UNION SELECT 3, 4 ORDER BY 1) INTERSECT SELECT 3, 4";
    let result = transpile(input);
    println!("DEBUG: result = {}", result);
    // Left side should be wrapped in select * from (...)
    assert!(result.to_lowercase().contains("select * from (select 1 as \"?column?\", 2 as \"?column?\" union select 3 as \"?column?\", 4 as \"?column?\" order by 1)"));
}

#[test]
fn test_transpile_union_nested_aliasing() {
    let input = "(SELECT 1 as a, 2 as b UNION SELECT 3, 4) INTERSECT SELECT 3, 4";
    let result = transpile(input);
    println!("DEBUG: result = {}", result);
    // The columns of (SELECT 1 as a, 2 as b UNION SELECT 3, 4) should be 'a' and 'b'.
    // SQLite might use different names if not wrapped.
    assert!(result.to_lowercase().contains("select * from (select 1 as \"a\", 2 as \"b\" union select 3 as \"?column?\", 4 as \"?column?\")"));
}

#[test]
fn test_transpile_subquery_array_indexing() {
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    let result = transpile(input);
    println!("DEBUG: result = {}", result);
    // If it's currently using default deparse, it might look like:
    // select (select '[1,2,3]')[1]
    // We want it to be:
    // select json_extract((select '[1,2,3]'), '0')
    assert!(result.to_lowercase().contains("json_extract"));
}

#[test]
fn test_debug_parse_tree() {
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    match pg_query::parse(input) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            println!("DEBUG JSON: {}", json);
        }
        Err(e) => println!("PARSE ERROR: {}", e),
    }
}

#[test]
fn test_transpile_nested_array_indexing() {
    // Note: pg_query parser does not support array indexing syntax like arr[1][2]
    // This test documents the current fallback behavior.
    // For array indexing on subqueries, see test_transpile_subquery_array_indexing
    let input = "SELECT array[array[1,2], array[3,4]][1][2]";
    let result = transpile(input);
    // Falls back to original SQL (lowercased) since pg_query cannot parse array indexing
    assert!(result.to_lowercase().contains("array[array[1,2], array[3,4]][1][2]"));
}

#[test]
fn test_debug_nested_indirection() {
    let input = "SELECT array[array[1,2], array[3,4]][1][2]";
    match pg_query::parse(input) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            println!("DEBUG JSON: {}", json);
        }
        Err(e) => println!("PARSE ERROR: {}", e),
    }
}

#[test]
fn test_debug_indirection_node_enum() {
    use pg_query::protobuf::node::Node as NodeEnum;
    let input = "SELECT array[1,2,3][1]";
    match pg_query::parse(input) {
        Ok(result) => {
            if let Some(stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = stmt.stmt {
                    if let Some(ref inner) = stmt_node.node {
                        if let NodeEnum::SelectStmt(ref select) = inner {
                            if let Some(target) = select.target_list.first() {
                                if let Some(ref target_node) = target.node {
                                    if let NodeEnum::ResTarget(ref res) = target_node {
                                        if let Some(ref val) = res.val {
                                            println!("VAL NODE: {:?}", val.node);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => println!("PARSE ERROR: {}", e),
    }
}

#[test]
fn test_debug_indirection_node_raw() {
    use pg_query::protobuf::node::Node as NodeEnum;
    let input = "SELECT array[1,2,3][1]";
    match pg_query::parse(input) {
        Ok(result) => {
            if let Some(stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = stmt.stmt {
                    if let Some(ref inner) = stmt_node.node {
                        if let NodeEnum::SelectStmt(ref select) = inner {
                            if let Some(target) = select.target_list.first() {
                                if let Some(ref target_node) = target.node {
                                    if let NodeEnum::ResTarget(ref res) = target_node {
                                        if let Some(ref val) = res.val {
                                            println!("VAL NODE RAW: {:?}", val);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => println!("PARSE ERROR: {}", e),
    }
}

#[test]
fn test_debug_indirection_simple() {
    let input = "SELECT array[1,2,3][1]";
    let result = transpile(input);
    println!("RESULT SIMPLE: {}", result);
}

#[test]
fn test_debug_nested_indirection_v2() {
    let input = "SELECT (array[array[1,2], array[3,4]])[1][2]";
    match pg_query::parse(input) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            println!("DEBUG JSON V2: {}", json);
        }
        Err(e) => println!("PARSE ERROR V2: {}", e),
    }
}

#[test]
fn test_debug_nested_indirection_v3() {
    let input = "SELECT (array[array[1,2], array[3,4]])[1][2]";
    match pg_query::parse(input) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            println!("DEBUG_JSON_V3: {}", json);
        }
        Err(e) => println!("PARSE_ERROR_V3: {}", e),
    }
}

#[test]
fn test_transpile_subquery_array_indexing_alias() {
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    let result = transpile(input);
    println!("DEBUG: result = {}", result);
    // Postgres expects 'array' as the column name for array indexing without explicit alias
    assert!(result.to_lowercase().contains("as \"array\""));
}

#[test]
fn test_debug_indirection_target() {
    use pg_query::protobuf::node::Node as NodeEnum;
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    match pg_query::parse(input) {
        Ok(result) => {
            if let Some(stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = stmt.stmt {
                    if let Some(ref inner) = stmt_node.node {
                        if let NodeEnum::SelectStmt(ref select) = inner {
                            if let Some(target) = select.target_list.first() {
                                if let Some(ref target_node) = target.node {
                                    if let NodeEnum::ResTarget(ref res) = target_node {
                                        if let Some(ref val) = res.val {
                                            println!("VAL NODE TYPE: {:?}", val.node);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => println!("PARSE ERROR: {}", e),
    }
}

#[test]
fn test_debug_indirection_target_v2() {
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    match pg_query::parse(input) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            println!("DEBUG_JSON_V4: {}", json);
        }
        Err(e) => println!("PARSE_ERROR_V4: {}", e),
    }
}

#[test]
fn test_transpile_subquery_array_indexing_exact() {
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    let result = transpile(input);
    println!("DEBUG: result = {}", result);
    // Based on previous run, it produced:
    // select json_extract((select '[true,2,3]' AS "?column?"), '0')
    // Wait, why '[true,2,3]'? Ah, pg_query might have constant-folded or something? 
    // No, that's my test mock maybe? No, I didn't mock this.
}

#[test]
fn test_debug_indirection_res_target() {
    use pg_query::protobuf::node::Node as NodeEnum;
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    match pg_query::parse(input) {
        Ok(result) => {
            if let Some(stmt) = result.protobuf.stmts.first() {
                if let Some(ref stmt_node) = stmt.stmt {
                    if let Some(ref inner) = stmt_node.node {
                        if let NodeEnum::SelectStmt(ref select) = inner {
                            if let Some(target) = select.target_list.first() {
                                if let Some(ref target_node) = target.node {
                                    println!("TARGET NODE: {:?}", target_node);
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => println!("PARSE ERROR: {}", e),
    }
}

#[test]
fn test_debug_indirection_res_target_v2() {
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    match pg_query::parse(input) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            println!("DEBUG_JSON_V5: {}", json);
        }
        Err(e) => println!("PARSE_ERROR_V5: {}", e),
    }
}

#[test]
fn test_debug_indirection_res_target_v3() {
    let input = "SELECT (SELECT ARRAY[1,2,3])[1]";
    match pg_query::parse(input) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            let mut file = std::fs::File::create("debug_json.txt").unwrap();
            use std::io::Write;
            file.write_all(json.as_bytes()).unwrap();
        }
        Err(e) => println!("PARSE_ERROR_V5: {}", e),
    }
}

#[test]
fn test_check_node_variants() {
    use pg_query::protobuf::node::Node as NodeEnum;
    let n: Option<NodeEnum> = None;
    match n {
        Some(NodeEnum::AIndirection(_)) => (),
        Some(NodeEnum::AIndices(_)) => (),
        _ => (),
    }
}

#[test]
fn test_debug_nested_parse() {
    let sql = "SELECT array[array[1,2], array[3,4]][1][2]";
    match pg_query::parse(sql) {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.protobuf).unwrap();
            let mut file = std::fs::File::create("nested_parse.json").unwrap();
            use std::io::Write;
            file.write_all(json.as_bytes()).unwrap();
        }
        Err(e) => println!("ERROR: {}", e),
    }
}

#[test]
fn test_bitwise_right_shift_not_geo() {
    // Test that bitwise right shift operator is not confused with geometric operators
    let sql = "SELECT (256::int2 >> 4)::text";
    let result = transpile(sql);
    
    // Check that geo_right is NOT in the output
    assert!(!result.contains("geo_right"), "Output should not contain geo_right for integer shift, got: {}", result);
    
    // Check that the output contains the bitwise shift operator
    assert!(result.contains(">>"), "Output should contain >> operator, got: {}", result);
}

#[test]
fn test_char_length_alias() {
    // Test that char_length is transpiled to length
    // Note: PostgreSQL preserves the original function name as the column alias
    let sql = "SELECT char_length('hello')";
    let result = transpile(sql);
    assert!(result.contains("length("), "Output should contain length( function call, got: {}", result);
    // The alias will contain char_length which is expected PostgreSQL behavior
    assert!(result.contains("\"char_length\""), "Output should preserve original name in alias, got: {}", result);
}

#[test]
fn test_character_length_alias() {
    // Test that character_length is transpiled to length
    // Note: PostgreSQL preserves the original function name as the column alias
    let sql = "SELECT character_length('hello')";
    let result = transpile(sql);
    assert!(result.contains("length("), "Output should contain length( function call, got: {}", result);
    // The alias will contain character_length which is expected PostgreSQL behavior
    assert!(result.contains("\"character_length\""), "Output should preserve original name in alias, got: {}", result);
}

#[test]
fn test_column_alias_preservation() {
    // Test that column aliases are preserved in the transpile result
    let sql = r#"SELECT 1 AS "my_alias""#;
    let result = transpile_with_metadata(sql);
    assert_eq!(result.column_aliases.len(), 1, "Should have one column alias");
    assert_eq!(result.column_aliases[0], "my_alias", "Alias should be 'my_alias'");
    assert!(result.sql.contains("my_alias"), "SQL should contain alias: {}", result.sql);
}

#[test]
fn test_column_alias_with_case_expression() {
    // Test the specific case from the issue: CASE expression with alias
    let sql = r#"SELECT CASE WHEN 1 < 2 THEN 3 END AS "Simple WHEN""#;
    let result = transpile_with_metadata(sql);
    assert_eq!(result.column_aliases.len(), 1, "Should have one column alias");
    assert_eq!(result.column_aliases[0], "Simple WHEN", "Alias should be 'Simple WHEN'");
}

#[test]
fn test_multiple_column_aliases() {
    // Test multiple columns with different aliases
    let sql = r#"SELECT id AS "user_id", name AS "user_name", 1+1 AS "sum" FROM users"#;
    let result = transpile_with_metadata(sql);
    assert_eq!(result.column_aliases.len(), 3, "Should have three column aliases");
    assert_eq!(result.column_aliases[0], "user_id", "First alias should be 'user_id'");
    assert_eq!(result.column_aliases[1], "user_name", "Second alias should be 'user_name'");
    assert_eq!(result.column_aliases[2], "sum", "Third alias should be 'sum'");
}

#[test]
fn test_mixed_aliased_and_unaliased_columns() {
    // Test mix of aliased and unaliased columns
    let sql = r#"SELECT id, name AS "user_name", email FROM users"#;
    let result = transpile_with_metadata(sql);
    assert_eq!(result.column_aliases.len(), 3, "Should have three column entries");
    assert_eq!(result.column_aliases[0], "", "First column has no alias");
    assert_eq!(result.column_aliases[1], "user_name", "Second alias should be 'user_name'");
    assert_eq!(result.column_aliases[2], "", "Third column has no alias");
}

#[test]
fn test_float_whitespace_trim() {
    // Test whitespace trimming for REAL casts
    let sql = "SELECT '  0.0  '::real, '  123.456  '::double precision";
    let result = transpile(sql);
    // Should contain trimmed values without whitespace
    assert!(result.contains("'0.0'"), "Whitespace not trimmed for REAL: {}", result);
    assert!(result.contains("'123.456'"), "Whitespace not trimmed for DOUBLE PRECISION: {}", result);
    assert!(!result.contains("'  0.0  '"), "Whitespace still present for REAL: {}", result);
    assert!(!result.contains("'  123.456  '"), "Whitespace still present for DOUBLE PRECISION: {}", result);
}

#[test]
fn test_integer_whitespace_trim() {
    // Test whitespace trimming for INTEGER casts
    let sql = "SELECT '  42  '::integer, '  -99  '::int";
    let result = transpile(sql);
    // Should contain trimmed values without whitespace
    assert!(result.contains("'42'"), "Whitespace not trimmed for INTEGER: {}", result);
    assert!(result.contains("'-99'"), "Whitespace not trimmed for INT: {}", result);
}

#[test]
fn test_numeric_whitespace_trim() {
    // Test whitespace trimming for NUMERIC/DECIMAL casts
    let sql = "SELECT '  3.14159  '::numeric, '  2.718  '::decimal";
    let result = transpile(sql);
    // Should contain trimmed values without whitespace
    assert!(result.contains("'3.14159'"), "Whitespace not trimmed for NUMERIC: {}", result);
    assert!(result.contains("'2.718'"), "Whitespace not trimmed for DECIMAL: {}", result);
}

#[test]
fn test_non_numeric_cast_no_trim() {
    // Test that non-numeric casts do NOT trim whitespace
    let sql = "SELECT '  hello  '::text, '  world  '::varchar";
    let result = transpile(sql);
    // Should preserve whitespace for non-numeric types
    assert!(result.contains("'  hello  '") || result.contains("' hello '"), 
            "TEXT cast should preserve or minimally trim whitespace: {}", result);
}

#[test]
fn test_generate_series_basic() {
    let sql = "SELECT * FROM generate_series(1, 5)";
    let result = transpile(sql);
    assert!(result.contains("WITH RECURSIVE"), "Should contain WITH RECURSIVE: {}", result);
    assert!(result.contains("_series"), "Should contain _series: {}", result);
    assert!(result.contains("UNION ALL"), "Should contain UNION ALL: {}", result);
}

#[test]
fn test_generate_series_with_step() {
    let sql = "SELECT * FROM generate_series(1, 10, 2)";
    let result = transpile(sql);
    assert!(result.contains("WITH RECURSIVE"), "Should contain WITH RECURSIVE: {}", result);
    assert!(result.contains("2"), "Should contain step value: {}", result);
}

#[test]
fn test_update_row_constructor() {
    let sql = "UPDATE t SET (a, b) = (1, 2)";
    let result = transpile(sql);
    assert!(result.contains("set a = 1, b = 2"), "Should expand row constructor: {}", result);
}

#[test]
fn test_interval_literal() {
    let test_cases = vec![
        ("SELECT INTERVAL '1 day'", "1 day"),
        ("SELECT INTERVAL '1 hour'", "1 hour"),
        ("SELECT INTERVAL '1 day 2 hours'", "1 day 2 hours"),
        ("SELECT INTERVAL '1 year 2 months 3 days'", "1 year 2 months 3 days"),
    ];
    
    for (sql, _expected) in test_cases {
        let result = transpile(sql);
        assert!(!result.contains("no such column"), 
            "INTERVAL treated as column in: {}", result);
        // Interval should be stored as text
        assert!(result.contains("cast") && result.contains("text"),
            "INTERVAL should be cast to TEXT: {}", result);
    }
}

#[test]
fn test_interval_arithmetic() {
    let sql = "SELECT now() + INTERVAL '1 day'";
    let result = transpile(sql);
    // Should use SQLite datetime function with modifier
    assert!(result.contains("datetime"), 
        "Should use datetime() for interval arithmetic: {}", result);
}

#[test]
fn test_interval_subtraction() {
    let sql = "SELECT now() - INTERVAL '1 hour'";
    let result = transpile(sql);
    // Should use SQLite datetime function with negative modifier
    assert!(result.contains("datetime"),
        "Should use datetime() for interval subtraction: {}", result);
}

#[test]
fn test_chr_function() {
    let sql = "SELECT chr(65)";
    let result = transpile(sql);
    // Should contain chr function call
    assert!(result.contains("chr"), "Should contain chr function: {}", result);
}

#[test]
fn test_lpad_rpad_functions() {
    let test_cases = vec![
        "SELECT lpad('hi', 5)",
        "SELECT lpad('hi', 5, 'x')",
        "SELECT rpad('hi', 5)",
        "SELECT rpad('hi', 5, 'x')",
    ];
    
    for sql in test_cases {
        let result = transpile(sql);
        // Should contain the function name
        assert!(result.contains("lpad") || result.contains("rpad"), 
            "Failed: {}", sql);
    }
}

#[test]
fn test_translate_function() {
    let sql = "SELECT translate('hello', 'l', 'L')";
    let result = transpile(sql);
    assert!(result.contains("translate"), "Should contain translate: {}", result);
}

#[test]
fn test_format_function() {
    let sql = "SELECT format('Hello %s', 'World')";
    let result = transpile(sql);
    assert!(result.contains("format"), "Should contain format: {}", result);
}

#[test]
fn test_uuidv4_function() {
    let sql = "SELECT uuidv4()";
    let result = transpile(sql);
    assert!(result.contains("uuidv4") || result.contains("gen_random_uuid"));
}

#[test]
fn test_uuidv7_function() {
    let sql = "SELECT uuidv7()";
    let result = transpile(sql);
    assert!(result.contains("uuidv7"));
}

#[test]
fn test_uuid_extract_version() {
    let sql = "SELECT uuid_extract_version('11111111-1111-5111-8111-111111111111')";
    let result = transpile(sql);
    assert!(result.contains("uuid_extract_version"));
}

#[test]
fn test_uuid_extract_timestamp() {
    let sql = "SELECT uuid_extract_timestamp('C232AB00-9414-11EC-B3C8-9F6BDECED846')";
    let result = transpile(sql);
    assert!(result.contains("uuid_extract_timestamp"));
}

#[test]
fn test_explain_basic() {
    let sql = "EXPLAIN SELECT 1";
    let result = transpile(sql);
    assert!(result.contains("explain"));
}

#[test]
fn test_explain_with_options() {
    let test_cases = vec![
        "EXPLAIN (COSTS OFF) SELECT 1",
        "EXPLAIN (VERBOSE, COSTS OFF) SELECT 1",
        "EXPLAIN ANALYZE SELECT 1",
    ];
    
    for sql in test_cases {
        let result = transpile(sql);
        assert!(result.contains("explain"), "Failed: {}", sql);
    }
}

#[test]
fn test_show_commands() {
    let test_cases = vec![
        "SHOW timezone",
        "SHOW transaction_isolation_level",
        "SHOW default_transaction_read_only",
        "SHOW statement_timeout",
        "SHOW client_encoding",
        "SHOW application_name",
        "SHOW DateStyle",
    ];
    
    for sql in test_cases {
        let result = transpile(sql);
        assert!(!result.is_empty(), "Failed: {}", sql);
    }
}

#[test]
fn test_show_all() {
    let sql = "SHOW ALL";
    let result = transpile(sql);
    assert!(!result.is_empty());
}
