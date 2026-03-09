-- =====================================================
-- Simple Password Authentication Test
-- =====================================================

-- Drop test users if they exist
DROP USER IF EXISTS test_pw_user;
DROP USER IF EXISTS test_no_pw;

-- Create test user with password
CREATE USER test_pw_user WITH LOGIN PASSWORD 'correct_password';

-- Create test user without password
CREATE USER test_no_pw WITH LOGIN;

-- Create test table
CREATE TABLE IF NOT EXISTS auth_test_table (
    id SERIAL PRIMARY KEY,
    username VARCHAR(50),
    test_data TEXT
);

-- Grant access
GRANT SELECT, INSERT ON auth_test_table TO test_pw_user;
GRANT USAGE, SELECT ON SEQUENCE auth_test_table_id_seq TO test_pw_user;
GRANT SELECT ON auth_test_table TO test_no_pw;

-- Insert test data
INSERT INTO auth_test_table (username, test_data) 
VALUES ('postgres', 'Test data');

-- Verify users were created
SELECT 
    rolname as username,
    CASE 
        WHEN rolpassword IS NULL THEN 'NO PASSWORD'
        WHEN rolpassword LIKE 'md5%' THEN 'MD5 HASHED'
        ELSE 'OTHER'
    END as password_type,
    substring(rolpassword from 1 for 15) as hash_preview
FROM __pg_authid__ 
WHERE rolname IN ('test_pw_user', 'test_no_pw')
ORDER BY rolname;

\echo ''
\echo '====================================================='
\echo 'USERS CREATED SUCCESSFULLY'
\echo '====================================================='
\echo ''
\echo 'Now test with these commands:'
\echo ''
\echo '1. Connect with correct password (should succeed):'
\echo '   psql -h 127.0.0.1 -p 5432 -U test_pw_user -d postgres'
\echo '   Password: correct_password'
\echo ''
\echo '2. Connect with wrong password (should fail):'
\echo '   psql -h 127.0.0.1 -p 5432 -U test_pw_user -d postgres'
\echo '   Password: wrong_password'
\echo ''
\echo '3. Connect as no-pass user with empty password (should succeed):'
\echo '   psql -h 127.0.0.1 -p 5432 -U test_no_pw -d postgres'
\echo '   Password: (just press Enter)'
\echo ''
\echo '4. URL format test (should succeed):'
\echo '   psql "postgresql://test_pw_user:correct_password@127.0.0.1:5432/postgres" -c "SELECT current_user;"'
\echo ''