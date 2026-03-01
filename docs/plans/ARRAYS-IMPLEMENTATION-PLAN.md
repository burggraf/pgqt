# PostgreSQL Array Support Implementation Plan

## Overview
Implement full PostgreSQL array compatibility for PostgreSQLite by emulating arrays via JSON strings in SQLite with transpilation for array operators and functions.

## Storage Strategy
- **Format**: Store arrays as JSON arrays in SQLite TEXT columns: `[1,2,3]` or `["a","b","c"]`
- **PostgreSQL Format**: Also support PostgreSQL array literal format: `{1,2,3}` or `{{1,2},{3,4}}`
- **Input Handling**: Accept both PostgreSQL array literals (`ARRAY[1,2,3]` or `'{1,2,3}'`) and convert to JSON
- **Output Handling**: Return arrays in PostgreSQL format `{1,2,3}` for compatibility

## Operators to Implement

### Overlap Operator: `&&`
- `ARRAY[1,2,3] && ARRAY[3,4]` → true (any element in common)
- Implementation: Check if intersection of two arrays is non-empty

### Contains Operator: `@>`
- `ARRAY[1,2,3] @> ARRAY[1,2]` → true (left contains all elements of right)
- Implementation: Check if all elements of right array exist in left array

### Contained By Operator: `<@`
- `ARRAY[1,2] <@ ARRAY[1,2,3]` → true (left is subset of right)
- Implementation: Check if all elements of left array exist in right array

### Concatenation Operator: `||`
- `ARRAY[1,2] || ARRAY[3,4]` → `{1,2,3,4}`
- `ARRAY[1,2] || 3` → `{1,2,3}` (element append)
- Implementation: Concatenate arrays or append element

### Comparison Operators: `=`, `<>`, `<`, `>`, `<=`, `>=`
- Lexicographical comparison based on element values
- Implementation: Compare element by element

## Functions to Implement

### Array Construction and Manipulation
1. `array_append(anyarray, anyelement)` - Append element to end
2. `array_prepend(anyelement, anyarray)` - Prepend element to beginning
3. `array_cat(anyarray, anyarray)` - Concatenate two arrays
4. `array_remove(anyarray, anyelement)` - Remove all occurrences of element
5. `array_replace(anyarray, anyelement, anyelement)` - Replace all occurrences

### Array Information
6. `array_length(anyarray, int)` - Length of specified dimension
7. `array_lower(anyarray, int)` - Lower bound of dimension
8. `array_upper(anyarray, int)` - Upper bound of dimension
9. `array_ndims(anyarray)` - Number of dimensions
10. `array_dims(anyarray)` - Text representation of dimensions
11. `cardinality(anyarray)` - Total number of elements

### Array Search
12. `array_position(anyarray, anyelement [, int])` - First position of element
13. `array_positions(anyarray, anyelement)` - All positions of element

### Array Conversion
14. `array_to_string(anyarray, text [, text])` - Convert to delimited string
15. `string_to_array(text, text [, text])` - Split string to array
16. `unnest(anyarray)` - Expand array to rows (table function)
17. `array_fill(anyelement, int[] [, int[]])` - Create filled array

### Array Utilities
18. `trim_array(anyarray, int)` - Remove n elements from end

## Transpiler Changes

### Operator Translation (in `reconstruct_a_expr`)
```rust
"&&" => format!("array_overlap({}, {})", lexpr_sql, rexpr_sql),
"@>" => format!("array_contains({}, {})", lexpr_sql, rexpr_sql),
"<@" => format!("array_contained({}, {})", lexpr_sql, rexpr_sql),
```

### Array Literal Handling
- `ARRAY[1,2,3]` → `[1,2,3]` (JSON format)
- `'{1,2,3}'` → `[1,2,3]` (convert PG literal to JSON)
- `'{a,b,c}'` → `["a","b","c"]` (strings)

### Type Detection
- `INT[]` → TEXT (with array functions)
- `TEXT[]` → TEXT (with array functions)
- `VARCHAR[]` → TEXT (with array functions)

## SQLite Function Registration

All array functions will be registered as SQLite scalar functions:
- `array_overlap(left, right)` → boolean
- `array_contains(left, right)` → boolean
- `array_contained(left, right)` → boolean
- `array_concat(left, right)` → text (JSON array)
- `array_append(arr, elem)` → text (JSON array)
- `array_prepend(elem, arr)` → text (JSON array)
- `array_remove(arr, elem)` → text (JSON array)
- `array_replace(arr, old, new)` → text (JSON array)
- `array_length(arr, dim)` → int
- `array_lower(arr, dim)` → int
- `array_upper(arr, dim)` → int
- `array_ndims(arr)` → int
- `array_dims(arr)` → text
- `array_cardinality(arr)` → int
- `array_position(arr, elem [, start])` → int
- `array_positions(arr, elem)` → text (JSON array of ints)
- `array_to_string(arr, delim [, null_str])` → text
- `string_to_array(str, delim [, null_str])` → text (JSON array)
- `array_fill(elem, dims [, bounds])` → text (JSON array)
- `trim_array(arr, n)` → text (JSON array)

