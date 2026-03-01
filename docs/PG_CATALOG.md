# PostgreSQL System Catalogs (pg_catalog)

This document describes the PostgreSQL system catalog implementation in PGlite Proxy.

## Overview

PGlite Proxy provides comprehensive PostgreSQL-compatible system catalog views that enable full ORM support including Prisma, TypeORM, Drizzle, and SQLAlchemy. The system catalogs are implemented as SQLite views that map SQLite's metadata to PostgreSQL-compatible catalog tables.

## Supported Catalog Views

### Core Catalog Tables

#### pg_class

Stores metadata about tables, indexes, views, sequences, and other relations.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Row identifier (unique object ID) |
| relname | name | Name of the relation |
| relnamespace | oid | OID of the namespace (schema) |
| reltype | oid | OID of the composite type |
| reloftype | oid | OID of the underlying type |
| relowner | oid | Owner of the relation |
| relam | oid | Access method (for indexes) |
| relfilenode | oid | File node number |
| reltablespace | oid | Tablespace OID |
| relpages | int4 | Size in pages (0 for SQLite) |
| reltuples | float4 | Number of rows (0 for SQLite) |
| relallvisible | int4 | All-visible pages (0) |
| reltoastrelid | oid | TOAST table OID (0) |
| relhasindex | bool | Has indexes |
| relisshared | bool | Shared across databases (false) |
| relpersistence | char | Persistence: p=permanent, t=temp, u=unlogged |
| relkind | char | r=table, v=view, i=index, S=sequence, m=matview |
| relnatts | int2 | Number of columns |
| relchecks | int2 | Number of check constraints |
| relhasrules | bool | Has rules (false) |
| relhastriggers | bool | Has triggers |
| relhassubclass | bool | Has inheritance children (false) |
| relrowsecurity | bool | Row security enabled |
| relforcerowsecurity | bool | Force row security |
| relispopulated | bool | Materialized view is populated (true) |
| relreplident | char | Replica identity: d=default, n=nothing, f=full, i=index |
| relispartition | bool | Is partition (false) |
| relrewrite | oid | Rewrite OID (0) |
| relfrozenxid | xid | Frozen XID (0) |
| relminmxid | xid | Min MXID (0) |
| relacl | aclitem[] | Access privileges (NULL) |
| reloptions | text[] | Options (NULL) |
| relpartbound | pg_node_tree | Partition bound (NULL) |

#### pg_attribute

Stores column (attribute) information for all relations.

| Column | Type | Description |
|--------|------|-------------|
| attrelid | oid | OID of the relation |
| attname | name | Column name |
| atttypid | oid | OID of the data type |
| attstattarget | int4 | Statistics target (-1) |
| attlen | int2 | Storage size (-1 for variable) |
| attnum | int2 | Column number (1-based) |
| attndims | int2 | Number of array dimensions |
| attcacheoff | int4 | Cache offset (-1) |
| atttypmod | int4 | Type modifier |
| attbyval | bool | Passed by value (false) |
| attstorage | char | Storage strategy: p=plain, e=external, m=main, x=extended |
| attalign | char | Alignment: c=char, s=short, i=int, d=double |
| attnotnull | bool | NOT NULL constraint |
| atthasdef | bool | Has default value |
| atthasmissing | bool | Has missing value (false) |
| attidentity | char | Identity: ''=none, a=always, d=by default |
| attgenerated | char | Generated: ''=none, s=stored, v=virtual |
| attisdropped | bool | Column is dropped (false) |
| attislocal | bool | Locally defined (true) |
| attinhcount | int2 | Inheritance count (0) |
| attcollation | oid | Collation OID (0) |
| attacl | aclitem[] | Access privileges (NULL) |
| attoptions | text[] | Options (NULL) |
| attfdwoptions | text[] | FDW options (NULL) |
| attmissingval | anyarray | Missing value (NULL) |

#### pg_type

