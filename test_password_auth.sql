-- =====================================================
-- Password Authentication Test Script
-- Run this to verify password authentication is working
-- =====================================================

\echo ''
\echo '====================================================='
\echo 'PASSWORD AUTHENTICATION TEST'
\echo '====================================================='
\echo ''
\echo 'This script tests the password authentication system.'
\echo 'Run these connection tests from your shell:'
\echo ''

-- Show current users and their password status
\echo 'Current users in the system:'
SELECT 
    rolname as username,
    rolcanlogin as can_login,
    CASE 
        WHEN rolpassword IS NULL THEN 'NO PASSWORD (NULL)'
        WHEN rolpassword = '' THEN 'EMPTY PASSWORD'
        WHEN rolpassword LIKE 'md5%' THEN 'MD5 HASH: ' || substring(rolpassword from 1 for 15) || '...'
        ELSE 'PLAINTEXT'
    END as password_status
FROM __pg_authid__ 
WHERE rolname NOT LIKE 'pg_%'
  AND rolname NOT IN ('postgres', 'rds_superuser', 'rds_replication', 'rds_password')
ORDER BY 
    CASE WHEN rolname = 'postgres' THEN 0 ELSE 1 END,
    rolname;

\echo ''
\echo '====================================================='
\echo 'TEST CASES TO RUN FROM SHELL'
\echo '====================================================='
\echo ''

-- Create test users if they don't exist
DO $$
BEGIN
    -- Create test user with password
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'test_pw_user') THEN
        CREATE USER test_pw_user WITH LOGIN PASSWORD 'correct_password';
        RAISE NOTICE 'Created test_pw_user with password';
    END IF;
    
    -- Create test user without password
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'test_no_pw') THEN
        CREATE USER test_no_pw WITH LOGIN;
        RAISE NOTICE 'Created test_no_pw without password';
    END IF;
END $$;

\echo ''
\echo '--- TEST 1: Correct Password ---'
\echo 'Command: psql -h 127.0.0.1 -p 5432 -U test_pw_user -d postgres'
\echo 'Password: correct_password'
\echo 'Expected: SUCCESS - Should connect and get postgres prompt'
\echo ''

\echo '--- TEST 2: Wrong Password ---'
\echo 'Command: psql -h 127.0.0.1 -p 5432 -U test_pw_user -d postgres'
\echo 'Password: wrong_password'
\echo 'Expected: FAILURE - Should get "password authentication failed" error'
\echo ''

\echo '--- TEST 3: No Password (when user has one) ---'
\echo 'Command: psql -h 127.0.0.1 -p 5432 -U test_pw_user -d postgres'
\echo 'Password: (just press Enter)'
\echo 'Expected: FAILURE - Should get "no password supplied" error'
\echo ''

\echo '--- TEST 4: No Password (when user has no password) ---'
\echo 'Command: psql -h 127.0.0.1 -p 5432 -U test_no_pw -d postgres'
\echo 'Password: (just press Enter)'
\echo 'Expected: SUCCESS - Should connect (user has no password)'
\echo ''

\echo '--- TEST 5: Non-existent User ---'
\echo 'Command: psql -h 127.0.0.1 -p 5432 -U nonexistent_user -d postgres'
\echo 'Password: anything'
\echo 'Expected: SUCCESS - Auto-creates user and connects'
\echo ''

\echo '--- TEST 6: ALTER USER to change password ---'
\echo 'SQL: ALTER USER test_pw_user WITH PASSWORD ''new_password_123'';'
\echo 'Then try connecting with:'
\echo '  psql -h 127.0.0.1 -p 5432 -U test_pw_user -d postgres'
\echo '  Password: new_password_123 (should work)'
\echo '  Password: correct_password (should fail - old password)'
\echo ''

\echo '====================================================='
\echo 'VERIFICATION QUERIES'
\echo '====================================================='
\echo ''

-- Show the test users
\echo 'Test users created:'
SELECT 
    rolname,
    CASE 
        WHEN rolpassword IS NULL THEN 'NO PASSWORD'
        WHEN rolpassword LIKE 'md5%' THEN 'MD5 HASHED'
        ELSE 'OTHER'
    END as password_type
FROM __pg_authid__ 
WHERE rolname IN ('test_pw_user', 'test_no_pw')
ORDER BY rolname;

\echo ''
\echo '====================================================='
\echo 'HASH VERIFICATION'
\echo '====================================================='
\echo ''
\echo 'The password hash is calculated as: MD5(password + username)'
\echo ''
\echo 'For test_pw_user with password "correct_password":'
\echo 'Expected hash: md5<hex_of(correct_password + test_pw_user)>'
\echo ''

-- Show the actual hash stored
SELECT 
    rolname,
    rolpassword as stored_hash
FROM __pg_authid__ 
WHERE rolname = 'test_pw_user';

\echo ''
\echo 'To verify this is correct, you can compute:'
\echo '  echo -n "correct_password" | md5sum'
\echo '  (PostgreSQL actually does: MD5(password + username))'
\echo ''

-- Create a simple table for testing
CREATE TABLE IF NOT EXISTS auth_test_table (
    id SERIAL PRIMARY KEY,
    username VARCHAR(50),
    test_data TEXT
);

-- Grant access to test users
GRANT SELECT, INSERT ON auth_test_table TO test_pw_user;
GRANT USAGE, SELECT ON SEQUENCE auth_test_table_id_seq TO test_pw_user;
GRANT SELECT ON auth_test_table TO test_no_pw;

INSERT INTO auth_test_table (username, test_data) 
SELECT 'postgres', 'Test data from setup'
WHERE NOT EXISTS (SELECT 1 FROM auth_test_table);

\echo ''
\echo 'Created auth_test_table for testing access'
\echo ''
\echo '====================================================='
\echo 'QUICK SHELL TEST SCRIPT'
\echo '====================================================='
\echo ''
\echo 'Copy and paste this into your shell:'
\echo ''
\echo '# Test 1: Connect with correct password'
\echo 'psql "postgresql://test_pw_user:correct_password@127.0.0.1:5432/postgres" -c "SELECT current_user;"'
\echo ''
\echo '# Test 2: Try wrong password (should fail)'
\echo 'psql "postgresql://test_pw_user:wrong_password@127.0.0.1:5432/postgres" -c "SELECT current_user;" 2>&1 || echo "Expected failure"'
\echo ''
\echo '# Test 3: Connect as no-pass user'
\echo 'psql "postgresql://test_no_pw:@127.0.0.1:5432/postgres" -c "SELECT current_user;"'
\echo ''
\echo '# Test 4: Connect as postgres (default superuser)'
\echo 'psql "postgresql://postgres:postgres@127.0.0.1:5432/postgres" -c "SELECT current_user;"'
\echo ''