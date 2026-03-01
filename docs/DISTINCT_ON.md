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
SELECT customer_id, order_date, amount FROM (
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

4. **SELECT * with __rn column**: For `SELECT *` queries, the internal `__rn` column may appear in results. Use explicit column lists to avoid this.

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