Stores data type information. PGlite Proxy includes 100+ PostgreSQL types.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Type OID |
| typname | name | Type name |
| typnamespace | oid | Schema OID |
| typowner | oid | Owner OID |
| typlen | int2 | Storage size |
| typbyval | bool | Passed by value |
| typtype | char | Type: b=base, c=composite, d=domain, e=enum, p=pseudo, r=range |
| typcategory | char | Category: A=array, B=boolean, C=composite, D=datetime, etc. |
| typispreferred | bool | Preferred in category |
| typisdefined | bool | Fully defined |
| typdelim | char | Array delimiter |
| typrelid | oid | Composite type's class OID |
| typelem | oid | Array element type OID |
| typarray | oid | Array type OID |
| typinput | regproc | Input function |
| typoutput | regproc | Output function |
| typreceive | regproc | Receive function |
| typsend | regproc | Send function |
| typmodin | regproc | Type modifier input |
| typmodout | regproc | Type modifier output |
| typanalyze | regproc | Analyze function |
| typalign | char | Alignment |
| typstorage | char | Storage |
| typnotnull | bool | Not null |
| typbasetype | oid | Base type for domains |
| typtypmod | int4 | Type modifier |
| typndims | int4 | Array dimensions |
| typcollation | oid | Collation |
| typdefaultbin | pg_node_tree | Default expression |
| typdefault | text | Default value |
| typacl | aclitem[] | Access privileges |

**Supported Types:**

| OID | Name | Description |
|-----|------|-------------|
| 16 | bool | Boolean |
| 17 | bytea | Byte array |
| 18 | char | Single character |
| 20 | int8 | Big integer |
| 21 | int2 | Small integer |
| 23 | int4 | Integer |
| 25 | text | Variable-length text |
| 26 | oid | Object identifier |
| 114 | json | JSON |
| 600 | point | Geometric point |
| 601 | lseg | Line segment |
| 602 | path | Geometric path |
| 603 | box | Bounding box |
| 604 | polygon | Polygon |
| 628 | line | Infinite line |
| 700 | float4 | Single precision |
| 701 | float8 | Double precision |
| 718 | circle | Circle |
| 790 | money | Currency |
| 829 | macaddr | MAC address |
| 869 | inet | IP address |
| 650 | cidr | IP network |
| 774 | macaddr8 | MAC address (EUI-64) |
| 1042 | bpchar | Fixed-length char |
| 1043 | varchar | Variable-length char |
| 1082 | date | Date |
| 1083 | time | Time |
| 1114 | timestamp | Timestamp |
| 1184 | timestamptz | Timestamp with timezone |
| 1186 | interval | Time interval |
| 1266 | timetz | Time with timezone |
| 1560 | bit | Bit string |
| 1562 | varbit | Variable bit string |
| 1700 | numeric | Decimal |
| 2950 | uuid | UUID |
| 3220 | pg_lsn | PostgreSQL LSN |
| 3614 | tsvector | Text search vector |
| 3615 | tsquery | Text search query |
| 3802 | jsonb | Binary JSON |
| 4089 | regrole | Role name |
| 4090 | regnamespace | Namespace name |
| 4096 | regconfig | Text search config |
| 4097 | regdictionary | Text search dictionary |

#### pg_namespace

Stores schema (namespace) information.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Schema OID |
| nspname | name | Schema name |
| nspowner | oid | Owner OID |
| nspacl | aclitem[] | Access privileges |

Default schemas: `public`, `pg_catalog`, `information_schema`

#### pg_index

Stores index metadata.

| Column | Type | Description |
|--------|------|-------------|
| indexrelid | oid | Index class OID |
| indrelid | oid | Table class OID |
| indnatts | int2 | Number of columns |
| indnkeyatts | int2 | Number of key columns |
| indisunique | bool | Is unique |
| indnullsnotdistinct | bool | NULLs not distinct (PG15+) |
| indisprimary | bool | Is primary key |
| indisexclusion | bool | Is exclusion constraint |
| indimmediate | bool | Immediate constraint checking |
| indisclustered | bool | Table clustered on this index |
| indisvalid | bool | Index is valid |
| indcheckxmin | bool | Check xmin |
| indisready | bool | Index is ready |
| indislive | bool | Index is live |
| indisreplident | bool | Used as replica identity |
| indkey | int2vector | Column numbers |
| indcollation | oidvector | Collation OIDs |
| indclass | oidvector | Operator class OIDs |
| indoption | int2vector | Per-column flags |
| indexprs | pg_node_tree | Expression tree |
| indpred | pg_node_tree | Partial index predicate |

