# DISTINCT ON Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Implement PostgreSQL's DISTINCT ON clause by polyfilling it using ROW_NUMBER() window functions for SQLite compatibility.

**Architecture:** Transform `SELECT DISTINCT ON (expr1, expr2) ... ORDER BY expr1, expr2, sort_col` into a subquery with `ROW_NUMBER() OVER (PARTITION BY expr1, expr2 ORDER BY ...)`, then filter `WHERE __rn = 1`. This approach is the most portable and maintains PostgreSQL compatibility.

**Tech Stack:** Rust, pg_query (PostgreSQL 17 parser), SQLite window functions

---

## PostgreSQL DISTINCT ON Behavior

### Syntax
```sql
SELECT DISTINCT ON (expression1, expression2, ...) 
    column1, column2, ...
FROM table_name
[WHERE condition]
ORDER BY expression1, expression2, ..., sort_column [ASC|DESC]
[LIMIT n]
```

### Key Rules (Must Implement Correctly)
1. **Leftmost Requirement**: DISTINCT ON expressions must match the leftmost ORDER BY expressions exactly, in the same order
2. **NULL Handling**: All NULL values are treated as equal (returns first NULL row encountered)
3. **Expression Support**: Can use column refs, function calls, or any valid expression
4. **First Row Per Group**: Returns the first row encountered in each distinct group
5. **Error on Mismatch**: PostgreSQL throws error if ORDER BY doesn't start with DISTINCT ON expressions

### Polyfill Transformation

**Input:**
```sql
SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
FROM orders
ORDER BY customer_id, order_date DESC
```

**Output:**
```sql
SELECT * FROM (
    SELECT customer_id, order_date, amount, 
           ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY customer_id, order_date DESC) as __rn
    FROM orders
) AS __distinct_on_sub
WHERE __rn = 1
ORDER BY customer_id, order_date DESC
```

---

## Task 1: Update distinct_on.rs with Full Transformation

**Files:**
- Modify: `src/distinct_on.rs`

**Step 1: Write the detection function**

Replace the placeholder with proper detection logic:

```rust
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

use pg_query::protobuf::{SelectStmt, Node as PgNode};
use pg_query::protobuf::node::Node as NodeEnum;

/// Check if this is a DISTINCT ON query (vs regular DISTINCT)
pub fn is_distinct_on(stmt: &SelectStmt) -> bool {
    // DISTINCT ON has expressions in distinct_clause (not just empty for regular DISTINCT)
    // Regular DISTINCT has an empty SetOperation or a single NULL node
    for node in &stmt.distinct_clause {
        if let Some(ref inner) = node.node {
            match inner {
                NodeEnum::ColumnRef(_) | NodeEnum::ResTarget(_) | NodeEnum::FuncCall(_) 
                | NodeEnum::AExpr(_) | NodeEnum::TypeCast(_) | NodeEnum::AConst(_) => {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

/// Extract DISTINCT ON expressions from the statement
pub fn extract_distinct_on_exprs(stmt: &SelectStmt) -> Vec<String> {
    let mut exprs = Vec::new();
    for node in &stmt.distinct_clause {
        if let Some(ref inner) = node.node {
            match inner {
                NodeEnum::ColumnRef(_) | NodeEnum::ResTarget(_) | NodeEnum::FuncCall(_) 
                | NodeEnum::AExpr(_) | NodeEnum::TypeCast(_) | NodeEnum::AConst(_) => {
                    // Use deparse to get the SQL string for this expression
                    if let Ok(sql) = node.deparse() {
                        exprs.push(sql.to_lowercase());
                    }
                }
                _ => {}
            }
        }
    }
    exprs
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
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
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
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
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
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            assert!(!is_distinct_on(select));
        }
    }

    #[test]
    fn test_distinct_on_with_function() {
        let sql = "SELECT DISTINCT ON (DATE(created_at)) * FROM logs ORDER BY DATE(created_at), priority DESC";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            assert!(is_distinct_on(select));
            let exprs = extract_distinct_on_exprs(select);
            assert!(exprs.len() == 1);
            assert!(exprs[0].contains("date"));
        }
    }

    #[test]
    fn test_distinct_on_with_expression() {
        let sql = "SELECT DISTINCT ON (a + b) a, b, c FROM t ORDER BY a + b";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            assert!(is_distinct_on(select));
        }
    }
}
```

**Step 2: Run tests to verify detection works**

Run: `cargo test distinct_on --lib`
Expected: Tests pass for detection

**Step 3: Commit**

```bash
git add src/distinct_on.rs
git commit -m "feat(distinct_on): add detection and expression extraction"
```

---

## Task 2: Implement Transformation Logic in distinct_on.rs

**Files:**
- Modify: `src/distinct_on.rs`

