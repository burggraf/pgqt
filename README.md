# PGlite Proxy

A PostgreSQL wire-compatible proxy for SQLite that allows you to use standard PostgreSQL clients, tools, and ORMs with a local SQLite database file.

## Overview

PGlite Proxy acts as a middleware server that translates the PostgreSQL wire protocol into SQLite operations. It provides:

- **Full PostgreSQL Compatibility**: Connect using `psql`, `pgAdmin`, DBeaver, or any PostgreSQL driver
- **Type Preservation**: Original PostgreSQL types (SERIAL, VARCHAR, TIMESTAMPTZ, etc.) are stored in a shadow catalog for reversible migrations
- **SQL Transpilation**: PostgreSQL-specific syntax is automatically rewritten for SQLite compatibility
- **ORM Support**: Works with Prisma, TypeORM, Drizzle, and other modern ORMs
- **Role-Based Access Control (RBAC)**: PostgreSQL-compatible users, roles, and permission management
- **Full-Text Search (FTS)**: PostgreSQL-compatible full-text search using to_tsvector, to_tsquery, and the @@ match operator
- **Vector Search**: pgvector-compatible vector similarity search for AI/ML applications

## Quick Start

### Installation

```bash
# Clone and build
git clone https://github.com/yourusername/pglite-proxy
cd pglite-proxy
cargo build --release

# Or install via cargo
cargo install pglite-proxy
```

### Running the Proxy

```bash
# Start with defaults (test.db on port 5432)
./target/release/pglite-proxy

# Or specify custom database and port
PG_LITE_DB=myapp.db PG_LITE_PORT=5433 ./target/release/pglite-proxy
```

### Connecting

```bash
# Using psql
psql -h 127.0.0.1 -p 5432 -U postgres

# Using connection string
postgresql://postgres@127.0.0.1:5432/test.db
```

## Features

### Type Mapping

| PostgreSQL Type | SQLite Storage | Original Type Preserved |
|----------------|----------------|------------------------|
| **Serial Types** |||
| SERIAL, BIGSERIAL, SMALLSERIAL | INTEGER PRIMARY KEY AUTOINCREMENT | ✅ |
| **Integer Types** |||
| INTEGER, BIGINT, SMALLINT, INT2/4/8 | INTEGER | ✅ |
| **Floating Point** |||
| REAL, FLOAT4, FLOAT8, DOUBLE PRECISION | REAL | ✅ |
| NUMERIC, DECIMAL, MONEY | REAL | ✅ |
| **Character/String** |||
| VARCHAR(n), CHAR(n), TEXT | TEXT | ✅ |
| **Binary** |||
| BYTEA | BLOB | ✅ |
| **Boolean** |||
| BOOLEAN, BOOL | INTEGER | ✅ |
| **Date/Time** |||
| TIMESTAMP [WITH/WITHOUT TIME ZONE], DATE, TIME | TEXT | ✅ |
| INTERVAL | TEXT | ✅ |
| **JSON** |||
| JSON, JSONB | TEXT | ✅ |
| **Network Address** |||
| INET, CIDR, MACADDR, MACADDR8 | TEXT | ✅ |
| **Geometric** |||
| POINT, LINE, LSEG, BOX, PATH, POLYGON, CIRCLE | TEXT | ✅ |
| **Range Types** |||
| INT4RANGE, INT8RANGE, NUMRANGE, TSRANGE, TSTZRANGE, DATERANGE | TEXT | ✅ |
| **Full-Text Search** |||
| TSVECTOR, TSQUERY | TEXT | ✅ |
| **Vector Search** |||
| VECTOR(N) | TEXT (JSON) | ✅ |
| **Other** |||
| UUID | TEXT | ✅ |
| BIT, VARBIT | TEXT | ✅ |
| XML | TEXT | ✅ |
| ARRAY types (INT[], TEXT[], etc.) | TEXT | ✅ |
| ENUM, DOMAIN | TEXT | ✅ |

### SQL Transpilation

The proxy automatically rewrites PostgreSQL-specific syntax:

```sql
-- Type casts
SELECT '1'::int;           -- → SELECT CAST('1' AS INTEGER)

-- Functions
SELECT now();              -- → SELECT datetime('now')

-- Schema references
SELECT * FROM public.users; -- → SELECT * FROM users

-- Operators
WHERE name ~~ 'alice%';    -- → WHERE name LIKE 'alice%'
```

### Role-Based Access Control (RBAC)

