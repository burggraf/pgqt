# PGlite Proxy API Reference

## Connection Parameters

### Standard PostgreSQL Connection String

```
postgresql://[user[:password]@][host][:port][/dbname][?param1=value1&...]
```

### Supported Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `host` | `127.0.0.1` | Proxy server hostname |
| `port` | `5432` | Proxy server port |
| `user` | `postgres` | Username (currently ignored) |
| `password` | (none) | Password (currently ignored) |
| `dbname` | `test.db` | SQLite database file path |
| `sslmode` | `disable` | SSL mode (disable, prefer, require) |

### Examples

```bash
# Basic connection
psql "postgresql://postgres@127.0.0.1:5432/myapp.db"

# With all parameters
psql "postgresql://user:pass@localhost:5433/production.db?sslmode=disable"
```

## SQL Compatibility

### Data Definition Language (DDL)

#### CREATE TABLE

```sql
-- Basic table creation
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(100),
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- With constraints
CREATE TABLE orders (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    total NUMERIC(10, 2) CHECK (total >= 0),
    status VARCHAR(20) DEFAULT 'pending'
);
```

**Supported:**
- ✅ Column types (with transpilation)
- ✅ PRIMARY KEY, UNIQUE, NOT NULL
- ✅ DEFAULT values
- ✅ CHECK constraints
- ✅ Foreign key constraints (SQLite enforced)
- ⚠️ EXCLUDE constraints (not supported)

#### ALTER TABLE

```sql
-- Add column
ALTER TABLE users ADD COLUMN phone VARCHAR(20);

-- Drop column
ALTER TABLE users DROP COLUMN phone;

-- Rename column
ALTER TABLE users RENAME COLUMN name TO full_name;
```

**Supported:**
- ✅ ADD COLUMN
- ✅ DROP COLUMN
- ✅ RENAME COLUMN
- ✅ ADD CONSTRAINT
- ✅ DROP CONSTRAINT

#### DROP TABLE

```sql
DROP TABLE users;
DROP TABLE IF EXISTS users;
DROP TABLE users CASCADE; -- CASCADE is ignored (SQLite behavior)
```

### Data Manipulation Language (DML)

#### SELECT

```sql
-- Basic query
SELECT * FROM users;

-- With conditions
SELECT id, email FROM users WHERE active = true;

-- With joins
SELECT u.name, o.total 
FROM users u 
JOIN orders o ON u.id = o.user_id;

-- With aggregation
SELECT status, COUNT(*) as count, AVG(total) as avg_total
FROM orders
GROUP BY status
HAVING COUNT(*) > 5;

-- With subqueries
SELECT * FROM users 
WHERE id IN (SELECT user_id FROM orders WHERE total > 100);

-- With LIMIT/OFFSET
SELECT * FROM users LIMIT 10 OFFSET 20;

-- DISTINCT
SELECT DISTINCT status FROM orders;
```

**Supported:**
- ✅ All standard SELECT features
- ✅ JOINs (INNER, LEFT, RIGHT, FULL)
- ✅ Subqueries (correlated and non-correlated)
- ✅ Aggregations (COUNT, SUM, AVG, MIN, MAX)
- ✅ GROUP BY, HAVING
- ✅ ORDER BY
- ✅ LIMIT, OFFSET
- ✅ DISTINCT
- ⚠️ DISTINCT ON (requires window function polyfill)
- ⚠️ Window functions (limited support)

#### INSERT

```sql
-- Single row
INSERT INTO users (email, name) VALUES ('alice@example.com', 'Alice');

-- Multiple rows
INSERT INTO users (email, name) VALUES 
    ('bob@example.com', 'Bob'),
    ('carol@example.com', 'Carol');

-- With RETURNING
INSERT INTO users (email, name) 
VALUES ('dave@example.com', 'Dave')
RETURNING id, created_at;

-- From SELECT
INSERT INTO users_archive 
SELECT * FROM users WHERE active = false;
```

**Supported:**
- ✅ Single row insert
- ✅ Multi-row insert
- ✅ INSERT ... SELECT
- ✅ RETURNING clause (SQLite 3.35.0+)

#### UPDATE

```sql
-- Basic update
UPDATE users SET name = 'Alice Smith' WHERE id = 1;

-- Multiple columns
UPDATE users SET 
    name = 'Bob Jones',
    active = false
WHERE id = 2;

-- With RETURNING
UPDATE users SET active = true 
WHERE id = 1
RETURNING *;
```

**Supported:**
- ✅ Basic UPDATE
- ✅ UPDATE ... FROM
- ✅ RETURNING clause

