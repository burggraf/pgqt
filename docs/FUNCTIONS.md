# PostgreSQLite Function Support

## Overview

**`pgqt`** provides PostgreSQL-compatible user-defined functions using `CREATE FUNCTION`, enabling you to encapsulate SQL logic and reuse it across queries.

## Built-in JSON Functions

The following PostgreSQL JSON builder functions are automatically transpiled to their SQLite equivalents:

| PostgreSQL | SQLite Equivalent | Description |
|------------|-------------------|-------------|
| `json_build_object(key, val, ...)` | `json_object(key, val, ...)` | Build JSON object from variadic key-value pairs |
| `jsonb_build_object(key, val, ...)` | `json_object(key, val, ...)` | Build JSONB object from variadic key-value pairs |
| `json_build_array(val, ...)` | `json_array(val, ...)` | Build JSON array from variadic values |
| `jsonb_build_array(val, ...)` | `json_array(val, ...)` | Build JSONB array from variadic values |

### Examples

```sql
-- Build a JSON object
SELECT json_build_object('name', 'John', 'age', 30);
-- Returns: {"name": "John", "age": 30}

-- Build a JSONB object (same as json_build_object in SQLite)
SELECT jsonb_build_object('key', 'value');
-- Returns: {"key": "value"}

-- Build a JSON array
SELECT json_build_array(1, 2, 3);
-- Returns: [1, 2, 3]

-- Build a JSON array with mixed types
SELECT json_build_array(1, 'text', true, NULL);
-- Returns: [1, "text", true, null]

-- Nested JSON objects
SELECT json_build_object('user', json_build_object('name', 'Jane', 'age', 25));
-- Returns: {"user": {"name": "Jane", "age": 25}}

-- Empty JSON object
SELECT json_build_object();
-- Returns: {}

-- Empty JSON array
SELECT json_build_array();
-- Returns: []
```

### Notes

- `jsonb_*` functions map to the same SQLite functions as their `json_*` counterparts since SQLite stores JSON as text
- NULL values are preserved in the output JSON
- Keys for `json_build_object` and `jsonb_build_object` must be text values

## Built-in Math Functions

The following PostgreSQL math functions are automatically transpiled to their SQLite equivalents:

| PostgreSQL | SQLite Equivalent | Description |
|------------|-------------------|-------------|
| `log(x)` | `log10(x)` | Base 10 logarithm |
| `log(b, x)` | `log(x) / log(b)` | Logarithm with arbitrary base (change of base formula) |
| `ln(x)` | `log(x)` | Natural logarithm (base e) |
| `sqrt(x)` | `sqrt(x)` | Square root |
| `exp(x)` | `exp(x)` | Exponential (e^x) |
| `div(x, y)` | `CAST(x / y AS INTEGER)` | Integer division (truncates toward zero) |

### Examples

```sql
-- Base 10 logarithm
SELECT log(100.0);  -- Returns 2.0

-- Natural logarithm
SELECT ln(2.718281828);  -- Returns ~1.0

-- Logarithm with arbitrary base
SELECT log(2.0, 64.0);  -- Returns 6.0 (log base 2 of 64)

-- Square root
SELECT sqrt(16.0);  -- Returns 4.0

-- Exponential
SELECT exp(1.0);  -- Returns ~2.718281828

-- Integer division
SELECT div(17, 5);   -- Returns 3
SELECT div(-17, 5);  -- Returns -3 (truncates toward zero)
```

## Creating Functions

### Simple Scalar Function

Create a function that returns a single value:

```sql
CREATE FUNCTION add_numbers(a integer, b integer)
RETURNS integer
LANGUAGE sql
AS $$
    SELECT a + b
$$;

-- Call the function
SELECT add_numbers(5, 3);  -- Returns 8
```

### Function with OUT Parameters

Functions can return multiple values using OUT parameters:

```sql
CREATE FUNCTION get_user_info(user_id integer, 
                              OUT username text, 
                              OUT email text)
LANGUAGE sql
AS $$
    SELECT username, email FROM users WHERE id = user_id
$$;

-- Call it - returns a row with username and email columns
SELECT * FROM get_user_info(1);
```