PGlite Proxy implements PostgreSQL-compatible role-based access control, allowing you to manage users, roles, and permissions:

#### Creating Roles

```sql
-- Create a basic role
CREATE ROLE app_user WITH LOGIN;

-- Create a superuser role
CREATE ROLE admin WITH SUPERUSER CREATEDB CREATEROLE;

-- Create a role with password
CREATE ROLE readonly WITH LOGIN PASSWORD 'secure_password';
```

#### Granting Privileges

```sql
-- Grant table-level privileges
GRANT SELECT ON users TO readonly;
GRANT SELECT, INSERT, UPDATE ON orders TO app_user;
GRANT ALL PRIVILEGES ON products TO admin;

-- Grant role membership (role inheritance)
GRANT app_user TO readonly;
GRANT admin TO app_user;
```

#### Revoking Privileges

```sql
-- Revoke specific privileges
REVOKE DELETE ON orders FROM app_user;
REVOKE INSERT ON users FROM readonly;

-- Revoke role membership
REVOKE admin FROM app_user;
```

#### Using SET ROLE

```sql
-- Switch to a different role (if you have membership)
SET ROLE app_user;

-- View current role
SELECT current_user;

-- Reset to original login role
SET ROLE NONE;
```

#### Permission Enforcement

The proxy enforces permissions on all DML operations:

- **SELECT**: Requires `SELECT` privilege on table
- **INSERT**: Requires `INSERT` privilege on table
- **UPDATE**: Requires `UPDATE` privilege on table
- **DELETE**: Requires `DELETE` privilege on table
- **DDL**: Requires superuser or table ownership

**Permission Resolution:**
1. Superusers bypass all permission checks
2. Table owners have implicit all privileges
3. Privileges are inherited through role membership
4. PUBLIC grants apply to all roles

#### System Catalog Views

Query PostgreSQL-compatible system catalogs:

```sql
-- List all roles
SELECT * FROM pg_roles;

-- View role memberships
SELECT * FROM pg_auth_members;

-- Check table permissions
SELECT * FROM has_table_privilege('app_user', 'users', 'SELECT');

-- View table ownership
SELECT relname, rolname as owner 
FROM pg_class c 
JOIN pg_roles r ON c.relowner = r.oid;
```

#### Example: Multi-User Setup

```sql
-- 1. Create roles
CREATE ROLE admin WITH SUPERUSER;
CREATE ROLE app_user WITH LOGIN;
CREATE ROLE readonly WITH LOGIN;

-- 2. Create tables (admin owns them)
CREATE TABLE users (id SERIAL, name TEXT);
CREATE TABLE orders (id SERIAL, user_id INT, total REAL);

-- 3. Grant permissions
GRANT SELECT ON users TO readonly;
GRANT SELECT, INSERT, UPDATE ON orders TO app_user;
GRANT ALL ON users TO app_user;

-- 4. Test permissions
-- As readonly: SELECT works, INSERT fails
SET ROLE readonly;
SELECT * FROM users;           -- ✅ Success
INSERT INTO users VALUES (1);  -- ❌ Permission denied

-- As app_user: Can modify users and orders
SET ROLE app_user;
INSERT INTO users VALUES (1, 'Alice');  -- ✅ Success
INSERT INTO orders VALUES (1, 1, 99.99); -- ✅ Success
DELETE FROM users WHERE id = 1;         -- ❌ Permission denied
```

### Row-Level Security (RLS)

PGlite Proxy implements PostgreSQL-compatible Row-Level Security (RLS), enabling fine-grained access control at the row level based on the current user or session context.

#### Enabling RLS

```sql
-- Enable RLS on a table
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;

-- Force RLS for table owners too
ALTER TABLE documents FORCE ROW LEVEL SECURITY;
```

#### Creating Policies

```sql
-- Users can only see their own documents
CREATE POLICY user_select ON documents
  FOR SELECT
  USING (owner = current_user());

-- Users can only insert documents they own
CREATE POLICY user_insert ON documents
  FOR INSERT
  WITH CHECK (owner = current_user());

-- Admin role can see all documents
CREATE POLICY admin_full ON documents
  TO admin
  USING (true);
```

#### Policy Modes

- **PERMISSIVE** (default): Multiple policies are combined with OR logic
- **RESTRICTIVE**: Multiple policies are combined with AND logic (for mandatory filters)