#### DELETE

```sql
-- Basic delete
DELETE FROM users WHERE id = 1;

-- With RETURNING
DELETE FROM users 
WHERE active = false 
RETURNING id, email;

-- Delete with join (SQLite syntax)
DELETE FROM users 
WHERE id IN (
    SELECT user_id FROM orders 
    WHERE created_at < date('now', '-1 year')
);
```

**Supported:**
- ✅ Basic DELETE
- ✅ DELETE ... RETURNING
- ✅ DELETE with subqueries

### Functions and Operators

#### Type Casts

```sql
-- PostgreSQL :: syntax
SELECT '123'::int;
SELECT 123::text;
SELECT '2024-01-01'::date;

-- Standard CAST syntax (also supported)
SELECT CAST('123' AS INTEGER);
```

**Transpilation:**
- `::int` → `CAST(x AS INTEGER)`
- `::text` → `CAST(x AS TEXT)`
- `::float` → `CAST(x AS REAL)`
- `::bool` → `CAST(x AS INTEGER)`

#### String Functions

| PostgreSQL | SQLite Equivalent | Notes |
|-----------|-------------------|-------|
| `concat(a, b)` | `a || b` | String concatenation |
| `lower(s)` | `lower(s)` | Direct mapping |
| `upper(s)` | `upper(s)` | Direct mapping |
| `length(s)` | `length(s)` | Direct mapping |
| `substring(s, start, len)` | `substr(s, start, len)` | 1-indexed in PG, same in SQLite |
| `trim(s)` | `trim(s)` | Direct mapping |
| `replace(s, from, to)` | `replace(s, from, to)` | Direct mapping |
| `position(sub in s)` | `instr(s, sub)` | Returns 0 if not found (same as PG) |

#### Date/Time Functions

| PostgreSQL | SQLite Equivalent | Notes |
|-----------|-------------------|-------|
| `now()` | `datetime('now')` | Current timestamp |
| `current_timestamp` | `datetime('now')` | Same as now() |
| `current_date` | `date('now')` | Current date |
| `current_time` | `time('now')` | Current time |
| `date_trunc('day', ts)` | `date(ts)` | Truncate to day |
| `extract(epoch from ts)` | `unixepoch(ts)` | Unix timestamp |
| `extract(year from ts)` | `strftime('%Y', ts)` | Extract year |
| `age(ts)` | No direct equivalent | Calculate interval |

#### Mathematical Functions

| PostgreSQL | SQLite | Notes |
|-----------|--------|-------|
| `abs(x)` | `abs(x)` | Direct mapping |
| `round(x)` | `round(x)` | Direct mapping |
| `ceil(x)` | `ceil(x)` | Requires SQLite 3.44+ |
| `floor(x)` | `floor(x)` | Requires SQLite 3.44+ |
| `power(x, y)` | `pow(x, y)` | Requires extension |
| `sqrt(x)` | `sqrt(x)` | Direct mapping |
| `random()` | `random()` | Different ranges |

#### Pattern Matching

```sql
-- LIKE operator
SELECT * FROM users WHERE name LIKE 'Alice%';

-- ILIKE (case-insensitive, transpiled to LIKE)
SELECT * FROM users WHERE name ILIKE 'alice%';

-- Regex (PostgreSQL ~ operator)
SELECT * FROM users WHERE email ~ '^[a-z]+@example\.com$';
-- Transpiled to: email REGEXP '^[a-z]+@example\.com$'
```

**Transpilation:**
- `~~` → `LIKE`
- `~~*` → `LIKE` (SQLite LIKE is case-insensitive for ASCII)
- `!~~` → `NOT LIKE`
- `~` → `REGEXP` (requires regex extension)

### Transactions

```sql
-- Basic transaction
BEGIN;
INSERT INTO users (email) VALUES ('test@example.com');
UPDATE orders SET status = 'processing' WHERE user_id = 1;
COMMIT;

-- Savepoints
BEGIN;
INSERT INTO users (email) VALUES ('a@example.com');
SAVEPOINT before_order;
INSERT INTO orders (user_id, total) VALUES (1, 100);
-- Oops, something went wrong
ROLLBACK TO SAVEPOINT before_order;
COMMIT;
```

**Supported:**
- ✅ BEGIN, COMMIT, ROLLBACK
- ✅ SAVEPOINT, RELEASE, ROLLBACK TO
- ⚠️ Isolation levels (ignored, SQLite has single isolation level)

## System Tables and Views

### Shadow Catalog (`__pg_meta__`)

