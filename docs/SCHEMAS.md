# Schema (Namespace) Support

PGlite Proxy implements PostgreSQL schema/namespace support using SQLite's `ATTACH DATABASE` feature. This allows you to organize database objects into logical namespaces, just like in PostgreSQL.

## Overview

Each PostgreSQL schema (except `public`) maps to a separate SQLite database file:

- **Main database file** (`myapp.db`) → `public` schema
- **Schema `inventory`** → `myapp_inventory.db`
- **Schema `analytics`** → `myapp_analytics.db`

This approach provides:
- ✅ Native `schema.table` syntax support
- ✅ Proper isolation between schemas
- ✅ Foreign key support within each schema
- ✅ PostgreSQL-compatible catalog views

## Creating Schemas

```sql
-- Create a new schema
CREATE SCHEMA inventory;

-- Create schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS analytics;

-- Create schema with specific owner
CREATE SCHEMA reporting AUTHORIZATION admin_user;
```

## Dropping Schemas

```sql
-- Drop an empty schema
DROP SCHEMA old_schema;

-- Drop schema if it exists
DROP SCHEMA IF EXISTS temp_schema;

-- Drop schema and all its objects
DROP SCHEMA old_schema CASCADE;
```

## Creating Tables in Schemas

```sql
-- Create table in a specific schema
CREATE TABLE inventory.products (
    id SERIAL PRIMARY KEY,
    name TEXT,
    price REAL
);

-- Create table in public schema (default)
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email TEXT
);

-- Explicit public schema
CREATE TABLE public.orders (
    id SERIAL PRIMARY KEY,
    user_id INTEGER
);
```

## Querying Tables

```sql
-- Query with schema prefix
SELECT * FROM inventory.products;

-- Query public schema (prefix optional)
SELECT * FROM users;
SELECT * FROM public.users;  -- equivalent

-- Join across schemas
SELECT p.name, u.email
FROM inventory.products p
JOIN public.users u ON p.user_id = u.id;
```

## Search Path

The `search_path` determines which schemas are searched when resolving unqualified table names:

```sql
-- Show current search path
SHOW search_path;

-- Set search path
SET search_path TO inventory, public;

-- Reset to default
SET search_path TO DEFAULT;
```

### Default Search Path

The default search path is `"$user", public`, which means:
1. First, look for a schema with the same name as the current user
2. If not found, use the `public` schema

### Search Path Resolution

When you reference a table without a schema prefix:

```sql
SELECT * FROM products;
```

The proxy searches for `products` in each schema in the search path order, using the first match found.

## System Catalog

### pg_namespace

Query the `pg_namespace` view to list all schemas:

```sql
SELECT nspname, nspowner
FROM pg_namespace
ORDER BY nspname;
```

Example output:
```
  nspname     | nspowner
--------------+----------
 analytics    |       10
 information  |       10
 inventory    |       10
 pg_catalog   |       10
 public       |       10
```

### Helper Functions

```sql
-- Current schema (first in search path)
SELECT current_schema();

-- All schemas in search path
SELECT current_schemas(true);
```

## Schema Privileges

Control access to schemas with GRANT and REVOKE:

```sql
-- Grant USAGE on schema (allows accessing objects)
GRANT USAGE ON SCHEMA inventory TO app_user;

-- Grant CREATE on schema (allows creating objects)
GRANT CREATE ON SCHEMA inventory TO developer;

-- Grant both
GRANT ALL PRIVILEGES ON SCHEMA inventory TO admin;

-- Revoke privileges
REVOKE CREATE ON SCHEMA inventory FROM developer;
```

## Limitations

### Cross-Schema Foreign Keys

SQLite does not support foreign key constraints across attached databases:

```sql
-- This will NOT work
CREATE TABLE public.orders (
    id SERIAL PRIMARY KEY,
    product_id INTEGER REFERENCES inventory.products(id)  -- ❌ Not supported
);
```

**Workaround**: Use triggers or application-level validation for cross-schema referential integrity.

### Cross-Schema Triggers

Triggers cannot target tables in different schemas:

```sql
-- This will NOT work
CREATE TRIGGER audit_trigger
AFTER INSERT ON public.orders
BEGIN
    INSERT INTO analytics.audit_log VALUES (...);  -- ❌ Not supported
END;
```