## Array Format Parsing

### PostgreSQL Array Literal Parser
Parse formats like:
- `{1,2,3}` - simple 1D array
- `{{1,2},{3,4}}` - 2D array
- `{a,b,"c,d"}` - quoted elements with delimiters
- `{1,NULL,3}` - null values
- `[2:4]={1,2,3}` - explicit bounds

### JSON Array Parser
Parse standard JSON arrays:
- `[1,2,3]`
- `["a","b","c"]`
- `[1,null,3]`

## Implementation Order

### Phase 1: Core Module
1. Create `src/array.rs` module
2. Implement array parsing (PG literal and JSON)
3. Implement array serialization (to PG format)
4. Implement basic operators (`&&`, `@>`, `<@`, `||`)

### Phase 2: Array Functions
5. Implement manipulation functions (append, prepend, cat, remove, replace)
6. Implement information functions (length, lower, upper, ndims, dims, cardinality)
7. Implement search functions (position, positions)
8. Implement conversion functions (to_string, string_to_array, fill, trim)

### Phase 3: Integration
9. Update transpiler to handle array operators
10. Register all functions in `main.rs`
11. Handle array type in column definitions

### Phase 4: Testing
12. Write unit tests in `src/array.rs`
13. Write integration tests in `tests/array_tests.rs`
14. Write E2E tests in `tests/array_e2e_test.py`

### Phase 5: Documentation
15. Create `docs/ARRAYS.md` documentation
16. Update `README.md` with array features
17. Update `docs/TODO-FEATURES.md`

## Test Cases

### Operator Tests
```sql
-- Overlap
SELECT ARRAY[1,2,3] && ARRAY[3,4]; -- true
SELECT ARRAY[1,2,3] && ARRAY[4,5]; -- false

-- Contains
SELECT ARRAY[1,2,3] @> ARRAY[1,2]; -- true
SELECT ARRAY[1,2,3] @> ARRAY[1,4]; -- false

-- Contained by
SELECT ARRAY[1,2] <@ ARRAY[1,2,3]; -- true
SELECT ARRAY[1,4] <@ ARRAY[1,2,3]; -- false

-- Concatenation
SELECT ARRAY[1,2] || ARRAY[3,4]; -- {1,2,3,4}
SELECT ARRAY[1,2] || 3; -- {1,2,3}
```

### Function Tests
```sql
-- Append/prepend
SELECT array_append(ARRAY[1,2], 3); -- {1,2,3}
SELECT array_prepend(0, ARRAY[1,2]); -- {0,1,2}

-- Remove/replace
SELECT array_remove(ARRAY[1,2,2,3], 2); -- {1,3}
SELECT array_replace(ARRAY[1,2,2,3], 2, 9); -- {1,9,9,3}

-- Length
SELECT array_length(ARRAY[1,2,3], 1); -- 3
SELECT cardinality(ARRAY[1,2,3]); -- 3

-- Position
SELECT array_position(ARRAY['a','b','c','b'], 'b'); -- 2
SELECT array_positions(ARRAY[1,2,1,3,1], 1); -- {1,3,5}

-- String conversion
SELECT array_to_string(ARRAY['a','b','c'], ','); -- 'a,b,c'
SELECT string_to_array('a,b,c', ','); -- {a,b,c}
```

## Edge Cases to Handle

1. **NULL arrays**: Functions should handle NULL input gracefully
2. **Empty arrays**: `ARRAY[]::int[]` → `[]`
3. **NULL elements**: `{1,NULL,3}` → `[1,null,3]`
4. **Nested arrays**: `{{1,2},{3,4}}` → `[[1,2],[3,4]]`
5. **Quoted strings**: `{"hello, world","test"}` → handle commas in strings
6. **Type coercion**: Ensure numeric vs string comparisons work correctly

## Files to Create/Modify

### New Files
- `src/array.rs` - Array module implementation
- `tests/array_tests.rs` - Rust integration tests
- `tests/array_e2e_test.py` - Python E2E tests
- `docs/ARRAYS.md` - Array documentation

### Modified Files
- `src/main.rs` - Register array functions
- `src/transpiler.rs` - Handle array operators
- `src/lib.rs` - Export array module
- `README.md` - Document array support
- `docs/TODO-FEATURES.md` - Update array status
