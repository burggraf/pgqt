//! Function call reconstruction and CREATE FUNCTION parsing
//!
//! This module handles the reconstruction of PostgreSQL function calls
//! and parsing of CREATE FUNCTION statements.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    Node, FuncCall, WindowDef, CreateFunctionStmt, FunctionParameter, TypeName
};
use crate::catalog::{ParamMode, ReturnTypeKind};
use super::context::TranspileContext;
use crate::transpiler::reconstruct_node;
use crate::transpiler::transpile_with_context;
use crate::transpiler::window::reconstruct_window_def;

pub(crate) fn reconstruct_func_call(func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
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

    // Try to inline user-defined functions (SQL language) or call PL/pgSQL functions
    let functions_registry = ctx.functions.clone();
    if let Some(ref functions) = functions_registry {
        // Try looking up by full name then by short name
        if let Some(metadata) = functions.get(&full_func_name).or_else(|| functions.get(func_name)) {
            let metadata = metadata.value();
            
            if metadata.language.to_lowercase() == "sql" {
                // Reconstruct arguments
                let mut arg_exprs = Vec::new();
                for arg_node in &func_call.args {
                    arg_exprs.push(reconstruct_node(arg_node, ctx));
                }
                
                let mut inlined = metadata.function_body.clone();
                
                // Replace positional parameters ($1, $2, etc.) with argument expressions
                // We do this in reverse order to avoid replacing $1 in $10
                for i in (0..arg_exprs.len()).rev() {
                    let placeholder = format!("${}", i + 1);
                    // Use parentheses around the expression to ensure correct operator precedence
                    let replacement = format!("({})", arg_exprs[i]);
                    inlined = inlined.replace(&placeholder, &replacement);
                }
                
                // Transpile the inlined body itself to ensure it's valid SQLite
                // Note: We avoid infinite recursion by not passing the functions registry
                let mut inner_ctx = TranspileContext::new();
                inner_ctx.referenced_tables = ctx.referenced_tables.clone();
                let transpiled_body = transpile_with_context(&inlined, &mut inner_ctx);
                
                // Update referenced tables in outer context
                ctx.referenced_tables = inner_ctx.referenced_tables;
                
                let sql_body = transpiled_body.sql.trim_end_matches(';');
                
                // Special handling for different return types
                return match metadata.return_type_kind {
                    ReturnTypeKind::Void => {
                        // For VOID functions, we want to return NULL but still execute the body if it's a SELECT
                        // If it's a side-effect (INSERT/UPDATE/DELETE), SQLite won't allow it in an expression anyway
                        format!("(select null from ({}) limit 1)", sql_body)
                    }
                    ReturnTypeKind::SetOf | ReturnTypeKind::Table => {
                        // For SETOF/TABLE functions, they work best in the FROM clause.
                        // When used in a SELECT list, SQLite will just take the first row/column.
                        format!("({})", sql_body)
                    }
                    ReturnTypeKind::Scalar => {
                        format!("({})", sql_body)
                    }
                };
            } else if metadata.language.to_lowercase() == "plpgsql" {
                // For PL/pgSQL functions, generate a call to our runtime wrapper
                // The wrapper will look up the function and execute it in the Lua runtime
                
                // Reconstruct arguments for the call
                let mut arg_exprs: Vec<String> = Vec::new();
                for arg_node in &func_call.args {
                    arg_exprs.push(reconstruct_node(arg_node, ctx));
                }
                
                // Generate a call to pgqt_plpgsql_call with function name and args
                // This will be handled by a custom SQLite function in main.rs
                let args_str = arg_exprs.join(", ");
                let func_name_literal = func_name.replace("'", "''");
                
                return match metadata.return_type_kind {
                    ReturnTypeKind::Void => {
                        // For VOID functions, execute and return NULL
                        format!("pgqt_plpgsql_call_void('{}', {})", func_name_literal, args_str)
                    }
                    ReturnTypeKind::SetOf | ReturnTypeKind::Table => {
                        // For SETOF/TABLE, return as subquery
                        format!("(SELECT * FROM pgqt_plpgsql_call_setof('{}', {}))", func_name_literal, args_str)
                    }
                    ReturnTypeKind::Scalar => {
                        // For scalar functions, return the value
                        format!("pgqt_plpgsql_call_scalar('{}', {})", func_name_literal, args_str)
                    }
                };
            }
        }
    }

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
pub(crate) fn add_window_clause(base: &str, func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
    if let Some(ref over) = func_call.over {
        let window_sql = reconstruct_window_def(over, ctx);
        // Always add OVER clause if the function has one, even if empty
        return format!("{} over ({})", base, window_sql);
    }
    base.to_string()
}

/// Reconstruct a CREATE ROLE statement as an INSERT into __pg_authid__

pub fn parse_create_function(sql: &str) -> anyhow::Result<crate::catalog::FunctionMetadata> {
    let result = pg_query::parse(sql)?;
    
    if let Some(raw_stmt) = result.protobuf.stmts.first() {
        if let Some(NodeEnum::CreateFunctionStmt(stmt)) = &raw_stmt.stmt.as_ref().and_then(|s| s.node.as_ref()) {
            return parse_create_function_stmt(stmt);
        }
    }
    
    anyhow::bail!("Not a CREATE FUNCTION statement")
}

/// Parse CreateFunctionStmt protobuf
pub(crate) fn parse_create_function_stmt(stmt: &CreateFunctionStmt) -> anyhow::Result<crate::catalog::FunctionMetadata> {
    // Extract function name
    let funcname = extract_funcname(&stmt.funcname)?;
    
    // Extract parameters
    let mut arg_types = Vec::new();
    let mut arg_names = Vec::new();
    let mut arg_modes = Vec::new();
    
    for param_node in &stmt.parameters {
        if let Some(NodeEnum::FunctionParameter(param)) = param_node.node.as_ref() {
            let (name, pg_type, mode) = parse_function_parameter(param)?;
            arg_names.push(name.unwrap_or_default());
            arg_types.push(pg_type);
            arg_modes.push(mode);
        }
    }
    
    // Extract return type and kind
    let (return_type, return_type_kind, return_table_cols) = parse_return_type(&stmt.return_type, &stmt.parameters)?;
    
    // Extract function body from options (defname="as")
    // The sql_body field is often None, so we need to look in options
    let function_body = extract_function_body_from_options(&stmt.options)
        .unwrap_or_else(|| "SELECT 1".to_string());
    
    // Convert named parameters to positional ($1, $2, etc.)
    // Only include IN, INOUT, or VARIADIC parameters for positional substitution
    let mut input_arg_names = Vec::new();
    for (i, mode) in arg_modes.iter().enumerate() {
        if *mode != ParamMode::Out {
            input_arg_names.push(arg_names[i].clone());
        }
    }
    
    // This is crucial for function execution
    let function_body_with_positions = convert_named_to_positional_params(&function_body, &input_arg_names);
    
    // Extract attributes from options
    let mut volatility = "VOLATILE".to_string();
    let mut strict = false;
    let mut security_definer = false;
    let mut parallel = "UNSAFE".to_string();
    
    for opt_node in &stmt.options {
        if let Some(NodeEnum::DefElem(opt)) = opt_node.node.as_ref() {
            match opt.defname.as_str() {
                "volatility" => {
                    if let Some(NodeEnum::String(s)) = opt.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        volatility = s.sval.clone().to_uppercase();
                    }
                }
                "strict" => {
                    if let Some(NodeEnum::Boolean(b)) = opt.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        strict = b.boolval;
                    }
                }
                "security" => {
                    if let Some(NodeEnum::String(s)) = opt.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        security_definer = s.sval.eq_ignore_ascii_case("definer");
                    }
                }
                "parallel" => {
                    if let Some(NodeEnum::String(s)) = opt.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        parallel = s.sval.clone().to_uppercase();
                    }
                }
                _ => {}
            }
        }
    }
    
    Ok(crate::catalog::FunctionMetadata {
        oid: 0,
        name: funcname,
        schema: "public".to_string(),
        arg_types,
        arg_names,
        arg_modes,
        return_type,
        return_type_kind,
        return_table_cols,
        function_body: function_body_with_positions,
        language: "sql".to_string(),
        volatility,
        strict,
        security_definer,
        parallel,
        owner_oid: 1, // TODO: Get current user OID
        created_at: None,
    })
}

