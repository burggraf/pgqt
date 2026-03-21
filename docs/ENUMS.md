# Enum Support in PGQT

PGQT supports PostgreSQL enum types by transpiling them to SQLite `TEXT` columns with `CHECK` constraints.

## Usage

You can create an enum type using the standard PostgreSQL syntax:

```sql
CREATE TYPE status AS ENUM ('open', 'closed', 'in_progress');
```

When you use this type in a table definition:

```sql
CREATE TABLE tasks (
    id SERIAL PRIMARY KEY,
    task_status status
);
```

PGQT transpiles it to:

```sql
CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_status TEXT CHECK (task_status IN ('open', 'closed', 'in_progress'))
);
```

## Metadata

Enum values are stored in the shadow catalog:
- `__pg_type__` stores the enum type with `typtype = 'e'`.
- `__pg_enum__` stores the individual labels and their sort order.

The `pg_enum` system view is also provided for compatibility with introspection tools.

## Catalog Tables

Enum metadata is stored in two shadow catalog tables:

### `__pg_type__`

Stores the enum type definition with `typtype = 'e'`:

```sql
SELECT typname, typtype FROM __pg_type__ WHERE typtype = 'e';
-- Returns: 'status', 'e'
```

### `__pg_enum__`

Stores individual enum labels with their sort order:

| Column      | Description                     |
|-------------|---------------------------------|
| `enumtypid` | OID of the enum type            |
| `enumsortorder` | Sort order (1, 2, 3, ...)    |
| `enumlabel` | The enum value label            |

```sql
SELECT enumlabel, enumsortorder
FROM __pg_enum__
WHERE enumtypid = (SELECT oid FROM __pg_type__ WHERE typname = 'status')
ORDER BY enumsortorder;
-- Returns: 'open', 1 | 'closed', 2 | 'in_progress', 3
```

## Limitations

- **No type safety at runtime**: SQLite stores enums as TEXT, so any string value can be inserted. The CHECK constraint provides validation, but it's not enforced at the type level.
- **ALTER TYPE not supported**: Adding or removing enum values after creation is not currently supported.
- **No enum functions**: PostgreSQL functions like `enum_range()`, `enum_first()`, and `enum_last()` are not implemented.
- **Case sensitivity**: Enum labels are case-sensitive. `'Open'` and `'open'` are different values.
- **No schema-qualified enums**: Enums are currently stored in the default schema only.
- **CHECK constraint only on new tables**: Adding an enum column to an existing table via ALTER TABLE may not automatically generate the CHECK constraint.

## PostgreSQL Compatibility

| Feature                    | PostgreSQL | PGQT | Notes                                      |
|---------------------------|------------|------|--------------------------------------------|
| CREATE TYPE ... AS ENUM   | ✅          | ✅    | Full syntax support                        |
| CHECK constraint          | N/A        | ✅    | PGQT-specific enforcement                  |
| pg_enum catalog           | ✅          | ✅    | Compatible view                            |
| ALTER TYPE ... ADD VALUE  | ✅          | ❌    | Not supported                              |
| enum_range() function     | ✅          | ❌    | Not implemented                            |
| enum_first/last()         | ✅          | ❌    | Not implemented                            |
| Schema-qualified enums    | ✅          | ❌    | Default schema only                        |
