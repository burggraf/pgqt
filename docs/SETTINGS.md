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

PGQT provides sensible defaults for many PostgreSQL-specific parameters to ensure compatibility with drivers and ORMs (e.g., `server_version`, `client_encoding`, `standard_conforming_strings`).
