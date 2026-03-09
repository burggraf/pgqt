-- =====================================================
-- RBAC (Role-Based Access Control) Test Script
-- Run this after test_auth_setup.sql
-- This demonstrates grant/revoke functionality
-- =====================================================

\echo ''
\echo '====================================================='
\echo 'RBAC TEST: Grant and Revoke Demonstration'
\echo '====================================================='
\echo ''

-- =====================================================
-- TEST 1: Grant SELECT on public tables
-- =====================================================
\echo 'TEST 1: Granting SELECT on public_info to read_only_user'
GRANT SELECT ON public.public_info TO read_only_user;

-- Verify grant
\echo 'Permissions on public_info:'
SELECT 
    relname as table_name,
    relacl::text as permissions
FROM pg_class 
WHERE relname = 'public_info';

-- =====================================================
-- TEST 2: Grant multiple privileges
-- =====================================================
\echo ''
\echo 'TEST 2: Granting SELECT, INSERT, UPDATE on orders to app_user'
GRANT SELECT, INSERT, UPDATE ON app_data.orders TO app_user;
GRANT SELECT ON app_data.products TO app_user;
GRANT USAGE ON SCHEMA app_data TO app_user;

-- Also grant sequence usage for the SERIAL column
GRANT USAGE, SELECT ON SEQUENCE app_data.orders_id_seq TO app_user;

\echo 'Granted app_user access to app_data schema and tables'

-- =====================================================
-- TEST 3: Grant ALL on sensitive data to admin only
-- =====================================================
\echo ''
\echo 'TEST 3: Granting ALL privileges on sensitive_data to admin_user'
GRANT ALL ON public.sensitive_data TO admin_user;

-- Note: Grant usage on the sequence too
GRANT USAGE, SELECT ON SEQUENCE public.sensitive_data_id_seq TO admin_user;

\echo 'Granted admin_user full access to sensitive_data'

-- =====================================================
-- TEST 4: Test revoking privileges
-- =====================================================
\echo ''
\echo 'TEST 4: Revoking UPDATE from app_user on orders'
-- First verify app_user has UPDATE
SELECT 
    'Before revoke - checking app_user privileges' as status;

-- Revoke UPDATE
REVOKE UPDATE ON app_data.orders FROM app_user;

\echo 'Revoked UPDATE privilege from app_user'

-- =====================================================
-- TEST 5: Grant then revoke all
-- =====================================================
\echo ''
\echo 'TEST 5: Grant ALL to read_only_user, then revoke all'
-- Temporarily grant all
GRANT ALL ON public.public_info TO read_only_user;
\echo 'Granted ALL to read_only_user (temporarily)'

-- Now revoke all
REVOKE ALL ON public.public_info FROM read_only_user;
-- Re-grant just SELECT
GRANT SELECT ON public.public_info TO read_only_user;
\echo 'Revoked all, re-granted only SELECT'

-- =====================================================
-- TEST 6: Show current permissions summary
-- =====================================================
\echo ''
\echo '====================================================='
\echo 'CURRENT PERMISSIONS SUMMARY'
\echo '====================================================='

-- Query to show grants from __pg_acl__
\echo ''
\echo 'Table-level permissions:'
SELECT 
    relname as table_name,
    relacl::text as raw_acl,
    CASE 
        WHEN relacl IS NULL THEN 'No explicit grants'
        ELSE relacl::text
    END as permissions
FROM pg_class c
JOIN pg_namespace n ON c.relnamespace = n.oid
WHERE relname IN ('sensitive_data', 'public_info', 'orders', 'products')
AND n.nspname IN ('public', 'app_data')
ORDER BY n.nspname, relname;

-- =====================================================
-- TEST 7: Demonstrate ALTER USER to change password
-- =====================================================
\echo ''
\echo 'TEST 7: Changing app_user password with ALTER USER'
ALTER USER app_user WITH PASSWORD 'new_secret_password_2024';

-- Verify password was changed (should show new hash)
SELECT 
    rolname,
    substring(rolpassword from 1 for 10) || '...' as password_hash_preview
FROM __pg_authid__
WHERE rolname = 'app_user';

\echo 'Password changed for app_user'

-- =====================================================
-- TEST 8: Create a role and grant to another user
-- =====================================================
\echo ''
\echo 'TEST 8: Role inheritance test'
-- Create a role (not a login user)
DROP ROLE IF EXISTS data_reader;
CREATE ROLE data_reader;

-- Grant privileges to the role
GRANT SELECT ON ALL TABLES IN SCHEMA public TO data_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA app_data TO data_reader;
GRANT USAGE ON SCHEMA app_data TO data_reader;

-- Grant the role to a user
GRANT data_reader TO read_only_user;

\echo 'Created data_reader role and granted to read_only_user'

-- Show role membership
\echo ''
\echo 'Role memberships:'
SELECT 
    r.rolname as member,
    m.rolname as role_granted
FROM pg_auth_members am
JOIN pg_roles r ON am.member = r.oid
JOIN pg_roles m ON am.roleid = m.oid
WHERE m.rolname = 'data_reader';

-- =====================================================
-- TEST 9: Set default privileges
-- =====================================================
\echo ''
\echo 'TEST 9: Setting default privileges'
-- Make it so new tables in app_data are automatically accessible to app_user
ALTER DEFAULT PRIVILEGES IN SCHEMA app_data 
GRANT SELECT, INSERT, UPDATE ON TABLES TO app_user;

\echo 'Set default privileges for future tables in app_data'

-- Create a new table to test default privileges
CREATE TABLE app_data.new_table_test (id SERIAL PRIMARY KEY, data TEXT);
INSERT INTO app_data.new_table_test (data) VALUES ('test data');

\echo 'Created app_data.new_table_test - app_user should have access via default privileges'

-- =====================================================
-- Summary
-- =====================================================
\echo ''
\echo '====================================================='
\echo 'RBAC TEST COMPLETE'
\echo '====================================================='
\echo ''
\echo 'Summary of what was configured:'
\echo ''
\echo 'Users created:'
\echo '  - app_user: Has SELECT, INSERT on orders; SELECT on products'
\echo '  - read_only_user: Has SELECT on public_info, inherits data_reader role'
\echo '  - admin_user: Has ALL on sensitive_data'
\echo '  - no_pass_user: No password, can login without one'
\echo ''
\echo 'Roles created:'
\echo '  - data_reader: Has SELECT on all tables'
\echo ''
\echo 'Tables created:'
\echo '  - public.sensitive_data: Admin only'
\echo '  - public.public_info: Read-only access'
\echo '  - app_data.orders: App user can read and insert'
\echo '  - app_data.products: App user can read'
\echo '  - app_data.new_table_test: Tests default privileges'
\echo ''
\echo 'Next steps:'
\echo '  1. Connect as different users to test access:'
\echo '     psql -h 127.0.0.1 -p 5432 -U app_user -d postgres'
\echo '     (password: new_secret_password_2024)'
\echo ''
\echo '  2. Try queries to verify permissions:'
\echo '     SELECT * FROM app_data.orders;  -- should work'
\echo '     DELETE FROM app_data.orders;    -- should fail'
\echo '     SELECT * FROM public.sensitive_data; -- should fail'
\echo ''
\echo '  3. Connect as admin_user to access sensitive data:'
\echo '     psql -h 127.0.0.1 -p 5432 -U admin_user -d postgres'
\echo '     (password: admin789!)'
\echo ''
\echo '  4. Test no_pass_user (no password required):'
\echo '     psql -h 127.0.0.1 -p 5432 -U no_pass_user -d postgres'
\echo '     (when prompted for password, just press Enter)'
\echo ''