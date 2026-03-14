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