**Step 1: Add transformation function**

Add after the extraction function:

```rust
/// Configuration for DISTINCT ON transformation
#[derive(Debug, Clone)]
pub struct DistinctOnConfig {
    /// The DISTINCT ON expressions (PARTITION BY columns)
    pub partition_exprs: Vec<String>,
    /// The full ORDER BY clause for the window function
    pub order_by: String,
    /// Whether there's an existing ORDER BY
    pub has_order_by: bool,
    /// The original LIMIT value (if any)
    pub limit: Option<String>,
    /// The original OFFSET value (if any)
    pub offset: Option<String>,
}

/// Parse a SELECT statement and extract DISTINCT ON configuration
pub fn parse_distinct_on_config(stmt: &SelectStmt, ctx: &mut crate::transpiler::TranspileContext) -> Option<DistinctOnConfig> {
    if !is_distinct_on(stmt) {
        return None;
    }
    
    let partition_exprs = extract_distinct_on_exprs(stmt);
    if partition_exprs.is_empty() {
        return None;
    }
    
    // Extract ORDER BY clause
    let has_order_by = !stmt.sort_clause.is_empty();
    let order_by = if has_order_by {
        let sorts: Vec<String> = stmt.sort_clause
            .iter()
            .map(|n| crate::transpiler::reconstruct_sort_by_public(n, ctx))
            .collect();
        sorts.join(", ")
    } else {
        // If no ORDER BY, use the DISTINCT ON expressions as ORDER BY
        partition_exprs.join(", ")
    };
    
    // Extract LIMIT and OFFSET
    let limit = stmt.limit_count.as_ref().and_then(|n| {
        let sql = crate::transpiler::reconstruct_node_public(n, ctx);
        if sql.is_empty() || sql.to_uppercase() == "NULL" {
            None
        } else {
            Some(sql)
        }
    });
    
    let offset = stmt.limit_offset.as_ref().and_then(|n| {
        let sql = crate::transpiler::reconstruct_node_public(n, ctx);
        if sql.is_empty() {
            None
        } else {
            Some(sql)
        }
    });
    
    Some(DistinctOnConfig {
        partition_exprs,
        order_by,
        has_order_by,
        limit,
        offset,
    })
}

#[cfg(test)]
mod transform_tests {
    use super::*;
    use crate::transpiler::TranspileContext;
    use pg_query::parse;

    #[test]
    fn test_parse_config_basic() {
        let sql = "SELECT DISTINCT ON (user_id) user_id, name FROM users ORDER BY user_id, created_at DESC";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            let mut ctx = TranspileContext::new();
            let config = parse_distinct_on_config(select, &mut ctx);
            assert!(config.is_some());
            let config = config.unwrap();
            assert_eq!(config.partition_exprs, vec!["user_id"]);
            assert!(config.has_order_by);
            assert!(config.order_by.contains("user_id"));
        }
    }

    #[test]
    fn test_parse_config_no_order_by() {
        let sql = "SELECT DISTINCT ON (dept) name FROM employees";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            let mut ctx = TranspileContext::new();
            let config = parse_distinct_on_config(select, &mut ctx);
            assert!(config.is_some());
            let config = config.unwrap();
            assert!(!config.has_order_by);
            // Should use DISTINCT ON expressions as ORDER BY
            assert_eq!(config.order_by, "dept");
        }
    }

    #[test]
    fn test_parse_config_with_limit() {
        let sql = "SELECT DISTINCT ON (user_id) * FROM orders ORDER BY user_id LIMIT 10";
        let result = parse(sql).unwrap();
        let stmt = result.protobuf.stmts[0].stmt.as_ref().unwrap();
        
        if let Some(pg_query::protobuf::node::Node::SelectStmt(select)) = &stmt.node {
            let mut ctx = TranspileContext::new();
            let config = parse_distinct_on_config(select, &mut ctx);
            assert!(config.is_some());
            let config = config.unwrap();
            assert_eq!(config.limit, Some("10".to_string()));
        }
    }
}
```

**Step 2: Make helper functions public in transpiler.rs**

