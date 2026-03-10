# Task 3: Fix Array Type Metadata - Investigation Findings

## Problem Summary
Array slice operations return wrong column type metadata:
- PostgreSQL returns the element type (e.g., 'int4')
- PGQT returns 'array' or TEXT

Example:
```sql
SELECT ('{1,2,3}'::int[])[1]
-- PostgreSQL: returns 'int4' (element type)
-- PGQT: returns 'array' or TEXT
```

## Root Cause Analysis

### 1. Transpilation Flow
When transpiling `('{1,2,3}'::int[])[1]`:
1. The AIndirection node (in `src/transpiler/expr/mod.rs:76`) handles the `[1]` access
2. It converts the expression to `json_extract(..., '$[0]')`
3. The original type information (`int[]`) is lost

### 2. Type Determination in Handler
In `src/handler/query.rs:501-590`, the `build_field_info` function:
1. First looks for column metadata from catalog tables
2. Falls back to expression-based heuristics (e.g., `count(` → INT8)
3. Defaults to TEXT for unknown expressions including `json_extract`

### 3. Key Code Locations

**AIndirection Handling** (`src/transpiler/expr/mod.rs:76-102`):
```rust
NodeEnum::AIndirection(ref ind) => {
    let mut arg_sql = ind.arg.as_ref().map(|n| reconstruct_node(n, ctx)).unwrap_or_default();
    // ... builds json_path ...
    format!("json_extract({}, '${}')", arg_sql, json_path)
}
```

**Type Mapping** (`src/handler/rewriter.rs`):
- `map_original_type_to_pg_type()` maps array types to TEXT
- No special handling for json_extract element access

**Field Info Building** (`src/handler/query.rs:501-590`):
- Only handles simple expressions like `count(`, `sum(`, `avg(`
- Everything else defaults to TEXT

## Solution Design

### Option 1: Transpiler Type Tracking (Recommended)
Add expression type tracking to `TranspileResult`:

1. **Modify `TranspileResult`** to include `column_types: Vec<Option<String>>`
2. **Track types during transpilation**:
   - When AIndirection is processed, look at the argument's type cast
   - Extract element type from array type (e.g., `int[]` → `int4`)
3. **Use in handler**: Check `column_types` in `build_field_info` before falling back to heuristics

### Implementation Plan

**File 1: `src/transpiler/context.rs`**
- Add `column_types: Vec<Option<String>>` to `TranspileResult`
- Add type tracking methods to `TranspileContext`

**File 2: `src/transpiler/expr/mod.rs`**
- Modify AIndirection handling to detect array type casts
- Extract element type and store in context

**File 3: `src/transpiler/mod.rs`**
- Pass column_types through transpilation pipeline

**File 4: `src/handler/query.rs`**
- Modify `build_field_info` to use column_types from TranspileResult
- Add json_extract element type detection

## Type Mapping Reference

| Array Type | Element Type | pgwire Type |
|------------|--------------|-------------|
| int[]      | int4         | Type::INT4  |
| integer[]  | int4         | Type::INT4  |
| bigint[]   | int8         | Type::INT8  |
| text[]     | text         | Type::TEXT  |
| varchar[]  | varchar      | Type::VARCHAR |
| bool[]     | bool         | Type::BOOL  |
| real[]     | float4       | Type::FLOAT4 |
| double[]   | float8       | Type::FLOAT8 |

## Testing Strategy

Create a test that:
1. Transpiles `SELECT ('{1,2,3}'::int[])[1]`
2. Verifies the transpiled SQL contains `json_extract`
3. Checks that column type metadata is INT4 (not TEXT)

Also test:
- Slices: `SELECT ('{1,2,3}'::int[])[1:2]` (should return array type)
- Nested access: `SELECT ('{{1,2},{3,4}}'::int[])[1][2]`
- Different types: text[], bool[], etc.