```sql
-- PERMISSIVE: User sees rows matching either condition
CREATE POLICY policy1 ON table1 AS PERMISSIVE FOR SELECT USING (col1 = 'a');
CREATE POLICY policy2 ON table1 AS PERMISSIVE FOR SELECT USING (col2 = 'b');
-- Result: (col1 = 'a') OR (col2 = 'b')

-- RESTRICTIVE: User must satisfy ALL conditions
CREATE POLICY policy3 ON table2 AS RESTRICTIVE FOR SELECT USING (status = 'active');
CREATE POLICY policy4 ON table2 AS RESTRICTIVE FOR SELECT USING (dept = 'sales');
-- Result: (status = 'active') AND (dept = 'sales')
```

For complete documentation, see [docs/RLS.md](./docs/RLS.md).

### Shadow Catalog

All original PostgreSQL type information is stored in the `__pg_meta__` table:

```sql
-- Query the shadow catalog
SELECT * FROM __pg_meta__ WHERE table_name = 'users';
-- Returns: column_name, original_type (VARCHAR(100)), constraints, etc.
```

This enables 100% reversible migrations back to PostgreSQL.

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  PostgreSQL     │────▶│   PGlite Proxy   │────▶│   SQLite        │
│  Client (psql)  │     │   (Rust/Tokio)   │     │   Database      │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                               │
                               ▼
                        ┌──────────────────┐
                        │  Shadow Catalog  │
                        │  (__pg_meta__)   │
                        └──────────────────┘
```

### Components

1. **Wire Protocol Handler** (`pgwire`): Handles PostgreSQL v3 protocol
2. **AST Transpiler** (`pg_query`): Parses and rewrites SQL using PostgreSQL 17 parser
3. **Type Registry**: Maps PostgreSQL types to SQLite with metadata preservation
4. **Query Executor**: Executes against SQLite with result streaming

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PG_LITE_DB` | `test.db` | SQLite database file path |
| `PG_LITE_PORT` | `5432` | TCP port to listen on |

### Programmatic Usage

```rust
use pglite_proxy::SqliteHandler;

let handler = SqliteHandler::new("myapp.db")?;
// Use with your own pgwire server setup
```

## Examples

### Basic CRUD Operations

```sql
-- Create table with PostgreSQL types
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(100),
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT now(),
    metadata JSONB
);

-- Insert data
INSERT INTO users (email, name, metadata) 
VALUES ('alice@example.com', 'Alice', '{"role": "admin"}');

-- Query with PostgreSQL syntax
SELECT * FROM users 
WHERE email ~~ '%@example.com' 
  AND created_at > now() - interval '1 day';

-- Update
UPDATE users SET active = false WHERE id = 1;

-- Delete
DELETE FROM users WHERE id = 1;
```

### Using with ORMs

#### Prisma

```prisma
// schema.prisma
generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = "postgresql://postgres@127.0.0.1:5432/myapp.db"
}

model User {
  id        Int      @id @default(autoincrement())
  email     String   @unique
  name      String?
  active    Boolean  @default(true)
  createdAt DateTime @default(now()) @map("created_at")
  metadata  Json?
}
```

#### TypeORM

```typescript
// data-source.ts
import { DataSource } from "typeorm";

export const AppDataSource = new DataSource({
  type: "postgres",
  host: "127.0.0.1",
  port: 5432,
  username: "postgres",
  database: "myapp.db",
  entities: ["src/entity/**/*.ts"],
  synchronize: true,
});
```

### Migration Example

```bash
# Export from PostgreSQL
pg_dump -h prod.db.com -U postgres myapp > myapp.sql

# Start proxy with new SQLite file
PG_LITE_DB=myapp.db ./pglite-proxy

# Import (transpiles automatically)
psql -h 127.0.0.1 -p 5432 -U postgres < myapp.sql

# Verify shadow catalog preserved types
psql -h 127.0.0.1 -p 5432 -U postgres -c "SELECT * FROM __pg_meta__"
```

## Development

### Building

```bash
cargo build --release
```

### Testing

```bash
# Run unit tests
cargo test

# Run integration tests with psql
./scripts/integration_test.sh
```

### Project Structure

```
src/
├── main.rs           # TCP server and connection handling
├── catalog.rs        # Shadow catalog (__pg_meta__) management
├── transpiler.rs     # SQL AST transpilation (PostgreSQL → SQLite)
└── lib.rs            # Library exports
```

## Limitations

### Current

- **Single-writer model**: SQLite's concurrency model (no MVCC like PostgreSQL)
- **No stored procedures**: PL/pgSQL not yet implemented (Phase 3 roadmap)
- **Limited window functions**: Basic support only