Add public wrappers (we'll add these in Task 3).

**Step 3: Run tests**

Run: `cargo test distinct_on --lib`
Expected: Tests pass

**Step 4: Commit**

```bash
git add src/distinct_on.rs
git commit -m "feat(distinct_on): add transformation configuration parsing"
```

---

## Task 3: Integrate DISTINCT ON into transpiler.rs

**Files:**
- Modify: `src/transpiler.rs`

**Step 1: Add public helper functions for distinct_on module**

Add near the end of transpiler.rs (before the tests module):

```rust
// ============================================================================
// Public helper functions for DISTINCT ON module
// ============================================================================

/// Public wrapper for reconstruct_sort_by for use by distinct_on module
pub fn reconstruct_sort_by_public(node: &Node, ctx: &mut TranspileContext) -> String {
    reconstruct_sort_by(node, ctx)
}

/// Public wrapper for reconstruct_node for use by distinct_on module  
pub fn reconstruct_node_public(node: &Node, ctx: &mut TranspileContext) -> String {
    reconstruct_node(node, ctx)
}
```

**Step 2: Modify reconstruct_select_stmt to handle DISTINCT ON**

Replace the existing DISTINCT ON handling in `reconstruct_select_stmt`:

```rust
/// Reconstruct a SELECT statement
fn reconstruct_select_stmt(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    // Check if this is a VALUES statement (used in INSERT)
    if !stmt.values_lists.is_empty() {
        return reconstruct_values_stmt(stmt, ctx);
    }

    // Handle DISTINCT ON - transform to ROW_NUMBER() window function
    if crate::distinct_on::is_distinct_on(stmt) {
        return reconstruct_distinct_on_select(stmt, ctx);
    }

    // Handle regular DISTINCT
    if !stmt.distinct_clause.is_empty() {
        parts.push("select distinct".to_string());
    } else {
        parts.push("select".to_string());
    }

    // ... rest of the existing function unchanged ...
```

**Step 3: Add the DISTINCT ON transformation function**

Add before `reconstruct_select_stmt`:

```rust
/// Reconstruct a SELECT statement with DISTINCT ON using ROW_NUMBER() polyfill
fn reconstruct_distinct_on_select(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    use crate::distinct_on::{is_distinct_on, extract_distinct_on_exprs};
    
    // Extract DISTINCT ON expressions
    let partition_exprs = extract_distinct_on_exprs(stmt);
    if partition_exprs.is_empty() {
        // Fallback to regular SELECT
        return reconstruct_select_stmt_fallback(stmt, ctx);
    }
    
    // Build inner query columns
    let mut inner_cols = Vec::new();
    if stmt.target_list.is_empty() {
        inner_cols.push("*".to_string());
    } else {
        for n in &stmt.target_list {
            inner_cols.push(reconstruct_node(n, ctx));
        }
    }
    
    // Build ROW_NUMBER() OVER clause
    let partition_by = partition_exprs.join(", ");
    
    // Build ORDER BY for window (must include DISTINCT ON expressions + additional sort)
    let order_by = if !stmt.sort_clause.is_empty() {
        let sorts: Vec<String> = stmt.sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        sorts.join(", ")
    } else {
        // No ORDER BY - use DISTINCT ON expressions
        partition_by.clone()
    };
    
    // Add ROW_NUMBER column
    let row_num_col = format!(
        "row_number() over (partition by {} order by {}) as \"__rn\"",
        partition_by, order_by
    );
    inner_cols.push(row_num_col);
    
    // Build inner query
    let mut inner_parts = Vec::new();
    inner_parts.push("select".to_string());
    inner_parts.push(inner_cols.join(", "));
    
    // FROM clause
    if !stmt.from_clause.is_empty() {
        inner_parts.push("from".to_string());
        let tables: Vec<String> = stmt.from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_parts.push(tables.join(", "));
    }
    
    // WHERE clause
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            inner_parts.push("where".to_string());
            inner_parts.push(where_sql);
        }
    }
    
    // GROUP BY clause
    if !stmt.group_clause.is_empty() {
        inner_parts.push("group by".to_string());
        let groups: Vec<String> = stmt.group_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        inner_parts.push(groups.join(", "));
    }
    
    // HAVING clause
    if let Some(ref having_clause) = stmt.having_clause {
        let having_sql = reconstruct_node(having_clause, ctx);
        if !having_sql.is_empty() {
            inner_parts.push("having".to_string());
            inner_parts.push(having_sql);
        }
    }
    
    let inner_query = inner_parts.join(" ");
    
    // Build outer query
    let mut outer_parts = Vec::new();
    outer_parts.push("select * from".to_string());
    outer_parts.push(format!("({}) as \"__distinct_on_sub\"", inner_query));
    outer_parts.push("where".to_string());
    outer_parts.push("\"__rn\" = 1".to_string());
    
    // Preserve ORDER BY from original query (outer query)
    if !stmt.sort_clause.is_empty() {
        outer_parts.push("order by".to_string());
        let sorts: Vec<String> = stmt.sort_clause
            .iter()
            .map(|n| reconstruct_sort_by(n, ctx))
            .collect();
        outer_parts.push(sorts.join(", "));
    }
    
    // Preserve LIMIT
    if let Some(ref limit_count) = stmt.limit_count {
        let limit_sql = reconstruct_node(limit_count, ctx);
        if !limit_sql.is_empty() && limit_sql.to_uppercase() != "NULL" {
            outer_parts.push("limit".to_string());
            outer_parts.push(limit_sql);
        }
    }
    
    // Preserve OFFSET
    if let Some(ref limit_offset) = stmt.limit_offset {
        let offset_sql = reconstruct_node(limit_offset, ctx);
        if !offset_sql.is_empty() {
            outer_parts.push("offset".to_string());
            outer_parts.push(offset_sql);
        }
    }
    
    outer_parts.join(" ")
}

/// Fallback for when DISTINCT ON transformation fails
fn reconstruct_select_stmt_fallback(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Just use regular SELECT without DISTINCT ON
    let mut parts = Vec::new();
    parts.push("select".to_string());
    
    if stmt.target_list.is_empty() {
        parts.push("*".to_string());
    } else {
        let columns: Vec<String> = stmt.target_list
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(columns.join(", "));
    }
    
    if !stmt.from_clause.is_empty() {
        parts.push("from".to_string());
        let tables: Vec<String> = stmt.from_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(tables.join(", "));
    }
    
    if let Some(ref where_clause) = stmt.where_clause {
        let where_sql = reconstruct_node(where_clause, ctx);
        if !where_sql.is_empty() {
            parts.push("where".to_string());
            parts.push(where_sql);
        }
    }
    
    parts.join(" ")
}
```

**Step 4: Run unit tests**

Run: `cargo test transpiler --lib`
Expected: Existing tests pass

**Step 5: Commit**

```bash
git add src/transpiler.rs src/distinct_on.rs
git commit -m "feat(distinct_on): integrate DISTINCT ON transformation into transpiler"
```

---

## Task 4: Write Unit Tests for DISTINCT ON

**Files:**
- Create: `tests/distinct_on_tests.rs`

**Step 1: Create unit test file**

```rust
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
fn test_distinct_on_with_group_by() {
    // DISTINCT ON with GROUP BY is unusual but should work
    let input = "SELECT DISTINCT ON (category) category, COUNT(*) as cnt FROM products GROUP BY category ORDER BY category";
    let result = transpile(input);
    
    assert!(result.contains("row_number()"));
    assert!(result.contains("group by category"));
}

#[test]
fn test_distinct_on_preserves_outer_order() {
    let input = "SELECT DISTINCT ON (user_id) user_id, name, score FROM users ORDER BY user_id, score DESC";
    let result = transpile(input);
    
    // The outer query should preserve the ORDER BY
    assert!(result.contains("order by user_id, score desc"));
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
```

**Step 2: Run tests to verify they pass**

Run: `cargo test distinct_on_tests`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/distinct_on_tests.rs
git commit -m "test(distinct_on): add unit tests for DISTINCT ON transformation"
```

---

## Task 5: Write E2E Tests for DISTINCT ON

**Files:**
- Create: `tests/distinct_on_e2e_test.py`

**Step 1: Create E2E test file**

```python
#!/usr/bin/env python3
"""
End-to-End tests for PostgreSQL DISTINCT ON transpilation to SQLite.

This test suite verifies that DISTINCT ON is correctly transpiled
from PostgreSQL syntax to SQLite using ROW_NUMBER() and produces correct results.
"""

import os
import sys
import psycopg2
import pytest
import subprocess
import time

# Test database file
TEST_DB = "/tmp/pglite_distinct_on_test.db"
TEST_PORT = 55433


@pytest.fixture(scope="module")
def proxy_server():
    """Start the PGlite proxy server for testing."""
    # Build the proxy if needed
    subprocess.run(["cargo", "build", "--release"], check=True, capture_output=True)
    
    # Remove old test database
    if os.path.exists(TEST_DB):
        os.remove(TEST_DB)
    
    # Start the proxy server
    env = os.environ.copy()
    env["PGQT_DB"] = TEST_DB
    env["PGQT_PORT"] = str(TEST_PORT)
    
    proc = subprocess.Popen(
        ["./target/release/pgqt"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    
    # Wait for server to start
    time.sleep(2)
    
    yield proc
    
    # Cleanup
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    
    if os.path.exists(TEST_DB):
        os.remove(TEST_DB)


@pytest.fixture(scope="module")
def db_connection(proxy_server):
    """Create a database connection and set up test data."""
    conn = psycopg2.connect(
        host="127.0.0.1",
        port=TEST_PORT,
        user="postgres",
        database="test"
    )
    conn.autocommit = True
    
    # Create test tables
    with conn.cursor() as cur:
        # Orders table - classic DISTINCT ON use case
        cur.execute("""
            CREATE TABLE orders (
                id SERIAL PRIMARY KEY,
                customer_id INT,
                order_date DATE,
                amount DECIMAL(10, 2),
                status VARCHAR(20)
            )
        """)
        
        # Insert test data - multiple orders per customer
        cur.execute("""
            INSERT INTO orders (customer_id, order_date, amount, status) VALUES
                (1, '2023-01-01', 100.00, 'completed'),
                (1, '2023-01-15', 200.00, 'completed'),
                (1, '2023-02-01', 150.00, 'pending'),
                (2, '2023-01-02', 300.00, 'completed'),
                (2, '2023-01-20', 250.00, 'completed'),
                (3, '2023-01-03', 50.00, 'completed'),
                (3, '2023-01-10', 75.00, 'cancelled'),
                (3, '2023-02-05', 100.00, 'completed')
        """)
        
        # Employees table for department tests
        cur.execute("""
            CREATE TABLE employees (
                id SERIAL PRIMARY KEY,
                name VARCHAR(100),
                department VARCHAR(50),
                role VARCHAR(50),
                salary DECIMAL(10, 2),
                hire_date DATE
            )
        """)
        
        cur.execute("""
            INSERT INTO employees (name, department, role, salary, hire_date) VALUES
                ('Alice', 'Engineering', 'Senior', 100000, '2020-01-15'),
                ('Bob', 'Engineering', 'Junior', 70000, '2021-03-20'),
                ('Charlie', 'Engineering', 'Senior', 95000, '2019-06-10'),
                ('Diana', 'Sales', 'Manager', 90000, '2020-09-01'),
                ('Eve', 'Sales', 'Associate', 65000, '2021-07-15'),
                ('Frank', 'Sales', 'Senior', 85000, '2020-04-22'),
                ('Grace', 'Marketing', 'Manager', 80000, '2022-01-10'),
                ('Henry', 'Marketing', 'Associate', 60000, '2021-11-05')
        """)
        
        # Events table for date-based DISTINCT ON
        cur.execute("""
            CREATE TABLE events (
                id SERIAL PRIMARY KEY,
                user_id INT,
                event_type VARCHAR(50),
                event_date DATE,
                details TEXT
            )
        """)
        
        cur.execute("""
            INSERT INTO events (user_id, event_type, event_date, details) VALUES
                (1, 'login', '2023-01-01', 'Morning login'),
                (1, 'login', '2023-01-02', 'Afternoon login'),
                (1, 'purchase', '2023-01-02', 'Bought item A'),
                (2, 'login', '2023-01-01', 'First login'),
                (2, 'purchase', '2023-01-03', 'Bought item B'),
                (3, 'login', '2023-01-02', 'New user login')
        """)
    
    yield conn
    
    conn.close()


class TestBasicDistinctOn:
    """Tests for basic DISTINCT ON functionality."""
    
    def test_distinct_on_single_column(self, db_connection):
        """Test DISTINCT ON with single column - get first order per customer."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
                FROM orders
                ORDER BY customer_id, order_date
            """)
            rows = cur.fetchall()
            
            # Should get exactly one row per customer (3 customers)
            assert len(rows) == 3
            
            # Each customer should appear exactly once
            customer_ids = [r[0] for r in rows]
            assert sorted(customer_ids) == [1, 2, 3]
            
            # Should be the earliest order for each customer
            for row in rows:
                customer_id = row[0]
                if customer_id == 1:
                    assert str(row[1]) == '2023-01-01'  # Earliest
                elif customer_id == 2:
                    assert str(row[1]) == '2023-01-02'  # Earliest
                elif customer_id == 3:
                    assert str(row[1]) == '2023-01-03'  # Earliest
    
    def test_distinct_on_latest_per_group(self, db_connection):
        """Test DISTINCT ON to get latest order per customer."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
                FROM orders
                ORDER BY customer_id, order_date DESC
            """)
            rows = cur.fetchall()
            
            assert len(rows) == 3
            
            # Should be the latest order for each customer
            for row in rows:
                customer_id = row[0]
                if customer_id == 1:
                    assert str(row[1]) == '2023-02-01'  # Latest
                elif customer_id == 2:
                    assert str(row[1]) == '2023-01-20'  # Latest
                elif customer_id == 3:
                    assert str(row[1]) == '2023-02-05'  # Latest
    
    def test_distinct_on_with_where(self, db_connection):
        """Test DISTINCT ON with WHERE clause."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
                FROM orders
                WHERE status = 'completed'
                ORDER BY customer_id, order_date DESC
            """)
            rows = cur.fetchall()
            
            # Customer 3 has a later cancelled order, so latest completed is 2023-02-05
            assert len(rows) == 3
            
            for row in rows:
                customer_id = row[0]
                if customer_id == 3:
                    # Latest completed order (not the cancelled one)
                    assert str(row[1]) == '2023-02-05'


