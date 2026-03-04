-- Test SHOW search_path
CREATE SCHEMA test_show_schema1;
CREATE SCHEMA test_show_schema2;
SET search_path TO test_show_schema1, public;
SHOW search_path;
DROP SCHEMA test_show_schema1;
DROP SCHEMA test_show_schema2;

-- Test SHOW server_version
SHOW server_version;

-- Test SHOW ALL
SHOW ALL;

-- Test SHOW with timezone
SHOW timezone;

-- Test SHOW with transaction isolation
SHOW transaction_isolation_level;

-- Test SHOW with default_transaction_read_only
SHOW default_transaction_read_only;

-- Test SHOW with statement_timeout
SHOW statement_timeout;

-- Test SHOW with client_encoding
SHOW client_encoding;

-- Test SHOW with application_name
SHOW application_name;

-- Test SHOW with DateStyle
SHOW DateStyle;

-- Clean up
RESET search_path;
