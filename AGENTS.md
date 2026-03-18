# PGQT Agent Guide

This document contains critical information for AI agents working on the PGQT project.

## Project Overview

PGQT is a PostgreSQL-compatible proxy that translates PostgreSQL wire protocol queries to SQLite. It enables PostgreSQL clients to connect to SQLite databases, supporting many PostgreSQL-specific features through transpilation.

## Subagent Strategy

- Always and aggressively offload online research (eg, docs), codebase exploration, and log analysis to subagents.
- When you're about to check logs, defer that to a subagent (ideally using gemini-2.5-flash).
- For complex problems you're going around in circles with, get a fresh perspective by asking subagents.

## Codebase Search with RustDex

RustDex is a powerful code indexing and search tool available for navigating the PGQT codebase. It provides:

- **Symbol search**: Find functions, structs, methods by exact name
- **Semantic search**: Search code using natural language descriptions
- **Route extraction**: Find HTTP routes from web framework definitions
- **Fast indexed search**: Results are instant - no grepping through files

### Indexing the Codebase

Before using RustDex, ensure the codebase is indexed:

```bash
rustdex_index /Users/markb/dev/pgqt --name pgqt
```

This creates a local SQLite database with symbol metadata and embeddings. Re-run this after major changes to keep the index current.

### Symbol Search (Exact Name)

Find specific functions, structs, or methods:

```rust
rustdex_search query="reconstruct_a_expr" repo=pgqt
rustdex_search query="transpile_with_rls" repo=pgqt
rustdex_search query="SqliteHandler" repo=pgqt
```

Returns matching symbols with file paths and byte ranges.

### Semantic Search (Natural Language)

Find code related to a concept or behavior:

```rust
rustdex_semantic query="how array operators are distinguished from range operators" repo=pgqt
rustdex_semantic query="row-level security policy injection" repo=pgqt
rustdex_semantic query="PostgreSQL wire protocol message handling" repo=pgqt
```

Use this when you don't know the exact function name but know what the code does.

### Reading Source Code

After finding a symbol with RustDex, read its actual source code:

```rust
rustdex_read_symbol file=/Users/markb/dev/pgqt/src/transpiler/expr.rs start_byte=12345 end_byte=12500
```

The byte ranges come from RustDex search results.

### When to Use RustDex

- **Finding implementation locations**: Where is a specific function defined?
- **Understanding code flow**: Which functions call a particular method?
- **Cross-file references**: Locate all uses of a struct or function
- **Feature discovery**: Find code related to a concept without knowing exact names

### Integration with pi Tools

RustDex is available as a pi tool and can be used alongside:

- `read`: Read full file contents
- `bash`: Use `rg`, `grep` for unindexed searches
- Subagents: Offload complex codebase exploration tasks

Always prefer RustDex over manual grepping for symbol searches - it's faster and more accurate.

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

| Test Type         | Location              | Count | Purpose                               |
| ----------------- | --------------------- | ----- | ------------------------------------- |
| Unit tests        | `src/*.rs` (embedded) | ~270  | Test individual functions and modules |
| Integration tests | `tests/*.rs`          | ~200  | Test module interactions              |
| E2E tests         | `tests/*_e2e_test.py` | 9     | Full wire protocol testing            |

**Current test files:**

- Unit: Embedded in `src/array.rs`, `src/catalog/mod.rs`, `src/distinct_on.rs`, `src/fts.rs`, `src/geo.rs`, `src/range.rs`, `src/rls.rs`, `src/rls_inject.rs`, `src/schema.rs`, `src/transpiler/mod.rs`, `src/vector.rs`
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

| Test Type         | Discovery Pattern                             | Location   |
| ----------------- | --------------------------------------------- | ---------- |
| Unit tests        | `#[test]` functions in `#[cfg(test)]` modules | `src/*.rs` |
| Integration tests | `tests/*.rs` files                            | `tests/`   |
| E2E tests         | `tests/*_e2e_test.py` files                   | `tests/`   |

**No registry updates needed** - just create files following the naming conventions and they'll be picked up automatically.

## Critical Implementation Details

### Array vs Range Operator Detection

The transpiler must distinguish between array operators (`&&`, `@>`, `<@`) and range operators. This is handled in `src/transpiler/expr.rs` in the `reconstruct_a_expr` function.

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