The proxy maintains a special table for PostgreSQL metadata:

```sql
-- View all stored metadata
SELECT * FROM __pg_meta__;

-- View metadata for specific table
SELECT column_name, original_type, constraints
FROM __pg_meta__
WHERE table_name = 'users';

-- Check if type was preserved correctly
SELECT column_name, original_type
FROM __pg_meta__
WHERE table_name = 'orders'
  AND original_type LIKE 'NUMERIC%';
```

### SQLite System Tables

Standard SQLite system tables are accessible:

```sql
-- List all tables
SELECT name FROM sqlite_master WHERE type = 'table';

-- Table schema
PRAGMA table_info(users);

-- Index info
PRAGMA index_info(users_email_idx);
```

## Error Codes

The proxy maps SQLite errors to PostgreSQL error codes:

| SQLite Error | PostgreSQL Code | Description |
|-------------|-----------------|-------------|
| `SQLITE_CONSTRAINT_UNIQUE` | `23505` | unique_violation |
| `SQLITE_CONSTRAINT_NOTNULL` | `23502` | not_null_violation |
| `SQLITE_CONSTRAINT_FOREIGNKEY` | `23503` | foreign_key_violation |
| `SQLITE_CONSTRAINT_CHECK` | `23514` | check_violation |
| `SQLITE_READONLY` | `25006` | read_only_sql_transaction |
| `SQLITE_BUSY` | `40001` | serialization_failure |

## Performance Hints

### Index Usage

```sql
-- Create index
CREATE INDEX idx_users_email ON users(email);

-- Covering index (includes additional columns)
CREATE INDEX idx_users_active_name ON users(active) INCLUDE (name);

-- Partial index (SQLite supports WHERE clause)
CREATE INDEX idx_orders_pending ON orders(total) WHERE status = 'pending';
```

### Query Optimization

```sql
-- Use EXPLAIN QUERY PLAN
EXPLAIN QUERY PLAN SELECT * FROM users WHERE email = 'test@example.com';

-- Check if index is used
-- Look for "USING INDEX" in output
```

## Limitations and Workarounds

### Not Supported

| Feature | Workaround |
|---------|-----------|
| `RETURNING` on older SQLite | Upgrade to SQLite 3.35.0+ |
| `DISTINCT ON` | Use `GROUP BY` or window functions |
| `FULL OUTER JOIN` | Use `UNION` of `LEFT` and `RIGHT` joins |
| `LATERAL` joins | Use correlated subqueries |
| `ARRAY` types | Store as JSON text |
| `JSONB` operators | Use `json_extract()` function |

### PostgreSQL-Specific Features

| Feature | Status | Notes |
|---------|--------|-------|
| Stored Procedures | ❌ Not supported | Planned for Phase 3 (Lua runtime) |
| Triggers | ⚠️ Limited | SQLite triggers only |
| Rules | ❌ Not supported | Use views instead |
| Listen/Notify | ❌ Not supported | Use polling or external message queue |
| Advisory Locks | ❌ Not supported | Use application-level locking |

## Client Library Examples

### Python (psycopg2)

```python
import psycopg2

conn = psycopg2.connect(
    host="127.0.0.1",
    port=5432,
    user="postgres",
    database="myapp.db"
)

cur = conn.cursor()
cur.execute("SELECT * FROM users WHERE email = %s", ("alice@example.com",))
rows = cur.fetchall()
```

### Node.js (pg)

```javascript
const { Client } = require('pg');

const client = new Client({
  host: '127.0.0.1',
  port: 5432,
  user: 'postgres',
  database: 'myapp.db'
});

await client.connect();
const result = await client.query('SELECT * FROM users WHERE id = $1', [1]);
console.log(result.rows);
```

### Rust (tokio-postgres)

```rust
use tokio_postgres::{Client, NoTls};

let (client, connection) = tokio_postgres::connect(
    "host=127.0.0.1 port=5432 user=postgres dbname=myapp.db",
    NoTls,
).await?;

let rows = client
    .query("SELECT * FROM users WHERE email = $1", [&"alice@example.com"
    ])
    .await?;
```

### Go (pgx)

```go
package main

import (
    "context"
    "github.com/jackc/pgx/v5"
)

func main() {
    conn, err := pgx.Connect(context.Background(), 
        "postgres://postgres@127.0.0.1:5432/myapp.db")
    if err != nil {
        panic(err)
    }
    defer conn.Close(context.Background())

    var name string
    err = conn.QueryRow(context.Background(),
        "SELECT name FROM users WHERE id=$1", 1).Scan(&name)
}
```