### PostgreSQL Features Not Supported

- **Extensions**: PostGIS, pgvector, etc. (use SpatiaLite, sqlite-vec instead)
- **Advanced indexing**: GIN, GiST indexes (use FTS5, R-Tree instead)
- **Replication**: Logical/physical replication (single-file database)

### Full-Text Search (FTS)

PGlite Proxy provides PostgreSQL-compatible full-text search functionality using SQLite's FTS5 extension:

```sql
-- Create a table with tsvector column
CREATE TABLE articles (
    id SERIAL PRIMARY KEY,
    title TEXT,
    body TEXT,
    search_vector TSVECTOR
);

-- Search using @@ operator
SELECT * FROM articles 
WHERE search_vector @@ to_tsquery('postgresql & database');

-- Search with ranking
SELECT title, ts_rank(search_vector, to_tsquery('postgresql')) as rank
FROM articles
WHERE search_vector @@ to_tsquery('postgresql')
ORDER BY rank DESC;

-- Web-style search (Google-like syntax)
SELECT * FROM articles
WHERE search_vector @@ websearch_to_tsquery('postgresql OR mysql -oracle');

-- Highlight matching terms
SELECT ts_headline('english', body, to_tsquery('postgresql')) as highlighted
FROM articles;
```

**Supported FTS Functions:**
- `to_tsvector([config,] text)` - Convert text to tsvector
- `to_tsquery([config,] text)` - Convert text to tsquery
- `plainto_tsquery([config,] text)` - Plain text to tsquery
- `phraseto_tsquery([config,] text)` - Phrase to tsquery
- `websearch_to_tsquery([config,] text)` - Web-style query
- `ts_rank(tsvector, tsquery)` - Rank results
- `ts_headline([config,] text, tsquery [, options])` - Highlight matches
- `setweight(tsvector, char)` - Set weight on vector
- `strip(tsvector)` - Remove positions

**Supported Operators:**
- `@@` - Match operator
- `&`, `|`, `!` - Boolean operators in tsquery
- `<->` - Phrase search
- `||` - Concatenate tsvectors

For complete documentation, see [docs/FTS.md](./docs/FTS.md).

### Vector Search (pgvector Compatible)

PGlite Proxy provides PostgreSQL pgvector-compatible vector search for similarity searches on embeddings:

```sql
-- Create table with vector column
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    content TEXT,
    embedding VECTOR(1536)
);

-- Insert with embedding
INSERT INTO documents (content, embedding)
VALUES ('Hello world', '[0.1, 0.2, 0.3, ...]');

-- Find similar documents using cosine distance
SELECT id, content, cosine_distance(embedding, '[0.12, 0.22, ...]') AS distance
FROM documents
ORDER BY distance
LIMIT 5;
```

**Supported Distance Functions:**
- `l2_distance(a, b)` / `vector_l2_distance(a, b)` - L2 (Euclidean) distance
- `cosine_distance(a, b)` / `vector_cosine_distance(a, b)` - Cosine distance
- `inner_product(a, b)` / `vector_inner_product(a, b)` - Inner product
- `l1_distance(a, b)` / `vector_l1_distance(a, b)` - L1 (Manhattan) distance

**Supported Operators:**
- `<->` - L2 distance
- `<=>` - Cosine distance
- `<#>` - Inner product
- `<+>` - L1 distance

**Utility Functions:**
- `vector_dims(vector)` - Get number of dimensions
- `l2_norm(vector)` - Calculate L2 norm
- `l2_normalize(vector)` - Normalize to unit vector
- `subvector(vector, start, len)` - Extract subvector
- `vector_add(a, b)` - Add vectors element-wise
- `vector_sub(a, b)` - Subtract vectors element-wise

For complete documentation, see [docs/VECTOR.md](./docs/VECTOR.md).

## Roadmap

### Phase 3 (In Progress)
- [x] **Users & Permissions (RBAC)** - Role-based access control with GRANT/REVOKE
- [ ] `DISTINCT ON` polyfill using window functions
- [ ] PL/pgSQL procedural blocks via Lua runtime
- [x] Row-Level Security (RLS) emulation
- [x] **Full-Text Search (FTS)** - PostgreSQL-compatible FTS using FTS5

### Phase 4 (In Progress)
- [x] **Vector Search** - pgvector-compatible vector search for embeddings
- [ ] Connection pooling and load balancing

## License

MIT License - See LICENSE file for details.

## Contributing

Contributions welcome! Please read CONTRIBUTING.md for guidelines.