class TestDistinctOnMultipleColumns:
    """Tests for DISTINCT ON with multiple columns."""
    
    def test_distinct_on_two_columns(self, db_connection):
        """Test DISTINCT ON with multiple columns."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (department, role) department, role, name, salary
                FROM employees
                ORDER BY department, role, salary DESC
            """)
            rows = cur.fetchall()
            
            # Should get one row per (department, role) combination
            dept_roles = set((r[0], r[1]) for r in rows)
            
            # Engineering: Senior (2), Junior (1)
            # Sales: Manager (1), Senior (1), Associate (1)
            # Marketing: Manager (1), Associate (1)
            assert len(dept_roles) == 6
            
            # For Engineering/Senior, should get highest salary (Alice: 100000)
            eng_senior = [r for r in rows if r[0] == 'Engineering' and r[1] == 'Senior']
            assert len(eng_senior) == 1
            assert float(eng_senior[0][3]) == 100000.0


class TestDistinctOnWithLimit:
    """Tests for DISTINCT ON with LIMIT and OFFSET."""
    
    def test_distinct_on_with_limit(self, db_connection):
        """Test DISTINCT ON with LIMIT."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date
                FROM orders
                ORDER BY customer_id, order_date
                LIMIT 2
            """)
            rows = cur.fetchall()
            
            # Should get only 2 rows
            assert len(rows) == 2
            
            # Should be first 2 customers
            customer_ids = sorted([r[0] for r in rows])
            assert customer_ids == [1, 2]
    
    def test_distinct_on_with_limit_offset(self, db_connection):
        """Test DISTINCT ON with LIMIT and OFFSET."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date
                FROM orders
                ORDER BY customer_id, order_date
                LIMIT 1 OFFSET 1
            """)
            rows = cur.fetchall()
            
            # Should get only 1 row
            assert len(rows) == 1
            
            # Should be customer 2 (skipping customer 1)
            assert rows[0][0] == 2


