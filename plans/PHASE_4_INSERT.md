# Phase 4: INSERT Statement Improvements

## Overview

**Goal:** Fix remaining INSERT statement issues to improve `insert.sql` from 57.8% to 75%.

**Estimated Score Gain:** +1-2% overall compatibility

**Current Status:**
- insert.sql: 57.8% (229/396 statements)

---

## Sub-Phase 4.1: RETURNING Clause Enhancements

### Objective
Fix remaining RETURNING clause issues.

### Issues to Address

| Issue | Description | Example |
|-------|-------------|---------|
| Complex expressions | RETURNING with expressions | `INSERT ... RETURNING id * 2` |
| Aggregate functions | RETURNING with aggregates | `INSERT ... RETURNING count(*)` |
| Subqueries | RETURNING with subqueries | `INSERT ... RETURNING (SELECT ...)` |
| Column aliases | RETURNING with aliases | `INSERT ... RETURNING id AS new_id` |

### Implementation Steps

1. **Review Current Implementation:**
   - Look at `src/transpiler/dml.rs` for INSERT handling
   - Find the RETURNING clause implementation
   - Identify gaps in expression handling

2. **Fix Expression Handling:**
   ```rust
   // In src/transpiler/dml.rs
   // Ensure RETURNING expressions are properly reconstructed
   fn reconstruct_returning_clause(...) -> String {
       // Current code may only handle simple column references
       // Update to handle any expression
       let exprs: Vec<String> = returning_list
           .iter()
           .map(|node| reconstruct_node(node, ctx))
           .collect();
       format!("RETURNING {}", exprs.join(", "))
   }
   ```

3. **Handle Aliases:**
   ```rust
   // Handle ResTarget with name (alias)
   if let Some(NodeEnum::ResTarget(res_target)) = node.node.as_ref() {
       let expr = reconstruct_node(res_target.val.as_ref().unwrap(), ctx);
       if let Some(alias) = &res_target.name {
           format!("{} AS {}", expr, alias)
       } else {
           expr
       }
   }
   ```

4. **Test with Triggers:**
   - Ensure RETURNING works correctly when triggers modify NEW rows
   - The returned values should reflect trigger modifications

### Testing

Create tests in `tests/insert_tests.rs` or extend existing:
```rust
#[test]
fn test_insert_returning_expression() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INT PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    let result: (i32, i32) = conn.query_row(
        "INSERT INTO test VALUES (1, 'test') RETURNING id, id * 2",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();
    
    assert_eq!(result, (1, 2));
}

#[test]
fn test_insert_returning_alias() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INT PRIMARY KEY, name TEXT)",
        [],
    ).unwrap();
    
    let result: (i32,) = conn.query_row(
        "INSERT INTO test VALUES (1, 'test') RETURNING id AS new_id",
        [],
        |row| Ok((row.get("new_id").unwrap(),)),
    ).unwrap();
    
    assert_eq!(result, (1,));
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

## Sub-Phase 4.2: ON CONFLICT Enhancements

### Objective
Fix remaining ON CONFLICT (upsert) issues.

### Issues to Address

| Issue | Description | Example |
|-------|-------------|---------|
| Multiple conflict targets | ON CONFLICT with multiple columns | `ON CONFLICT (col1, col2)` |
| Complex WHERE clauses | DO UPDATE with WHERE | `DO UPDATE SET ... WHERE ...` |
| Subqueries in DO UPDATE | Subqueries in SET | `SET col = (SELECT ...)` |
| ON CONFLICT with RETURNING | Combined with RETURNING | `ON CONFLICT ... RETURNING` |

### Implementation Steps

1. **Review Current Implementation:**
   - Look at ON CONFLICT handling in `src/transpiler/dml.rs`
   - Check how it maps to SQLite's UPSERT syntax

2. **SQLite UPSERT Syntax:**
   ```sql
   -- PostgreSQL
   INSERT INTO table (a, b) VALUES (1, 2)
   ON CONFLICT (a) DO UPDATE SET b = excluded.b WHERE a > 0
   RETURNING *;
   
   -- SQLite equivalent
   INSERT INTO table (a, b) VALUES (1, 2)
   ON CONFLICT (a) DO UPDATE SET b = excluded.b WHERE a > 0
   RETURNING *;
   ```
   SQLite 3.35.0+ supports RETURNING in UPSERT.

3. **Fix DO UPDATE WHERE:**
   ```rust
   // Ensure WHERE clause in DO UPDATE is transpiled
   if let Some(where_clause) = &on_conflict.do_update_where {
       let where_sql = reconstruct_node(where_clause, ctx);
       result.push_str(&format!(" WHERE {}", where_sql));
   }
   ```

4. **Fix EXCLUDED Reference:**
   ```rust
   // EXCLUDED is a special table reference in ON CONFLICT
   // It should be passed through as-is for SQLite
   // PostgreSQL: EXCLUDED.column
   // SQLite: excluded.column (lowercase is fine)
   ```

### Testing

```rust
#[test]
fn test_upsert_with_where() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INT PRIMARY KEY, value INT, updated BOOLEAN)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO test VALUES (1, 10, false)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO test VALUES (1, 20, true) 
         ON CONFLICT (id) DO UPDATE SET value = excluded.value, updated = true 
         WHERE test.value < excluded.value",
        [],
    ).unwrap();
    
    let result: (i32, bool) = conn.query_row(
        "SELECT value, updated FROM test WHERE id = 1",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();
    
    assert_eq!(result, (20, true));
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

## Sub-Phase 4.3: Multi-Row INSERT Improvements

### Objective
Fix multi-row INSERT edge cases.

### Issues to Address

| Issue | Description | Example |
|-------|-------------|---------|
| Different column orders | VALUES with different ordering | `VALUES (1, 'a'), ('b', 2)` |
| DEFAULT values | Multi-row with DEFAULT | `VALUES (1, DEFAULT), (2, 'a')` |
| Complex expressions | VALUES with expressions | `VALUES (1+1, now()), (2, upper('a'))` |

### Implementation Steps

1. **Review Current Implementation:**
   - Check how multi-row INSERT is transpiled
   - SQLite supports: `INSERT INTO t (a, b) VALUES (1, 2), (3, 4)`

2. **Ensure Proper Transpilation:**
   ```rust
   // Multi-row VALUES should be passed through to SQLite
   // as SQLite natively supports it
   ```

3. **Handle DEFAULT in Multi-Row:**
   ```rust
   // SQLite doesn't support DEFAULT in multi-row INSERT
   // May need to split into separate INSERTs or use NULL
   ```

### Testing

```rust
#[test]
fn test_multi_row_insert() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INT, name TEXT)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO test VALUES (1, 'a'), (2, 'b'), (3, 'c')",
        [],
    ).unwrap();
    
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM test",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 3);
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

## Sub-Phase 4.4: Integration & Compatibility Suite Run

### Objective
Run the full compatibility suite and verify INSERT improvements.

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
   - Baseline: insert.sql: 57.8%
   - Target: insert.sql: 75%+
   - Document improvements

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in insert.sql score
- [ ] CHANGELOG.md updated with phase summary
- [ ] README.md updated with new compatibility percentage

---

## Summary

This phase focuses on INSERT statement improvements. By implementing these features, we expect to:

- Improve `insert.sql` from 57.8% to ~75% (+17 percentage points)
- Add ~1-2% to overall compatibility score

**Key Implementation Files:**
- `src/transpiler/dml.rs` (INSERT/RETURNING/ON CONFLICT handling)
- `tests/insert_tests.rs` (create or extend)
