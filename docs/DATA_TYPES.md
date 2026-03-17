# PGQT Data Types Reference

This document provides a comprehensive reference of all PostgreSQL data types supported by PGQT, including their SQLite storage mappings, type preservation capabilities, and usage examples.

## Overview

PGQT supports **97 PostgreSQL data types** across the full spectrum of PostgreSQL's type system. All original type information is preserved in the shadow catalog (`__pg_meta__`), enabling 100% reversible migrations back to PostgreSQL.

## Type Categories

### 1. Numeric Types

| PostgreSQL Type | Aliases | SQLite Storage | Notes |
|----------------|---------|----------------|-------|
| `SMALLINT` | `INT2` | `INTEGER` | 16-bit signed integer |
| `INTEGER` | `INT`, `INT4` | `INTEGER` | 32-bit signed integer (default) |
| `BIGINT` | `INT8` | `INTEGER` | 64-bit signed integer |
| `REAL` | `FLOAT4` | `REAL` | 32-bit floating point |
| `DOUBLE PRECISION` | `FLOAT8`, `FLOAT` | `REAL` | 64-bit floating point |
| `NUMERIC` | `DECIMAL` | `REAL` | Arbitrary precision (stored as REAL) |
| `MONEY` | - | `REAL` | Currency amount |

**Serial Types (Auto-increment):**

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `SERIAL` | `INTEGER PRIMARY KEY AUTOINCREMENT` | Auto-incrementing 32-bit integer |
| `BIGSERIAL` | `INTEGER PRIMARY KEY AUTOINCREMENT` | Auto-incrementing 64-bit integer |
| `SMALLSERIAL` | `INTEGER PRIMARY KEY AUTOINCREMENT` | Auto-incrementing 16-bit integer |

**Example:**
```sql
CREATE TABLE numeric_demo (
    id SERIAL PRIMARY KEY,
    count INTEGER,
    big_count BIGINT,
    price NUMERIC(10,2),
    temperature REAL,
    precise_value DOUBLE PRECISION,
    balance MONEY
);
```

### 2. Character/String Types

| PostgreSQL Type | Aliases | SQLite Storage | Notes |
|----------------|---------|----------------|-------|
| `CHAR(n)` | `CHARACTER(n)`, `BPCHAR` | `TEXT` | Fixed-length, blank-padded |
| `VARCHAR(n)` | `CHARACTER VARYING(n)` | `TEXT` | Variable-length with limit |
| `TEXT` | - | `TEXT` | Variable unlimited length |
| `NAME` | - | `TEXT` | 64-character type for object names |

**Example:**
```sql
CREATE TABLE string_demo (
    id SERIAL PRIMARY KEY,
    code CHAR(5),              -- Fixed length
    title VARCHAR(255),        -- Variable length with max
    description TEXT,          -- Unlimited length
    object_name NAME           -- System identifier
);
```

### 3. Binary Data Types

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `BYTEA` | `BLOB` | Variable-length binary data |

**Example:**
```sql
CREATE TABLE binary_demo (
    id SERIAL PRIMARY KEY,
    image_data BYTEA,
    document BYTEA
);
```

### 4. Boolean Type

| PostgreSQL Type | Aliases | SQLite Storage | Notes |
|----------------|---------|----------------|-------|
| `BOOLEAN` | `BOOL` | `INTEGER` | Stored as 0 (false) or 1 (true) |

**Example:**
```sql
CREATE TABLE boolean_demo (
    id SERIAL PRIMARY KEY,
    is_active BOOLEAN DEFAULT true,
    is_deleted BOOL DEFAULT false
);
```

### 5. Date/Time Types

| PostgreSQL Type | Aliases | SQLite Storage | Notes |
|----------------|---------|----------------|-------|
| `DATE` | - | `TEXT` | Calendar date |
| `TIME` | `TIME WITHOUT TIME ZONE` | `TEXT` | Time of day |
| `TIMETZ` | `TIME WITH TIME ZONE` | `TEXT` | Time with timezone |
| `TIMESTAMP` | `TIMESTAMP WITHOUT TIME ZONE` | `TEXT` | Date and time |
| `TIMESTAMPTZ` | `TIMESTAMP WITH TIME ZONE` | `TEXT` | Date and time with timezone |
| `INTERVAL` | - | `TEXT` | Time interval |

