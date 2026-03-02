use pgqt::transpiler::transpile;

// ============================================================================
// Basic DISTINCT ON Tests
// ============================================================================

#[test]
fn test_distinct_on_single_column() {
    let input = "SELECT DISTINCT ON (user_id) user_id, name FROM users ORDER BY user_id";
    let result = transpile(input);
    
    // Should contain ROW_NUMBER() window function
    assert!(result.contains("row_number()"));
    assert!(result.contains("over"));
    assert!(result.contains("partition by user_id"));
    assert!(result.contains("__rn"));
    assert!(result.contains("where \"__rn\" = 1"));
}

#[test]
fn test_distinct_on_multiple_columns() {
    let input = "SELECT DISTINCT ON (dept, role) name, salary FROM employees ORDER BY dept, role, salary DESC";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("partition by dept, role"));
    assert!(result.contains("order by dept, role, salary desc"));
}

#[test]
fn test_distinct_on_with_where() {
    let input = "SELECT DISTINCT ON (customer_id) * FROM orders WHERE status = 'active' ORDER BY customer_id, created_at DESC";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("where status = 'active'"));
    assert!(result.contains("partition by customer_id"));
}

#[test]
fn test_distinct_on_with_limit() {
    let input = "SELECT DISTINCT ON (user_id) * FROM orders ORDER BY user_id LIMIT 10";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("limit 10"));
}

#[test]
fn test_distinct_on_with_offset() {
    let input = "SELECT DISTINCT ON (category) * FROM products ORDER BY category LIMIT 5 OFFSET 10";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("limit 5"));
    assert!(result.contains("offset 10"));
}

#[test]
fn test_distinct_on_no_order_by() {
    let input = "SELECT DISTINCT ON (dept) name FROM employees";
    let result = transpile(input);
    
    // Should use DISTINCT ON column as ORDER BY in window
    assert!(result.contains("row_number()"));
    assert!(result.contains("partition by dept"));
    assert!(result.contains("order by dept"));
}

#[test]
fn test_distinct_on_with_function() {
    let input = "SELECT DISTINCT ON (DATE(created_at)) * FROM logs ORDER BY DATE(created_at)";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("partition by"));
    assert!(result.contains("date(created_at)"));
}

#[test]
fn test_distinct_on_star() {
    let input = "SELECT DISTINCT ON (user_id) * FROM users ORDER BY user_id";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("partition by user_id"));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_distinct_on_with_join() {
    let input = "SELECT DISTINCT ON (o.customer_id) o.id, c.name FROM orders o JOIN customers c ON o.customer_id = c.id ORDER BY o.customer_id, o.created_at DESC";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("join"));
    assert!(result.contains("partition by o.customer_id"));
}

#[test]
fn test_distinct_on_preserves_outer_order() {
    let input = "SELECT DISTINCT ON (user_id) user_id, name, score FROM users ORDER BY user_id, score DESC";
    let result = transpile(input);
    
    // The outer query should preserve the ORDER BY
    // Check that order by appears at the end (after WHERE __rn = 1)
    let parts: Vec<&str> = result.split(" where ").collect();
    if parts.len() > 1 {
        let after_where = parts[1];
        assert!(after_where.contains("order by"));
    }
}

// ============================================================================
// Regular DISTINCT (should NOT transform)
// ============================================================================

#[test]
fn test_regular_distinct_unchanged() {
    let input = "SELECT DISTINCT user_id, name FROM users";
    let result = transpile(input);
    
    // Should NOT contain ROW_NUMBER()
    assert!(!result.contains("row_number()"));
    assert!(!result.contains("__rn"));
    // Should just be SELECT DISTINCT
    assert!(result.contains("select distinct"));
}

#[test]
fn test_no_distinct_unchanged() {
    let input = "SELECT user_id, name FROM users ORDER BY user_id";
    let result = transpile(input);
    
    assert!(!result.contains("row_number()"));
    assert!(!result.contains("__rn"));
    assert!(!result.contains("partition by"));
}

#[test]
fn test_subquery_structure() {
    let input = "SELECT DISTINCT ON (customer_id) customer_id, order_date FROM orders ORDER BY customer_id";
    let result = transpile(input);
    
    // Should have a subquery structure
    assert!(result.contains("select"));
    assert!(result.contains("from ("));
    assert!(result.contains(") as"));
    assert!(result.contains("where \"__rn\" = 1"));
}

#[test]
fn test_distinct_on_expression() {
    let input = "SELECT DISTINCT ON (a + b) a, b, c FROM t ORDER BY a + b";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("partition by"));
}
