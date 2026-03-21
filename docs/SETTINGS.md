# Session Configuration in PGQT

PGQT supports session-level configuration parameters through the `SET` command and `set_config` function.

## Supported Commands

### SET

```sql
SET search_path TO myschema, public;
SET TimeZone = 'UTC';
```

### set_config

```sql
SELECT set_config('application_name', 'myapp', false);
```

## Parameter Persistence

Configuration parameters are stored in the `SessionContext` for each client connection. They persist for the duration of the session.

## Introspection

You can view settings using:

- `SHOW ALL`
- `SHOW <parameter_name>`
- `SELECT current_setting('<parameter_name>')`
- `SELECT * FROM pg_settings`

## Default Parameters

PGQT provides sensible defaults for many PostgreSQL-specific parameters to ensure compatibility with drivers and ORMs:

| Parameter                      | Default Value          | Description                                      |
|-------------------------------|------------------------|--------------------------------------------------|
| `server_version`              | `17.0`                 | PostgreSQL server version reported to clients     |
| `client_encoding`             | `UTF8`                 | Client character encoding                        |
| `standard_conforming_strings` | `on`                   | Backslashes treated literally in strings         |
| `TimeZone`                    | `UTC`                  | Session timezone                                 |
| `application_name`            | `""` (empty)           | Client application name                          |
| `search_path`                 | `public`               | Schema search path                               |
| `DateStyle`                   | `ISO, MDY`             | Date format style                                |
| `intervalstyle`               | `postgres`             | Interval output format                           |
| `extra_float_digits`          | `1`                    | Extra digits for float output                    |
| `integer_datetimes`           | `on`                   | 64-bit integer timestamps                        |
| `server_encoding`             | `UTF8`                 | Server character encoding                        |

## Common Settings Examples

### Timezone Configuration

```sql
-- Set timezone for the session
SET TimeZone = 'America/New_York';

-- Or using set_config
SELECT set_config('TimeZone', 'Europe/London', false);

-- Verify the setting
SHOW TimeZone;
```

### Application Name

```sql
-- Set application name for debugging/monitoring
SET application_name = 'my-app-worker-1';

-- Useful for identifying connections in logs
SELECT current_setting('application_name');
```

### Search Path

```sql
-- Set schema search order
SET search_path TO myschema, public;

-- Verify
SHOW search_path;
```

## pg_settings View

PGQT provides the `pg_settings` view for introspection:

```sql
SELECT name, setting, source
FROM pg_settings
WHERE name IN ('server_version', 'TimeZone', 'search_path');
```

## Limitations

- **No PERSISTENT settings**: Settings are per-session only. They reset when the connection closes.
- **Limited validation**: Some settings accept any value without validation.
- **No ALTER SYSTEM**: The `ALTER SYSTEM` command for server-wide configuration is not supported.
- **No role-specific defaults**: `ALTER ROLE ... SET` is not supported.

## PostgreSQL Compatibility

| Feature                    | PostgreSQL | PGQT | Notes                                      |
|---------------------------|------------|------|--------------------------------------------|
| SET command               | ✅          | ✅    | Per-session settings                       |
| SHOW command              | ✅          | ✅    | Including SHOW ALL                         |
| set_config()              | ✅          | ✅    | Third parameter (is_local) accepted but ignored |
| current_setting()         | ✅          | ✅    | Returns setting or default                 |
| pg_settings view          | ✅          | ✅    | Read-only introspection                    |
| ALTER SYSTEM              | ✅          | ❌    | Not supported                              |
| ALTER ROLE ... SET        | ✅          | ❌    | Not supported                              |
| PERSISTENT settings       | ✅          | ❌    | Session-only in PGQT                       |
