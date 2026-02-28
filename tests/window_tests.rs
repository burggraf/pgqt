use postgresqlite::transpiler::transpile;

// ============================================================================
// Basic Window Functions
// ============================================================================

#[test]
fn test_row_number_basic() {
    let input = "SELECT row_number() OVER () FROM users";
    let result = transpile(input);
    assert!(result.contains("row_number()"));
    assert!(result.contains("over"));
}

#[test]
fn test_row_number_with_order() {
    let input = "SELECT row_number() OVER (ORDER BY id) FROM users";
    let result = transpile(input);
    assert!(result.contains("row_number()"));
    assert!(result.contains("over"));
    assert!(result.contains("order by id"));
}

#[test]
fn test_rank_basic() {
    let input = "SELECT rank() OVER (ORDER BY score DESC) FROM players";
    let result = transpile(input);
    assert!(result.contains("rank()"));
    assert!(result.contains("over"));
    assert!(result.contains("order by score desc"));
}

#[test]
fn test_dense_rank() {
    let input = "SELECT dense_rank() OVER (ORDER BY score) FROM players";
    let result = transpile(input);
    assert!(result.contains("dense_rank()"));
    assert!(result.contains("over"));
}

#[test]
fn test_percent_rank() {
    let input = "SELECT percent_rank() OVER (ORDER BY score) FROM players";
    let result = transpile(input);
    assert!(result.contains("percent_rank()"));
    assert!(result.contains("over"));
}

#[test]
fn test_cume_dist() {
    let input = "SELECT cume_dist() OVER (ORDER BY score) FROM players";
    let result = transpile(input);
    assert!(result.contains("cume_dist()"));
    assert!(result.contains("over"));
}

#[test]
fn test_ntile() {
    let input = "SELECT ntile(4) OVER (ORDER BY score) FROM players";
    let result = transpile(input);
    assert!(result.contains("ntile(4)"));
    assert!(result.contains("over"));
}

// ============================================================================
// Offset Functions
// ============================================================================

#[test]
fn test_lag_basic() {
    let input = "SELECT lag(name) OVER (ORDER BY id) FROM users";
    let result = transpile(input);
    assert!(result.contains("lag(name)"));
    assert!(result.contains("over"));
    assert!(result.contains("order by id"));
}

#[test]
fn test_lag_with_offset() {
    let input = "SELECT lag(name, 2) OVER (ORDER BY id) FROM users";
    let result = transpile(input);
    assert!(result.contains("lag(name, 2)"));
    assert!(result.contains("over"));
}

#[test]
fn test_lag_with_default() {
    let input = "SELECT lag(name, 1, 'N/A') OVER (ORDER BY id) FROM users";
    let result = transpile(input);
    // Note: string literals preserve their original case
    assert!(result.contains("lag(name, 1, 'N/A')"));
    assert!(result.contains("over"));
}

#[test]
fn test_lead_basic() {
    let input = "SELECT lead(name) OVER (ORDER BY id) FROM users";
    let result = transpile(input);
    assert!(result.contains("lead(name)"));
    assert!(result.contains("over"));
}

#[test]
fn test_lead_with_offset() {
    let input = "SELECT lead(name, 3) OVER (ORDER BY id) FROM users";
    let result = transpile(input);
    assert!(result.contains("lead(name, 3)"));
    assert!(result.contains("over"));
}

#[test]
fn test_first_value() {
    let input = "SELECT first_value(name) OVER (PARTITION BY dept) FROM employees";
    let result = transpile(input);
    assert!(result.contains("first_value(name)"));
    assert!(result.contains("over"));
    assert!(result.contains("partition by dept"));
}

#[test]
fn test_last_value() {
    let input = "SELECT last_value(name) OVER (PARTITION BY dept ORDER BY id) FROM employees";
    let result = transpile(input);
    assert!(result.contains("last_value(name)"));
    assert!(result.contains("over"));
    assert!(result.contains("partition by dept"));
}

#[test]
fn test_nth_value() {
    let input = "SELECT nth_value(name, 3) OVER (ORDER BY id) FROM employees";
    let result = transpile(input);
    assert!(result.contains("nth_value(name, 3)"));
    assert!(result.contains("over"));
}

// ============================================================================
// Aggregate Functions as Window Functions
// ============================================================================

