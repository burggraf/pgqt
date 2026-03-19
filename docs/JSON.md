# JSON and JSONB Functions

PGQT provides comprehensive support for PostgreSQL JSON and JSONB functions, enabling you to work with JSON data in a PostgreSQL-compatible way while storing data in SQLite.

## Overview

JSON functions in PGQT allow you to:
- Convert values to JSON format
- Build JSON objects and arrays from SQL data
- Query and manipulate JSON data
- Use PostgreSQL-compatible JSON operators

## JSON Constructor Functions

### `to_json(anyelement)` / `to_jsonb(anyelement)`

Converts any SQL value to JSON or JSONB format.

```sql
-- Convert a string to JSON
SELECT to_json('hello');           -- Returns: "hello"

-- Convert a number to JSON
SELECT to_json(42);                -- Returns: 42
SELECT to_json(3.14);              -- Returns: 3.14

-- Convert NULL to JSON
SELECT to_json(NULL);              -- Returns: null

-- Parse a JSON string (if valid JSON)
SELECT to_json('[1,2,3]');         -- Returns: [1,2,3]
SELECT to_json('{"a": 1}');        -- Returns: {"a":1}
```

**Type Conversions:**
- `NULL` → `null`
- Integer → number
- Real/Float → number
- Text → string (or parsed JSON if valid)
- Blob → hex string

### `array_to_json(anyarray)`

Converts an array to a JSON array.

```sql
-- Convert array literal to JSON
SELECT array_to_json('[1, 2, 3]');           -- Returns: [1,2,3]
SELECT array_to_json('["a", "b", "c"]');      -- Returns: ["a","b","c"]

-- Convert nested array
SELECT array_to_json('[[1, 2], [3, 4]]');    -- Returns: [[1,2],[3,4]]
```

### `json_build_object(VARIADIC "any")` / `jsonb_build_object(VARIADIC "any")`

Builds a JSON object from key-value pairs. Keys must be text, values can be any type.

```sql
-- Build a simple object
SELECT json_build_object('name', 'John', 'age', 30);
-- Returns: {"name": "John", "age": 30}

-- Build with mixed types
SELECT json_build_object(
    'str', 'hello',
    'num', 42,
    'float', 3.14,
    'null_val', NULL
);
-- Returns: {"str": "hello", "num": 42, "float": 3.14, "null_val": null}

-- Empty object
SELECT json_build_object();        -- Returns: {}
```

**Note:** Arguments must come in key-value pairs (even number of arguments).

### `json_build_array(VARIADIC "any")` / `jsonb_build_array(VARIADIC "any")`

Builds a JSON array from values.

```sql
-- Build array with mixed types
SELECT json_build_array(1, 'two', 3.0, NULL);
-- Returns: [1, "two", 3.0, null]

-- Empty array
SELECT json_build_array();         -- Returns: []

-- Single element
SELECT json_build_array('single'); -- Returns: ["single"]
```

### Nested Construction

You can nest JSON builder functions:

```sql
SELECT json_build_object(
    'user', json_build_object('name', 'John', 'age', 30),
    'tags', json_build_array('admin', 'user')
);
-- Returns: {"user": {"name": "John", "age": 30}, "tags": ["admin", "user"]}
```

## JSON Query Functions

### `jsonb_contains(jsonb, jsonb)` / `@>` operator

Checks if the first JSON value contains the second.

```sql
-- Check if object contains key-value
SELECT jsonb_contains('{"a": 1, "b": 2}', '{"a": 1}');  -- Returns: true
SELECT '{"a": 1, "b": 2}'::jsonb @> '{"a": 1}';          -- Returns: true

-- Check array containment
SELECT jsonb_contains('[1, 2, 3]', '[1, 2]');            -- Returns: true
```

### `jsonb_exists(jsonb, text)` / `?` operator

Checks if a key exists in a JSON object.

```sql
SELECT jsonb_exists('{"a": 1, "b": 2}', 'a');          -- Returns: true
SELECT '{"a": 1, "b": 2}'::jsonb ? 'c';                  -- Returns: false
```

### `jsonb_exists_any(jsonb, text[])` / `?|` operator

Checks if any key exists in the JSON object.

```sql
SELECT jsonb_exists_any('{"a": 1, "b": 2}', '["a", "c"]');  -- Returns: true
SELECT '{"a": 1, "b": 2}'::jsonb ?| '["c", "d"]';           -- Returns: false
```