### RETURNS TABLE Function

Return multiple rows with columns:

```sql
CREATE FUNCTION get_active_users()
RETURNS TABLE(id integer, username text, email text)
LANGUAGE sql
AS $$
    SELECT id, username, email FROM users WHERE active = true
$$;

-- Call it
SELECT * FROM get_active_users();
```

### RETURNS SETOF Function

Return multiple values of the same type:

```sql
CREATE FUNCTION get_user_ids()
RETURNS SETOF integer
LANGUAGE sql
AS $$
    SELECT id FROM users
$$;

-- Call it
SELECT * FROM get_user_ids();
```

### Function with Attributes

```sql
CREATE FUNCTION square(x integer)
RETURNS integer
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT x * x
$$;

-- This returns NULL (not an error)
SELECT square(NULL);
```

## Function Attributes

### STRICT (RETURNS NULL ON NULL INPUT)

If any input argument is NULL, the function returns NULL immediately without executing the body:

```sql
CREATE FUNCTION square(x integer)
RETURNS integer
LANGUAGE sql
STRICT
AS $$
    SELECT x * x
$$;

SELECT square(NULL);  -- Returns NULL, body not executed
```

### IMMUTABLE, STABLE, VOLATILE

These attributes tell the query optimizer how the function behaves:

| Attribute | Behavior | Use Case |
|-----------|----------|----------|
| **IMMUTABLE** | Always returns same result for same inputs | Pure math, string manipulation |
| **STABLE** | Returns same result within a transaction | Table lookups, `current_user()` |
| **VOLATILE** (default) | Can return different results | `random()`, database writes |

```sql
CREATE FUNCTION add_tax(price numeric)
RETURNS numeric
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT price * 1.05
$$;
```

### SECURITY DEFINER / SECURITY INVOKER

- **SECURITY INVOKER** (default): Function executes with caller's privileges
- **SECURITY DEFINER**: Function executes with creator's privileges

```sql
CREATE FUNCTION reset_password(user_id integer, new_password text)
RETURNS void
LANGUAGE sql
SECURITY DEFINER
AS $$
    UPDATE users SET password = new_password WHERE id = user_id
$$;
```

### PARALLEL

Controls whether the function can be executed in parallel query execution:

- **PARALLEL UNSAFE** (default): Cannot run in parallel
- **PARALLEL RESTRICTED**: Can run in parallel but only in leader
- **PARALLEL SAFE**: Can run fully in parallel workers

## PL/pgSQL Functions

**`pgqt`** supports PL/pgSQL (Procedural Language/PostgreSQL) functions via transpilation to Lua. This enables control flow, variable declarations, and procedural logic.

### Simple PL/pgSQL Function

```sql
CREATE FUNCTION plpgsql_add(a integer, b integer)
RETURNS integer
LANGUAGE plpgsql
AS $$
BEGIN
    RETURN a + b;
END;
$$;

-- Call it
SELECT plpgsql_add(5, 3);  -- Returns 8
```

### PL/pgSQL with Control Flow

```sql
CREATE FUNCTION plpgsql_max(a integer, b integer)
RETURNS integer
LANGUAGE plpgsql
AS $$
BEGIN
    IF a > b THEN
        RETURN a;
    ELSE
        RETURN b;
    END IF;
END;
$$;

-- Call it
SELECT plpgsql_max(10, 5);  -- Returns 10
```

### PL/pgSQL Limitations

While basic PL/pgSQL is supported, some advanced features are not yet available:

| Feature | Status | Notes |
|---------|--------|-------|
| Basic control flow (IF/ELSE) | ✅ Supported | IF, THEN, ELSE, ELSIF, END IF |
| Simple loops | ✅ Supported | LOOP, WHILE, FOR |
| Variable declarations | ✅ Supported | DECLARE section |
| Exception handling | ⚠️ Partial | Basic RAISE supported |
| Cursors | ❌ Not supported | Use RETURNS TABLE instead |
| Triggers | ❌ Not supported | Planned for future |
| Dynamic SQL | ❌ Not supported | EXECUTE not available |

## Calling Functions

### In SELECT Clause

