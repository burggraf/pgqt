# Phase 5: CTE (WITH Clause) Enhancements

## Overview

**Goal:** Fix CTE issues to improve `with.sql` from 52.5% to 75%.

**Estimated Score Gain:** +1-2% overall compatibility

**Current Status:**
- with.sql: 52.5% (165/314 statements)

---

## Sub-Phase 5.1: Recursive CTE Support

### Objective
Support recursive CTEs (`WITH RECURSIVE`).

### Features to Support

| Feature | Description | Example |
|---------|-------------|---------|
| Basic recursive CTE | Self-referencing CTE | `WITH RECURSIVE t AS (base UNION ALL recursive) ...` |
| Multiple recursive CTEs | Multiple recursive definitions | `WITH RECURSIVE a AS (...), b AS (...) ...` |
| Cycle detection | Detect and handle cycles | Use in graph traversal |

### Implementation Steps

1. **Review Current Implementation:**
   - Look at `src/transpiler/dml.rs` for CTE handling
   - Check if RECURSIVE keyword is passed through

2. **SQLite Recursive CTE Support:**
   SQLite supports recursive CTEs natively with the same syntax:
   ```sql
   WITH RECURSIVE t(n) AS (
       VALUES (1)                    -- Base case
       UNION ALL
       SELECT n+1 FROM t WHERE n < 5 -- Recursive case
   )
   SELECT * FROM t;
   ```

3. **Transpiler Updates:**
   ```rust
   // In src/transpiler/dml.rs
   // Ensure RECURSIVE keyword is preserved
   fn reconstruct_with_clause(with_clause: &WithClause, ctx: &mut TranspileContext) -> String {
       let mut result = String::new();
       
       if with_clause.recursive {
           result.push_str("WITH RECURSIVE ");
       } else {
           result.push_str("WITH ");
       }
       
       // ... handle CTE list
   }
   ```

4. **Handle UNION vs UNION ALL:**
   - Recursive CTEs typically need `UNION ALL` for recursion
   - `UNION` alone removes duplicates and can prevent infinite loops
   - Both should be supported

### Testing

Create `tests/cte_tests.rs`:
```rust
use pgqt::test_utils::setup_test_db;

#[test]
fn test_recursive_cte() {
    let conn = setup_test_db();
    
    // Simple number sequence
    let results: Vec<i32> = conn
        .prepare("WITH RECURSIVE t(n) AS (VALUES (1) UNION ALL SELECT n+1 FROM t WHERE n < 5) SELECT n FROM t")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    
    assert_eq!(results, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_tree_traversal() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE tree (id INT PRIMARY KEY, parent_id INT, name TEXT)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO tree VALUES (1, NULL, 'root'), (2, 1, 'child1'), (3, 1, 'child2'), (4, 2, 'grandchild')",
        [],
    ).unwrap();
    
    // Find all descendants of root
    let results: Vec<String> = conn
        .prepare(
            "WITH RECURSIVE descendants AS (
                SELECT id, parent_id, name FROM tree WHERE id = 1
                UNION ALL
                SELECT t.id, t.parent_id, t.name 
                FROM tree t 
                JOIN descendants d ON t.parent_id = d.id
            )
            SELECT name FROM descendants"
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    
    assert_eq!(results, vec!["root", "child1", "child2", "grandchild"]);
}
```

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Documentation updated in `docs/CTE.md`
- [ ] CHANGELOG.md updated

---

## Sub-Phase 5.2: Multiple CTEs Enhancement

### Objective
Fix issues with multiple CTEs.

### Issues to Address

| Issue | Description | Example |
|-------|-------------|---------|
| CTEs referencing each other | Later CTEs referencing earlier ones | `WITH a AS (...), b AS (SELECT * FROM a) ...` |
| Column name lists | CTEs with explicit column names | `WITH a(x, y) AS (SELECT 1, 2) ...` |
| CTEs in subqueries | CTEs within subqueries | `SELECT * FROM (WITH a AS (...) SELECT * FROM a)` |

### Implementation Steps

1. **Ensure CTE Order Preservation:**
   ```rust
   // CTEs should be processed in order
   // Later CTEs can reference earlier ones
   ```

