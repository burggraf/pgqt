# Trigger Support in PGQT

PGQT supports PostgreSQL-compatible triggers, allowing you to execute PL/pgSQL logic automatically in response to `INSERT`, `UPDATE`, or `DELETE` operations on your SQLite tables.

## Supported Trigger Types

- **BEFORE Triggers**: Run before the data is modified. Can modify the `NEW` row or return `NULL` to abort the operation.
- **AFTER Triggers**: Run after the operation completes. Cannot modify the data.
- **FOR EACH ROW**: Triggers fire once for every row affected by the SQL statement. Multi-row updates and deletes correctly fire triggers for each row.

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

### Date/Time Functions
- `NOW()` - Current timestamp
- `CURRENT_TIMESTAMP` - Current timestamp (alias for NOW())
- `CURRENT_DATE` - Current date
- `CURRENT_TIME` - Current time
- `DATE_TRUNC(field, timestamp)` - Truncate timestamp to specified precision
- `AGE(timestamp)` - Calculate age from timestamp

### String Functions
- `LOWER(string)` - Convert to lowercase
- `UPPER(string)` - Convert to uppercase
- `LENGTH(string)` - Get string length
- `TRIM(string)` - Remove leading/trailing whitespace
- `SUBSTRING(string, start, length)` - Extract substring
- `REPLACE(string, from, to)` - Replace occurrences of substring

### Math Functions
- `ABS(x)` - Absolute value
- `ROUND(x)` - Round to nearest integer
- `CEIL(x)` - Ceiling (smallest integer >= x)
- `FLOOR(x)` - Floor (largest integer <= x)
- `GREATEST(a, b, ...)` - Return largest argument
- `LEAST(a, b, ...)` - Return smallest argument

### Logic Functions
- `COALESCE(a, b, ...)` - Return first non-null argument
- `NULLIF(a, b)` - Return NULL if a = b, else return a

## Example Triggers

### 1. Automatic Timestamps (BEFORE INSERT)

Automatically set `created_at` timestamp when a row is inserted:

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

### 2. Validation Trigger (BEFORE INSERT/UPDATE)

Validate data before allowing insertion:

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

### 3. Audit Trigger (AFTER UPDATE)

Log changes to an audit table:

```sql
CREATE FUNCTION log_changes() RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO audit_log (table_name, action, old_data, new_data, changed_at)
    VALUES (TG_TABLE_NAME, TG_OP, OLD::text, NEW::text, NOW());
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audit
AFTER UPDATE ON customers
FOR EACH ROW EXECUTE FUNCTION log_changes();
```

### 4. Data Normalization (BEFORE INSERT/UPDATE)

Normalize data before storage:

```sql
CREATE FUNCTION normalize_email() RETURNS TRIGGER AS $$
BEGIN
    NEW.email = LOWER(TRIM(NEW.email));
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_normalize_email
BEFORE INSERT OR UPDATE ON users
FOR EACH ROW EXECUTE FUNCTION normalize_email();
```

### 5. Soft Delete (BEFORE DELETE)

Implement soft deletes by converting DELETE to UPDATE:

```sql
CREATE FUNCTION soft_delete() RETURNS TRIGGER AS $$
BEGIN
    UPDATE users SET deleted_at = NOW(), is_active = false
    WHERE id = OLD.id;
    RETURN NULL; -- Prevent actual deletion
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_soft_delete
BEFORE DELETE ON users
FOR EACH ROW EXECUTE FUNCTION soft_delete();
```

### 6. Aborting Operations

A `BEFORE` trigger that returns `NULL` will prevent the operation from proceeding for that specific row:

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

### 7. Default Values with Logic

Set default values based on conditions:

```sql
CREATE FUNCTION set_defaults() RETURNS TRIGGER AS $$
BEGIN
    NEW.status = COALESCE(NEW.status, 'pending');
    NEW.created_at = COALESCE(NEW.created_at, NOW());
    NEW.priority = GREATEST(COALESCE(NEW.priority, 0), 0);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_set_defaults
BEFORE INSERT ON tickets
FOR EACH ROW EXECUTE FUNCTION set_defaults();
```

## Creating Triggers

### Basic Syntax

```sql
CREATE TRIGGER trigger_name
{BEFORE | AFTER | INSTEAD OF} {INSERT | UPDATE | DELETE}
ON table_name
[FOR EACH ROW]
EXECUTE FUNCTION function_name();
```

### Multiple Events

```sql
CREATE TRIGGER trg_validate
BEFORE INSERT OR UPDATE ON products
FOR EACH ROW EXECUTE FUNCTION validate_product();
```

### Drop Trigger

```sql
DROP TRIGGER IF EXISTS trigger_name ON table_name;
```

## Known Limitations

- **FOR EACH STATEMENT**: Statement-level triggers are not yet supported. Only `FOR EACH ROW` triggers work.
- **Complex WHERE Clauses**: Triggers with complex WHERE clause deparsing for OLD row fetching have limited support.
- **Multi-Row Operations**: While triggers fire for each row, the implementation may have edge cases with very large multi-row operations.
- **DML within Triggers**: Using `INSERT/UPDATE/DELETE` inside a trigger function (recursive DML) has limited support.
- **INSTEAD OF Triggers**: Only supported on views (not yet implemented).
- **Trigger Arguments**: Trigger function arguments via `TG_ARGV` are supported but have limited testing.

## Error Handling

Triggers can raise exceptions to abort operations:

```sql
RAISE EXCEPTION 'Error message';
```

The exception will be propagated to the client with the error message.

## Testing Triggers

Test your triggers with simple SQL statements:

```sql
-- Test INSERT trigger
INSERT INTO products (name, price) VALUES ('Test', -10);  -- Should fail validation

-- Test UPDATE trigger
UPDATE users SET email = '  TEST@EXAMPLE.COM  ' WHERE id = 1;  -- Should normalize

-- Test DELETE trigger
DELETE FROM users WHERE username = 'admin';  -- Should be prevented
```

## Performance Considerations

- Triggers add overhead to DML operations. Each row affected will execute the trigger function.
- BEFORE triggers that modify the NEW row require rebuilding the SQL statement, which adds slight overhead.
- Complex trigger logic is transpiled to Lua for execution, which may have performance implications for high-volume operations.

## See Also

- [PL/pgSQL Documentation](./FUNCTIONS.md) - For more details on writing PL/pgSQL functions
- [README.md](../README.md) - General PGQT documentation
