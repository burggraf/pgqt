//! DISTINCT ON polyfill for PostgreSQL compatibility
//!
//! PostgreSQL's DISTINCT ON is not supported in SQLite.
//! We polyfill it using ROW_NUMBER() window function.
//!
//! Transformation:
//! SELECT DISTINCT ON (a, b) x, y FROM t ORDER BY a, b, c
//! -->
//! SELECT * FROM (
//!   SELECT x, y, ROW_NUMBER() OVER (PARTITION BY a, b ORDER BY a, b, c) as __rn
//!   FROM t
//! ) AS __distinct_on_sub WHERE __rn = 1 ORDER BY a, b, c

use pg_query::protobuf::SelectStmt;
use pg_query::protobuf::node::Node as NodeEnum;

/// Check if this is a DISTINCT ON query (vs regular DISTINCT)
pub fn is_distinct_on(stmt: &SelectStmt) -> bool {
    // DISTINCT ON has expressions in distinct_clause (not just empty for regular DISTINCT)
    // Regular DISTINCT has an empty list or a single node that doesn't represent an expression
    for node in &stmt.distinct_clause {
        if let Some(ref inner) = node.node {
            match inner {
                // These node types indicate DISTINCT ON with actual expressions
                NodeEnum::ColumnRef(_) 
                | NodeEnum::ResTarget(_) 
                | NodeEnum::FuncCall(_) 
                | NodeEnum::AExpr(_) 
                | NodeEnum::TypeCast(_) 
                | NodeEnum::AConst(_)
                | NodeEnum::CoalesceExpr(_)
                | NodeEnum::CaseExpr(_) => {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

/// Extract DISTINCT ON expressions as SQL strings
pub fn extract_distinct_on_exprs(stmt: &SelectStmt) -> Vec<String> {
    let mut exprs = Vec::new();
    for node in &stmt.distinct_clause {
        if let Some(ref inner) = node.node {
            let expr_sql = match inner {
                NodeEnum::ColumnRef(cr) => {
                    // Manually extract column reference fields
                    let fields: Vec<String> = cr.fields.iter()
                        .filter_map(|f| {
                            if let Some(NodeEnum::String(s)) = &f.node {
                                Some(s.sval.to_lowercase())
                            } else if let Some(NodeEnum::AStar(_)) = &f.node {
                                Some("*".to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                    Some(fields.join("."))
                }
                NodeEnum::FuncCall(fc) => {
                    // Extract function name and args
                    let func_parts: Vec<String> = fc.funcname.iter()
                        .filter_map(|n| {
                            if let Some(NodeEnum::String(s)) = &n.node {
                                Some(s.sval.to_lowercase())
                            } else {
                                None
                            }
                        })
                        .collect();
                    let func_name = func_parts.last().unwrap_or(&"".to_string()).clone();
                    
                    if fc.agg_star {
                        Some(format!("{}(*)", func_name))
                    } else {
                        let args: Vec<String> = fc.args.iter()
                            .filter_map(|a| {
                                if let Some(ref arg_inner) = a.node {
                                    extract_expr_string(arg_inner)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        Some(format!("{}({})", func_name, args.join(", ")))
                    }
                }
                NodeEnum::TypeCast(tc) => {
                    // Handle type casts like DATE(column)
                    if let Some(ref arg) = tc.arg {
                        if let Some(ref arg_inner) = arg.node {
                            if let Some(expr) = extract_expr_string(arg_inner) {
                                let type_name = tc.type_name.as_ref()
                                    .map(|tn| {
                                        tn.names.iter()
                                            .filter_map(|n| {
                                                if let Some(NodeEnum::String(s)) = &n.node {
                                                    Some(s.sval.to_lowercase())
                                                } else {
                                                    None
                                                }
                                            })
                                            .last()
                                            .unwrap_or_default()
                                    })
                                    .unwrap_or_default();
                                Some(format!("cast({} as {})", expr, type_name))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                NodeEnum::AExpr(ae) => {
                    // Handle expressions like a + b
                    extract_aexpr_string(ae)
                }
                _ => {
                    // Fallback to deparse for other types
                    node.deparse().ok().map(|s| s.to_lowercase())
                }
            };
            if let Some(sql) = expr_sql {
                exprs.push(sql);
            }
        }
    }
    exprs
}

/// Extract expression string from a node
fn extract_expr_string(node: &NodeEnum) -> Option<String> {
    match node {
        NodeEnum::ColumnRef(cr) => {
            let fields: Vec<String> = cr.fields.iter()
                .filter_map(|f| {
                    if let Some(NodeEnum::String(s)) = &f.node {
                        Some(s.sval.to_lowercase())
                    } else {
                        None
                    }
                })
                .collect();
            Some(fields.join("."))
        }
        NodeEnum::AConst(aconst) => {
            if let Some(ref val) = aconst.val {
                match val {
                    pg_query::protobuf::a_const::Val::Ival(i) => Some(i.ival.to_string()),
                    pg_query::protobuf::a_const::Val::Fval(f) => Some(f.fval.clone()),
                    pg_query::protobuf::a_const::Val::Sval(s) => Some(format!("'{}'", s.sval)),
                    pg_query::protobuf::a_const::Val::Boolval(b) => Some(if b.boolval { "true" } else { "false" }.to_string()),
                    _ => None,
                }
            } else {
                None
            }
        }
        NodeEnum::FuncCall(fc) => {
            let func_parts: Vec<String> = fc.funcname.iter()
                .filter_map(|n| {
                    if let Some(NodeEnum::String(s)) = &n.node {
                        Some(s.sval.to_lowercase())
                    } else {
                        None
                    }
                })
                .collect();
            let func_name = func_parts.last().unwrap_or(&"".to_string()).clone();
            
            if fc.agg_star {
                Some(format!("{}(*)", func_name))
            } else {
                let args: Vec<String> = fc.args.iter()
                    .filter_map(|a| {
                        if let Some(ref arg_inner) = a.node {
                            extract_expr_string(arg_inner)
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(format!("{}({})", func_name, args.join(", ")))
            }
        }
        NodeEnum::TypeCast(tc) => {
            if let Some(ref arg) = tc.arg {
                if let Some(ref arg_inner) = arg.node {
                    if let Some(expr) = extract_expr_string(arg_inner) {
                        let type_name = tc.type_name.as_ref()
                            .map(|tn| {
                                tn.names.iter()
                                    .filter_map(|n| {
                                        if let Some(NodeEnum::String(s)) = &n.node {
                                            Some(s.sval.to_lowercase())
                                        } else {
                                            None
                                        }
                                    })
                                    .last()
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default();
                        return Some(format!("cast({} as {})", expr, type_name));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Extract AExpr string (binary operations)
fn extract_aexpr_string(ae: &pg_query::protobuf::AExpr) -> Option<String> {
    
    let left = ae.lexpr.as_ref().and_then(|n| {
        if let Some(ref inner) = n.node {
            extract_expr_string(inner)
        } else {
            None
        }
    });
    
    let right = ae.rexpr.as_ref().and_then(|n| {
        if let Some(ref inner) = n.node {
            extract_expr_string(inner)
        } else {
            None
        }
    });
    
    let op = ae.name.first().and_then(|n| {
        if let Some(NodeEnum::String(s)) = &n.node {
            Some(s.sval.clone())
        } else {
            None
        }
    });
    
    match (left, op, right) {
        (Some(l), Some(op), Some(r)) => Some(format!("{} {} {}", l, op, r)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_query::parse;

    #[test]
    fn test_distinct_on_detection_single_col() {
        let sql = "SELECT DISTINCT ON (user_id) user_id, name FROM users";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(NodeEnum::SelectStmt(select)) = &stmt.node {
            assert!(is_distinct_on(select));
            let exprs = extract_distinct_on_exprs(select);
            assert_eq!(exprs, vec!["user_id"]);
        }
    }

    #[test]
    fn test_distinct_on_detection_multiple_cols() {
        let sql = "SELECT DISTINCT ON (dept, role) name, salary FROM employees ORDER BY dept, role, salary DESC";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(NodeEnum::SelectStmt(select)) = &stmt.node {
            assert!(is_distinct_on(select));
            let exprs = extract_distinct_on_exprs(select);
            assert_eq!(exprs, vec!["dept", "role"]);
        }
    }

    #[test]
    fn test_regular_distinct_not_detected() {
        let sql = "SELECT DISTINCT user_id, name FROM users";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(NodeEnum::SelectStmt(select)) = &stmt.node {
            assert!(!is_distinct_on(select));
        }
    }

    #[test]
    fn test_distinct_on_with_function() {
        let sql = "SELECT DISTINCT ON (DATE(created_at)) * FROM logs ORDER BY DATE(created_at), priority DESC";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(NodeEnum::SelectStmt(select)) = &stmt.node {
            assert!(is_distinct_on(select));
            let exprs = extract_distinct_on_exprs(select);
            assert_eq!(exprs.len(), 1);
            assert!(exprs[0].contains("date"));
        }
    }

    #[test]
    fn test_distinct_on_with_expression() {
        let sql = "SELECT DISTINCT ON (a + b) a, b, c FROM t ORDER BY a + b";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(NodeEnum::SelectStmt(select)) = &stmt.node {
            assert!(is_distinct_on(select));
        }
    }

    #[test]
    fn test_no_distinct() {
        let sql = "SELECT user_id, name FROM users";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(NodeEnum::SelectStmt(select)) = &stmt.node {
            assert!(!is_distinct_on(select));
        }
    }
}