### `jsonb_exists_all(jsonb, text[])` / `?&` operator

Checks if all keys exist in the JSON object.

```sql
SELECT jsonb_exists_all('{"a": 1, "b": 2}', '["a", "b"]');  -- Returns: true
SELECT '{"a": 1, "b": 2}'::jsonb ?& '["a", "c"]';           -- Returns: false
```

## JSON Processing Functions

These functions expand JSON objects and arrays into row sets, enabling you to iterate over JSON data in SQL queries.

### `json_each(json)` / `jsonb_each(jsonb)`

Expands a JSON object or array into a set of rows. For objects, returns key-value pairs. For arrays, returns index-value pairs.

```sql
-- Expand JSON object
SELECT * FROM json_each('{"a": 1, "b": 2}');
-- Returns rows: ("a", 1), ("b", 2)

-- Expand JSON array
SELECT * FROM json_each('[10, 20, 30]');
-- Returns rows: (0, 10), (1, 20), (2, 30)
```

**Note:** These functions are implemented as scalar functions that return JSON arrays, which are then iterated using SQLite's `json_each()`. The transpiler automatically handles this transformation.

### `json_each_text(json)` / `jsonb_each_text(jsonb)`

Like `json_each`, but returns values as text strings instead of JSON.

```sql
SELECT * FROM json_each_text('{"num": 42, "str": "hello"}');
-- Returns rows: ("num", "42"), ("str", "hello")
-- Note: 42 is returned as text "42", not number 42
```

### `json_array_elements(json)` / `jsonb_array_elements(jsonb)`

Expands a JSON array into a set of rows, returning just the array elements.

```sql
SELECT * FROM json_array_elements('[1, 2, 3]');
-- Returns rows: 1, 2, 3

-- With mixed types
SELECT * FROM json_array_elements('[1, "two", true, null]');
-- Returns rows: 1, "two", true, null
```

### `json_array_elements_text(json)` / `jsonb_array_elements_text(jsonb)`

Like `json_array_elements`, but returns elements as text strings.

```sql
SELECT * FROM json_array_elements_text('[1, "hello", true]');
-- Returns rows: "1", "hello", "true"
-- All values are converted to text
```

### `json_object_keys(json)` / `jsonb_object_keys(jsonb)`

Returns the keys of a JSON object as a set of rows.

```sql
SELECT * FROM json_object_keys('{"c": 1, "a": 2, "b": 3}');
-- Returns rows: "c", "a", "b"

-- Empty object returns no rows
SELECT * FROM json_object_keys('{}');
-- Returns: (no rows)
```

### Using with LATERAL Joins

JSON processing functions work seamlessly with LATERAL joins:

```sql
-- Process JSON from a table
SELECT u.name, elem.key, elem.value
FROM users u,
LATERAL json_each(u.metadata) AS elem;

-- Get all array elements from nested JSON
SELECT u.name, elem.value
FROM users u,
LATERAL json_array_elements(u.metadata->'tags') AS elem;
```

### Working with Nested JSON

These functions handle nested JSON structures:

```sql
-- json_each with nested object
SELECT * FROM json_each('{"outer": {"inner": 123}}');
-- Returns: ("outer", {"inner": 123})

-- json_array_elements with nested arrays
SELECT * FROM json_array_elements('[[1, 2], [3, 4]]');
-- Returns: [1, 2], [3, 4]
```

### Empty and Null Handling

```sql
-- Empty object
SELECT * FROM json_each('{}');
-- Returns: (no rows)

-- Empty array
SELECT * FROM json_array_elements('[]');
-- Returns: (no rows)

-- Null values are preserved
SELECT * FROM json_each('{"a": null, "b": 1}');
-- Returns: ("a", null), ("b", 1)

-- Scalar values (not objects/arrays)
SELECT * FROM json_each('"just a string"');
-- Returns: (no rows)
```

## JSON Type Casting and Validation Functions

### `json_typeof(json)` / `jsonb_typeof(jsonb)`

Returns the type of the JSON value as a string. Possible return values are:
- `null` - JSON null value
- `boolean` - JSON true/false
- `number` - JSON number (integer or float)
- `string` - JSON string
- `array` - JSON array
- `object` - JSON object

```sql
-- Check type of various JSON values
SELECT json_typeof('null');           -- Returns: null
SELECT json_typeof('true');           -- Returns: boolean
SELECT json_typeof('42');             -- Returns: number
SELECT json_typeof('"hello"');        -- Returns: string
SELECT json_typeof('[1,2,3]');        -- Returns: array
SELECT json_typeof('{"a":1}');       -- Returns: object

-- Use with JSONB
SELECT jsonb_typeof('{"key":"value"}');  -- Returns: object
```

