//! DISTINCT ON polyfill for PostgreSQL compatibility
//!
//! PostgreSQL's DISTINCT ON is not supported in SQLite.
//! We polyfill it using ROW_NUMBER() window function.

use pg_query::protobuf::SelectStmt;

/// Check if this is a DISTINCT ON query
pub fn is_distinct_on(stmt: &SelectStmt) -> bool {
    // Check if any item in distinct_clause is a column reference
    use pg_query::protobuf::node::Node as NodeEnum;
    
    for node in &stmt.distinct_clause {
        if let Some(ref inner) = node.node {
            match inner {
                NodeEnum::ColumnRef(_) | NodeEnum::ResTarget(_) => return true,
                _ => {}
            }
        }
    }
    
    false
}

/// Transform DISTINCT ON to ROW_NUMBER() window function
pub fn transform_distinct_on(_stmt: &SelectStmt) -> String {
    // TODO: Implement full transformation
    // For now, return a placeholder that indicates the feature is recognized
    String::from("SELECT 1 as __distinct_on_placeholder")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_query::parse;

    #[test]
    fn test_distinct_on_detection() {
        let sql = "SELECT DISTINCT ON (user_id) user_id, name FROM users";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            assert!(is_distinct_on(select));
        }
    }

    #[test]
    fn test_regular_distinct_not_detected() {
        let sql = "SELECT DISTINCT user_id, name FROM users";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            assert!(!is_distinct_on(select));
        }
    }
}