#### pg_constraint

Stores constraints (primary key, foreign key, unique, check).

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Constraint OID |
| conname | name | Constraint name |
| connamespace | oid | Schema OID |
| contype | char | Type: c=check, f=foreign key, p=primary key, u=unique, x=exclusion |
| condeferrable | bool | Is deferrable |
| condeferred | bool | Initially deferred |
| convalidated | bool | Is validated |
| conrelid | oid | Table OID |
| contypid | oid | Domain type OID |
| conindid | oid | Index OID |
| conparentid | oid | Parent constraint OID |
| confrelid | oid | Referenced table OID (FK) |
| confupdtype | char | FK update action |
| confdeltype | char | FK delete action |
| confmatchtype | char | FK match type |
| conislocal | bool | Is locally defined |
| coninhcount | int2 | Inheritance count |
| connoinherit | bool | Not inheritable |
| conkey | int2vector | Constrained columns |
| confkey | int2vector | Referenced columns (FK) |
| conpfeqop | oidvector | PK = FK equality operators |
| conppeqop | oidvector | PK = PK equality operators |
| conffeqop | oidvector | FK = FK equality operators |
| conexclop | oidvector | Exclusion operators |
| conbin | pg_node_tree | Check expression |

#### pg_roles / pg_authid

Stores role (user) information.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Role OID |
| rolname | name | Role name |
| rolsuper | bool | Is superuser |
| rolinherit | bool | Inherits privileges |
| rolcreaterole | bool | Can create roles |
| rolcreatedb | bool | Can create databases |
| rolcanlogin | bool | Can login |
| rolconnlimit | int4 | Connection limit (-1 = unlimited) |
| rolpassword | text | Password hash (hidden in pg_roles) |
| rolvaliduntil | timestamptz | Password expiry |
| rolreplication | bool | Can initiate replication |
| rolbypassrls | bool | Bypasses row-level security |
| rolconfig | text[] | Session defaults |

**pg_roles** is a public view of pg_authid with the password column hidden.

#### pg_database

Stores database information.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Database OID |
| datname | name | Database name |
| datdba | oid | Owner OID |
| encoding | int4 | Character encoding |
| datcollate | name | LC_COLLATE |
| datctype | name | LC_CTYPE |
| datlocprovider | char | Locale provider |
| daticulocale | text | ICU locale |
| daticurules | text | ICU rules |
| datistemplate | bool | Is template |
| datallowconn | bool | Allow connections |
| datconnlimit | int4 | Connection limit |
| datlastsysoid | oid | Last system OID |
| datfrozenxid | xid | Frozen XID |
| datminmxid | xid | Min MXID |
| dattablespace | oid | Default tablespace |
| datacl | aclitem[] | Access privileges |

#### pg_proc

Stores function/procedure information.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Function OID |
| proname | name | Function name |
| pronamespace | oid | Schema OID |
| proowner | oid | Owner OID |
| prolang | oid | Language OID |
| procost | float4 | Estimated execution cost |
| prorows | float4 | Estimated result rows |
| provariadic | oid | Variadic array type |
| prokind | char | Function kind: f=function, p=procedure, a=aggregate, w=window |
| prosecdef | bool | Security definer |
| proleakproof | bool | Leakproof |
| proisstrict | bool | Strict (NULL in → NULL out) |
| proretset | bool | Returns set |
| provolatile | char | Volatility: i=immutable, s=stable, v=volatile |
| pronargs | int2 | Number of arguments |
| pronargdefaults | int2 | Number of default arguments |
| prorettype | oid | Return type OID |
| proargtypes | oidvector | Argument types |
| proallargtypes | oid[] | All argument types (including OUT) |
| proargmodes | char[] | Argument modes: i=in, o=out, b=inout, v=variadic, t=table |
| proargnames | text[] | Argument names |
| proargdefaults | pg_node_tree | Default expressions |
| protrftypes | oid[] | Transform types |
| prosrc | text | Function source code |
| probin | text | Binary library file |
| prosqlbody | pg_node_tree | SQL body |
| proconfig | text[] | Configuration settings |
| proacl | aclitem[] | Access privileges |