| Module     | File(s)                                    | Description                                    |
| ---------- | ------------------------------------------ | ---------------------------------------------- |
| Array      | `src/array.rs`                             | PostgreSQL array functions and operators       |
| Array Agg  | `src/array_agg.rs`                         | array_agg aggregate function                   |
| Range      | `src/range.rs`                             | PostgreSQL range types and operators           |
| Regex      | `src/regex_funcs.rs`                       | Regular expression functions                   |
| Vector     | `src/vector.rs`                            | pgvector-compatible vector operations          |
| FTS        | `src/fts.rs`                               | Full-text search (to_tsvector, to_tsquery)     |
| RLS        | `src/rls.rs`, `src/rls_inject.rs`          | Row-Level Security                             |
| Schema     | `src/schema.rs`                            | Schema management (CREATE SCHEMA, search_path) |
| Geo        | `src/geo.rs`                               | Geometric types (point, box, circle)           |
| Functions  | `src/functions.rs`                         | User-defined function execution                |
| Copy       | `src/copy.rs`                              | COPY FROM/TO command support                   |
| Distinct   | `src/distinct_on.rs`                       | DISTINCT ON polyfill via ROW_NUMBER()          |
| Catalog    | `src/catalog/` (mod, init, table, fn, rls, system_views) | Metadata storage in SQLite    |
| Transpiler | `src/transpiler/` (mod, context, ddl, dml, expr, func, rls_aug, utils, window) | SQL parsing and transformation |
| Handler    | `src/handler/mod.rs`                       | PostgreSQL wire protocol (SqliteHandler)       |

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
│   ├── main.rs                # CLI, server setup, main() entry point
│   ├── handler/               # PostgreSQL wire protocol handler (SqliteHandler)
│   │   └── mod.rs             # SqliteHandler impl: query exec, schema, functions, RLS, COPY
│   ├── lib.rs                 # Library exports
│   ├── transpiler/            # SQL transpilation (PostgreSQL → SQLite)
│   │   ├── mod.rs             # Public API: transpile(), TranspileResult, main dispatch
│   │   ├── context.rs         # TranspileContext and result types
│   │   ├── ddl.rs             # CREATE TABLE, ALTER, DROP, TRUNCATE, INDEX, COPY
│   │   ├── dml.rs             # SELECT, INSERT, UPDATE, DELETE
│   │   ├── expr.rs            # Expression/node reconstruction
│   │   ├── func.rs            # FuncCall reconstruction + CREATE FUNCTION parsing
│   │   ├── rls_aug.rs         # Roles, grants, policies, transpile_with_rls()
│   │   ├── utils.rs           # Type rewriting (rewrite_type_for_sqlite)
│   │   └── window.rs          # Window function support
│   ├── catalog/               # Shadow catalog management
│   │   ├── mod.rs             # Public API and shared types
│   │   ├── init.rs            # Catalog init and pg_types
│   │   ├── table.rs           # Table/column metadata
│   │   ├── function.rs        # UDF metadata
│   │   ├── rls.rs             # RLS policy storage
│   │   └── system_views.rs    # pg_catalog view init
│   ├── array.rs               # Array support
│   ├── copy.rs                # COPY FROM/TO support
│   ├── distinct_on.rs         # DISTINCT ON polyfill
│   ├── fts.rs                 # Full-text search
│   ├── functions.rs           # UDF execution
│   ├── geo.rs                 # Geometric types
│   ├── plpgsql/               # PL/pgSQL parser and Lua transpiler
│   │   ├── mod.rs             # Public API
│   │   ├── ast.rs             # AST type definitions
│   │   ├── parser.rs          # Parser
│   │   ├── runtime.rs         # Lua execution runtime
│   │   ├── sqlstate.rs        # SQLSTATE error codes
│   │   └── transpiler.rs      # PL/pgSQL → Lua
│   ├── range.rs               # Range types
│   ├── rls.rs                 # Row-Level Security
│   ├── rls_inject.rs          # RLS AST injection
│   ├── schema.rs              # Schema management
│   └── vector.rs              # Vector operations
├── tests/
│   ├── *_tests.rs             # Rust integration tests
│   ├── *_e2e_test.py          # Python e2e tests
│   └── run_all_e2e.py         # Unified e2e runner
├── examples/                  # Example usage code
├── docs/                      # Documentation
├── run_tests.sh               # Test runner script
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

## Build Configuration

PGQT supports feature flags for conditional compilation:

### Available Features

| Feature | Description | Default | Binary Impact |
|---------|-------------|---------|---------------|
| `tls` | TLS/SSL support via rustls | ✓ | +2.5MB |
| `plpgsql` | PL/pgSQL stored procedure support | ✓ | ~500KB |

### Build Commands

```bash
# Default build (with TLS and PL/pgSQL)
cargo build --release

# Smaller build without TLS
cargo build --release --no-default-features --features plpgsql

# Minimal build (no optional features)
cargo build --release --no-default-features
```

### Build Scripts

Three convenience scripts are provided:

```bash
./build-release.sh        # Full build with TLS (~12MB)
./build-release-small.sh  # Without TLS (~9.5MB)
./build-both.sh           # Build both variants
```

### Conditional Compilation

When adding TLS-dependent code, use:

```rust
#[cfg(feature = "tls")]
use crate::tls::TlsConfig;

#[cfg(feature = "tls")]
fn tls_function() { /* ... */ }

#[cfg(not(feature = "tls"))]
fn tls_function() { /* fallback */ }
```

See `src/main.rs` for examples of TLS feature gating.

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
