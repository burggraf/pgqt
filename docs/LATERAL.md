# LATERAL Joins

PostgreSQL's `LATERAL` joins allow correlated subqueries in the `FROM` clause. SQLite does not natively support `LATERAL` for subqueries, but it *does* support them implicitly for table-valued functions.

## Supported: Table-Valued Functions

The proxy supports `LATERAL` when used with table-valued functions like `jsonb_each`, `jsonb_array_elements`, `fts5`, etc. In SQLite, these functions are implicitly lateral.

**PostgreSQL:**
```sql
SELECT name, key, value 
FROM test_jsonb, LATERAL jsonb_each(props) AS x(key, value);
```

**Transpiled SQLite:**
```sql
SELECT name, key, value 
FROM test_jsonb, json_each(props) AS x;
```

## Not Supported: Correlated Subqueries

The proxy does **not** currently support `LATERAL` for arbitrary subqueries.

**Unsupported Query:**
```sql
SELECT * FROM (SELECT 1 as x) a, LATERAL (SELECT a.x + 1 as y) b;
```

Attempting to run such a query will result in a transpilation error or a "no such column" error from SQLite.

### Workaround: Window Functions

Many use cases for `LATERAL` subqueries (like "Top N per group") can be rewritten using window functions, which *are* supported.

**Instead of:**
```sql
SELECT c.name, o.order_date
FROM customers c
CROSS JOIN LATERAL (
  SELECT order_date FROM orders 
  WHERE customer_id = c.id 
  ORDER BY order_date DESC LIMIT 3
) o;
```

**Use:**
```sql
SELECT name, order_date
FROM (
  SELECT c.name, o.order_date,
         ROW_NUMBER() OVER (PARTITION BY c.id ORDER BY o.order_date DESC) as rank
  FROM customers c
  JOIN orders o ON o.customer_id = c.id
)
WHERE rank <= 3;
```

### Workaround: CTEs

For calculations, you can often use Common Table Expressions (CTEs).

**Instead of:**
```sql
SELECT t.val, sub.double_val, sub.double_val + 10
FROM my_table t,
LATERAL (SELECT t.val * 2 AS double_val) sub;
```

**Use:**
```sql
WITH calculated AS (
  SELECT val, (val * 2) AS double_val
  FROM my_table
)
SELECT val, double_val, double_val + 10
FROM calculated;
```
