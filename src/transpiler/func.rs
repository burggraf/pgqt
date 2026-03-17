//! Function call reconstruction and CREATE FUNCTION parsing
//!
//! This module handles the reconstruction of PostgreSQL function calls
//! and parsing of CREATE FUNCTION statements.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    Node, FuncCall, CreateFunctionStmt, FunctionParameter, TypeName
};
use crate::catalog::{ParamMode, ReturnTypeKind};
use super::context::TranspileContext;
use crate::transpiler::reconstruct_node;
use crate::transpiler::transpile_with_context;
use crate::transpiler::window::reconstruct_window_def;

pub(crate) fn reconstruct_func_call(func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
    // Build full function name from all parts (handle schema-qualified functions)
    let func_name_lower: String = func_call
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
        .last()
        .unwrap_or_default();
    
    // Special handling: pg_get_function_result(p.oid) -> p.proresult
    if func_name_lower == "pg_get_function_result" && func_call.args.len() == 1 {
        if let Some(arg) = func_call.args.first() {
            if let Some(NodeEnum::ColumnRef(col_ref)) = arg.node.as_ref() {
                let col_parts: Vec<String> = col_ref.fields.iter()
                    .filter_map(|f| {
                        if let Some(NodeEnum::String(s)) = f.node.as_ref() {
                            Some(s.sval.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                if col_parts.len() == 2 {
                    // It's "alias.oid", rewrite to "alias.proresult"
                    return format!("{}.proresult", col_parts[0]);
                }
            }
        }
    }
    
    // Special handling: pg_get_function_arguments(p.oid) -> '' (empty string for now)
    if func_name_lower == "pg_get_function_arguments" && func_call.args.len() == 1 {
        return "''".to_string();
    }
    
    // Special handling: pg_get_function_identity_arguments(p.oid) -> ''
    if func_name_lower == "pg_get_function_identity_arguments" && func_call.args.len() == 1 {
        return "''".to_string();
    }
    
    // Handle generate_series as a recursive CTE
    if func_name_lower == "generate_series" {
        return reconstruct_generate_series(func_call, ctx);
    }
    
    // Special handling for timestamp functions with precision argument
    // PostgreSQL: current_timestamp(0), now(2), etc. -> ignore precision, return datetime('now')
    if matches!(func_name_lower.as_str(), "current_timestamp" | "current_time" | "current_date" | "now" | "clock_timestamp" | "statement_timestamp" | "transaction_timestamp") {
        return match func_name_lower.as_str() {
            "current_date" => "date('now')".to_string(),
            "current_time" => "time('now')".to_string(),
            _ => "datetime('now')".to_string(),
        };
    }
    
    let func_parts: Vec<String> = func_call
        .funcname
        .iter()
        .filter_map(|n| {
            if let Some(ref inner) = n.node {
                if let NodeEnum::String(s) = inner {
                    let name = s.sval.to_lowercase();
                    // Strip "public" schema prefix for SQLite compatibility
                    if name == "public" {
                        return None;
                    }
                    return Some(name);
                }
            }
            None
        })
        .collect();

    let full_func_name = func_parts.join(".");
    let func_name = func_parts.last().map(|s| s.as_str()).unwrap_or("");

    // Detect hypothetical-set aggregates (rank, dense_rank, percent_rank, cume_dist)
    if func_call.agg_within_group {
        match func_name {
            "rank" | "dense_rank" | "percent_rank" | "cume_dist" => {
                if !func_call.args.is_empty() && !func_call.agg_order.is_empty() {
                    let hyp_val = reconstruct_node(&func_call.args[0], ctx);
                    
                    if let Some(NodeEnum::SortBy(sort_by)) = &func_call.agg_order[0].node {
                        if let Some(ref order_node) = sort_by.node {
                            let order_col = reconstruct_node(order_node, ctx);
                            
                            let sqlite_func = match func_name {
                                "rank" => "__pg_hypothetical_rank",
                                "dense_rank" => "__pg_hypothetical_dense_rank",
                                "percent_rank" => "__pg_hypothetical_percent_rank",
                                "cume_dist" => "__pg_hypothetical_cume_dist",
                                _ => "",
                            };
                            
                            if !sqlite_func.is_empty() {
                                return format!("{}({}, {})", sqlite_func, hyp_val, order_col);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

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
                
                let mut sql_body = transpiled_body.sql.trim_end_matches(';').to_string();
                
                // UNWRAP SIMPLE SELECTS: If the body is a simple SELECT (no FROM, no complex clauses),
                // we strip "SELECT " to avoid double-wrapping which causes syntax errors in SQLite
                // like (SELECT (SELECT 3 + 7)) when used in expressions.
                // However, we MUST NOT unwrap if this is part of an INSERT statement's VALUES context,
                // as that leads to "INSERT INTO ... 1 AS id" which is invalid.
                if sql_body.to_uppercase().starts_with("SELECT ") && !ctx.in_insert_values {
                    let mut is_simple = true;
                    let upper_body = sql_body.to_uppercase();
                    let restricted = [" FROM ", " WHERE ", " GROUP BY ", " HAVING ", " WINDOW ", " ORDER BY ", " LIMIT ", " OFFSET ", " UNION ", " INTERSECT ", " EXCEPT ", " VALUES "];
                    for word in restricted {
                        if upper_body.contains(word) {
                            is_simple = false;
                            break;
                        }
                    }
                    
                    if is_simple {
                        // Check if it has aliases (AS) or multiple columns (comma) which makes it not a simple scalar expression.
                        // We must be careful not to match commas inside functions like randomblob(4).
                        let mut depth = 0;
                        let mut has_comma = false;
                        for c in sql_body.chars() {
                            if c == '(' { depth += 1; }
                            else if c == ')' { depth -= 1; }
                            else if c == ',' && depth == 0 {
                                has_comma = true;
                                break;
                            }
                        }

                        if !upper_body.contains(" AS ") && !has_comma {
                            sql_body = sql_body[7..].trim().to_string();
                        }
                    }
                }

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

    // Process arguments
    let mut args: Vec<String> = Vec::new();
    let mut has_qualified_star = false;
    if !func_call.agg_star {
        for n in &func_call.args {
            // Check if this is a qualified star like "t2.*" in count(t2.*)
            // PostgreSQL allows this but SQLite doesn't, so we convert to "*"
            if func_name == "count" {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::ColumnRef(col_ref) = inner {
                        let fields: Vec<_> = col_ref.fields.iter()
                            .filter_map(|f| f.node.as_ref())
                            .collect();
                        if let Some(NodeEnum::AStar(_)) = fields.last() {
                            has_qualified_star = true;
                            continue; // Skip adding this argument, we'll use "*"
                        }
                    }
                }
            }
            args.push(reconstruct_node(n, ctx));
        }
    }
    
    let args_str = if func_call.agg_star || has_qualified_star {
        "*".to_string()
    } else {
        args.join(", ")
    };

    use crate::transpiler::registry::FunctionMapping;
    
    // Lookup function in registry
    let mapping = ctx.registry.functions.mappings.get(func_name).or_else(|| ctx.registry.functions.mappings.get(&full_func_name));
    
    let mut _original_func_name: Option<&str> = None;
    let sqlite_func = match mapping {
        Some(FunctionMapping::Simple(name)) => name.to_string(),
        Some(FunctionMapping::Rewrite(rewrite)) => {
            if func_name == "pg_sleep" || func_name == "timezone" || func_name == "any_value" {
                _original_func_name = Some(func_name);
            }
            rewrite.to_string()
        },
        Some(FunctionMapping::Complex(func)) => {
            if func_name == "timezone" {
                _original_func_name = Some("timezone");
            }
            return func(&args); // For complex mappings, return immediately. This handles formatting directly.
        },
        Some(FunctionMapping::NoOp) => {
            return "NULL".to_string();
        },
        None => {
            if ctx.registry.stubbing_mode {
                // Return NULL for unknown functions when stubbing is enabled
                // except for schema-qualified where we might strip 'pg_catalog'
                if func_parts.len() > 1 && func_parts[0] == "pg_catalog" {
                    func_name.to_string()
                } else if func_parts.len() > 1 {
                    full_func_name.to_string()
                } else {
                    func_name.to_string()
                }
            } else {
                if func_parts.len() > 1 {
                    if func_parts[0] == "pg_catalog" {
                        func_name.to_string()
                    } else {
                        full_func_name.to_string()
                    }
                } else {
                    func_name.to_string()
                }
            }
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

    // Handle constant replacements for tracking rename (like any_value to min, etc. - those take arguments)
    // Actually pg_sleep maps to "0"
    if let Some(_orig_name) = _original_func_name {
        if sqlite_func == "0" || sqlite_func == "1" {
            // Constant replacement
            return sqlite_func.to_string();
        }
    }

    // Handle zero-argument aggregates (PostgreSQL allows these)
    // count() returns 0, all others return NULL
    if args.is_empty() && !func_call.agg_star {
        // Check if this is an aggregate function (either in registry or standard SQL)
        let is_aggregate = matches!(sqlite_func.as_str(),
            "max" | "min" | "sum" | "avg" | "count" |
            "stddev" | "stddev_samp" | "stddev_pop" |
            "variance" | "var_samp" | "var_pop" |
            "bool_and" | "bool_or"
        ) || matches!(func_name,
            "max" | "min" | "sum" | "avg" | "count" |
            "stddev" | "stddev_samp" | "stddev_pop" |
            "variance" | "var_samp" | "var_pop" |
            "bool_and" | "bool_or"
        );
        
        if is_aggregate {
            return match func_name {
                "count" => "0".to_string(),
                _ => "NULL".to_string(),
            };
        }
    }

    let base = format!("{}({})", sqlite_func, args_str);
    add_window_clause(&base, func_call, ctx)
}

/// Add OVER clause to a function call if present
pub(crate) fn add_window_clause(base: &str, func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
    if let Some(ref over) = func_call.over {
        let window_sql = reconstruct_window_def(over, ctx);

        // Don't add alias if the window is a named reference (OVER w)
        // because it would break when used multiple times (e.g., SELECT ... ORDER BY)
        // Also, don't wrap named references in parentheses
        if !over.refname.is_empty() {
            return format!("{} over {}", base, window_sql);
        }

        format!("{} over ({})", base, window_sql)
    } else {
        base.to_string()
    }
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

/// Extract schema name from qualified function name (e.g., "public"."funcname" -> "public")
fn extract_schema_name(funcname: &[Node]) -> Option<String> {
    if funcname.len() >= 2 {
        // Schema-qualified name: take first component as schema
        funcname.first().and_then(|n| n.node.as_ref()).and_then(|node| {
            if let NodeEnum::String(s) = node {
                Some(s.sval.clone())
            } else {
                None
            }
        })
    } else {
        None
    }
}

/// Parse CreateFunctionStmt protobuf
pub(crate) fn parse_create_function_stmt(stmt: &CreateFunctionStmt) -> anyhow::Result<crate::catalog::FunctionMetadata> {
    // Extract function name
    let funcname = extract_funcname(&stmt.funcname)?;
    let schema_name = extract_schema_name(&stmt.funcname).unwrap_or_else(|| "public".to_string());
    
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
    let mut language = "sql".to_string();
    
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
                "language" => {
                    if let Some(NodeEnum::String(s)) = opt.arg.as_ref().and_then(|a| a.node.as_ref()) {
                        language = s.sval.clone().to_lowercase();
                    }
                }
                _ => {}
            }
        }
    }
    
    Ok(crate::catalog::FunctionMetadata {
        oid: 0,
        name: funcname,
        schema: schema_name,
        arg_types,
        arg_names,
        arg_modes,
        return_type,
        return_type_kind,
        return_table_cols,
        function_body: if language == "plpgsql" { function_body } else { function_body_with_positions },
        language,
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
/// Handles schema-qualified names like "public"."funcname" by taking the last component
pub(crate) fn extract_funcname(funcname: &[Node]) -> anyhow::Result<String> {
    // Take the last element for schema-qualified names (e.g., "public"."funcname" -> "funcname")
    if let Some(NodeEnum::String(s)) = funcname.last().and_then(|n| n.node.as_ref()) {
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

/// Reconstruct generate_series function as a recursive CTE
/// generate_series(start, stop) -> WITH RECURSIVE _series(n) AS (SELECT start UNION ALL SELECT n + 1 FROM _series WHERE n < stop) SELECT n FROM _series
fn reconstruct_generate_series(func_call: &FuncCall, ctx: &mut TranspileContext) -> String {
    if func_call.args.len() < 2 || func_call.args.len() > 3 {
        return "generate_series(/* invalid arguments */)".to_string();
    }
    
    let start = reconstruct_node(&func_call.args[0], ctx);
    let stop = reconstruct_node(&func_call.args[1], ctx);
    let step = if func_call.args.len() == 3 {
        reconstruct_node(&func_call.args[2], ctx)
    } else {
        "1".to_string()
    };
    
    // Generate a unique CTE name
    let cte_name = "_series";
    
    // Add LIMIT 100000 to prevent infinite loops from zero or invalid steps
    format!(
        "(WITH RECURSIVE {}(n) AS (SELECT {} UNION ALL SELECT n + {} FROM {} WHERE ({} > 0 AND n + {} <= {}) OR ({} < 0 AND n + {} >= {}) LIMIT 100000) SELECT n FROM {})",
        cte_name, start, step, cte_name, step, step, stop, step, step, stop, cte_name
    )
}

