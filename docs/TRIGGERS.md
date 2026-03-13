# Trigger Support in PGQT

PGQT supports PostgreSQL-compatible triggers, allowing you to execute PL/pgSQL logic automatically in response to `INSERT`, `UPDATE`, or `DELETE` operations on your SQLite tables.

## Supported Trigger Types

- **BEFORE Triggers**: Run before the data is modified. Can modify the `NEW` row or return `NULL` to abort the operation.
- **AFTER Triggers**: Run after the operation completes. Cannot modify the data.
- **FOR EACH ROW**: Triggers fire once for every row affected by the SQL statement.

## Trigger Variables

Trigger functions written in PL/pgSQL have access to special variables:

| Variable | Description |
|----------|-------------|
| `NEW` | Record holding the new database row for `INSERT`/`UPDATE` operations. |
| `OLD` | Record holding the old database row for `UPDATE`/`DELETE` operations. |
| `TG_NAME` | The name of the trigger. |
| `TG_WHEN` | `BEFORE` or `AFTER`. |
| `TG_OP` | `INSERT`, `UPDATE`, or `DELETE`. |
| `TG_TABLE_NAME` | Name of the table that caused the trigger to fire. |
| `TG_NARGS` | Number of arguments passed to the trigger function. |
| `TG_ARGV` | Array of arguments (1-indexed). |

## Supported Built-in Functions

The following PostgreSQL functions are supported within trigger logic:

- **Date/Time**: `NOW()`, `CURRENT_TIMESTAMP`, `CURRENT_DATE`, `CURRENT_TIME`.
- **Logic**: `COALESCE()`, `NULLIF()`.
- **Strings**: `LOWER()`, `UPPER()`, `LENGTH()`, `REPLACE()`.
- **Math**: `ABS()`, `ROUND()`, `CEIL()`, `FLOOR()`.

## Examples

### 1. Automatic Timestamps (BEFORE INSERT)

```sql
CREATE FUNCTION set_created_at() RETURNS TRIGGER AS $$
BEGIN
    NEW.created_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_created_at
BEFORE INSERT ON orders
FOR EACH ROW EXECUTE FUNCTION set_created_at();
```

### 2. Validation (BEFORE INSERT/UPDATE)

```sql
CREATE FUNCTION check_price() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.price < 0 THEN
        RAISE EXCEPTION 'Price cannot be negative';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_price_check
BEFORE INSERT OR UPDATE ON products
FOR EACH ROW EXECUTE FUNCTION check_price();
```

### 3. Aborting Operations

A `BEFORE` trigger that returns `NULL` will prevent the operation from proceeding for that specific row.

```sql
CREATE FUNCTION protect_admin() RETURNS TRIGGER AS $$
BEGIN
    IF OLD.username = 'admin' THEN
        RETURN NULL; -- Prevent deletion of admin
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_protect_admin
BEFORE DELETE ON users
FOR EACH ROW EXECUTE FUNCTION protect_admin();
```

## Known Limitations

- **FOR EACH STATEMENT**: Statement-level triggers are not yet supported.
- **Complex Expressions**: Triggers that involve complex subqueries in their logic may have performance overhead as they are transpiled to Lua for execution.
- **DML within Triggers**: Using `INSERT/UPDATE/DELETE` inside a trigger function (recursive DML) has limited support.
