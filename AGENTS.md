# PGQT Agent Guide

This document contains critical information for AI agents working on the PGQT project.

## Project Overview

PGQT is a PostgreSQL-compatible proxy that translates PostgreSQL wire protocol queries to SQLite. It enables PostgreSQL clients to connect to SQLite databases, supporting many PostgreSQL-specific features through transpilation.

## Testing Infrastructure

### Running Tests

We have a comprehensive test suite that should be run after any changes:

#### Quick Test (Unit + Integration)
```bash
cargo test
```

#### Full Test Suite (Recommended before committing)
```bash
./run_tests.sh
```

This runs:
- **Unit tests** (embedded in source files with `#[cfg(test)]`)
- **Integration tests** (`tests/*.rs` files)
- **E2E tests** (`tests/*_e2e_test.py` files) - requires Python + psycopg2

#### Test Options
```bash
./run_tests.sh --unit-only        # Unit tests only
./run_tests.sh --integration-only # Integration tests only
./run_tests.sh --e2e-only         # E2E tests only
./run_tests.sh --no-e2e           # Skip E2E tests
```

#### Running E2E Tests Only
```bash
# Run all e2e tests efficiently (single proxy instance)
python3 tests/run_all_e2e.py

# Run individual e2e test
python3 tests/array_e2e_test.py
```

### Test Organization

| Test Type | Location | Count | Purpose |
|-----------|----------|-------|---------|
| Unit tests | `src/*.rs` (embedded) | ~270 | Test individual functions and modules |
| Integration tests | `tests/*.rs` | ~200 | Test module interactions |
| E2E tests | `tests/*_e2e_test.py` | 9 | Full wire protocol testing |

**Current test files:**
- Unit: Embedded in `src/array.rs`, `src/catalog.rs`, `src/distinct_on.rs`, `src/fts.rs`, `src/geo.rs`, `src/range.rs`, `src/rls.rs`, `src/rls_inject.rs`, `src/schema.rs`, `src/transpiler.rs`, `src/vector.rs`
- Integration: `tests/array_tests.rs`, `tests/catalog_tests.rs`, `tests/distinct_on_tests.rs`, `tests/fts_integration_tests.rs`, `tests/integration_test.rs`, `tests/rls_integration_tests.rs`, `tests/schema_tests.rs`, `tests/transpiler_tests.rs`, `tests/vector_tests.rs`, `tests/window_tests.rs`
- E2E: `tests/array_e2e_test.py`, `tests/distinct_on_e2e_test.py`, `tests/geo_e2e_test.py`, `tests/range_e2e_test.py`, `tests/rls_e2e_test.py`, `tests/schema_e2e_test.py`, `tests/vector_e2e_test.py`, `tests/window_e2e_test.py`

### Adding New Tests

When adding a new feature, you MUST add corresponding tests. The test suite automatically discovers tests based on naming conventions.

**Unit tests**: Add to the bottom of the source file:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        assert_eq!(my_function(), expected_result);
    }
}
```
- Run with: `cargo test my_feature`
- Automatically picked up by `run_tests.sh`

**Integration tests**: Create `tests/my_feature_tests.rs`:
```rust
use pgqt::transpiler::transpile;