### `json_strip_nulls(json)` / `jsonb_strip_nulls(jsonb)`

Removes all object fields that have null values from the given JSON value. Other null values are untouched.

```sql
-- Remove null fields from object
SELECT json_strip_nulls('{"a":1,"b":null,"c":3}');
-- Returns: {"a":1,"c":3}

-- Nested objects are processed recursively
SELECT json_strip_nulls('{"outer":{"x":1,"y":null},"z":2}');
-- Returns: {"outer":{"x":1},"z":2}

-- Nulls in arrays are preserved
SELECT json_strip_nulls('[1,null,3]');
-- Returns: [1,null,3]
```

### `json_pretty(json)` / `jsonb_pretty(jsonb)`

Returns the JSON value as formatted, indented text for human readability.

```sql
-- Pretty-print a JSON object
SELECT json_pretty('{"name":"John","age":30}');
-- Returns:
-- {
--   "name": "John",
--   "age": 30
-- }

-- Pretty-print a JSON array
SELECT json_pretty('[1,2,3]');
-- Returns:
-- [
--   1,
--   2,
--   3
-- ]
```

### `jsonb_set(target, path, new_value)`

Returns `target` with the item designated by `path` replaced by `new_value`, or with `new_value` added if the path doesn't exist. Creates intermediate structures as needed.

```sql
-- Update existing field
SELECT jsonb_set('{"a":1}', '{a}', '99');
-- Returns: {"a":99}

-- Create new field
SELECT jsonb_set('{"a":1}', '{b}', '2');
-- Returns: {"a":1,"b":2}

-- Update nested field
SELECT jsonb_set('{"outer":{"inner":1}}', '{outer,inner}', '99');
-- Returns: {"outer":{"inner":99}}

-- Update array element
SELECT jsonb_set('[1,2,3]', '{1}', '99');
-- Returns: [1,99,3]

-- Create deeply nested structure
SELECT jsonb_set('{"a":1}', '{b,c}', '2');
-- Returns: {"a":1,"b":{"c":2}}
```

**Path Syntax:**
- PostgreSQL uses `{a,b,c}` format
- Array indices are specified as numbers
- Creates intermediate objects/arrays as needed

### `jsonb_insert(target, path, new_value)`

Returns `target` with `new_value` inserted. Similar to `jsonb_set` but only creates new fields, does not replace existing values at the leaf level.

```sql
-- Insert new field into object
SELECT jsonb_insert('{"a":1}', '{b}', '2');
-- Returns: {"a":1,"b":2}

-- Insert into nested object
SELECT jsonb_insert('{"outer":{"x":1}}', '{outer,y}', '2');
-- Returns: {"outer":{"x":1,"y":2}}

-- Note: jsonb_insert will error if the path already exists
-- (unlike jsonb_set which would replace it)
```

## JSON Operators

PGQT supports PostgreSQL JSON operators:

| Operator | Description | Example |
|----------|-------------|---------|
| `->` | Get JSON field by key | `'{"a": 1}'::json->'a'` → `1` |
| `->>` | Get JSON field as text | `'{"a": 1}'::json->>'a'` → `"1"` |
| `#>` | Get JSON at path | `'{"a": {"b": 3}}'::json#>'{a,b}'` → `3` |
| `#>>` | Get JSON at path as text | `'{"a": {"b": 3}}'::json#>>'{a,b}'` → `"3"` |
| `@>` | Contains | `'{"a": 1}'::jsonb @> '{"a": 1}'` → `true` |
| `<@` | Is contained by | `'{"a": 1}'::jsonb <@ '{"a": 1, "b": 2}'` → `true` |
| `?` | Key exists | `'{"a": 1}'::jsonb ? 'a'` → `true` |
| `?|` | Any key exists | `'{"a": 1}'::jsonb ?| '["a", "b"]'` → `true` |
| `?&` | All keys exist | `'{"a": 1, "b": 2}'::jsonb ?& '["a", "b"]'` → `true` |

## Type Mapping

PostgreSQL JSON types are mapped to SQLite TEXT:

| PostgreSQL | SQLite | Notes |
|------------|--------|-------|
| `json` | `TEXT` | Stored as JSON text |
| `jsonb` | `TEXT` | Stored as JSON text (no binary difference in SQLite) |

