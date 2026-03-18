# Phase 1: JSON/JSONB Functions & Operators

## Overview

**Goal:** Implement comprehensive JSON/JSONB support to improve `json.sql` from 38.5% to 80% and `jsonb.sql` from 58.5% to 85%.

**Estimated Score Gain:** +7-10% overall compatibility

**Current Status:**
- json.sql: 38.5% (180/468 statements)
- jsonb.sql: 58.5% (642/1098 statements)

---

## Sub-Phase 1.1: JSON Constructor Functions

### Objective
Implement JSON construction functions that convert various inputs to JSON.

### Functions to Implement

| Function | Signature | Description |
|----------|-----------|-------------|
| `to_json` | `to_json(anyelement)` | Convert any value to JSON |
| `to_jsonb` | `to_jsonb(anyelement)` | Convert any value to JSONB |
| `row_to_json` | `row_to_json(record)` | Convert a row to JSON object |
| `array_to_json` | `array_to_json(anyarray)` | Convert array to JSON array |
| `json_build_object` | `json_build_object(VARIADIC "any")` | Build JSON object from variadic args |
| `jsonb_build_object` | `jsonb_build_object(VARIADIC "any")` | Build JSONB object from variadic args |
| `json_build_array` | `json_build_array(VARIADIC "any")` | Build JSON array from variadic args |
| `jsonb_build_array` | `jsonb_build_array(VARIADIC "any")` | Build JSONB array from variadic args |

### Implementation Steps

1. **Create/Update `src/json.rs`** (or add to `src/jsonb.rs`)
   ```rust
   // Example structure
   use rusqlite::functions::FunctionFlags;
   use rusqlite::Connection;
   use serde_json::Value as JsonValue;

   pub fn register_json_functions(conn: &Connection) -> rusqlite::Result<()> {
       // Register to_json
       conn.create_scalar_function(
           "to_json",
           1,
           FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
           |ctx| {
               let val = ctx.get_raw(0);
               // Convert to JSON string
               Ok(convert_to_json(val)?)
           },
       )?;
       // ... register other functions
       Ok(())
   }
   ```

2. **Handle Type Conversions:**
   - NULL → `null`
   - Integer → number
   - Real → number
   - Text → string (or parse if valid JSON)
   - Blob → hex string
   - Arrays → JSON array
   - Records → JSON object

3. **Variadic Function Handling:**
   - SQLite doesn't support variadic functions directly
   - Use multiple function registrations with different arities (1-10 args)
   - Or use a single function that parses a JSON array of arguments

4. **Register Functions:**
   - Add to `src/handler/mod.rs` in `register_custom_functions()`
   - Call `register_json_functions(conn)?;`

5. **Transpiler Updates:**
   - Update `src/transpiler/func.rs` to recognize these functions
   - Map to appropriate SQLite function calls

### Testing