#[test]
fn test_sum_window() {
    let input = "SELECT sum(salary) OVER (PARTITION BY dept) FROM employees";
    let result = transpile(input);
    assert!(result.contains("sum(salary)"));
    assert!(result.contains("over"));
    assert!(result.contains("partition by dept"));
}

#[test]
fn test_avg_window() {
    let input = "SELECT avg(score) OVER (PARTITION BY class) FROM students";
    let result = transpile(input);
    assert!(result.contains("avg(score)"));
    assert!(result.contains("over"));
}

#[test]
fn test_count_window() {
    let input = "SELECT count(*) OVER (PARTITION BY dept) FROM employees";
    let result = transpile(input);
    assert!(result.contains("count(*)"));
    assert!(result.contains("over"));
}

#[test]
fn test_min_max_window() {
    let input = "SELECT min(score) OVER (), max(score) OVER () FROM players";
    let result = transpile(input);
    assert!(result.contains("min(score)"));
    assert!(result.contains("max(score)"));
    assert!(result.contains("over"));
}

// ============================================================================
// PARTITION BY
// ============================================================================

#[test]
fn test_partition_by_single() {
    let input = "SELECT row_number() OVER (PARTITION BY dept) FROM employees";
    let result = transpile(input);
    assert!(result.contains("partition by dept"));
}

#[test]
fn test_partition_by_multiple() {
    let input = "SELECT row_number() OVER (PARTITION BY dept, role ORDER BY salary) FROM employees";
    let result = transpile(input);
    assert!(result.contains("partition by dept, role"));
    assert!(result.contains("order by salary"));
}

#[test]
fn test_partition_by_expression() {
    let input = "SELECT sum(amount) OVER (PARTITION BY date_trunc('month', created_at)) FROM orders";
    let result = transpile(input);
    assert!(result.contains("partition by"));
    assert!(result.contains("date_trunc('month', created_at)"));
}

// ============================================================================
// ORDER BY in Window
// ============================================================================

#[test]
fn test_order_by_asc() {
    let input = "SELECT row_number() OVER (ORDER BY id ASC) FROM users";
    let result = transpile(input);
    assert!(result.contains("order by id asc"));
}

#[test]
fn test_order_by_desc() {
    let input = "SELECT row_number() OVER (ORDER BY id DESC) FROM users";
    let result = transpile(input);
    assert!(result.contains("order by id desc"));
}

#[test]
fn test_order_by_multiple() {
    let input = "SELECT row_number() OVER (ORDER BY dept ASC, salary DESC) FROM employees";
    let result = transpile(input);
    assert!(result.contains("order by dept asc, salary desc"));
}

// ============================================================================
// Frame Specifications - ROWS
// ============================================================================

#[test]
fn test_rows_unbounded_preceding() {
    let input = "SELECT sum(salary) OVER (ORDER BY id ROWS UNBOUNDED PRECEDING) FROM employees";
    let result = transpile(input);
    assert!(result.contains("rows unbounded preceding"));
}

#[test]
fn test_rows_between_unbounded_current() {
    let input = "SELECT sum(salary) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM employees";
    let result = transpile(input);
    assert!(result.contains("rows between unbounded preceding and current row"));
}

#[test]
fn test_rows_between_n_preceding_following() {
    let input = "SELECT avg(price) OVER (ORDER BY date ROWS BETWEEN 3 PRECEDING AND 3 FOLLOWING) FROM stocks";
    let result = transpile(input);
    assert!(result.contains("rows between 3 preceding and 3 following"));
}

#[test]
fn test_rows_between_unbounded_following() {
    let input = "SELECT sum(salary) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) FROM employees";
    let result = transpile(input);
    assert!(result.contains("rows between unbounded preceding and unbounded following"));
}

#[test]
fn test_rows_current_row() {
    let input = "SELECT count(*) OVER (ORDER BY id ROWS CURRENT ROW) FROM users";
    let result = transpile(input);
    assert!(result.contains("rows current row"));
}

// ============================================================================
// Frame Specifications - RANGE
// ============================================================================

#[test]
fn test_range_unbounded_preceding() {
    let input = "SELECT sum(salary) OVER (ORDER BY id RANGE UNBOUNDED PRECEDING) FROM employees";
    let result = transpile(input);
    assert!(result.contains("range unbounded preceding"));
}

#[test]
fn test_range_between() {
    let input = "SELECT sum(salary) OVER (ORDER BY id RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM employees";
    let result = transpile(input);
    assert!(result.contains("range between unbounded preceding and current row"));
}