#[test]
fn test_transpilation() {
    let sql = "SELECT ...";
    let result = transpile(sql);
    assert!(result.contains("expected"));
}
```
- File naming: `tests/<feature>_tests.rs`
- Run with: `cargo test --test my_feature_tests`
- Automatically picked up by `run_tests.sh` (any `tests/*.rs` file)

**E2E tests**: Create `tests/my_feature_e2e_test.py`:
```python
#!/usr/bin/env python3
"""
End-to-end tests for my feature.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_myfeature_e2e.db"

def start_proxy():
    # ... see existing e2e tests for pattern
    pass

def stop_proxy(proc):
    # ... see existing e2e tests for pattern
    pass

def test_my_feature():
    """Test my feature through wire protocol."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Test implementation
        cur.execute("SELECT ...")
        result = cur.fetchall()
        assert result == expected
        
        cur.close()
        conn.close()
        print("test_my_feature: PASSED")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    test_my_feature()
```
- File naming: `tests/<feature>_e2e_test.py`
- Must print "PASSED" on success
- Run with: `python3 tests/my_feature_e2e_test.py`
- Automatically picked up by `run_tests.sh` (any `tests/*_e2e_test.py` file)
- Also picked up by `tests/run_all_e2e.py` unified runner

### Test Discovery Summary

The test suite discovers tests automatically:

| Test Type | Discovery Pattern | Location |
|-----------|------------------|----------|
| Unit tests | `#[test]` functions in `#[cfg(test)]` modules | `src/*.rs` |
| Integration tests | `tests/*.rs` files | `tests/` |
| E2E tests | `tests/*_e2e_test.py` files | `tests/` |

**No registry updates needed** - just create files following the naming conventions and they'll be picked up automatically.

## Critical Implementation Details

### Array vs Range Operator Detection

The transpiler must distinguish between array operators (`&&`, `@>`, `<@`) and range operators. This is handled in `src/transpiler.rs` in the `reconstruct_a_expr` function.

**Key logic**:
- Check if operands are `ArrayExpr` or `AArrayExpr` AST nodes
- Check if operands are string literals containing JSON arrays (`'[...]'`)
- If either is true → use array functions (`array_overlap`, `array_contains`, `array_contained`)
- Otherwise → use range functions (`range_overlaps`, `range_contains`, `range_contained`)

**Example**:
```sql
-- Array operations (use array_* functions)
SELECT ARRAY[1,2,3] && ARRAY[3,4];
SELECT tags @> '["admin"]' FROM users;

-- Range operations (use range_* functions)
SELECT r1 @> r2 FROM ranges;
SELECT '[1,10)' && '[5,15)';
```

### Feature Modules

| Module | File | Description |
|--------|------|-------------|
| Array | `src/array.rs` | PostgreSQL array functions and operators |
| Range | `src/range.rs` | PostgreSQL range types and operators |
| Vector | `src/vector.rs` | pgvector-compatible vector operations |
| FTS | `src/fts.rs` | Full-text search (to_tsvector, to_tsquery) |
| RLS | `src/rls.rs`, `src/rls_inject.rs` | Row-Level Security |
| Schema | `src/schema.rs` | Schema management (CREATE SCHEMA, search_path) |
| Window | `src/window.rs` | Window functions (ROW_NUMBER, RANK, etc.) |
| Geo | `src/geo.rs` | Geometric types (point, box, circle) |
| Catalog | `src/catalog.rs` | Metadata storage in SQLite |
| Transpiler | `src/transpiler.rs` | SQL parsing and transformation |

### Transpilation Pipeline

1. **Parse**: Use `pg_query` to parse PostgreSQL SQL into AST
2. **Transform**: Walk AST, convert PostgreSQL-specific nodes to SQLite equivalents
3. **Reconstruct**: Generate SQLite SQL from transformed AST
4. **Execute**: Run against SQLite database

### Common Pitfalls

1. **String literals in operators**: `'["admin"]'` is a string literal, not an ArrayExpr. Check `is_json_array_string()` for these cases.

2. **Type detection**: Many PostgreSQL types don't exist in SQLite. Store type metadata in the catalog (`__pg_catalog__` tables).

3. **Function overloading**: PostgreSQL functions like `||` work differently for text, JSONB, and tsvector. Check operand types.

4. **Case sensitivity**: PostgreSQL identifiers are case-insensitive unless quoted. SQLite is case-sensitive for table names but not column names.

## Development Workflow

### Before Starting Work

1. Create a worktree for isolation:
   ```bash
   git worktree add .worktrees/my-feature -b feature/my-feature
   cd .worktrees/my-feature
   ```

2. Run tests to ensure clean baseline:
   ```bash
   cargo test
   ```

### During Development

1. Run relevant tests frequently:
   ```bash
   cargo test --test my_feature_tests  # Integration tests
   cargo test my_feature               # Unit tests
   ```

2. Check transpilation output:
   ```bash
   cargo run -- --transpile "SELECT ..."
   ```

### Before Committing

1. Run full test suite:
   ```bash
   ./run_tests.sh
   ```

2. Check for warnings:
   ```bash
   cargo clippy
   cargo check
   ```

3. Clean up temporary files:
   ```bash
   rm -f *.db *.db.error.log
   ```

## Project Structure

```
.
├── src/
│   ├── main.rs           # Proxy server and protocol handling
│   ├── lib.rs            # Library exports
│   ├── transpiler.rs     # SQL transpilation (largest file)
│   ├── catalog.rs        # Metadata management
│   ├── array.rs          # Array support
│   ├── range.rs          # Range types
│   ├── vector.rs         # Vector operations
│   ├── fts.rs            # Full-text search
│   ├── rls.rs            # Row-Level Security
│   ├── rls_inject.rs     # RLS query injection
│   ├── schema.rs         # Schema management
│   ├── window.rs         # Window functions
│   ├── geo.rs            # Geometric types
│   └── plpgsql.rs        # PL/pgSQL parser stub
├── tests/
│   ├── *_tests.rs        # Rust integration tests
│   ├── *_e2e_test.py     # Python e2e tests
│   └── run_all_e2e.py    # Unified e2e runner
├── examples/             # Example usage code
├── docs/                 # Documentation
├── run_tests.sh          # Test runner script
├── Cargo.toml
└── README.md
```

## Important: Using the Proxy Server

**DO NOT** manually start/stop the proxy with `pkill` and background processes like:
```bash
./target/release/pgqt --port 5434 --database /tmp/test.db &
# ... run tests ...
pkill -f pgqt
```

**INSTEAD** use the `process` tool which properly manages background processes:
```bash
# Start a managed process
process(action="start", name="pgqt-proxy", command="./target/release/pgqt --port 5434 --database /tmp/test.db")

# Check its status and output
process(action="output", id="pgqt-proxy")

# Kill it when done
process(action="kill", id="pgqt-proxy")
```

This ensures:
- Proper error handling and notifications (alertOnFailure, alertOnSuccess)
- Clean process management without zombies
- Access to stdout/stderr outputs via `process(action="output", id=...)`
- Integration with pi's process lifecycle management

## Dependencies

**Core**:
- `tokio` - Async runtime
- `pgwire` - PostgreSQL wire protocol
- `rusqlite` - SQLite bindings
- `pg_query` - PostgreSQL SQL parser

**Development**:
- `serde_json` - JSON handling

**E2E Testing**:
- Python 3
- `psycopg2-binary` - PostgreSQL Python driver

## Known Limitations

1. Not all PostgreSQL features are supported (see README.md for full list)
2. Some type conversions are lossy (e.g., NUMERIC → REAL)
3. Concurrent write performance limited by SQLite
4. Some window functions use in-memory processing

## Resources

- [PostgreSQL Documentation](https://www.postgresql.org/docs/)
- [SQLite Documentation](https://www.sqlite.org/docs.html)
- [pg_query documentation](https://docs.rs/pg_query/)
- [pgwire documentation](https://docs.rs/pgwire/)

## Contact

For questions or issues, refer to the project README.md or create an issue in the repository.