Create `tests/json_constructor_tests.rs`:
```rust
use pgqt::test_utils::setup_test_db;

#[test]
fn test_to_json_basic() {
    let conn = setup_test_db();
    
    let result: String = conn.query_row(
        "SELECT to_json('hello')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(result, "\"hello\"");
}

#[test]
fn test_json_build_object() {
    let conn = setup_test_db();
    
    let result: String = conn.query_row(
        "SELECT json_build_object('a', 1, 'b', 'text')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(result.contains("\"a\":1"));
    assert!(result.contains("\"b\":\"text\""));
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] New unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated with new functions

---

## Sub-Phase 1.2: JSON Processing Functions

### Objective
Implement functions that extract and process JSON data.

### Functions to Implement

| Function | Signature | Description |
|----------|-----------|-------------|
| `json_each` | `json_each(json)` | Expand JSON object/array to row set |
| `jsonb_each` | `jsonb_each(jsonb)` | Expand JSONB object to row set |
| `json_each_text` | `json_each_text(json)` | Like json_each but returns text values |
| `jsonb_each_text` | `jsonb_each_text(jsonb)` | Like jsonb_each but returns text values |
| `json_array_elements` | `json_array_elements(json)` | Expand JSON array to row set |
| `jsonb_array_elements` | `jsonb_array_elements(jsonb)` | Expand JSONB array to row set |
| `json_array_elements_text` | `json_array_elements_text(json)` | Like json_array_elements but text |
| `jsonb_array_elements_text` | `jsonb_array_elements_text(jsonb)` | Like jsonb_array_elements but text |
| `json_object_keys` | `json_object_keys(json)` | Return keys of JSON object |
| `jsonb_object_keys` | `jsonb_object_keys(jsonb)` | Return keys of JSONB object |

### Implementation Steps

1. **Table-Valued Functions:**
   - These return multiple rows
   - Options:
     a. Use SQLite's `create_module` for virtual tables
     b. Return JSON array and use `json_each()` in SQLite
     c. Use scalar functions that return delimited strings

2. **Recommended Approach:**
   - Implement as scalar functions that return JSON arrays
   - Transpiler wraps calls with `json_each()` for iteration
   - Example: `json_each('{"a":1}')` → returns `'[["a", 1]]'`

3. **Implementation Example:**
   ```rust
   conn.create_scalar_function(
       "json_each_impl",
       1,
       FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
       |ctx| {
           let json_str: String = ctx.get(0)?;
           let val: JsonValue = serde_json::from_str(&json_str)
               .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
           
           let result = match val {
               JsonValue::Object(map) => {
                   let pairs: Vec<Vec<JsonValue>> = map
                       .into_iter()
                       .map(|(k, v)| vec![JsonValue::String(k), v])
                       .collect();
                   JsonValue::Array(pairs.iter().map(|p| JsonValue::Array(p.clone())).collect())
               }
               JsonValue::Array(arr) => {
                   JsonValue::Array(arr.into_iter().enumerate()
                       .map(|(i, v)| JsonValue::Array(vec![JsonValue::Number(i.into()), v]))
                       .collect())
               }
               _ => JsonValue::Array(vec![]),
           };
           
           Ok(result.to_string())
       },
   )?;
   ```

4. **Transpiler Updates:**
   - In `src/transpiler/func.rs`, detect `json_each()` calls
   - Wrap with SQLite's `json_each()`: `SELECT * FROM json_each(json_each_impl('{...}'))`

### Testing

Test with LATERAL joins (should already work from JOIN improvements):
```sql
SELECT * FROM my_table, LATERAL json_each(my_table.data);
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 1.3: JSON Aggregation Functions

### Objective
Implement aggregate functions for JSON.

### Functions to Implement

| Function | Signature | Description |
|----------|-----------|-------------|
| `json_agg` | `json_agg(anyelement)` | Aggregate values into JSON array |
| `jsonb_agg` | `jsonb_agg(anyelement)` | Aggregate values into JSONB array |
| `json_object_agg` | `json_object_agg(key, value)` | Aggregate key-value pairs into JSON object |
| `jsonb_object_agg` | `jsonb_object_agg(key, value)` | Aggregate key-value pairs into JSONB object |

### Implementation Steps

1. **Aggregate Function Pattern:**
   Look at `src/array_agg.rs` for the pattern:
   ```rust
   use rusqlite::functions::{Aggregate, FunctionFlags};
   
   struct JsonAggState {
       values: Vec<JsonValue>,
   }
   
   impl Aggregate<JsonAggState, String> for JsonAgg {
       fn init(&self, _ctx: &mut Context) -> JsonAggState {
           JsonAggState { values: Vec::new() }
       }
       
       fn step(&self, ctx: &mut Context, state: &mut JsonAggState) {
           let val = convert_to_json(ctx.get_raw(0));
           state.values.push(val);
       }
       
       fn finalize(&self, _ctx: &mut Context, state: Option<JsonAggState>) -> String {
           let state = state?;
           JsonValue::Array(state.values).to_string()
       }
   }
   ```

