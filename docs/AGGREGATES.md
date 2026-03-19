# Aggregate Functions

PGQT provides PostgreSQL-compatible aggregate functions for computing values across sets of rows.

## Boolean Aggregate Functions

Boolean aggregates compute logical AND/OR operations across all non-null boolean values in a group.

### `bool_and(boolean)` → `boolean`

Returns the logical AND of all non-null input values.

- Returns `true` if **all** non-null values are `true`
- Returns `false` if **any** non-null value is `false`
- Returns `true` for an empty result set (identity for AND)
- NULL values are skipped

**Example:**
```sql
-- All values true
SELECT bool_and(active) FROM users;  -- Returns true if all users are active

-- One value false
SELECT bool_and(in_stock) FROM products;  -- Returns false if any product is out of stock

-- With NULLs
SELECT bool_and(verified) FROM accounts;  -- Skips NULLs, computes AND of non-null values
```

### `bool_or(boolean)` → `boolean`

Returns the logical OR of all non-null input values.

- Returns `true` if **any** non-null value is `true`
- Returns `false` if **all** non-null values are `false`
- Returns `false` for an empty result set (identity for OR)
- NULL values are skipped

**Example:**
```sql
-- Any value true
SELECT bool_or(has_error) FROM logs;  -- Returns true if any log has an error

-- All values false
SELECT bool_or(is_admin) FROM users;  -- Returns false if no user is admin

-- With NULLs
SELECT bool_or(approved) FROM requests;  -- Skips NULLs, computes OR of non-null values
```

### `every(boolean)` → `boolean`

Equivalent to `bool_and()`. This is the SQL standard name for the boolean AND aggregate.

**Example:**
```sql
-- Same as bool_and
SELECT every(paid) FROM orders;  -- Returns true if all orders are paid
```

### State Transition Functions

These are internal scalar functions used for implementing the aggregates:

#### `booland_statefunc(boolean, boolean)` → `boolean`

Computes the next accumulator state for `bool_and`.

```sql
SELECT booland_statefunc(true, true);   -- Returns true
SELECT booland_statefunc(true, false);  -- Returns false
SELECT booland_statefunc(NULL, true);   -- Returns true (NULL treated as identity)
```

#### `boolor_statefunc(boolean, boolean)` → `boolean`

Computes the next accumulator state for `bool_or`.

```sql
SELECT boolor_statefunc(false, false);  -- Returns false
SELECT boolor_statefunc(false, true);   -- Returns true
SELECT boolor_statefunc(NULL, true);    -- Returns true (NULL treated as identity)
```

## Usage with GROUP BY

Boolean aggregates work with `GROUP BY` to compute per-group values:

```sql
-- Check if all products in each category are in stock
SELECT category, bool_and(in_stock) as all_in_stock
FROM products
GROUP BY category;

-- Check if any store in each region has the item
SELECT region, bool_or(has_item) as available_somewhere
FROM inventory
GROUP BY region;
```

## Usage with HAVING

Use boolean aggregates in `HAVING` to filter groups:

```sql
-- Find categories where all products are active
SELECT category
FROM products
GROUP BY category
HAVING bool_and(active) = true;

-- Find regions where no store has the item
SELECT region
FROM inventory
GROUP BY region
HAVING bool_or(has_item) = false;
```

## Bitwise Aggregate Functions

Bitwise aggregates compute bitwise AND/OR/XOR operations across all non-null integer values in a group.

### `bit_and(integer)` → `integer`

Returns the bitwise AND of all non-null input values.

- Performs bitwise AND (`&`) across all non-null values
- Returns `NULL` for an empty result set
- NULL values are skipped

**Example:**
```sql
-- 5 = 101, 3 = 011, 1 = 001
-- 5 & 3 & 1 = 001 = 1
SELECT bit_and(val) FROM (VALUES (5), (3), (1)) AS t(val);  -- Returns 1

-- With permissions (e.g., checking common permissions across users)
SELECT bit_and(permissions) FROM user_permissions;
```

### `bit_or(integer)` → `integer`

Returns the bitwise OR of all non-null input values.

- Performs bitwise OR (`|`) across all non-null values
- Returns `NULL` for an empty result set
- NULL values are skipped

**Example:**
```sql
-- 1 = 001, 2 = 010, 4 = 100
-- 1 | 2 | 4 = 111 = 7
SELECT bit_or(val) FROM (VALUES (1), (2), (4)) AS t(val);  -- Returns 7

-- Combining permission flags
SELECT bit_or(permission_flag) FROM granted_permissions;
```

### `bit_xor(integer)` → `integer`

Returns the bitwise XOR of all non-null input values.