/// Convert named parameters (a, b) to positional ($1, $2)
pub(crate) fn convert_named_to_positional_params(body: &str, arg_names: &[String]) -> String {
    // If there are no named parameters, return as-is
    if arg_names.is_empty() || arg_names.iter().all(|n| n.is_empty()) {
        return body.to_string();
    }
    
    // Parse the SQL body to find identifier references
    // For now, use a simple approach: replace identifiers that match parameter names
    // This is imperfect but works for simple cases like "SELECT a + b"
    let mut result = body.to_string();
    
    // Replace parameter names with $1, $2, etc. in reverse order to avoid conflicts
    for (i, name) in arg_names.iter().enumerate().rev() {
        if !name.is_empty() {
            // Use regex to replace whole word matches only
            let pattern = format!(r"\b{}\b", regex::escape(name));
            let replacement = format!("${}", i + 1);
            // Use a closure to generate the replacement to avoid $ being treated as capture group
            result = regex::Regex::new(&pattern)
                .map(|re| {
                    re.replace_all(&result, |_: &regex::Captures| replacement.clone()).to_string()
                })
                .unwrap_or(result);
        }
    }
    
    result
}

/// Extract function name from ObjectWithArgs
pub(crate) fn extract_funcname(funcname: &[Node]) -> anyhow::Result<String> {
    if let Some(NodeEnum::String(s)) = funcname.first().and_then(|n| n.node.as_ref()) {
        Ok(s.sval.clone())
    } else {
        anyhow::bail!("Could not extract function name")
    }
}

