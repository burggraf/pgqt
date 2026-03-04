# PGQT PostgreSQL Compatibility Test Suite

This is a comprehensive test suite for validating PostgreSQL compatibility of the PGQT proxy. It is designed to find bugs, identify missing features, and provide a clear "Compatibility Score."

## Categories (PG Scorecard)

1.  **Data Types**: Tests for INT, TEXT, UUID, JSON, JSONB, Arrays, etc.
2.  **DDL Features**: CREATE TABLE, ALTER TABLE, DROP TABLE, etc.
3.  **SQL Features**: CTEs, Upsert, Window Functions, etc.
4.  **Procedural**: PL/pgSQL-like logic.
5.  **Constraints**: Primary Keys, Foreign Keys, etc.
6.  **Extensions**: pgvector, PostGIS style geo-types.
7.  **Security**: Row-Level Security (RLS).
8.  **Transaction**: Savepoints, Isolation Levels.
9.  **Indexing**: B-Tree (mapped to SQLite indexes).

## Prerequisites

- **Python 3.10+**
- **psycopg2** (`pip install psycopg2-binary`)
- **pytest** (`pip install pytest`)
- (Optional) **PostgreSQL** running locally to serve as "Ground Truth" (Reference).

## Running the tests

### 1. Start the proxy and run tests

The test harness automatically starts the `pgqt` proxy using `cargo build --release`.

```bash
# Set your reference Postgres connection string
export PG_DSN="host=localhost port=5432 user=postgres password=postgres dbname=postgres"

# Run all tests
pytest postgres-compatibility-suite/runner.py

# Run only a specific category
pytest postgres-compatibility-suite/runner.py -k "pg_regress"
pytest postgres-compatibility-suite/runner.py -k "pgqt_specific"
```

### 2. Generate a Report

The runner will collect pass/fail data and generate a `compatibility_report.md`.

## Folder Structure

- `sql/pg_regress/`: Official PostgreSQL regression SQL scripts.
- `sql/sqltest/`: SQL-92 templates from `elliotchance/sqltest`.
- `sql/pgqt_specific/`: Custom tests for transpilation features (Arrays, Vectors, Ranges).
- `expected/pg_regress/`: Reference `.out` files from PostgreSQL.
- `scripts/`: Downloaders and maintainer utilities.

## How it works

1.  **Dual Execution**: Each SQL statement is executed against both the PGQT proxy and a real PostgreSQL instance.
2.  **Result Comparison**: The harness compares the result sets (column names, row values, and counts).
3.  **Diff Reporting**: Differences in output or errors are logged as failures.
