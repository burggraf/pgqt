# Window Functions Support

PGlite Proxy provides full PostgreSQL-compatible window function support by transpiling window function syntax to SQLite (which has native support since version 3.25.0).

## Overview

Window functions perform calculations across a set of table rows that are somehow related to the current row. Unlike aggregate functions, window functions do not cause rows to become grouped into a single output row.

## Supported Features

### Window Functions

All standard PostgreSQL window functions are supported:

| Function | Description |
|----------|-------------|
| `row_number()` | Number of the current row within its partition |
| `rank()` | Rank of the current row with gaps |
| `dense_rank()` | Rank of the current row without gaps |
| `percent_rank()` | Relative rank of the current row |
| `cume_dist()` | Cumulative distribution |
| `ntile(n)` | Integer ranging from 1 to n dividing the partition as equally as possible |
| `lag(value [, offset [, default]])` | Returns value evaluated at the row that is offset rows before the current row |
| `lead(value [, offset [, default]])` | Returns value evaluated at the row that is offset rows after the current row |
| `first_value(value)` | Returns value evaluated at the row that is the first row of the window frame |
| `last_value(value)` | Returns value evaluated at the row that is the last row of the window frame |
| `nth_value(value, n)` | Returns value evaluated at the row that is the nth row of the window frame |

### Aggregate Functions as Window Functions

All aggregate functions can be used as window functions by adding an `OVER` clause:

- `sum()`, `avg()`, `count()`, `min()`, `max()`
- `count(*)` for counting all rows
- Any custom aggregate function

### OVER Clause Components

#### PARTITION BY

Divides rows into partitions to which the window function is applied:

```sql
SELECT department, salary,
       sum(salary) OVER (PARTITION BY department) as dept_total
FROM employees;
```

#### ORDER BY

Orders rows within each partition:

```sql
SELECT name, salary,
       rank() OVER (ORDER BY salary DESC) as salary_rank
FROM employees;
```

#### Frame Specification

Defines the set of rows constituting the window frame:

**Frame Modes:**
- `ROWS` - Based on physical row offsets
- `RANGE` - Based on logical value ranges
- `GROUPS` - Based on peer groups (rows with same ORDER BY values)

**Frame Bounds:**
- `UNBOUNDED PRECEDING` - Start of the partition
- `UNBOUNDED FOLLOWING` - End of the partition
- `CURRENT ROW` - Current row
- `offset PRECEDING` - offset rows before current row
- `offset FOLLOWING` - offset rows after current row

**Examples:**

```sql
-- Running total
SELECT order_date, amount,
       sum(amount) OVER (ORDER BY order_date 
                         ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) as running_total
FROM orders;

-- Moving average (3-day window)
SELECT order_date, amount,
       avg(amount) OVER (ORDER BY order_date 
                         ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) as moving_avg
FROM orders;

-- Full partition aggregation
SELECT department, salary,
       max(salary) OVER (PARTITION BY department 
                         ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) as dept_max
FROM employees;
```

## Usage Examples

### Basic Row Numbering

```sql
SELECT id, name, 
       row_number() OVER (ORDER BY created_at) as row_num
FROM users;
```

### Ranking Within Groups

```sql
SELECT department, name, salary,
       rank() OVER (PARTITION BY department ORDER BY salary DESC) as dept_rank
FROM employees;
```

### Comparing with Previous Row

```sql
SELECT date, revenue,
       revenue - lag(revenue) OVER (ORDER BY date) as daily_change
FROM daily_stats;
```

### Top N per Group

```sql
SELECT * FROM (
    SELECT category, product, sales,
           row_number() OVER (PARTITION BY category ORDER BY sales DESC) as rn
    FROM products
) ranked
WHERE rn <= 3;
```

### Running and Moving Aggregates

```sql
SELECT date, 
       amount,
       sum(amount) OVER (ORDER BY date ROWS UNBOUNDED PRECEDING) as running_total,
       avg(amount) OVER (ORDER BY date ROWS BETWEEN 6 PRECEDING AND CURRENT ROW) as weekly_avg
FROM daily_sales;
```

### Percentiles and Distribution

```sql
SELECT name, score,
       percent_rank() OVER (ORDER BY score) as percentile,
       cume_dist() OVER (ORDER BY score) as cumulative_dist,
       ntile(4) OVER (ORDER BY score) as quartile
FROM students;
```

## Transpilation

The transpiler converts PostgreSQL window function syntax directly to SQLite syntax, which is nearly identical. Here are some examples:

| PostgreSQL | SQLite (Transpiled) |
|------------|---------------------|
| `row_number() OVER ()` | `row_number() over ()` |
| `rank() OVER (ORDER BY x DESC)` | `rank() over (order by x desc)` |
| `sum(x) OVER (PARTITION BY y)` | `sum(x) over (partition by y)` |
| `lag(x, 1) OVER (ORDER BY y)` | `lag(x, 1) over (order by y)` |
| `ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW` | `rows between unbounded preceding and current row` |

## Compatibility Notes

### SQLite Version Requirements

- Window functions: SQLite 3.25.0+ (September 2018)
- GROUPS frame mode: SQLite 3.28.0+ (April 2019)
- EXCLUDE clause: SQLite 3.28.0+ (April 2019)

Most modern systems have SQLite 3.35+, which is required for the RETURNING clause, so window functions should work without issues.

### Default Frame Behavior

PostgreSQL has specific defaults for frame specification:

- **With ORDER BY**: `RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW`
- **Without ORDER BY**: `RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING`

These defaults match SQLite's behavior.

### last_value() and nth_value() Caveats

The default frame for `last_value()` and `nth_value()` ends at `CURRENT ROW`, which may not be the intended behavior. For these functions, explicitly specify the full frame:

```sql
-- This returns the last value in the current row's frame (which is just the current row)
SELECT last_value(x) OVER (ORDER BY y) FROM t;  -- Probably not what you want

-- This returns the last value in the entire partition
SELECT last_value(x) OVER (ORDER BY y 
                           ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) 
FROM t;  -- Better!
```

## Testing

Window function support is tested with:
- 45 unit tests covering syntax transpilation
- 19 E2E tests verifying correct execution and results

See `tests/window_tests.rs` and `tests/window_e2e_test.py` for the full test suite.