2. **Register as Aggregate:**
   ```rust
   conn.create_aggregate_function(
       "json_agg",
       1,
       FunctionFlags::SQLITE_UTF8,
       JsonAgg,
   )?;
   ```

3. **Handle NULL Values:**
   - Standard `json_agg` includes NULLs as `null`
   - May need `json_agg_strict` variant that skips NULLs

4. **json_object_agg:**
   - Takes two arguments: key and value
   - Build a JSON object from all key-value pairs
   - Handle duplicate keys (last one wins)

### Testing

Test with GROUP BY:
```sql
SELECT category, json_agg(name) FROM products GROUP BY category;
SELECT json_object_agg(name, price) FROM products;
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 1.4: JSON Operators

### Objective
Implement PostgreSQL JSON operators in the transpiler.

### Operators to Implement

| Operator | Description | SQLite Equivalent |
|----------|-------------|-------------------|
| `->` | Get JSON field/element by key/index | `json_extract(json, '$.key')` or `json_extract(json, '$[index]')` |
| `->>` | Get JSON field/element as text | `json_extract(json, '$.key')` |
| `#>` | Get JSON at path | `json_extract(json, '$.a.b.c')` |
| `#>>` | Get JSON at path as text | `json_extract(json, '$.a.b.c')` |
| `@>` | JSON contains | `jsonb_contains(json1, json2)` |
| `<@` | JSON is contained by | `jsonb_contained(json1, json2)` |
| `?` | Does key exist? | `jsonb_exists(json, key)` |
| `?|` | Does any key exist? | `jsonb_exists_any(json, keys_array)` |
| `?&` | Do all keys exist? | `jsonb_exists_all(json, keys_array)` |
| `||` | Concatenate JSON | Custom function |
| `-` | Delete key/array element | Custom function |
| `#-` | Delete at path | Custom function |

### Implementation Steps

1. **Path Conversion:**
   PostgreSQL uses `{a,b,c}` syntax, SQLite uses `$.a.b.c`:
   ```rust
   fn convert_pg_path_to_sqlite(path: &str) -> String {
       // Input: '{a,b,0,c}'
       // Output: '$.a.b[0].c'
       let trimmed = path.trim_matches('{').trim_matches('}');
       let parts: Vec<&str> = trimmed.split(',').collect();
       let mut result = String::from("$");
       for part in parts {
           let part = part.trim();
           if part.parse::<i64>().is_ok() {
               result.push_str(&format!("[{}]", part));
           } else {
               result.push_str(&format!(".{}"), part.trim_matches('"'));
           }
       }
       result
   }
   ```

2. **Transpiler Updates in `src/transpiler/expr.rs`:**
   ```rust
   // In reconstruct_a_expr or similar function
   match operator {
       "->" => {
           let json = reconstruct_node(left, ctx);
           let key = reconstruct_node(right, ctx);
           if key.parse::<i64>().is_ok() {
               format!("json_extract({}, '$[{}]')", json, key)
           } else {
               format!("json_extract({}, '$.{}')", json, key.trim_matches('\''))
           }
       }
       "@>" => {
           let container = reconstruct_node(left, ctx);
           let contained = reconstruct_node(right, ctx);
           format!("jsonb_contains({}, {})", container, contained)
       }
       // ... other operators
   }
   ```

3. **Add Missing Functions:**
   - `json_concat(json1, json2)` - Merge two JSON objects
   - `json_delete(json, key)` - Delete key from object
   - `json_delete_at(json, index)` - Delete element at index
   - `json_delete_path(json, path)` - Delete at path

### Testing