```sql
SELECT add_numbers(5, 3);  -- Returns 8

SELECT id, add_numbers(price, tax) as total
FROM orders;
```

### In WHERE Clause

```sql
SELECT * FROM users
WHERE is_even(id);
```

### In FROM Clause (Table Functions)

```sql
SELECT * FROM get_active_users();

SELECT u.*, p.product_name
FROM get_user_orders(1) u
JOIN products p ON u.product_id = p.id;
```

### Nested Function Calls

```sql
SELECT add_numbers(square(3), square(4));  -- Returns 25
```

## Managing Functions

### CREATE OR REPLACE FUNCTION

Replace an existing function without dropping it:

```sql
CREATE OR REPLACE FUNCTION add_numbers(a integer, b integer)
RETURNS integer
LANGUAGE sql
AS $$
    SELECT a + b + 1  -- Modified implementation
$$;
```

### DROP FUNCTION

Remove a function:

```sql
DROP FUNCTION add_numbers;

-- Drop with specific signature
DROP FUNCTION add_numbers(integer, integer);

-- Drop if exists (no error if not found)
DROP FUNCTION IF EXISTS add_numbers;
```

## Parameter Modes

### IN (Default)

Input-only parameter (default):

```sql
CREATE FUNCTION add(a integer, b integer)
RETURNS integer
LANGUAGE sql
AS $$
    SELECT a + b
$$;
```

### OUT

