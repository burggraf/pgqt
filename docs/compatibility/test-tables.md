# PostgreSQL Regression Test Tables

PGQT's compatibility test suite includes the standard PostgreSQL regression test tables to ensure compatibility with PostgreSQL's test suite.

## Tables

### tenk1
10,000 row test table with various columns for testing queries.

| Column | Type | Description |
|--------|------|-------------|
| unique1 | int | Unique values 0-9999 |
| unique2 | int | Permuted unique values |
| two | int | unique1 % 2 |
| four | int | unique1 % 4 |
| ten | int | unique1 % 10 |
| twenty | int | unique1 % 20 |
| hundred | int | unique1 % 100 |
| thousand | int | unique1 % 1000 |
| twothousand | int | unique1 % 2000 |
| fivethous | int | unique1 % 5000 |
| tenthous | int | unique1 % 10000 |
| odd | int | 1 or 3 |
| even | int | 0 or 2 |
| stringu1 | varchar | Formatted string from unique1 |
| stringu2 | varchar | Formatted string from unique2 |
| string4 | varchar | Formatted four column |

### onek
1,000 row test table with same schema as tenk1.

### onek2
Another 1,000 row test table with same schema.

### int8_tbl
64-bit integer test table.

| Column | Type | Description |
|--------|------|-------------|
| q1 | bigint | First 64-bit integer |
| q2 | bigint | Second 64-bit integer |

### int4_tbl
32-bit integer test table.

| Column | Type | Description |
|--------|------|-------------|
| f1 | integer | 32-bit integer value |

### int2_tbl
16-bit integer test table.

| Column | Type | Description |
|--------|------|-------------|
| f1 | smallint | 16-bit integer value |

### arrtest
Array test table.

| Column | Type | Description |
|--------|------|-------------|
| a | text | 1D array (stored as JSON) |
| b | text | 2D array (stored as JSON) |

### Other Tables
- `aggtest` - Aggregate function testing
- `testjsonb` - JSONB testing
- `jsonb_populate_record` - JSONB record testing
- `json_populate_record` - JSON record testing
- `varchar_tbl` - VARCHAR testing
- `point_tbl` - Geometric point testing

## Data Generation

Test data is generated deterministically to match the patterns expected by PostgreSQL's regression tests. The data generation scripts are in `postgres-compatibility-suite/scripts/`.

## Usage

Test tables are automatically created when running the compatibility suite:

```bash
cd postgres-compatibility-suite
python3 runner_with_stats.py
```

To manually populate test data:

```bash
cd postgres-compatibility-suite
python3 scripts/populate_test_data.py
```
