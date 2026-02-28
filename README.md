# PGlite Proxy

A PostgreSQL wire-compatible proxy for SQLite that allows you to use standard PostgreSQL clients, tools, and ORMs with a local SQLite database file.

## Overview

PGlite Proxy acts as a middleware server that translates the PostgreSQL wire protocol into SQLite operations. It provides:

- **Full PostgreSQL Compatibility**: Connect using `psql`, `pgAdmin`, DBeaver, or any PostgreSQL driver
- **Type Preservation**: Original PostgreSQL types (SERIAL, VARCHAR, TIMESTAMPTZ, etc.) are stored in a shadow catalog for reversible migrations
- **SQL Transpilation**: PostgreSQL-specific syntax is automatically rewritten for SQLite compatibility
- **ORM Support**: Works with Prisma, TypeORM, Drizzle, and other modern ORMs

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

| PostgreSQL | SQLite Storage | Original Type Preserved |
|-----------|----------------|------------------------|
| SERIAL | INTEGER PRIMARY KEY AUTOINCREMENT | ✅ |
| VARCHAR(n) | TEXT | ✅ |
| INTEGER | INTEGER | ✅ |
| TIMESTAMP WITH TIME ZONE | TEXT | ✅ |
| JSON/JSONB | TEXT | ✅ |
| BOOLEAN | INTEGER | ✅ |

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

## Roadmap

### Phase 3 (In Progress)
- [ ] `DISTINCT ON` polyfill using window functions
- [ ] PL/pgSQL procedural blocks via Lua runtime
- [ ] Row-Level Security (RLS) emulation

### Phase 4 (Planned)
- [ ] Full-text search (FTS5 integration)
- [ ] Vector search (sqlite-vec integration)
- [ ] Connection pooling and load balancing

## License

MIT License - See LICENSE file for details.

## Contributing

Contributions welcome! Please read CONTRIBUTING.md for guidelines.
