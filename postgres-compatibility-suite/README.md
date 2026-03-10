# PGQT PostgreSQL Compatibility Test Suite

This directory contains a comprehensive compatibility test suite that compares PGQT's behavior against a reference PostgreSQL instance.

## Quick Start

From the project root, run:

```bash
./run_compatibility_suite.sh
```

## Prerequisites

1. **PostgreSQL** - A running PostgreSQL instance for ground-truth comparison
   - Docker: `docker run --name pg-test -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres`
   - Or use an existing PostgreSQL instance

2. **Python 3** with `psycopg2`:
   ```bash
   pip install psycopg2-binary
   ```

3. **Rust/Cargo** - To build PGQT

## Test Runners

### Enhanced Runner (Recommended)

`runner_with_stats.py` - Provides detailed statement-level statistics:

```bash
# Run with detailed statistics
python3 runner_with_stats.py

# Verbose mode (shows each statement)
python3 runner_with_stats.py --verbose

# Fail fast (stop on first failure)
python3 runner_with_stats.py --fail-fast

# Both options
python3 runner_with_stats.py -v -f
```

### Shell Script Wrapper

`../run_compatibility_suite.sh` - Convenient wrapper with colored output:

```bash
# Run full suite
./run_compatibility_suite.sh

# Verbose output
./run_compatibility_suite.sh -v

# Summary only (no per-file progress)
./run_compatibility_suite.sh -s

# JSON output
./run_compatibility_suite.sh -j
```

### Legacy Pytest Runner

`runner.py` - Original pytest-based runner (stops at first failure per file):

```bash
pytest runner.py
pytest runner.py -v                    # Verbose
pytest runner.py -x                    # Fail fast
pytest runner.py -k "boolean"          # Run specific test
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PG_DSN` | `host=localhost port=5432 user=postgres password=postgres dbname=postgres` | PostgreSQL connection string |
| `PROXY_PORT` | `5435` | Port for PGQT proxy |

### Examples

```bash
# Use remote PostgreSQL
export PG_DSN="host=192.168.1.100 port=5432 user=pg password=secret dbname=test"
./run_compatibility_suite.sh

# Use different proxy port
export PROXY_PORT=5436
./run_compatibility_suite.sh
```

## Test Files

### SQL Test Files (`sql/`)

| Directory | Description |
|-----------|-------------|
| `sql/pg_regress/` | PostgreSQL regression tests (ported from PostgreSQL source) |
| `sql/pgqt_specific/` | PGQT-specific feature tests |
| `sql/sqltest/` | SLT (SQLite Test) format tests (placeholders) |

### Current Test Coverage

- **37 active test files** (13 placeholder files skipped)
- **~10,000+ SQL statements** tested
- Covers: DDL, DML, SELECT, JOINs, aggregates, types, functions, etc.

## Understanding Results

### Statement-Level Pass Rate

The enhanced runner reports actual statement-level compatibility:

```
OVERALL STATISTICS
----------------------------------------
  Total SQL Files:        37
  Total Statements:       10217
  Passed Statements:      5756
  Failed Statements:      4461
  Skipped Statements:     59

  OVERALL PASS RATE:      56.34%
```

### Error Categories

Failures are categorized for easier debugging:

| Category | Description |
|----------|-------------|
| `Syntax Error` | SQL parsing or transpilation errors |
| `Missing Function` | PostgreSQL function not implemented |
| `Missing Table/View` | System catalog or table not found |
| `Column Mismatch` | Column names/types don't match |
| `Row Count Mismatch` | Different number of rows returned |
| `Error Handling Gap` | PG should error but doesn't, or vice versa |

### Per-File Results

```
  [PASS] pg_regress/boolean.sql: 98/98 passed (100%)
  [~]    pg_regress/arrays.sql: 289/527 passed (54.8%)
  [~]    pg_regress/varchar.sql: 3/22 passed (13.6%)
```

Symbols:
- `[PASS]` - 100% pass rate
- `[~]` - Partial pass (1-99%)
- `[FAIL]` - 0% pass rate

## Output Files

| File | Description |
|------|-------------|
| `test_results.json` | Detailed JSON results from last run |
| `test_run.log` | Full test output log |
| `test_db.db.error.log` | PGQT error log during tests |

## Interpreting Low Pass Rates

A low pass rate doesn't necessarily mean PGQT is broken. Many failures are due to:

1. **PostgreSQL-specific features** not meant to be supported (e.g., `COPY`, `VACUUM`)
2. **System catalog queries** (PGQT has limited `pg_catalog` views)
3. **Error semantics** (PGQT may accept what PostgreSQL rejects, or vice versa)
4. **Type differences** (SQLite's dynamic typing vs PostgreSQL's strict typing)

## Adding New Tests

1. Create `.sql` file in appropriate `sql/` subdirectory
2. Write SQL statements separated by semicolons
3. Run the test suite to see results
4. The runner will automatically pick up new files

## Troubleshooting

### "Reference PostgreSQL not available"

Start PostgreSQL:
```bash
docker run --name pg-test -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres
```

### "pg_isready not found"

Install PostgreSQL client tools or set `PG_DSN` manually.

### Proxy connection errors

Check if port 5435 is available:
```bash
lsof -i :5435  # macOS
netstat -tlnp | grep 5435  # Linux
```

### Build errors

Ensure Rust is installed:
```bash
cargo build --release
```

## CI/CD Integration

Example GitHub Actions step:

```yaml
- name: Run Compatibility Tests
  run: |
    ./run_compatibility_suite.sh -s
  env:
    PG_DSN: "host=localhost port=5432 user=postgres password=postgres dbname=postgres"
```

## See Also

- `pgqt-test-suite-plan.md` - Original test suite design document
- `../AGENTS.md` - Project guide for AI agents
- `../README.md` - Main project documentation