class TestDistinctOnWithJoins:
    """Tests for DISTINCT ON with JOINs."""
    
    def test_distinct_on_with_join(self, db_connection):
        """Test DISTINCT ON with JOIN - should work correctly."""
        with db_connection.cursor() as cur:
            # First create a customers table
            cur.execute("""
                CREATE TABLE IF NOT EXISTS customers (
                    id INT PRIMARY KEY,
                    name VARCHAR(100)
                )
            """)
            cur.execute("""
                INSERT OR REPLACE INTO customers (id, name) VALUES
                    (1, 'Customer One'),
                    (2, 'Customer Two'),
                    (3, 'Customer Three')
            """)
            
            cur.execute("""
                SELECT DISTINCT ON (o.customer_id) o.customer_id, c.name, o.order_date
                FROM orders o
                JOIN customers c ON o.customer_id = c.id
                ORDER BY o.customer_id, o.order_date DESC
            """)
            rows = cur.fetchall()
            
            # Should get one row per customer with their latest order
            assert len(rows) == 3
            
            # Each should have the correct customer name
            for row in rows:
                assert row[1] is not None


class TestDistinctOnNoOrderBy:
    """Tests for DISTINCT ON without explicit ORDER BY."""
    
    def test_distinct_on_without_order_by(self, db_connection):
        """Test DISTINCT ON without ORDER BY - uses DISTINCT ON columns as implicit ORDER BY."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, amount
                FROM orders
            """)
            rows = cur.fetchall()
            
            # Should still get one row per customer
            assert len(rows) == 3
            
            # Each customer should appear exactly once
            customer_ids = [r[0] for r in rows]
            assert sorted(customer_ids) == [1, 2, 3]


