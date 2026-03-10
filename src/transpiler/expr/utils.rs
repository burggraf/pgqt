//! Shared utilities for expression reconstruction
//!
//! Contains common helper functions used across multiple expression types.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{Node, AConst, ColumnRef, TypeCast, BoolExpr};
use crate::transpiler::context::TranspileContext;
use crate::transpiler::utils::{extract_original_type, rewrite_type_for_sqlite};

/// Reconstruct a constant value
pub(crate) fn reconstruct_aconst(aconst: &AConst) -> String {
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

/// Reconstruct a ColumnRef node
pub(crate) fn reconstruct_column_ref(col_ref: &ColumnRef, _ctx: &mut TranspileContext) -> String {
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

/// Reconstruct a TypeCast node
pub(crate) fn reconstruct_type_cast(type_cast: &TypeCast, ctx: &mut TranspileContext, reconstruct_node: impl Fn(&Node, &mut TranspileContext) -> String) -> String {
    let arg_sql = type_cast
        .arg
        .as_ref()
        .map(|n| reconstruct_node(n, ctx))
        .unwrap_or_default();
    let original_type = extract_original_type(&type_cast.type_name);
    let sqlite_type = rewrite_type_for_sqlite(&original_type, &ctx.registry);

    // Check if target type is numeric and argument is a string literal with whitespace
    let type_lower = original_type.to_lowercase();
    let is_numeric = type_lower.contains("real") 
        || type_lower.contains("double") 
        || type_lower.contains("float")
        || type_lower.contains("int")
        || type_lower.contains("numeric")
        || type_lower.contains("decimal")
        || type_lower.contains("number");
    
    if is_numeric && arg_sql.starts_with('\'') && arg_sql.ends_with('\'') {
        // Extract inner value and trim whitespace
        let inner = &arg_sql[1..arg_sql.len()-1];
        let trimmed = inner.trim();
        if trimmed != inner {
            // Reconstruct with trimmed value
            let trimmed_arg = format!("'{}'", trimmed);
            return format!("cast({} as {})", trimmed_arg, sqlite_type.to_lowercase());
        }
    }

    if original_type.to_uppercase() == "REGCLASS" {
        return format!("(SELECT oid FROM pg_class WHERE relname = {0} OR oid = CAST({0} AS INTEGER) LIMIT 1)", arg_sql);
    }
    if original_type.to_uppercase() == "REGTYPE" {
        return format!("(SELECT oid FROM pg_type WHERE typname = {0} OR oid = CAST({0} AS INTEGER) LIMIT 1)", arg_sql);
    }
    if original_type.to_uppercase() == "REGPROC" || original_type.to_uppercase() == "REGPROCEDURE" {
        return format!("(SELECT oid FROM pg_proc WHERE proname = {0} OR oid = CAST({0} AS INTEGER) LIMIT 1)", arg_sql);
    }

    // Validate boolean literals
    if original_type.to_uppercase() == "BOOLEAN" || original_type.to_uppercase() == "BOOL" {
        // Extract the literal value even if it's wrapped in another cast
        let mut inner_val = arg_sql.clone();
        if inner_val.to_lowercase().starts_with("cast(") && inner_val.ends_with(')') {
            if let Some(start) = inner_val.find('(') {
                if let Some(end) = inner_val.rfind(" as ") {
                    inner_val = inner_val[start+1..end].trim().to_string();
                }
            }
        }

        // Check if the argument is a string literal and validate it
        if inner_val.starts_with('\'') && inner_val.ends_with('\'') {
            let inner = inner_val[1..inner_val.len()-1].trim().to_lowercase();
            // Valid boolean literals in PostgreSQL (exact matches only for brevity)
            let valid_true = matches!(inner.as_str(), "t" | "tr" | "tru" | "true" | "y" | "ye" | "yes" | "on" | "1");
            let valid_false = matches!(inner.as_str(), "f" | "fa" | "fal" | "fals" | "false" | "n" | "no" | "of" | "off" | "0");

            if valid_true {
                return "1".to_string();
            } else if valid_false {
                return "0".to_string();
            } else {
                ctx.add_error(format!("invalid input syntax for type boolean: \"{}\"", &inner_val[1..inner_val.len()-1]));
            }
        }
    }

    format!("cast({} as {})", arg_sql, sqlite_type.to_lowercase())
}

/// Reconstruct a BoolExpr node (AND, OR, NOT)
pub(crate) fn reconstruct_bool_expr(bool_expr: &BoolExpr, ctx: &mut TranspileContext, reconstruct_node: impl Fn(&Node, &mut TranspileContext) -> String) -> String {
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

/// Transform PostgreSQL default expressions to SQLite equivalents
/// 
/// Handles common PostgreSQL default expressions like:
/// - now() -> datetime('now')
/// - current_timestamp -> datetime('now')
/// - nextval('seq') -> NULL (SQLite handles autoincrement separately)
pub(crate) fn transform_default_expression(expr: &str) -> String {
    let upper = expr.trim().to_uppercase();
    
    match upper.as_str() {
        "NOW()" | "CURRENT_TIMESTAMP" | "CURRENT_TIMESTAMP()" => {
            "datetime(now)".to_string()
        }
        "CURRENT_DATE" | "CURRENT_DATE()" => {
            "date(now)".to_string()
        }
        "CURRENT_TIME" | "CURRENT_TIME()" => {
            "time(now)".to_string()
        }
        "TRUE" => "1".to_string(),
        "FALSE" => "0".to_string(),
        _ => {
            if upper.starts_with("NEXTVAL") {
                "NULL".to_string()
            } else {
                expr.to_string()
            }
        }
    }
}

/// Check if a node represents LIMIT ALL
#[allow(dead_code)]
pub(crate) fn is_limit_all(node: &Node) -> bool {
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