2. **Handle Column Lists:**
   ```rust
   // WITH a(x, y) AS (SELECT 1, 2)
   // Should transpile to same (SQLite supports this)
   ```

3. **Transpiler Updates:**
   ```rust
   // In reconstruct_cte
   if !cte.aliascolnames.is_empty() {
       let col_names: Vec<String> = cte.aliascolnames
           .iter()
           .filter_map(|n| extract_string_value(n))
           .collect();
       result.push_str(&format!("({}) ", col_names.join(", ")));
   }
   ```

### Testing

```rust
#[test]
fn test_multiple_ctes() {
    let conn = setup_test_db();
    
    let result: (i32, i32) = conn.query_row(
        "WITH a AS (SELECT 1 AS x), 
              b AS (SELECT x + 1 AS y FROM a)
         SELECT * FROM a, b",
        [],
        |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
    ).unwrap();
    
    assert_eq!(result, (1, 2));
}

#[test]
fn test_cte_with_column_list() {
    let conn = setup_test_db();
    
    let result: i32 = conn.query_row(
        "WITH a(x, y) AS (SELECT 1, 2)
         SELECT x + y FROM a",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, 3);
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

## Sub-Phase 5.3: Data-Modifying CTEs

### Objective
Support data-modifying statements in CTEs.

### Features to Support

| Feature | Description | Example |
|---------|-------------|---------|
| INSERT in CTE | INSERT as CTE | `WITH t AS (INSERT ... RETURNING ...) SELECT ...` |
| UPDATE in CTE | UPDATE as CTE | `WITH t AS (UPDATE ... RETURNING ...) SELECT ...` |
| DELETE in CTE | DELETE as CTE | `WITH t AS (DELETE ... RETURNING ...) SELECT ...` |

### Implementation Steps

1. **SQLite Support:**
   SQLite 3.35.0+ supports RETURNING in CTEs:
   ```sql
   WITH inserted AS (
       INSERT INTO t (a) VALUES (1), (2)
       RETURNING id, a
   )
   SELECT * FROM inserted;
   ```

2. **Transpiler Updates:**
   ```rust
   // Ensure data-modifying statements in CTEs are properly transpiled
   // The RETURNING clause is key here
   ```

3. **Handle Chained CTEs:**
   ```sql
   WITH 
       deleted AS (DELETE FROM old_table WHERE ... RETURNING *),
       inserted AS (INSERT INTO new_table SELECT * FROM deleted RETURNING *)
   SELECT * FROM inserted;
   ```

### Testing

```rust
#[test]
fn test_insert_cte() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, value INT)",
        [],
    ).unwrap();
    
    let results: Vec<i32> = conn
        .prepare(
            "WITH inserted AS (
                INSERT INTO test (value) VALUES (10), (20)
                RETURNING id
            )
            SELECT id FROM inserted"
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    
    assert_eq!(results.len(), 2);
}

#[test]
fn test_chained_ctes() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE TABLE source (id INT, value INT)",
        [],
    ).unwrap();
    conn.execute(
        "CREATE TABLE dest (id INT, value INT)",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT INTO source VALUES (1, 100), (2, 200)",
        [],
    ).unwrap();
    
    let count: i32 = conn.query_row(
        "WITH 
            deleted AS (DELETE FROM source RETURNING *),
            inserted AS (INSERT INTO dest SELECT * FROM deleted RETURNING *)
         SELECT COUNT(*) FROM inserted",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(count, 2);
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

## Sub-Phase 5.4: Integration & Compatibility Suite Run

### Objective
Run the full compatibility suite and verify CTE improvements.

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
   - Baseline: with.sql: 52.5%
   - Target: with.sql: 75%+
   - Document improvements

### Verification Checklist

- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy --release` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in with.sql score
- [ ] CHANGELOG.md updated with phase summary
- [ ] README.md updated with new compatibility percentage

---

## Summary

This phase focuses on CTE (WITH clause) improvements. By implementing these features, we expect to:

- Improve `with.sql` from 52.5% to ~75% (+22 percentage points)
- Add ~1-2% to overall compatibility score

**Key Implementation Files:**
- `src/transpiler/dml.rs` (CTE handling)
- `tests/cte_tests.rs` (create)
- `docs/CTE.md` (create)