## Implementation Notes

### Variadic Function Support

PostgreSQL supports variadic functions (variable number of arguments), but SQLite does not. PGQT handles this by registering multiple function arities:

- `json_build_object()` - 0 to 10 arguments (even numbers)
- `json_build_array()` - 0 to 10 arguments

If you need more arguments, you can nest calls:

```sql
-- Instead of 12 arguments, nest objects
SELECT json_build_object(
    'a', 1,
    'nested', json_build_object('b', 2, 'c', 3, 'd', 4, 'e', 5)
);
```

### JSON vs JSONB

In PGQT, `json` and `jsonb` functions behave identically because SQLite stores everything as TEXT. The distinction is maintained for PostgreSQL compatibility:

- `to_json()` and `to_jsonb()` produce the same output
- `json_build_object()` and `jsonb_build_object()` produce the same output
- `json_build_array()` and `jsonb_build_array()` produce the same output

## Examples

### Working with JSON Columns

```sql
-- Create a table with JSON column
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name TEXT,
    metadata JSONB
);

-- Insert JSON data
INSERT INTO users (name, metadata)
VALUES ('John', jsonb_build_object('age', 30, 'city', 'NYC'));

-- Query JSON data
SELECT name, metadata->>'age' as age FROM users;

-- Check if key exists
SELECT * FROM users WHERE metadata ? 'age';

-- Check containment
SELECT * FROM users WHERE metadata @> '{"city": "NYC"}';
```

## JSON Operators

PGQT supports PostgreSQL JSON operators for querying and manipulating JSON data:

### Field/Element Extraction

| Operator | Description | Example | Result |
|----------|-------------|---------|--------|
| `->` | Get JSON object field or array element | `'{"a":1}'::json->'a'` | `1` |
| `->>` | Get JSON object field or array element as text | `'{"a":1}'::json->>'a'` | `"1"` |
| `#>` | Get JSON object at specified path | `'{"a":{"b":2}}'::json#>'{a,b}'` | `2` |
| `#>>` | Get JSON object at specified path as text | `'{"a":{"b":2}}'::json#>>'{a,b}'` | `"2"` |

### Containment and Existence

| Operator | Description | Example | Result |
|----------|-------------|---------|--------|
| `@>` | JSON contains | `'{"a":1,"b":2}'::jsonb @> '{"a":1}'` | `true` |
| `<@` | JSON is contained by | `'{"a":1}'::jsonb <@ '{"a":1,"b":2}'` | `true` |
| `?` | Does key exist? | `'{"a":1}'::jsonb ? 'a'` | `true` |
| `?\|` | Does any key exist? | `'{"a":1,"b":2}'::jsonb ?\| array['a','c']` | `true` |
| `?&` | Do all keys exist? | `'{"a":1,"b":2}'::jsonb ?& array['a','b']` | `true` |

### Concatenation and Deletion

| Operator | Description | Example | Result |
|----------|-------------|---------|--------|
| `\|\|` | Concatenate JSON | `'{"a":1}'::jsonb \|\| '{"b":2}'::jsonb` | `{"a":1,"b":2}` |
| `-` | Delete key/array element | `'{"a":1,"b":2}'::jsonb - 'a'` | `{"b":2}` |
| `#-` | Delete at path | `'{"a":{"b":2}}'::jsonb #- '{a,b}'` | `{"a":{}}` |

### Path Syntax

PostgreSQL uses `{a,b,c}` syntax for JSON paths, which PGQT automatically converts to SQLite's `$.a.b.c` syntax:

```sql
-- PostgreSQL syntax
SELECT '{"a": {"b": {"c": 1}}}'::json#>'{a,b,c}';

-- Transpiled to SQLite
SELECT json_extract('{"a": {"b": {"c": 1}}}', '$.a.b.c');
```

Array indices in paths:
```sql
-- Access array element
SELECT '{"items": [10, 20, 30]}'::json#>'{items,1}';
-- Returns: 20
```

### Aggregating to JSON

```sql
-- Build JSON object from query results
SELECT json_build_object(
    'total_users', COUNT(*),
    'users', json_build_array(
        json_build_object('name', name, 'age', metadata->>'age')
    )
)
FROM users;
```

## See Also

- [PostgreSQL JSON Functions Documentation](https://www.postgresql.org/docs/current/functions-json.html)
- [DATA_TYPES.md](DATA_TYPES.md) - Type mappings in PGQT
