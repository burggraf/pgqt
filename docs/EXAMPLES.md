# PGlite Proxy Examples

## Table of Contents

1. [Getting Started](#getting-started)
2. [Web Application Backend](#web-application-backend)
3. [Data Analysis](#data-analysis)
4. [Testing and CI/CD](#testing-and-cicd)
5. [Migration Scenarios](#migration-scenarios)
6. [Advanced Patterns](#advanced-patterns)

---

## Getting Started

### Example 1: Basic Setup

```bash
# Terminal 1: Start the proxy
$ cargo run --release
Server listening on 127.0.0.1:5432

# Terminal 2: Connect and explore
$ psql -h 127.0.0.1 -p 5432 -U postgres

postgres=# CREATE TABLE todos (
    id SERIAL PRIMARY KEY,
    title VARCHAR(200) NOT NULL,
    completed BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT now()
);
CREATE TABLE

postgres=# INSERT INTO todos (title) VALUES 
    ('Learn Rust'),
    ('Build a proxy'),
    ('Write documentation');
INSERT 0 3

postgres=# SELECT * FROM todos;
 id |        title        | completed |         created_at         
----+---------------------+-----------+----------------------------
  1 | Learn Rust          | f         | 2024-01-15 10:30:00.000000
  2 | Build a proxy       | f         | 2024-01-15 10:30:00.000000
  3 | Write documentation | f         | 2024-01-15 10:30:00.000000
(3 rows)

postgres=# UPDATE todos SET completed = true WHERE id = 1;
UPDATE 1

postgres=# SELECT * FROM todos WHERE completed = true;
 id |    title     | completed |         created_at         
----+--------------+-----------+----------------------------
  1 | Learn Rust   | t         | 2024-01-15 10:30:00.000000
(1 row)
```

### Example 2: Type Preservation Verification

```sql
-- Check what SQLite actually stores
postgres=# .mode line
postgres=# PRAGMA table_info(todos);
        cid = 0
       name = id
       type = INTEGER
    notnull = 0
 dflt_value = NULL
         pk = 1

        cid = 1
       name = title
       type = TEXT
    notnull = 1
 dflt_value = NULL
         pk = 0

-- Check the shadow catalog for original types
postgres=# SELECT column_name, original_type 
FROM __pg_meta__ 
WHERE table_name = 'todos';
 column_name |        original_type        
-------------+---------------------------
 id          | SERIAL
 title       | VARCHAR(200)
 completed   | BOOLEAN
 created_at  | TIMESTAMP WITH TIME ZONE
(4 rows)
```

---

## Web Application Backend

### Example 3: Express.js + TypeORM Application

```typescript
// src/data-source.ts
import { DataSource } from "typeorm";
import { User } from "./entity/User";
import { Post } from "./entity/Post";

export const AppDataSource = new DataSource({
  type: "postgres",
  host: "127.0.0.1",
  port: 5432,
  username: "postgres",
  password: "",  // No auth in development
  database: "blog.db",
  synchronize: true,  // Auto-create tables
  logging: true,
  entities: [User, Post],
});

// src/entity/User.ts
import { Entity, PrimaryGeneratedColumn, Column, OneToMany } from "typeorm";
import { Post } from "./Post";

@Entity()
export class User {
  @PrimaryGeneratedColumn()
  id: number;

  @Column({ type: "varchar", length: 255, unique: true })
  email: string;

  @Column({ type: "varchar", length: 100 })
  name: string;

  @Column({ type: "boolean", default: true })
  isActive: boolean;

  @Column({ type: "timestamp with time zone", default: () => "now()" })
  createdAt: Date;

  @OneToMany(() => Post, post => post.author)
  posts: Post[];
}

// src/entity/Post.ts
import { Entity, PrimaryGeneratedColumn, Column, ManyToOne, JoinColumn } from "typeorm";
import { User } from "./User";

@Entity()
export class Post {
  @PrimaryGeneratedColumn()
  id: number;

  @Column({ type: "varchar", length: 200 })
  title: string;

  @Column({ type: "text" })
  content: string;

  @Column({ type: "jsonb", nullable: true })
  metadata: object;

  @Column({ type: "timestamp with time zone", default: () => "now()" })
  publishedAt: Date;

  @ManyToOne(() => User, user => user.posts)
  @JoinColumn({ name: "author_id" })
  author: User;

  @Column()
  authorId: number;
}

// src/server.ts
import express from "express";
import { AppDataSource } from "./data-source";
import { User } from "./entity/User";
import { Post } from "./entity/Post";

const app = express();
app.use(express.json());

// Initialize database
AppDataSource.initialize()
  .then(() => console.log("Database connected"))
  .catch((err) => console.error("Database connection failed:", err));

// Routes
app.post("/users", async (req, res) => {
  const userRepo = AppDataSource.getRepository(User);
  const user = userRepo.create(req.body);
  const result = await userRepo.save(user);
  res.json(result);
});

app.get("/users/:id/posts", async (req, res) => {
  const postRepo = AppDataSource.getRepository(Post);
  const posts = await postRepo.find({
    where: { authorId: parseInt(req.params.id) },
    relations: ["author"],
  });
  res.json(posts);
});

app.listen(3000, () => console.log("Server running on port 3000"));
```

**Usage:**
```bash
# Terminal 1: Start proxy
$ PGQT_DB=blog.db ./pglite-proxy

# Terminal 2: Start app
$ npm run dev

# Terminal 3: Test API
$ curl -X POST http://localhost:3000/users \
  -H "Content-Type: application/json" \
  -d '{"email": "alice@example.com", "name": "Alice"}'

$ curl http://localhost:3000/users/1/posts
```

---

## Data Analysis

### Example 4: Jupyter Notebook with Python

```python
# analysis.ipynb
import pandas as pd
import psycopg2
import matplotlib.pyplot as plt

# Connect to proxy
conn = psycopg2.connect(
    host="127.0.0.1",
    port=5432,
    user="postgres",
    database="sales.db"
)

# Load data
query = """
SELECT 
    date_trunc('month', order_date) as month,
    region,
    SUM(amount) as total_sales,
    COUNT(*) as order_count
FROM orders
WHERE order_date >= '2024-01-01'
GROUP BY 1, 2
ORDER BY 1, 2
"""

df = pd.read_sql(query, conn)

# Pivot for analysis
pivot = df.pivot(index='month', columns='region', values='total_sales')

# Visualize
pivot.plot(kind='line', figsize=(12, 6))
plt.title('Monthly Sales by Region')
plt.ylabel('Sales ($)')
plt.show()

# Statistical analysis
print(df.groupby('region').agg({
    'total_sales': ['mean', 'std', 'min', 'max'],
    'order_count': 'sum'
}))
```

### Example 5: Complex Analytics Query

```sql
-- Sales performance with window functions
WITH monthly_sales AS (
    SELECT 
        date_trunc('month', order_date) as month,
        salesperson_id,
        SUM(amount) as monthly_total,
        COUNT(*) as order_count
    FROM orders
    WHERE order_date >= date('now', '-12 months')
    GROUP BY 1, 2
),
ranked_sales AS (
    SELECT 
        month,
        salesperson_id,
        monthly_total,
        order_count,
        RANK() OVER (PARTITION BY month ORDER BY monthly_total DESC) as rank,
        LAG(monthly_total) OVER (
            PARTITION BY salesperson_id 
            ORDER BY month
        ) as prev_month_total
    FROM monthly_sales
)
SELECT 
    month,
    salesperson_id,
    monthly_total,
    order_count,
    rank,
    CASE 
        WHEN prev_month_total IS NULL THEN 'New'
        WHEN monthly_total > prev_month_total THEN 'Growing'
        WHEN monthly_total < prev_month_total THEN 'Declining'
        ELSE 'Stable'
    END as trend
FROM ranked_sales
WHERE rank <= 10
ORDER BY month DESC, rank;
```

---

## Testing and CI/CD

### Example 6: pytest with Database Fixtures

```python
# conftest.py
import pytest
import psycopg2
import subprocess
import time

@pytest.fixture(scope="session")
def pglite_proxy():
    """Start PGlite Proxy for testing"""
    proc = subprocess.Popen(
        ["./pglite-proxy"],
        env={"PGQT_DB": ":memory:", "PGQT_PORT": "15432"},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    time.sleep(1)  # Wait for startup
    yield proc
    proc.terminate()

@pytest.fixture
def db_conn(pglite_proxy):
    """Create a fresh database connection"""
    conn = psycopg2.connect(
        host="127.0.0.1",
        port=15432,
        user="postgres",
        database="test"
    )
    # Clean slate for each test
    with conn.cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS test_items")
        cur.execute("""
            CREATE TABLE test_items (
                id SERIAL PRIMARY KEY,
                name VARCHAR(100),
                value INTEGER
            )
        """)
    conn.commit()
    yield conn
    conn.close()

# test_api.py
def test_insert_and_query(db_conn):
    with db_conn.cursor() as cur:
        cur.execute(
            "INSERT INTO test_items (name, value) VALUES (%s, %s) RETURNING id",
            ("test", 42)
        )
        item_id = cur.fetchone()[0]
        db_conn.commit()
        
        cur.execute("SELECT * FROM test_items WHERE id = %s", (item_id,))
        row = cur.fetchone()
        
        assert row[1] == "test"
        assert row[2] == 42

def test_transaction_rollback(db_conn):
    with db_conn.cursor() as cur:
        cur.execute("INSERT INTO test_items (name) VALUES ('before')")
        db_conn.commit()
        
        cur.execute("INSERT INTO test_items (name) VALUES ('during')")
        db_conn.rollback()
        
        cur.execute("SELECT COUNT(*) FROM test_items")
        count = cur.fetchone()[0]
        
        assert count == 1  # Only 'before' remains
```

### Example 7: GitHub Actions CI

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: dtolnay/rust-action@stable
    
    - name: Build PGlite Proxy
      run: cargo build --release
    
    - name: Setup Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.11'
    
    - name: Install dependencies
      run: |
        pip install pytest psycopg2-binary pandas
    
    - name: Start Proxy
      run: |
        ./target/release/pglite-proxy &
        sleep 2
    
    - name: Run tests
      run: pytest tests/
    
    - name: Integration test
      run: |
        psql -h 127.0.0.1 -p 5432 -U postgres -c "CREATE TABLE ci_test (id SERIAL);"
        psql -h 127.0.0.1 -p 5432 -U postgres -c "INSERT INTO ci_test DEFAULT VALUES;"
        psql -h 127.0.0.1 -p 5432 -U postgres -c "SELECT * FROM ci_test;"
```

---

## Migration Scenarios

### Example 8: PostgreSQL to SQLite Migration

```bash
#!/bin/bash
# migrate_to_sqlite.sh

SOURCE_DB="postgresql://user:pass@prod.db.com:5432/myapp"
TARGET_DB="myapp.db"

# 1. Schema migration (using pg_dump --schema-only)
echo "Extracting schema..."
pg_dump --schema-only --no-owner --no-privileges "$SOURCE_DB" > schema.sql

# 2. Start proxy with fresh database
echo "Starting PGlite Proxy..."
PGQT_DB="$TARGET_DB" ./pglite-proxy &
PROXY_PID=$!
sleep 2

# 3. Apply schema (transpiles automatically)
echo "Applying schema..."
psql -h 127.0.0.1 -p 5432 -U postgres < schema.sql

# 4. Data migration (in batches)
echo "Migrating data..."
psql "$SOURCE_DB" -c "\COPY (SELECT * FROM users) TO '/tmp/users.csv' CSV HEADER"
psql -h 127.0.0.1 -p 5432 -U postgres -c "\COPY users FROM '/tmp/users.csv' CSV HEADER"

# Repeat for other tables...

# 5. Verify migration
echo "Verifying..."
psql -h 127.0.0.1 -p 5432 -U postgres -c "SELECT COUNT(*) FROM users;"
psql -h 127.0.0.1 -p 5432 -U postgres -c "SELECT * FROM __pg_meta__;"

# 6. Stop proxy
kill $PROXY_PID

echo "Migration complete: $TARGET_DB"
```

### Example 9: SQLite to PostgreSQL Migration (Reverse)

```python
# migrate_to_postgres.py
import psycopg2
import json

def migrate_to_postgres(sqlite_proxy_conn, postgres_conn):
    """Migrate from PGlite Proxy back to PostgreSQL"""
    
    # Get all tables from shadow catalog
    with sqlite_proxy_conn.cursor() as cur:
        cur.execute("""
            SELECT DISTINCT table_name 
            FROM __pg_meta__
        """)
        tables = [row[0] for row in cur.fetchall()]
    
    for table in tables:
        print(f"Migrating {table}...")
        
        # Get original schema
        with sqlite_proxy_conn.cursor() as cur:
            cur.execute("""
                SELECT column_name, original_type, constraints
                FROM __pg_meta__
                WHERE table_name = %s
            """, (table,))
            columns = cur.fetchall()
        
        # Build CREATE TABLE with original PostgreSQL types
        col_defs = []
        for col_name, orig_type, constraints in columns:
            col_def = f"{col_name} {orig_type}"
            if constraints:
                col_def += f" {constraints}"
            col_defs.append(col_def)
        
        create_sql = f"CREATE TABLE {table} ({', '.join(col_defs)})"
        
        with postgres_conn.cursor() as cur:
            cur.execute(f"DROP TABLE IF EXISTS {table}")
            cur.execute(create_sql)
        
        # Copy data
        with sqlite_proxy_conn.cursor() as cur:
            cur.execute(f"SELECT * FROM {table}")
            rows = cur.fetchall()
            colnames = [desc[0] for desc in cur.description]
        
        with postgres_conn.cursor() as cur:
            placeholders = ','.join(['%s'] * len(colnames))
            insert_sql = f"INSERT INTO {table} ({','.join(colnames)}) VALUES ({placeholders})"
            cur.executemany(insert_sql, rows)
        
        postgres_conn.commit()
        print(f"  Migrated {len(rows)} rows")

# Usage
sqlite_conn = psycopg2.connect(
    host="127.0.0.1", port=5432,
    user="postgres", database="myapp.db"
)

postgres_conn = psycopg2.connect(
    host="prod.db.com", port=5432,
    user="admin", password="secret",
    database="myapp"
)

migrate_to_postgres(sqlite_conn, postgres_conn)
```

---

## Advanced Patterns

### Example 10: Multi-Tenant Application

```sql
-- Schema-per-tenant using attached databases

-- Main database (shared tables)
CREATE TABLE tenants (
    id SERIAL PRIMARY KEY,
    subdomain VARCHAR(100) UNIQUE NOT NULL,
    db_name VARCHAR(100) NOT NULL
);

-- Tenant-specific database: tenant_1.db
-- Attached as schema "tenant_1"

-- Application routes queries:
-- tenant1.myapp.com → SELECT * FROM tenant_1.users
-- tenant2.myapp.com → SELECT * FROM tenant_2.users

-- In proxy: Map schema to attached database
-- public.users → main.users
-- tenant_1.users → tenant_1.db.users
```

### Example 11: Time-Series Data

```sql
-- Partitioning by time (manual, since SQLite doesn't support declarative partitioning)

-- Create monthly tables
CREATE TABLE events_2024_01 (
    id SERIAL PRIMARY KEY,
    event_time TIMESTAMPTZ NOT NULL,
    event_type VARCHAR(50),
    payload JSONB
);

CREATE TABLE events_2024_02 (
    LIKE events_2024_01 INCLUDING ALL
);

-- View to union all partitions
CREATE VIEW events AS
    SELECT * FROM events_2024_01
    UNION ALL
    SELECT * FROM events_2024_02;

-- Insert with routing (application or trigger)
-- Events in January → events_2024_01
-- Events in February → events_2024_02

-- Query the view
SELECT event_type, COUNT(*) 
FROM events 
WHERE event_time >= '2024-01-01' AND event_time < '2024-02-01'
GROUP BY event_type;
```

### Example 12: Full-Text Search Setup

```sql
-- Enable FTS5 (SQLite extension)

-- Create FTS5 virtual table
CREATE VIRTUAL TABLE documents_fts USING fts5(
    title,
    content,
    content_rowid=rowid
);

-- Main documents table
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    title VARCHAR(200),
    content TEXT,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Triggers to keep FTS index in sync
CREATE TRIGGER documents_ai AFTER INSERT ON documents BEGIN
    INSERT INTO documents_fts(rowid, title, content)
    VALUES (new.id, new.title, new.content);
END;

CREATE TRIGGER documents_ad AFTER DELETE ON documents BEGIN
    INSERT INTO documents_fts(documents_fts, rowid, title, content)
    VALUES ('delete', old.id, old.title, old.content);
END;

CREATE TRIGGER documents_au AFTER UPDATE ON documents BEGIN
    INSERT INTO documents_fts(documents_fts, rowid, title, content)
    VALUES ('delete', old.id, old.title, old.content);
    INSERT INTO documents_fts(rowid, title, content)
    VALUES (new.id, new.title, new.content);
END;

-- Search (transpiled from PostgreSQL text search)
-- PostgreSQL: SELECT * FROM documents WHERE to_tsvector(content) @@ 'search'
-- SQLite: SELECT * FROM documents_fts WHERE documents_fts MATCH 'search'
```

### Example 13: Audit Logging

```sql
-- Audit log table
CREATE TABLE audit_log (
    id SERIAL PRIMARY KEY,
    table_name VARCHAR(100),
    operation VARCHAR(10),
    old_data JSONB,
    new_data JSONB,
    changed_at TIMESTAMPTZ DEFAULT now(),
    changed_by VARCHAR(100)
);

-- Generic audit trigger function (simplified)
CREATE TRIGGER users_audit
AFTER INSERT OR UPDATE OR DELETE ON users
FOR EACH ROW
BEGIN
    INSERT INTO audit_log (table_name, operation, old_data, new_data)
    VALUES (
        'users',
        CASE 
            WHEN OLD.id IS NULL THEN 'INSERT'
            WHEN NEW.id IS NULL THEN 'DELETE'
            ELSE 'UPDATE'
        END,
        json_object(OLD),
        json_object(NEW)
    );
END;

-- Query audit trail
SELECT 
    changed_at,
    operation,
    old_data->>'email' as old_email,
    new_data->>'email' as new_email
FROM audit_log
WHERE table_name = 'users'
ORDER BY changed_at DESC;
```

---

## Troubleshooting

### Common Issues

**Issue: "database is locked"**
```
Solution: SQLite only allows one writer at a time. 
- Reduce concurrent writes
- Use WAL mode (enabled by default)
- Consider connection pooling with limited writers
```

**Issue: "no such function: now"**
```
Solution: The transpiler should convert this automatically.
If not working, check:
- Proxy is running
- Using correct port
- Check logs for transpilation errors
```

**Issue: Type not preserved in __pg_meta__**
```
Solution: Ensure CREATE TABLE goes through the proxy.
Direct SQLite connections won't populate the shadow catalog.
```

### Debug Mode

```bash
# Enable verbose logging
RUST_LOG=debug ./pglite-proxy

# Test transpilation directly
cargo test transpiler:: -- --nocapture
```