**Example:**
```sql
CREATE TABLE datetime_demo (
    id SERIAL PRIMARY KEY,
    birth_date DATE,
    start_time TIME,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    duration INTERVAL
);
```

### 6. JSON Types

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `JSON` | `TEXT` | JSON data stored as text |
| `JSONB` | `TEXT` | Binary JSON (stored as text with validation) |
| `JSONPATH` | `TEXT` | JSON path expression |

**Example:**
```sql
CREATE TABLE json_demo (
    id SERIAL PRIMARY KEY,
    config JSON,
    metadata JSONB,
    path_query JSONPATH
);
```

### 7. Network Address Types

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `INET` | `TEXT` | IPv4/IPv6 host address |
| `CIDR` | `TEXT` | IPv4/IPv6 network address |
| `MACADDR` | `TEXT` | MAC address (6 bytes) |
| `MACADDR8` | `TEXT` | MAC address (8 bytes/EUI-64) |

**Example:**
```sql
CREATE TABLE network_demo (
    id SERIAL PRIMARY KEY,
    client_ip INET,
    network CIDR,
    device_mac MACADDR,
    device_mac_eui64 MACADDR8
);
```

### 8. Geometric Types

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `POINT` | `TEXT` | 2D point (x, y) |
| `LINE` | `TEXT` | Infinite line |
| `LSEG` | `TEXT` | Line segment |
| `BOX` | `TEXT` | Rectangular box |
| `PATH` | `TEXT` | Open or closed path |
| `POLYGON` | `TEXT` | Polygon |
| `CIRCLE` | `TEXT` | Circle |

**Example:**
```sql
CREATE TABLE geometric_demo (
    id SERIAL PRIMARY KEY,
    location POINT,
    boundary BOX,
    route PATH,
    coverage POLYGON,
    range CIRCLE
);
```

**Geometric Operators:**
- `+` - Translation
- `-` - Translation
- `*` - Scaling/rotation
- `/` - Scaling/rotation
- `@` - Center/area
- `##` - Closest point
- `<->` - Distance
- `&&` - Overlap
- `@>` - Contains
- `<@` - Contained in
- `~=` - Same as

### 9. Range Types

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `INT4RANGE` | `TEXT` | Range of 32-bit integers |
| `INT8RANGE` | `TEXT` | Range of 64-bit integers |
| `NUMRANGE` | `TEXT` | Range of numerics |
| `TSRANGE` | `TEXT` | Range of timestamps |
| `TSTZRANGE` | `TEXT` | Range of timestamps with timezone |
| `DATERANGE` | `TEXT` | Range of dates |

**Example:**
```sql
CREATE TABLE range_demo (
    id SERIAL PRIMARY KEY,
    price_range INT4RANGE,
    availability TSRANGE,
    vacation_period DATERANGE
);

-- Insert range values
INSERT INTO range_demo (price_range) VALUES ('[10, 100)');
INSERT INTO range_demo (availability) VALUES ('[2024-01-01, 2024-12-31]');
```

**Range Operators:**
- `&&` - Overlap
- `@>` - Contains element
- `<@` - Contained in
- `<<` - Strictly left of
- `>>` - Strictly right of
- `&<` - Does not extend to the right of
- `&>` - Does not extend to the left of
- `-|-` - Adjacent to
- `+` - Union
- `*` - Intersection
- `-` - Difference

### 10. Full-Text Search Types

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `TSVECTOR` | `TEXT` | Document representation for text search |
| `TSQUERY` | `TEXT` | Text search query |

**Example:**
```sql
CREATE TABLE fts_demo (
    id SERIAL PRIMARY KEY,
    title TEXT,
    content TEXT,
    search_vector TSVECTOR
);

-- Create search vector
INSERT INTO fts_demo (title, content, search_vector)
VALUES ('PostgreSQL Guide', 'A comprehensive guide...', to_tsvector('PostgreSQL Guide comprehensive'));

-- Search
SELECT * FROM fts_demo WHERE search_vector @@ to_tsquery('postgresql & guide');
```