Output-only parameter (caller doesn't provide):

```sql
CREATE FUNCTION get_user_info(user_id integer, 
                              OUT username text, 
                              OUT email text)
LANGUAGE sql
AS $$
    SELECT username, email FROM users WHERE id = user_id
$$;

SELECT * FROM get_user_info(1);
-- Returns: username | email
```

### INOUT

Both input and output:

```sql
CREATE FUNCTION increment(x integer)
RETURNS integer
LANGUAGE sql
AS $$
    SELECT x + 1
$$;

-- Same effect as:
CREATE FUNCTION increment(INOUT x integer)
LANGUAGE sql
AS $$
    SELECT x + 1
$$;
```

## Return Types

### Scalar

Single value return:

```sql
CREATE FUNCTION add(a integer, b integer)
RETURNS integer
LANGUAGE sql
AS $$
    SELECT a + b
$$;
```

### SETOF

Multiple values of the same type:

```sql
CREATE FUNCTION get_user_ids()
RETURNS SETOF integer
LANGUAGE sql
AS $$
    SELECT id FROM users
$$;
```

### TABLE

Multiple rows with columns:

```sql
CREATE FUNCTION get_active_users()
RETURNS TABLE(id integer, username text, email text)
LANGUAGE sql
AS $$
    SELECT id, username, email FROM users WHERE active = true
$$;
```

### VOID

No return value:

```sql
CREATE FUNCTION log_message(msg text)
RETURNS void
LANGUAGE sql
AS $$
    INSERT INTO logs (message) VALUES (msg)
$$;
```

## Examples

### Mathematical Functions

```sql
-- Calculate factorial
CREATE FUNCTION factorial(n integer)
RETURNS integer
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    WITH RECURSIVE fact(i, result) AS (
        VALUES (1, 1)
        UNION ALL
        SELECT i + 1, result * (i + 1)
        FROM fact
        WHERE i < n
    )
    SELECT result FROM fact WHERE i = n
$$;

SELECT factorial(5);  -- Returns 120
```

### Business Logic

```sql
-- Calculate discount
CREATE FUNCTION calculate_discount(price numeric, is_vip boolean)
RETURNS numeric
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE
        WHEN is_vip THEN price * 0.8
        ELSE price * 0.9
    END
$$;

SELECT calculate_discount(100, true);  -- Returns 80
```

### Data Validation

```sql
-- Check if email is valid
CREATE FUNCTION is_valid_email(email text)
RETURNS boolean
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'
$$;

SELECT * FROM users WHERE is_valid_email(email);
```

## Limitations

### Current Limitations

1. **SQL Language**: `LANGUAGE sql` is fully supported
2. **PL/pgSQL Language**: `LANGUAGE plpgsql` is supported for basic functions with control flow (IF/ELSE, loops, etc.)
2. **No Function Overloading**: Functions with same name but different signatures not yet supported
3. **No Triggers**: Trigger functions not yet supported
4. **No Aggregates**: Aggregate functions (CREATE AGGREGATE) not yet supported
5. **No Polymorphic Types**: Generic types (anyelement, anyarray) not yet supported

### Future Roadmap (Phase 2)

- PL/pgSQL procedural language support via Lua runtime
- Trigger functions
- Aggregate functions
- Function overloading by argument types
- Polymorphic types
- Security definer with proper search_path handling

## Catalog Tables

Functions are stored in the `__pg_functions__` catalog table:

```sql
-- View all functions
SELECT funcname, arg_types, return_type, language, volatility, strict
FROM __pg_functions__;

-- View function details
SELECT * FROM __pg_functions__ WHERE funcname = 'add_numbers';
```

**Catalog Schema:**

| Column | Type | Description |
|--------|------|-------------|
| oid | INTEGER | Function OID |
| funcname | TEXT | Function name |
| schema_name | TEXT | Schema (default: 'public') |
| arg_types | TEXT (JSON) | Argument types array |
| arg_names | TEXT (JSON) | Argument names array |
| arg_modes | TEXT (JSON) | Parameter modes (IN, OUT, INOUT) |
| return_type | TEXT | Return type |
| return_type_kind | TEXT | SCALAR, SETOF, TABLE, or VOID |
| return_table_cols | TEXT (JSON) | TABLE return column definitions |
| function_body | TEXT | SQL function body |
| language | TEXT | Language (sql, plpgsql) |
| volatility | TEXT | IMMUTABLE, STABLE, or VOLATILE |
| strict | BOOLEAN | STRICT attribute |
| security_definer | BOOLEAN | SECURITY DEFINER attribute |
| parallel | TEXT | PARALLEL UNSAFE, RESTRICTED, or SAFE |
| owner_oid | INTEGER | Owner role OID |
| created_at | TEXT | Creation timestamp |

## Performance Considerations

1. **IMMUTABLE Functions**: Mark pure functions as IMMUTABLE for optimization
2. **STRICT Functions**: Use STRICT to avoid unnecessary NULL checks in body
3. **VOLATILE Functions**: Avoid in WHERE clauses when possible (executes per row)
4. **Function Inlining**: Simple SQL functions may be inlined by optimizer

## Best Practices

1. **Use IMMUTABLE** for pure functions (no side effects, deterministic)
2. **Use STRICT** when function can't handle NULL inputs
3. **Use descriptive names** (e.g., `calculate_total` not `calc`)
4. **Document with comments**:
   ```sql
   CREATE FUNCTION add_numbers(a integer, b integer)
   RETURNS integer
   LANGUAGE sql
   IMMUTABLE
   AS $$
       -- Adds two numbers together
       SELECT a + b
   $$;
   ```
5. **Test thoroughly** before using in production
6. **Use CREATE OR REPLACE** for updates instead of DROP + CREATE

## Troubleshooting

### Function Not Found

```sql
SELECT add_numbers(5, 3);
-- ERROR: function add_numbers(integer, integer) does not exist
```

**Solution**: Check function exists in catalog:
```sql
SELECT * FROM __pg_functions__ WHERE funcname = 'add_numbers';
```

### Wrong Number of Arguments

```sql
SELECT add_numbers(5);
-- ERROR: function add_numbers(integer) does not exist
```

**Solution**: Check function signature and provide correct number of arguments.

### Type Mismatch

```sql
SELECT add_numbers('5', '3');
-- ERROR: function add_numbers(text, text) does not exist
```

**Solution**: Cast arguments to correct types:
```sql
SELECT add_numbers('5'::integer, '3'::integer);
```

## Related Documentation

- [PostgreSQL CREATE FUNCTION](https://www.postgresql.org/docs/current/sql-createfunction.html)
- [PostgreSQL Function Volatility](https://www.postgresql.org/docs/current/xfunc-volatility.html)
- [pg_query Rust Documentation](https://docs.rs/pg_query/)
- [SQLite Custom Functions](https://docs.rs/rusqlite/)
