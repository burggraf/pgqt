# PostgreSQL Array Support

PGlite Proxy provides full PostgreSQL-compatible array support, storing arrays as JSON strings in SQLite while providing transparent transpilation of array operators and functions.

## Overview

Arrays are stored internally as JSON arrays in SQLite TEXT columns, but are exposed through the PostgreSQL wire protocol as native array types. This allows full compatibility with PostgreSQL clients and ORMs.

```sql
-- Create table with array column
CREATE TABLE items (
    id SERIAL PRIMARY KEY,
    name TEXT,
    tags TEXT[],
    prices INTEGER[]
);

-- Insert with array values
INSERT INTO items (name, tags, prices) 
VALUES ('Product', '{"featured","sale"}', '{10,20,30}');
```

## Array Operators

### Overlap Operator: `&&`

Returns true if two arrays have any elements in common.

```sql
-- Find items with any matching tags
SELECT * FROM items WHERE tags && '{"featured","new"}';

-- With integer arrays
SELECT * FROM products WHERE category_ids && '{1,2,3}';
```

### Contains Operator: `@>`

Returns true if the left array contains all elements of the right array.

```sql
-- Find items that have BOTH "featured" AND "sale" tags
SELECT * FROM items WHERE tags @> '{"featured","sale"}';

-- Check for specific categories
SELECT * FROM products WHERE categories @> '{1,5}';
```

### Contained By Operator: `<@`

Returns true if the left array is contained by (is a subset of) the right array.

```sql
-- Find items where all tags are in the allowed set
SELECT * FROM items WHERE tags <@ '{"featured","sale","new"}';
```

## Array Functions

### Manipulation Functions

#### `array_append(anyarray, anyelement)`

Appends an element to the end of an array.

```sql
SELECT array_append('{1,2,3}'::int[], 4);
-- Result: {1,2,3,4}

-- In UPDATE
UPDATE items SET tags = array_append(tags, 'new') WHERE id = 1;
```

#### `array_prepend(anyelement, anyarray)`

Prepends an element to the beginning of an array.

```sql
SELECT array_prepend(0, '{1,2,3}'::int[]);
-- Result: {0,1,2,3}
```

#### `array_cat(anyarray, anyarray)`

Concatenates two arrays.

```sql
SELECT array_cat('{1,2}'::int[], '{3,4}'::int[]);
-- Result: {1,2,3,4}
```

#### `array_remove(anyarray, anyelement)`

Removes all occurrences of an element from an array.

```sql
SELECT array_remove('{1,2,2,3}'::int[], 2);
-- Result: {1,3}
```

#### `array_replace(anyarray, anyelement, anyelement)`

Replaces all occurrences of an element with another value.

```sql
SELECT array_replace('{1,2,2,3}'::int[], 2, 9);
-- Result: {1,9,9,3}
```

### Information Functions

#### `array_length(anyarray, int)`

Returns the length of the specified array dimension (1-indexed).

```sql
SELECT array_length('{1,2,3,4,5}'::int[], 1);
-- Result: 5

SELECT array_length('{{1,2},{3,4}}'::int[][], 2);
-- Result: 2 (second dimension)
```

#### `array_lower(anyarray, int)`

Returns the lower bound of the specified array dimension.

```sql
SELECT array_lower('{1,2,3}'::int[], 1);
-- Result: 1
```

#### `array_upper(anyarray, int)`

Returns the upper bound of the specified array dimension.

```sql
SELECT array_upper('{1,2,3}'::int[], 1);
-- Result: 3
```

#### `array_ndims(anyarray)`

Returns the number of array dimensions.

```sql
SELECT array_ndims('{1,2,3}'::int[]);
-- Result: 1

SELECT array_ndims('{{1,2},{3,4}}'::int[][]);
-- Result: 2
```

#### `array_dims(anyarray)`

Returns a text representation of the array's dimensions.

```sql
SELECT array_dims('{1,2,3}'::int[]);
-- Result: [1:3]

SELECT array_dims('{{1,2},{3,4}}'::int[][]);
-- Result: [1:2][1:2]
```

#### `cardinality(anyarray)`

Returns the total number of elements in the array (across all dimensions).

```sql
SELECT cardinality('{1,2,3}'::int[]);
-- Result: 3

SELECT cardinality('{{1,2},{3,4}}'::int[][]);
-- Result: 4
```

### Search Functions

#### `array_position(anyarray, anyelement [, int])`

Returns the subscript of the first occurrence of the element.

```sql
SELECT array_position('{"a","b","c","b"}'::text[], 'b');
-- Result: 2

-- Search starting from position 3
SELECT array_position('{"a","b","c","b"}'::text[], 'b', 3);
-- Result: 4
```

#### `array_positions(anyarray, anyelement)`

Returns an array of subscripts of all occurrences of the element.

```sql
SELECT array_positions('{1,2,1,3,1}'::int[], 1);
-- Result: {1,3,5}
```

### Conversion Functions

#### `array_to_string(anyarray, text [, text])`

Converts an array to a delimited string.

```sql
SELECT array_to_string('{"a","b","c"}'::text[], ',');
-- Result: 'a,b,c'

-- With NULL replacement
SELECT array_to_string('{"a",NULL,"c"}'::text[], ',', '*');
-- Result: 'a,*,c'
```

#### `string_to_array(text, text [, text])`

Splits a string into an array.

```sql
SELECT string_to_array('a,b,c', ',');
-- Result: {a,b,c}

-- With NULL marker
SELECT string_to_array('a,*,c', ',', '*');
-- Result: {a,NULL,c}
```