#[test]
fn test_range_between_full() {
    let input = "SELECT sum(salary) OVER (ORDER BY id RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) FROM employees";
    let result = transpile(input);
    assert!(result.contains("range between unbounded preceding and unbounded following"));
}

// ============================================================================
// Frame Specifications - GROUPS
// ============================================================================

#[test]
fn test_groups_between() {
    let input = "SELECT sum(amount) OVER (ORDER BY category GROUPS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM items";
    let result = transpile(input);
    assert!(result.contains("groups between 1 preceding and 1 following"));
}

// ============================================================================
// Complex Window Queries
// ============================================================================

#[test]
fn test_multiple_windows() {
    let input = "SELECT row_number() OVER (ORDER BY id), rank() OVER (ORDER BY score) FROM players";
    let result = transpile(input);
    assert!(result.contains("row_number()"));
    assert!(result.contains("rank()"));
    // Should have two OVER clauses
    assert!(result.matches("over").count() >= 2);
}

#[test]
fn test_running_total() {
    let input = "SELECT id, amount, sum(amount) OVER (ORDER BY date ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS running_total FROM orders";
    let result = transpile(input);
    assert!(result.contains("sum(amount)"));
    assert!(result.contains("rows between unbounded preceding and current row"));
    // Note: aliases may be quoted by deparse
    assert!(result.contains("running_total"));
}

#[test]
fn test_moving_average() {
    let input = "SELECT date, price, avg(price) OVER (ORDER BY date ROWS BETWEEN 6 PRECEDING AND CURRENT ROW) AS moving_avg FROM stocks";
    let result = transpile(input);
    assert!(result.contains("avg(price)"));
    assert!(result.contains("rows between 6 preceding and current row"));
}

#[test]
fn test_window_with_where() {
    let input = "SELECT name, sum(salary) OVER (PARTITION BY dept) FROM employees WHERE active = true";
    let result = transpile(input);
    assert!(result.contains("where"));
    assert!(result.contains("active = 1"));
    assert!(result.contains("sum(salary)"));
    assert!(result.contains("partition by dept"));
}

#[test]
fn test_window_with_join() {
    let input = "SELECT e.name, d.name as dept, rank() OVER (PARTITION BY d.id ORDER BY e.salary DESC) FROM employees e JOIN departments d ON e.dept_id = d.id";
    let result = transpile(input);
    assert!(result.contains("join"));
    assert!(result.contains("rank()"));
    assert!(result.contains("partition by d.id"));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_over() {
    let input = "SELECT row_number() OVER () FROM users";
    let result = transpile(input);
    assert!(result.contains("row_number() over ()"));
}

#[test]
fn test_nested_function_in_window() {
    let input = "SELECT sum(coalesce(bonus, 0)) OVER (PARTITION BY dept) FROM employees";
    let result = transpile(input);
    assert!(result.contains("sum(coalesce(bonus, 0))"));
    assert!(result.contains("partition by dept"));
}

#[test]
fn test_window_function_in_subquery() {
    let input = "SELECT * FROM (SELECT id, rank() OVER (ORDER BY score) as rnk FROM players) sub WHERE rnk <= 10";
    let result = transpile(input);
    assert!(result.contains("rank()"));
    assert!(result.contains("over"));
}

// ============================================================================
// Frame Offset Tests
// ============================================================================

#[test]
fn test_rows_offset_preceding() {
    let input = "SELECT avg(price) OVER (ORDER BY date ROWS 5 PRECEDING) FROM stocks";
    let result = transpile(input);
    assert!(result.contains("rows 5 preceding"));
}

#[test]
fn test_rows_offset_following_start() {
    let input = "SELECT sum(amount) OVER (ORDER BY id ROWS BETWEEN 1 FOLLOWING AND UNBOUNDED FOLLOWING) FROM orders";
    let result = transpile(input);
    assert!(result.contains("rows between 1 following and unbounded following"));
}

#[test]
fn test_range_with_offset() {
    // Note: RANGE with offset requires proper data types in SQLite
    let input = "SELECT sum(value) OVER (ORDER BY ts RANGE BETWEEN INTERVAL '1 day' PRECEDING AND CURRENT ROW) FROM metrics";
    let result = transpile(input);
    // Should contain the window clause (even if INTERVAL needs special handling)
    assert!(result.contains("over"));
}