#### pg_settings

Stores server settings.

| Column | Type | Description |
|--------|------|-------------|
| name | text | Setting name |
| setting | text | Current value |
| unit | text | Unit |
| category | text | Category |
| short_desc | text | Short description |
| extra_desc | text | Extra description |
| context | text | Context |
| vartype | text | Value type |
| source | text | Source |
| min_val | text | Minimum value |
| max_val | text | Maximum value |
| enumvals | text[] | Enum values |
| boot_val | text | Boot value |
| reset_val | text | Reset value |
| sourcefile | text | Source file |
| sourceline | int4 | Source line |
| pending_restart | bool | Pending restart |

### User-Friendly Views

#### pg_tables

Simplified view of tables.

| Column | Type | Description |
|--------|------|-------------|
| schemaname | name | Schema name |
| tablename | name | Table name |
| tableowner | name | Owner |
| tablespace | name | Tablespace |
| hasindexes | bool | Has indexes |
| hasrules | bool | Has rules |
| hastriggers | bool | Has triggers |
| rowsecurity | bool | Row security enabled |

#### pg_views

Simplified view of views.

| Column | Type | Description |
|--------|------|-------------|
| schemaname | name | Schema name |
| viewname | name | View name |
| viewowner | name | Owner |
| definition | text | View definition |

#### pg_indexes

Simplified view of indexes.

| Column | Type | Description |
|--------|------|-------------|
| schemaname | name | Schema name |
| tablename | name | Table name |
| indexname | name | Index name |
| tablespace | name | Tablespace |
| indexdef | text | Index definition |

#### pg_extension

Installed extensions.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Extension OID |
| extname | name | Extension name |
| extowner | oid | Owner |
| extnamespace | oid | Schema |
| extrelocatable | bool | Is relocatable |
| extversion | text | Version |
| extconfig | oid[] | Configuration tables |
| extcondition | text[] | Configuration conditions |

Default extensions shown: `plpgsql`, `uuid-ossp`, `pg_trgm`, `pgcrypto`

#### pg_enum

Enum values.

| Column | Type | Description |
|--------|------|-------------|
| oid | oid | Enum OID |
| enumtypid | oid | Type OID |
| enumsortorder | float4 | Sort order |
| enumlabel | name | Label |

## ORM Compatibility

### Prisma

Prisma introspection queries the following catalog tables:

```sql
-- Tables and columns
SELECT 
    c.relname as table_name,
    a.attname as column_name,
    t.typname as data_type,
    a.attnotnull as is_nullable,
    a.attnum as ordinal_position
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_attribute a ON a.attrelid = c.oid
JOIN pg_type t ON t.oid = a.atttypid
WHERE n.nspname = 'public'
  AND c.relkind = 'r'
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY c.relname, a.attnum;

-- Indexes
SELECT 
    i.relname as index_name,
    c.relname as table_name,
    ix.indisunique as is_unique,
    ix.indisprimary as is_primary
FROM pg_index ix
JOIN pg_class i ON i.oid = ix.indexrelid
JOIN pg_class c ON c.oid = ix.indrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'public';

-- Foreign keys
SELECT 
    con.conname as constraint_name,
    con.conrelid::regclass as table_name,
    con.confrelid::regclass as foreign_table_name
FROM pg_constraint con
JOIN pg_namespace n ON n.oid = con.connamespace
WHERE con.contype = 'f'
  AND n.nspname = 'public';
```

### TypeORM

TypeORM uses similar queries with additional metadata:

```sql
-- Column defaults
SELECT 
    a.attname as column_name,
    pg_get_expr(d.adbin, d.adrelid) as column_default
FROM pg_attrdef d
JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum
WHERE d.adrelid = 'table_name'::regclass;
```