### 11. Vector Type (pgvector-compatible)

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `VECTOR(n)` | `TEXT` | n-dimensional vector for similarity search |

**Example:**
```sql
CREATE TABLE vector_demo (
    id SERIAL PRIMARY KEY,
    content TEXT,
    embedding VECTOR(1536)
);

-- Insert vector
INSERT INTO vector_demo (content, embedding)
VALUES ('Hello world', '[0.1, 0.2, 0.3, ...]');

-- Find similar vectors
SELECT id, content, cosine_distance(embedding, '[0.12, 0.22, ...]') AS distance
FROM vector_demo
ORDER BY distance
LIMIT 5;
```

**Vector Functions:**
- `l2_distance(a, b)` / `vector_l2_distance(a, b)` - Euclidean distance
- `cosine_distance(a, b)` / `vector_cosine_distance(a, b)` - Cosine distance
- `inner_product(a, b)` / `vector_inner_product(a, b)` - Inner product
- `l1_distance(a, b)` / `vector_l1_distance(a, b)` - Manhattan distance
- `vector_dims(vector)` - Get dimensions
- `l2_norm(vector)` - Calculate L2 norm

**Vector Operators:**
- `<->` - L2 distance
- `<=>` - Cosine distance
- `<#>` - Inner product
- `<+>` - L1 distance

### 12. Bit String Types

| PostgreSQL Type | Aliases | SQLite Storage | Notes |
|----------------|---------|----------------|-------|
| `BIT(n)` | - | `TEXT` | Fixed-length bit string |
| `VARBIT` | `BIT VARYING` | `TEXT` | Variable-length bit string |

**Example:**
```sql
CREATE TABLE bitstring_demo (
    id SERIAL PRIMARY KEY,
    flags BIT(8),
    variable_flags VARBIT
);
```

### 13. UUID Type

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `UUID` | `TEXT` | Universally unique identifier |

**Example:**
```sql
CREATE TABLE uuid_demo (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT
);
```

### 14. XML Type

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `XML` | `TEXT` | XML data |

**Example:**
```sql
CREATE TABLE xml_demo (
    id SERIAL PRIMARY KEY,
    data XML
);
```

### 15. Array Types

PGQT supports arrays of any base type:

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `INTEGER[]` | `TEXT` | Array of integers |
| `TEXT[]` | `TEXT` | Array of text |
| `BOOLEAN[]` | `TEXT` | Array of booleans |
| `ANYTYPE[]` | `TEXT` | Array of any supported type |

**Example:**
```sql
CREATE TABLE array_demo (
    id SERIAL PRIMARY KEY,
    tags TEXT[],
    scores INTEGER[],
    matrix REAL[][]
);

-- Insert arrays
INSERT INTO array_demo (tags, scores)
VALUES ('{"featured", "sale"}', '{95, 87, 92}');

-- Array operations
SELECT * FROM array_demo WHERE tags && '{"featured"}';  -- Overlap
SELECT * FROM array_demo WHERE tags @> '{"featured", "sale"}';  -- Contains
```

**Array Operators:**
- `&&` - Overlap (have elements in common)
- `@>` - Contains
- `<@` - Is contained by
- `||` - Concatenation

**Array Functions:**
- `array_append()`, `array_prepend()`, `array_cat()`
- `array_remove()`, `array_replace()`
- `array_length()`, `cardinality()`, `array_ndims()`
- `array_position()`, `array_positions()`
- `array_to_string()`, `string_to_array()`

### 16. Enumerated Types (ENUM)

User-defined enumerated types are fully supported:

**Example:**
```sql
-- Create enum type
CREATE TYPE status AS ENUM ('pending', 'active', 'archived');

-- Use in table
CREATE TABLE enum_demo (
    id SERIAL PRIMARY KEY,
    name TEXT,
    current_status STATUS DEFAULT 'pending'
);

-- Insert with enum
INSERT INTO enum_demo (name, current_status) VALUES ('Item 1', 'active');
```