class TestDistinctOnEdgeCases:
    """Tests for edge cases and special scenarios."""
    
    def test_distinct_on_with_nulls(self, db_connection):
        """Test DISTINCT ON with NULL values."""
        with db_connection.cursor() as cur:
            # Add some rows with NULL customer_id
            cur.execute("""
                INSERT INTO orders (customer_id, order_date, amount, status) VALUES
                    (NULL, '2023-03-01', 10.00, 'pending'),
                    (NULL, '2023-03-02', 20.00, 'completed')
            """)
            
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
                FROM orders
                WHERE customer_id IS NULL
                ORDER BY customer_id, order_date
            """)
            rows = cur.fetchall()
            
            # All NULLs are treated as equal, so only one row
            assert len(rows) == 1
    
    def test_distinct_on_select_star(self, db_connection):
        """Test DISTINCT ON with SELECT *."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) *
                FROM orders
                ORDER BY customer_id, order_date
            """)
            rows = cur.fetchall()
            
            # Should get one row per customer
            assert len(rows) >= 3  # May include NULL row from previous test
            
            # Should have all columns (id, customer_id, order_date, amount, status)
            assert len(rows[0]) == 5


class TestRegularDistinct:
    """Tests to ensure regular DISTINCT still works correctly."""
    
    def test_regular_distinct_single_column(self, db_connection):
        """Test regular DISTINCT on single column."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT customer_id FROM orders WHERE customer_id IS NOT NULL
            """)
            rows = cur.fetchall()
            
            # Should get 3 unique customer_ids
            assert len(rows) == 3
            customer_ids = sorted([r[0] for r in rows])
            assert customer_ids == [1, 2, 3]
    
    def test_regular_distinct_multiple_columns(self, db_connection):
        """Test regular DISTINCT on multiple columns."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT department, role FROM employees
            """)
            rows = cur.fetchall()
            
            # Engineering/Senior, Engineering/Junior, Sales/Manager, 
            # Sales/Senior, Sales/Associate, Marketing/Manager, Marketing/Associate
            assert len(rows) == 6


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v", "-s"]))
```

**Step 2: Run E2E tests**

Run: `python3 tests/distinct_on_e2e_test.py`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/distinct_on_e2e_test.py
git commit -m "test(distinct_on): add E2E tests for DISTINCT ON transformation"
```

---

## Task 6: Update Documentation

**Files:**
- Modify: `docs/TODO-FEATURES.md`
- Modify: `README.md`
- Create: `docs/DISTINCT_ON.md`

**Step 1: Update TODO-FEATURES.md**

Change the DISTINCT ON row from:
```markdown
| **`DISTINCT ON (...)`** | ⚠️ | Medium | Phase 3 Roadmap: Polyfill using window functions `ROW_NUMBER()`. |
```

To:
```markdown
| **`DISTINCT ON (...)`** | ✅ | Medium | Polyfilled using ROW_NUMBER() window function. See [docs/DISTINCT_ON.md](./DISTINCT_ON.md) for details. |
```

**Step 2: Update README.md**

Add to the Features section (after Window Functions):

```markdown
### DISTINCT ON

PGlite Proxy provides PostgreSQL-compatible DISTINCT ON support using ROW_NUMBER() window function polyfill:

```sql
-- Get the latest order for each customer
SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
FROM orders
ORDER BY customer_id, order_date DESC;

-- Get the highest paid employee in each department/role
SELECT DISTINCT ON (department, role) department, role, name, salary
FROM employees
ORDER BY department, role, salary DESC;
```

**Supported DISTINCT ON Features:**
- Single and multiple column expressions
- Expression-based DISTINCT ON (e.g., `DISTINCT ON (DATE(created_at))`)
- ORDER BY with different sort columns for tie-breaking
- LIMIT and OFFSET support
- WHERE clause filtering
- JOIN support

**Transformation:**
`SELECT DISTINCT ON (a) x, y FROM t ORDER BY a, b` is transformed to:
```sql
SELECT * FROM (
    SELECT x, y, ROW_NUMBER() OVER (PARTITION BY a ORDER BY a, b) as __rn
    FROM t
) AS __distinct_on_sub
WHERE __rn = 1
ORDER BY a, b
```

For complete documentation, see [docs/DISTINCT_ON.md](./docs/DISTINCT_ON.md).
```

**Step 3: Update Roadmap in README.md**

Change:
```markdown
### Phase 3 (In Progress)
- [x] **Users & Permissions (RBAC)** - Role-based access control with GRANT/REVOKE
- [x] **Schemas (Namespaces)** - Full schema support using SQLite ATTACH DATABASE
- [x] **Window Functions** - Full support for all PostgreSQL window functions with frame specifications
- [ ] `DISTINCT ON` polyfill using window functions
```

To:
```markdown
### Phase 3 (In Progress)
- [x] **Users & Permissions (RBAC)** - Role-based access control with GRANT/REVOKE
- [x] **Schemas (Namespaces)** - Full schema support using SQLite ATTACH DATABASE
- [x] **Window Functions** - Full support for all PostgreSQL window functions with frame specifications
- [x] `DISTINCT ON` polyfill using window functions
```

**Step 4: Create docs/DISTINCT_ON.md**

```markdown
# DISTINCT ON Support

PGlite Proxy implements PostgreSQL's `DISTINCT ON` clause using a `ROW_NUMBER()` window function polyfill.

## Overview

`DISTINCT ON` is a PostgreSQL-specific extension that returns only the first row of each group where specified expressions evaluate equally. It's commonly used to:

- Get the "most recent" row per group
- Get the "highest/lowest" value per group
- Deduplicate based on specific columns while preserving associated data

## Syntax

```sql
SELECT DISTINCT ON (expression1, expression2, ...) 
    column1, column2, ...
FROM table_name
[WHERE condition]
ORDER BY expression1, expression2, ..., sort_column [ASC|DESC]
[LIMIT n [OFFSET m]]
```

## Key Rules

### 1. Leftmost ORDER BY Requirement

The expressions in `DISTINCT ON` must match the leftmost `ORDER BY` expressions exactly, in the same order:

**Valid:**
```sql
SELECT DISTINCT ON (customer_id) customer_id, order_date
FROM orders
ORDER BY customer_id, order_date DESC;
```

**Invalid (PostgreSQL error):**
```sql
SELECT DISTINCT ON (customer_id) * FROM orders 
ORDER BY order_date DESC;
-- ERROR: SELECT DISTINCT ON expressions must match initial ORDER BY expressions
```

### 2. NULL Handling

All NULL values are treated as equal. If multiple rows have NULL in the DISTINCT ON column, only the first one (by ORDER BY) is returned.

### 3. Expression Support

You can use expressions in DISTINCT ON:

```sql
SELECT DISTINCT ON (DATE(created_at)) created_at, priority, message
FROM logs
ORDER BY DATE(created_at), priority DESC;
```

## Examples

### Get Latest Order Per Customer

```sql
SELECT DISTINCT ON (customer_id) 
    customer_id, order_date, amount
FROM orders
ORDER BY customer_id, order_date DESC;
```

### Get Highest Paid Employee Per Department

```sql
SELECT DISTINCT ON (department) 
    department, name, salary
FROM employees
ORDER BY department, salary DESC;
```

### Multiple Columns in DISTINCT ON

```sql
SELECT DISTINCT ON (department, role) 
    department, role, name, salary
FROM employees
ORDER BY department, role, salary DESC;
```

### With WHERE Clause

```sql
SELECT DISTINCT ON (customer_id) 
    customer_id, order_date, amount
FROM orders
WHERE status = 'completed'
ORDER BY customer_id, order_date DESC;
```

### With LIMIT

```sql
SELECT DISTINCT ON (customer_id) 
    customer_id, order_date
FROM orders
ORDER BY customer_id, order_date
LIMIT 10;
```

### With JOIN

```sql
SELECT DISTINCT ON (o.customer_id) 
    o.customer_id, c.name, o.order_date
FROM orders o
JOIN customers c ON o.customer_id = c.id
ORDER BY o.customer_id, o.order_date DESC;
```

## How It Works

PGlite Proxy transforms `DISTINCT ON` queries into equivalent `ROW_NUMBER()` window function queries:

**Original PostgreSQL:**
```sql
SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
FROM orders
ORDER BY customer_id, order_date DESC;
```

**Transformed SQLite:**
```sql
SELECT * FROM (
    SELECT customer_id, order_date, amount,
           ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY customer_id, order_date DESC) as __rn
    FROM orders
) AS __distinct_on_sub
WHERE __rn = 1
ORDER BY customer_id, order_date DESC;
```

## Limitations

1. **Performance**: For very large tables with many distinct groups, consider adding appropriate indexes.

2. **Complex Expressions**: Very complex expressions in DISTINCT ON may require additional transpilation.

3. **Window Functions in DISTINCT ON**: Using window functions within the DISTINCT ON expression itself is not supported.

## Comparison with Alternatives

| Feature | DISTINCT ON | GROUP BY | ROW_NUMBER() |
|---------|-------------|----------|--------------|
| Select arbitrary columns | ✅ | ❌ (only grouped/aggregated) | ✅ |
| Custom ordering per group | ✅ | ❌ | ✅ |
| SQL standard | ❌ (PostgreSQL-specific) | ✅ | ✅ |
| Performance | Good | Best | Good |

## See Also

- [PostgreSQL DISTINCT ON Documentation](https://www.postgresql.org/docs/current/sql-select.html#SQL-DISTINCT)
- [Window Functions](./WINDOW.md)
```

**Step 5: Commit**

```bash
git add docs/TODO-FEATURES.md docs/DISTINCT_ON.md README.md
git commit -m "docs: update documentation for DISTINCT ON support"
```

---

## Task 7: Final Verification and Push

**Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run E2E tests**

Run: `python3 tests/distinct_on_e2e_test.py`
Expected: All tests pass

**Step 3: Build release**

Run: `cargo build --release`
Expected: Build succeeds

**Step 4: Commit any final changes**

```bash
git status
# If any changes:
git add -A
git commit -m "chore: final cleanup for DISTINCT ON implementation"
```

**Step 5: Push all changes**

```bash
git push origin main
```

---

## Summary

This implementation plan covers:

1. **Detection**: Parse DISTINCT ON expressions from PostgreSQL AST
2. **Transformation**: Convert to ROW_NUMBER() window function with subquery
3. **Integration**: Hook into transpiler SELECT handling
4. **Testing**: Comprehensive unit and E2E tests
5. **Documentation**: Update all relevant docs

The approach maintains 100% PostgreSQL compatibility while using only SQLite-supported features.