/// Parse function parameter
pub(crate) fn parse_function_parameter(param: &FunctionParameter) -> anyhow::Result<(Option<String>, String, ParamMode)> {
    let name = if !param.name.is_empty() {
        Some(param.name.clone())
    } else {
        None
    };
    
    let pg_type = extract_type_name(&param.arg_type)?;
    
    let mode = match param.mode() {
        pg_query::protobuf::FunctionParameterMode::FuncParamIn => ParamMode::In,
        pg_query::protobuf::FunctionParameterMode::FuncParamOut => ParamMode::Out,
        pg_query::protobuf::FunctionParameterMode::FuncParamInout => ParamMode::InOut,
        pg_query::protobuf::FunctionParameterMode::FuncParamVariadic => ParamMode::Variadic,
        pg_query::protobuf::FunctionParameterMode::FuncParamTable => ParamMode::Out,
        _ => ParamMode::In,
    };
    
    Ok((name, pg_type, mode))
}

/// Extract type name from TypeName
pub(crate) fn extract_type_name(type_name: &Option<TypeName>) -> anyhow::Result<String> {
    if let Some(tn) = type_name {
        let names: Vec<String> = tn.names
            .iter()
            .filter_map(|n| n.node.as_ref())
            .map(|n| {
                if let NodeEnum::String(s) = n {
                    s.sval.clone()
                } else {
                    String::new()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        
        Ok(names.last().unwrap_or(&String::new()).to_uppercase())
    } else {
        Ok("UNKNOWN".to_string())
    }
}

/// Extract function body from the "as" option in CREATE FUNCTION
pub(crate) fn extract_function_body_from_options(options: &[Node]) -> Option<String> {
    for opt_node in options {
        if let Some(NodeEnum::DefElem(opt)) = opt_node.node.as_ref() {
            if opt.defname == "as" {
                // The body is in a List containing a String
                if let Some(ref arg) = opt.arg {
                    if let Some(NodeEnum::List(list)) = arg.node.as_ref() {
                        // Get the first item from the list
                        if let Some(first_item) = list.items.first() {
                            if let Some(NodeEnum::String(s)) = first_item.node.as_ref() {
                                return Some(s.sval.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse return type
pub(crate) fn parse_return_type(
    return_type: &Option<TypeName>,
    return_attrs: &[Node]
) -> anyhow::Result<(String, ReturnTypeKind, Option<Vec<(String, String)>>)> {
    // Check if this is RETURNS TABLE (identified by mode Table in parameters)
    let mut table_columns = Vec::new();
    let mut out_params = Vec::new();
    for attr_node in return_attrs {
        if let Some(NodeEnum::FunctionParameter(param)) = attr_node.node.as_ref() {
            if param.mode() == pg_query::protobuf::FunctionParameterMode::FuncParamTable {
                let name = param.name.clone();
                let pg_type = extract_type_name(&param.arg_type)?;
                table_columns.push((name, pg_type));
            } else if param.mode() == pg_query::protobuf::FunctionParameterMode::FuncParamOut || 
                      param.mode() == pg_query::protobuf::FunctionParameterMode::FuncParamInout {
                let name = param.name.clone();
                let pg_type = extract_type_name(&param.arg_type)?;
                out_params.push((name, pg_type));
            }
        }
    }
    
    if !table_columns.is_empty() {
        return Ok(("TABLE".to_string(), ReturnTypeKind::Table, Some(table_columns)));
    }
    
    // If we have OUT parameters, it's like returning a record/table
    if !out_params.is_empty() {
        return Ok(("RECORD".to_string(), ReturnTypeKind::Table, Some(out_params)));
    }
    
    // Check for VOID, SETOF, or SCALAR
    if let Some(tn) = return_type {
        let return_type_str = extract_type_name(&Some(tn.clone()))?;
        
        if return_type_str == "VOID" {
            return Ok(("VOID".to_string(), ReturnTypeKind::Void, None));
        }
        
        let kind = if tn.setof {
            if return_type_str == "RECORD" {
                ReturnTypeKind::Table
            } else {
                ReturnTypeKind::SetOf
            }
        } else {
            ReturnTypeKind::Scalar
        };
        Ok((return_type_str, kind, None))
    } else {
        Ok(("VOID".to_string(), ReturnTypeKind::Void, None))
    }
}