### 17. OID Types (Object Identifiers)

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `OID` | `INTEGER` | Numeric object identifier |
| `REGCLASS` | `INTEGER` | Relation name |
| `REGTYPE` | `INTEGER` | Data type name |
| `REGPROC` | `INTEGER` | Function name |
| `REGPROCEDURE` | `INTEGER` | Function with argument types |
| `REGOPER` | `INTEGER` | Operator name |
| `REGOPERATOR` | `INTEGER` | Operator with argument types |
| `REGNAMESPACE` | `INTEGER` | Namespace name |
| `REGROLE` | `INTEGER` | Role name |
| `REGCONFIG` | `INTEGER` | Text search configuration |
| `REGDICTIONARY` | `INTEGER` | Text search dictionary |

**Example:**
```sql
CREATE TABLE oid_demo (
    id SERIAL PRIMARY KEY,
    table_ref REGCLASS,
    type_ref REGTYPE
);

-- Insert with OID types
INSERT INTO oid_demo (table_ref, type_ref) VALUES ('users', 'integer');
```

### 18. Special Types

| PostgreSQL Type | SQLite Storage | Notes |
|----------------|----------------|-------|
| `PG_LSN` | `TEXT` | PostgreSQL Log Sequence Number |
| `TXID_SNAPSHOT` | `TEXT` | Transaction ID snapshot |
| `XID` | `INTEGER` | Transaction ID |
| `CID` | `INTEGER` | Command ID |
| `TID` | `TEXT` | Tuple identifier (row location) |

## Type Preservation

All original PostgreSQL type information is stored in the shadow catalog:

```sql
-- Query the shadow catalog
SELECT * FROM __pg_meta__ WHERE table_name = 'my_table';
```

This enables:
- **Reversible migrations**: Export back to PostgreSQL with original types
- **ORM compatibility**: Tools can query pg_catalog for accurate type info
- **Type constraints**: Length limits, precision/scale preserved

## Type Conversion Functions

PGQT provides PostgreSQL-compatible type casting:

```sql
-- Explicit cast
SELECT CAST('123' AS INTEGER);
SELECT '123'::INTEGER;
SELECT '123'::INT;

-- Type conversion functions
SELECT to_char(12345, '999,999');
SELECT to_date('2024-03-15', 'YYYY-MM-DD');
SELECT to_timestamp(1710500000);
```

## System Catalog

PGQT maintains a complete `pg_type` catalog with all 97 type definitions:

```sql
-- List all supported types
SELECT oid, typname, typtype, typcategory 
FROM pg_type 
ORDER BY typname;

-- Get type info for a column
SELECT a.attname, t.typname, t.typlen
FROM pg_attribute a
JOIN pg_type t ON a.atttypid = t.oid
WHERE a.attrelid = 'my_table'::regclass;
```

## Summary

| Category | Count | Types |
|----------|-------|-------|
| Numeric | 10 | SMALLINT, INTEGER, BIGINT, REAL, DOUBLE PRECISION, NUMERIC, DECIMAL, MONEY, SERIAL, BIGSERIAL, SMALLSERIAL |
| Character | 4 | CHAR, VARCHAR, TEXT, NAME |
| Binary | 1 | BYTEA |
| Boolean | 1 | BOOLEAN |
| Date/Time | 6 | DATE, TIME, TIMETZ, TIMESTAMP, TIMESTAMPTZ, INTERVAL |
| JSON | 3 | JSON, JSONB, JSONPATH |
| Network | 4 | INET, CIDR, MACADDR, MACADDR8 |
| Geometric | 7 | POINT, LINE, LSEG, BOX, PATH, POLYGON, CIRCLE |
| Range | 6 | INT4RANGE, INT8RANGE, NUMRANGE, TSRANGE, TSTZRANGE, DATERANGE |
| Full-Text Search | 2 | TSVECTOR, TSQUERY |
| Vector | 1 | VECTOR |
| Bit String | 2 | BIT, VARBIT |
| UUID | 1 | UUID |
| XML | 1 | XML |
| OID/Identifier | 11 | OID, REGCLASS, REGTYPE, REGPROC, etc. |
| Arrays | Unlimited | ANYTYPE[] |
| ENUM | User-defined | CREATE TYPE ... AS ENUM |

**Total: 97+ distinct PostgreSQL data types supported**
