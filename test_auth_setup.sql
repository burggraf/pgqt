-- =====================================================
-- Password Authentication & RBAC Test Setup Script
-- Run this first as the postgres superuser
-- =====================================================

-- Clean up any existing test users (ignore errors if they don't exist)
DROP USER IF EXISTS app_user;
DROP USER IF EXISTS read_only_user;
DROP USER IF EXISTS admin_user;
DROP USER IF EXISTS no_pass_user;

-- Create test users with different configurations

-- 1. Regular application user with password
CREATE USER app_user WITH LOGIN PASSWORD 'app_secret123';

-- 2. Read-only user with password
CREATE USER read_only_user WITH LOGIN PASSWORD 'readonly456';

-- 3. Admin user with password
CREATE USER admin_user WITH LOGIN PASSWORD 'admin789!';

-- 4. User without password (for comparison)
CREATE USER no_pass_user WITH LOGIN;

-- Verify users were created with password hashes
SELECT 
    rolname,
    rolcanlogin,
    CASE 
        WHEN rolpassword IS NULL THEN 'NO PASSWORD'
        WHEN rolpassword = '' THEN 'EMPTY PASSWORD'
        WHEN rolpassword LIKE 'md5%' THEN 'MD5 HASHED'
        ELSE 'PLAINTEXT: ' || substring(rolpassword from 1 for 20) || '...'
    END as password_status,
    substring(rolpassword from 1 for 40) as password_preview
FROM __pg_authid__ 
WHERE rolname IN ('app_user', 'read_only_user', 'admin_user', 'no_pass_user')
ORDER BY rolname;

-- =====================================================
-- Create test tables for RBAC testing
-- =====================================================

-- Drop existing test tables
DROP TABLE IF EXISTS public.sensitive_data CASCADE;
DROP TABLE IF EXISTS public.public_info CASCADE;
DROP TABLE IF EXISTS app_data.orders CASCADE;
DROP TABLE IF EXISTS app_data.products CASCADE;

-- Create test schema
DROP SCHEMA IF EXISTS app_data CASCADE;
CREATE SCHEMA app_data;

-- 1. Table with sensitive financial data
CREATE TABLE public.sensitive_data (
    id SERIAL PRIMARY KEY,
    account_number VARCHAR(50),
    ssn VARCHAR(11),
    salary DECIMAL(10,2),
    notes TEXT
);

INSERT INTO public.sensitive_data (account_number, ssn, salary, notes) VALUES
('ACC-001', '123-45-6789', 75000.00, 'Confidential employee record'),
('ACC-002', '987-65-4321', 82000.00, 'Manager salary'),
('ACC-003', '456-78-9012', 65000.00, 'Regular staff');

-- 2. Public information table
CREATE TABLE public.public_info (
    id SERIAL PRIMARY KEY,
    title VARCHAR(100),
    description TEXT,
    published_date DATE
);

INSERT INTO public.public_info (title, description, published_date) VALUES
('Company News', 'Quarterly earnings report', '2024-01-15'),
('Product Launch', 'New feature announcement', '2024-02-01'),
('Holiday Schedule', 'Office closures for holidays', '2024-03-01');

-- 3. Orders table in app_data schema
CREATE TABLE app_data.orders (
    id SERIAL PRIMARY KEY,
    customer_name VARCHAR(100),
    order_total DECIMAL(10,2),
    order_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    status VARCHAR(20)
);

INSERT INTO app_data.orders (customer_name, order_total, status) VALUES
('Acme Corp', 5000.00, 'pending'),
('TechStart Inc', 12500.50, 'completed'),
('Global Systems', 8750.25, 'processing');

-- 4. Products table in app_data schema
CREATE TABLE app_data.products (
    id SERIAL PRIMARY KEY,
    sku VARCHAR(50) UNIQUE,
    name VARCHAR(100),
    price DECIMAL(10,2),
    stock_quantity INTEGER
);

INSERT INTO app_data.products (sku, name, price, stock_quantity) VALUES
('SKU-001', 'Professional Widget', 299.99, 150),
('SKU-002', 'Deluxe Gadget', 499.99, 75),
('SKU-003', 'Basic Tool', 49.99, 500);

-- Show what we created
\echo ''
\echo '=== Test Objects Created ==='
\echo ''
\echo 'Users:'
SELECT rolname FROM pg_roles WHERE rolname LIKE '%_user' ORDER BY rolname;

\echo ''
\echo 'Tables:'
SELECT schemaname, tablename, 'public.sensitive_data - sensitive financial data' as description
FROM pg_tables WHERE tablename = 'sensitive_data'
UNION ALL
SELECT schemaname, tablename, 'public.public_info - public information'
FROM pg_tables WHERE tablename = 'public_info'
UNION ALL
SELECT schemaname, tablename, 'app_data.orders - order data'
FROM pg_tables WHERE tablename = 'orders' AND schemaname = 'app_data'
UNION ALL
SELECT schemaname, tablename, 'app_data.products - product catalog'
FROM pg_tables WHERE tablename = 'products' AND schemaname = 'app_data';

\echo ''
\echo 'Setup complete! Now run test_auth_rbac.sql to test permissions.'