### Atomic Transactions

While SQLite supports atomic commits across attached databases, there are limitations:
- WAL mode transactions are per-file only
- A system crash during multi-database commits could leave some files updated and others not

For critical data, ensure proper backup and recovery procedures.

### Schema Rename

`ALTER SCHEMA ... RENAME` is not supported in the current implementation. To rename a schema:
1. Create a new schema with the desired name
2. Copy all objects to the new schema
3. Drop the old schema

## File Structure

When using schemas, the database files are organized as follows:

```
myapp.db                    # Main database (public schema)
myapp_inventory.db          # inventory schema
myapp_analytics.db          # analytics schema
myapp_reporting.db          # reporting schema
```

**Note**: All schema database files are stored in the same directory as the main database file.

## Best Practices

### 1. Use Schemas for Logical Organization

```sql
-- Good: Organize by domain
CREATE TABLE inventory.products (...);
CREATE TABLE inventory.suppliers (...);
CREATE TABLE sales.orders (...);
CREATE TABLE sales.customers (...);

-- Avoid: Everything in public
CREATE TABLE products (...);
CREATE TABLE suppliers (...);
CREATE TABLE orders (...);
CREATE TABLE customers (...);
```

### 2. Set Appropriate Search Path

```sql
-- For applications primarily using one schema
SET search_path TO inventory, public;
```

### 3. Use Explicit Schema Qualification for Clarity

```sql
-- Clear and explicit
SELECT * FROM inventory.products;

-- Less clear, depends on search_path
SELECT * FROM products;
```

### 4. Backup All Schema Files

When backing up your database, ensure you include all schema database files:

```bash
# Backup all schema files
cp myapp*.db /backup/
```

## Migration from PostgreSQL

When migrating from PostgreSQL to PGlite Proxy with schemas:

1. **Export schemas** from PostgreSQL:
   ```bash
   pg_dump -h prod.db.com --schema-only myapp > schemas.sql
   pg_dump -h prod.db.com --data-only myapp > data.sql
   ```

2. **Import to PGlite Proxy**:
   ```sql
   -- Connect to proxy
   \connect host=127.0.0.1 port=5432 user=postgres
   
   -- Create schemas
   \i schemas.sql
   
   -- Import data
   \i data.sql
   ```

3. **Verify schema creation**:
   ```sql
   SELECT * FROM pg_namespace;
   ```

## Examples

### Multi-Tenant Application

```sql
-- Create tenant schemas
CREATE SCHEMA tenant_acme;
CREATE SCHEMA tenant_globex;

-- Each tenant has their own tables
CREATE TABLE tenant_acme.users (...);
CREATE TABLE tenant_globex.users (...);

-- Switch between tenants
SET search_path TO tenant_acme, public;
SELECT * FROM users;  -- Returns Acme's users
```

### Modular Application

```sql
-- Organize by module
CREATE SCHEMA auth;
CREATE SCHEMA billing;
CREATE SCHEMA notifications;

CREATE TABLE auth.users (...);
CREATE TABLE auth.sessions (...);

CREATE TABLE billing.invoices (...);
CREATE TABLE billing.payments (...);

CREATE TABLE notifications.emails (...);
CREATE TABLE notifications.sms (...);
```

## Compatibility Matrix

| Feature | PostgreSQL | PGlite Proxy |
|---------|------------|--------------|
| CREATE SCHEMA | ✅ | ✅ |
| DROP SCHEMA | ✅ | ✅ |
| DROP SCHEMA CASCADE | ✅ | ✅ |
| Schema-qualified names | ✅ | ✅ |
| search_path | ✅ | ✅ |
| current_schema() | ✅ | ✅ |
| current_schemas() | ✅ | ✅ |
| pg_namespace | ✅ | ✅ |
| GRANT ON SCHEMA | ✅ | ✅ |
| ALTER SCHEMA RENAME | ✅ | ❌ |
| Cross-schema FKs | ✅ | ❌ |
| Cross-schema triggers | ✅ | ❌ |
| CREATE SCHEMA ... CREATE TABLE | ✅ | ❌ |