#### `array_fill(anyelement, int[] [, int[]])`

Creates an array filled with the specified value and dimensions.

```sql
-- Create 1D array
SELECT array_fill(7, '{3}'::int[]);
-- Result: {7,7,7}

-- Create 2D array
SELECT array_fill(0, '{2,3}'::int[]);
-- Result: {{0,0,0},{0,0,0}}
```

#### `trim_array(anyarray, int)`

Removes the specified number of elements from the end of the array.

```sql
SELECT trim_array('{1,2,3,4,5}'::int[], 2);
-- Result: {1,2,3}
```

## Usage Examples

### Tag Management

```sql
-- Create table with tags
CREATE TABLE articles (
    id SERIAL PRIMARY KEY,
    title TEXT,
    tags TEXT[]
);

-- Insert with tags
INSERT INTO articles (title, tags) VALUES 
    ('Introduction to SQL', '{"tutorial","beginner","sql"}'),
    ('Advanced PostgreSQL', '{"tutorial","advanced","postgresql"}'),
    ('Database Design', '{"design","postgresql","advanced"}');

-- Find articles with SQL or PostgreSQL tags
SELECT * FROM articles WHERE tags && '{"sql","postgresql"}';

-- Find articles that have BOTH tutorial AND advanced tags
SELECT * FROM articles WHERE tags @> '{"tutorial","advanced"}';

-- Add a tag to an article
UPDATE articles SET tags = array_append(tags, 'featured') WHERE id = 1;

-- Remove a tag
UPDATE articles SET tags = array_remove(tags, 'beginner') WHERE id = 1;
```

### Multi-Category Products

```sql
-- Products with multiple categories
CREATE TABLE products (
    id SERIAL PRIMARY KEY,
    name TEXT,
    category_ids INTEGER[],
    price DECIMAL
);

INSERT INTO products (name, category_ids, price) VALUES 
    ('Laptop', '{1,5,10}', 999.99),
    ('Mouse', '{1,8}', 29.99),
    ('Keyboard', '{1,8,12}', 79.99);

-- Find products in category 1 (Electronics)
SELECT * FROM products WHERE category_ids @> '{1}';

-- Find products in multiple categories (OR logic)
SELECT * FROM products WHERE category_ids && '{5,12}';

-- Count products per category
SELECT category_id, COUNT(*) 
FROM products, unnest(category_ids) AS category_id 
GROUP BY category_id;
```

### Integer Arrays

```sql
-- Store scores or measurements
CREATE TABLE experiments (
    id SERIAL PRIMARY KEY,
    name TEXT,
    measurements INTEGER[]
);

INSERT INTO experiments (name, measurements) VALUES 
    ('Test A', '{10,12,11,13,10}'),
    ('Test B', '{15,14,16,15,14}');

-- Calculate average (using array_to_string and parsing)
SELECT name, array_to_string(measurements, ',') FROM experiments;

-- Get first measurement
SELECT name, array_length(measurements, 1) as count FROM experiments;

-- Find experiments with measurement > 13
SELECT * FROM experiments WHERE measurements && '{14,15,16}';
```

## Array Format Compatibility

PGlite Proxy supports both PostgreSQL array literal format and JSON array format:

### PostgreSQL Format

```sql
-- String array
SELECT '{a,b,c}'::text[];
-- Result: {a,b,c}

-- Integer array
SELECT '{1,2,3}'::int[];
-- Result: {1,2,3}

-- 2D array
SELECT '{{1,2},{3,4}}'::int[][];
-- Result: {{1,2},{3,4}}
```

### JSON Format (also accepted)

```sql
-- JSON array format
SELECT '["a","b","c"]'::text[];
-- Result: {a,b,c}
```

## NULL Handling

Arrays can contain NULL elements:

```sql
-- Array with NULL
SELECT '{1,NULL,3}'::int[];
-- Result: {1,NULL,3}

-- NULL in string conversion
SELECT array_to_string('{a,NULL,c}'::text[], ',', '*');
-- Result: 'a,*,c'
```

## Type Mapping

| PostgreSQL Type | SQLite Storage |
|----------------|----------------|
| `INT[]`, `INTEGER[]` | TEXT (JSON) |
| `BIGINT[]` | TEXT (JSON) |
| `TEXT[]` | TEXT (JSON) |
| `VARCHAR[]` | TEXT (JSON) |
| `BOOLEAN[]` | TEXT (JSON) |
| `NUMERIC[]`, `DECIMAL[]` | TEXT (JSON) |
| `ANYARRAY` | TEXT (JSON) |

## Limitations

- **Array Indexing**: Direct element access (`arr[1]`) is supported through function equivalents
- **Array Slicing**: Slice notation (`arr[1:3]`) requires use of `trim_array` and `array_fill`
- **GIN Indexes**: Not natively supported in SQLite; use full table scans or FTS for large arrays

## Performance Tips

1. **Use `@>` and `&&` operators** for efficient containment checks
2. **Normalize data** for very large arrays; consider separate junction tables
3. **Use `cardinality`** instead of `array_length(arr, 1)` for 1D arrays
4. **Prefer `array_position`** over manual iteration for searching

## Compatibility

This implementation aims for 100% PostgreSQL compatibility for:

- Array operators: `&&`, `@>`, `<@`
- Array functions per PostgreSQL 17 documentation
- NULL handling semantics
- Multi-dimensional array support

Some advanced features like custom array type definitions and GIN index operators are not supported due to SQLite limitations.