Test each operator:
```sql
SELECT '{"a":1,"b":2}'::json->'a';  -- Should return 1
SELECT '{"a":1,"b":2}'::json->>'a'; -- Should return "1" (text)
SELECT '{"a":{"b":3}}'::json#>'{a,b}'; -- Should return 3
SELECT '{"a":1,"b":2}'::json @> '{"a":1}'; -- Should return true
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 1.5: JSON Type Casting & Validation

### Objective
Support JSON type casting and validation functions.

### Functions to Implement

| Function | Signature | Description |
|----------|-----------|-------------|
| `json_typeof` | `json_typeof(json)` | Return type of JSON value |
| `jsonb_typeof` | `jsonb_typeof(jsonb)` | Return type of JSONB value |
| `json_strip_nulls` | `json_strip_nulls(json)` | Remove object fields with null values |
| `jsonb_strip_nulls` | `jsonb_strip_nulls(jsonb)` | Remove object fields with null values |
| `json_pretty` | `json_pretty(json)` | Pretty-print JSON |
| `jsonb_pretty` | `jsonb_pretty(jsonb)` | Pretty-print JSONB |
| `jsonb_set` | `jsonb_set(target, path, new_value)` | Update value at path |
| `jsonb_insert` | `jsonb_insert(target, path, new_value)` | Insert value at path |

### Implementation Steps

1. **Type Detection:**
   ```rust
   fn json_typeof(json_str: &str) -> String {
       let val: JsonValue = serde_json::from_str(json_str).unwrap_or(JsonValue::Null);
       match val {
           JsonValue::Null => "null".to_string(),
           JsonValue::Bool(_) => "boolean".to_string(),
           JsonValue::Number(_) => "number".to_string(),
           JsonValue::String(_) => "string".to_string(),
           JsonValue::Array(_) => "array".to_string(),
           JsonValue::Object(_) => "object".to_string(),
       }
   }
   ```

2. **Pretty Printing:**
   Use serde_json's pretty printer:
   ```rust
   fn json_pretty(json_str: &str) -> String {
       let val: JsonValue = serde_json::from_str(json_str)
           .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
       serde_json::to_string_pretty(&val)
           .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))
   }
   ```

3. **Path-Based Modification:**
   Implement `jsonb_set` and `jsonb_insert`:
   ```rust
   fn jsonb_set(target: &str, path: &str, new_value: &str) -> String {
       let mut val: JsonValue = serde_json::from_str(target)
           .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
       let new_val: JsonValue = serde_json::from_str(new_value)
           .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
       
       // Parse path and navigate/set value
       let path_parts = parse_path(path);
       set_value_at_path(&mut val, &path_parts, new_val);
       
       val.to_string()
   }
   ```

### Testing

Test type detection, pretty printing, and path modifications.

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 1.6: Integration & Compatibility Suite Run

### Objective
Run the full compatibility suite and verify JSON improvements.

### Tasks

1. **Build and Test:**
   ```bash
   cargo build --release
   cargo clippy --release
   ./run_tests.sh
   ```

2. **Run Compatibility Suite:**
   ```bash
   cd postgres-compatibility-suite
   source venv/bin/activate
   python3 runner_with_stats.py
   ```

3. **Compare Results:**
   - Baseline: json.sql: 38.5%, jsonb.sql: 58.5%
   - Target: json.sql: 80%+, jsonb.sql: 85%+
   - Document improvements

4. **Fix Remaining High-Priority Failures:**
   - Identify top 5 remaining failure patterns
   - Fix if quick wins (< 2 hours each)

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in json.sql and jsonb.sql scores
- [ ] CHANGELOG.md updated with phase summary
- [ ] README.md updated with new compatibility percentage

---

## Summary

This phase focuses on comprehensive JSON/JSONB support, which is critical for modern applications. By implementing these functions and operators, we expect to:

- Improve `json.sql` from 38.5% to ~80% (+41.5 percentage points)
- Improve `jsonb.sql` from 58.5% to ~85% (+26.5 percentage points)
- Add ~7-10% to overall compatibility score

**Key Implementation Files:**
- `src/jsonb.rs` (extend) or `src/json.rs` (create)
- `src/handler/mod.rs` (register functions)
- `src/transpiler/func.rs` (function handling)
- `src/transpiler/expr.rs` (operator handling)
- `tests/json_function_tests.rs` (create)
- `docs/JSON.md` (create)