- Performs bitwise XOR (`^`) across all non-null values
- Returns `NULL` for an empty result set
- NULL values are skipped

**Example:**
```sql
-- 5 = 101, 3 = 011
-- 5 ^ 3 = 110 = 6
SELECT bit_xor(val) FROM (VALUES (5), (3)) AS t(val);  -- Returns 6

-- Compute parity across a set of values
SELECT bit_xor(value) FROM sensor_readings;
```

## NULL Handling

- NULL values are **skipped** during aggregation
- If all values are NULL, the aggregate returns the identity value:
  - `bool_and` (and `every`) returns `true`
  - `bool_or` returns `false`
  - `bit_and`, `bit_or`, `bit_xor` return `NULL`
- For empty result sets:
  - `bool_and` (and `every`) returns `true`
  - `bool_or` returns `false`
  - `bit_and`, `bit_or`, `bit_xor` return `NULL`

## Statistical Aggregate Support Functions

These are internal functions used by PostgreSQL's statistical aggregates (variance, stddev, regression, etc.). They maintain accumulator arrays that store running statistics.

### `float8_accum(real[], real)` → `real[]`

Accumulates a value for statistical computation. The accumulator array stores `[n, sum, sum_sqr]` for computing variance and standard deviation.

**Parameters:**
- `accum`: JSON array string representing the current accumulator state
- `value`: New value to accumulate

**Returns:** Updated accumulator array as JSON string

**Example:**
```sql
-- Start with empty array, accumulate 10.0
SELECT float8_accum('[]', 10.0);  -- Returns '[1.0, 10.0, 100.0]'

-- Continue accumulating
SELECT float8_accum('[1.0, 10.0, 100.0]', 20.0);  -- Returns '[2.0, 30.0, 500.0]'
```

### `float8_combine(real[], real[])` → `real[]`

Combines two accumulators element-wise. Used for parallel aggregation.

**Parameters:**
- `accum1`: First accumulator as JSON string
- `accum2`: Second accumulator as JSON string

**Returns:** Combined accumulator as JSON string

**Example:**
```sql
-- Combine accumulators from two workers
SELECT float8_combine('[2.0, 10.0, 50.0]', '[3.0, 15.0, 75.0]');
-- Returns '[5.0, 25.0, 125.0]'
```

### `float8_regr_accum(real[], real, real)` → `real[]`

Accumulates for regression analysis. The accumulator array stores `[n, sum_x, sum_x2, sum_y, sum_y2, sum_xy, 0, 0]`.

**Parameters:**
- `accum`: JSON array string representing the current accumulator state
- `y`: Y value (dependent variable)
- `x`: X value (independent variable)

**Returns:** Updated accumulator array as JSON string

**Example:**
```sql
-- Accumulate point (x=2, y=10)
SELECT float8_regr_accum('[]', 10.0, 2.0);
-- Returns '[1.0, 2.0, 4.0, 10.0, 100.0, 20.0, 0.0, 0.0]'
```

### `float8_regr_combine(real[], real[])` → `real[]`

Combines two regression accumulators element-wise.

**Parameters:**
- `accum1`: First regression accumulator as JSON string
- `accum2`: Second regression accumulator as JSON string

**Returns:** Combined accumulator as JSON string

**Example:**
```sql
-- Combine regression accumulators
SELECT float8_regr_combine(
    '[2.0, 6.0, 20.0, 10.0, 50.0, 30.0, 0.0, 0.0]',
    '[3.0, 12.0, 50.0, 15.0, 75.0, 60.0, 0.0, 0.0]'
);
-- Returns '[5.0, 18.0, 70.0, 25.0, 125.0, 90.0, 0.0, 0.0]'
```

## Parallel Aggregation Pattern

The combine functions enable parallel aggregation by allowing partial results to be merged:

```sql
-- Worker 1 accumulates values 1 and 2
-- Worker 2 accumulates values 3 and 4
-- Final result combines both workers

WITH 
worker1 AS (SELECT float8_accum(float8_accum('[]', 1.0), 2.0) as acc),
worker2 AS (SELECT float8_accum(float8_accum('[]', 3.0), 4.0) as acc)
SELECT float8_combine(worker1.acc, worker2.acc) 
FROM worker1, worker2;
-- Result: '[4.0, 10.0, 30.0]'
```

## Implementation Notes

- The aggregates are implemented using SQLite's custom aggregate function API via `rusqlite`
- The state functions are scalar functions that can be used independently
- Statistical accumulator functions use JSON strings to store array state in SQLite
- Accumulator arrays are padded with zeros when needed
- All functions are registered automatically when the proxy starts