### Drizzle

Drizzle ORM queries for drizzle-kit introspection:

```sql
-- Enums
SELECT 
    t.typname as enum_name,
    e.enumlabel as enum_value
FROM pg_type t
JOIN pg_enum e ON e.enumtypid = t.oid
JOIN pg_namespace n ON n.oid = t.typnamespace
WHERE n.nspname = 'public';
```

## Usage Examples

### Get all tables in a schema

```sql
SELECT tablename, tableowner 
FROM pg_tables 
WHERE schemaname = 'public'
ORDER BY tablename;
```

### Get columns for a table

```sql
SELECT 
    a.attname as column_name,
    t.typname as data_type,
    a.attnotnull as is_nullable,
    a.attnum as ordinal_position
FROM pg_attribute a
JOIN pg_type t ON a.atttypid = t.oid
JOIN pg_class c ON a.attrelid = c.oid
JOIN pg_namespace n ON c.relnamespace = n.oid
WHERE n.nspname = 'public'
  AND c.relname = 'my_table'
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY a.attnum;
```

### Get all indexes for a table

```sql
SELECT 
    i.relname as index_name,
    ix.indisunique as is_unique,
    ix.indisprimary as is_primary
FROM pg_index ix
JOIN pg_class i ON i.oid = ix.indexrelid
JOIN pg_class c ON c.oid = ix.indrelid
WHERE c.relname = 'my_table';
```

### Get foreign key constraints

```sql
SELECT 
    con.conname as constraint_name,
    con.confrelid::regclass as references_table,
    con.conkey as local_columns,
    con.confkey as foreign_columns
FROM pg_constraint con
WHERE con.contype = 'f'
  AND con.conrelid = 'my_table'::regclass;
```

### Get all constraints for a table

```sql
SELECT 
    conname as constraint_name,
    contype as constraint_type
FROM pg_constraint
WHERE conrelid = 'my_table'::regclass;
```

### Check if a user has a specific privilege

```sql
SELECT has_table_privilege('username', 'table_name', 'SELECT');
```

### List all roles

```sql
SELECT rolname, rolsuper, rolcreatedb, rolcreaterole
FROM pg_roles
WHERE rolname !~ '^pg_'
ORDER BY rolname;
```

### Get current database settings

```sql
SELECT name, setting 
FROM pg_settings 
WHERE name IN ('max_connections', 'server_version', 'TimeZone');
```

## Implementation Details

### Storage

The catalog views are implemented using:

1. **SQLite views** that query `sqlite_master` for table/index information
2. **Shadow tables** (`__pg_*__`) for extended metadata not available in SQLite
3. **Type mapping** from SQLite types to PostgreSQL type OIDs

### Type Mapping

SQLite types are mapped to PostgreSQL types as follows:

| SQLite Type | PostgreSQL Type | OID |
|-------------|-----------------|-----|
| INTEGER | int4 | 23 |
| REAL | float8 | 701 |
| TEXT | text | 25 |
| BLOB | bytea | 17 |
| BOOLEAN | bool | 16 |

### Limitations

1. **Statistics columns**: `reltuples`, `relpages`, etc. return 0 as SQLite doesn't maintain these statistics
2. **Type OIDs**: Based on SQLite type inference, not actual PostgreSQL type storage
3. **Some columns**: Return default values (NULL, 0, false) for data that doesn't exist in SQLite
4. **ACL arrays**: Access privilege arrays return NULL (not implemented)
5. **Expression trees**: Stored as text, not actual pg_node_tree format

## Migration from PostgreSQL

When migrating from PostgreSQL to PGlite Proxy:

1. The shadow catalog (`__pg_meta__`) preserves original PostgreSQL types
2. System catalog views provide PostgreSQL-compatible metadata
3. ORMs can introspect the schema using standard PostgreSQL queries

## See Also

- [PostgreSQL Documentation: System Catalogs](https://www.postgresql.org/docs/current/catalogs.html)
- [Prisma Introspection](https://www.prisma.io/docs/concepts/components/introspection)
- [TypeORM Documentation](https://typeorm.io/)
