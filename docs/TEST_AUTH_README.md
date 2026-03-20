# Password Authentication & RBAC Testing Guide

This guide helps you test the new password authentication system and RBAC (Role-Based Access Control) features in PGQT.

## Prerequisites

1. PGQT server must be running (without `--trust-mode` for password auth testing)
2. psql client installed
3. Connection to the PGQT proxy (default: localhost:5432)

## Quick Start

### 1. Start the PGQT Server (Password Auth Mode)

```bash
# Build first if needed
cargo build --release

# Run without --trust-mode to enable password authentication
./target/release/pgqt --port 5432 --database test.db
```

### 2. Run the Setup Script

Connect as the postgres superuser and run the setup:

```bash
psql -h 127.0.0.1 -p 5432 -U postgres -d postgres -f test_auth_setup.sql
```

### 3. Test Password Authentication

Run the password auth tests:

```bash
psql -h 127.0.0.1 -p 5432 -U postgres -d postgres -f test_password_auth.sql
```

### 4. Test RBAC (Grants/Revokes)

Run the RBAC tests:

```bash
psql -h 127.0.0.1 -p 5432 -U postgres -d postgres -f test_auth_rbac.sql
```

## Manual Testing Commands

### Password Authentication Tests

#### Test 1: Correct Password
```bash
psql -h 127.0.0.1 -p 5432 -U app_user -d postgres
# Password: app_secret123
# Expected: SUCCESS
```

#### Test 2: Wrong Password
```bash
psql -h 127.0.0.1 -p 5432 -U app_user -d postgres
# Password: wrong_password
# Expected: FAILURE - "password authentication failed"
```

#### Test 3: No Password (when required)
```bash
psql -h 127.0.0.1 -p 5432 -U app_user -d postgres
# Password: (just press Enter)
# Expected: FAILURE - "no password supplied"
```

#### Test 4: No Password (user has no password)
```bash
psql -h 127.0.0.1 -p 5432 -U no_pass_user -d postgres
# Password: (just press Enter)
# Expected: SUCCESS - user has no password set
```

#### Test 5: Non-existent User (Auto-created)
```bash
psql -h 127.0.0.1 -p 5432 -U brand_new_user -d postgres
# Password: anything
# Expected: SUCCESS - user auto-created with superuser privileges
```

### RBAC Tests

After running the setup, test these scenarios:

#### Test as read_only_user:
```bash
psql -h 127.0.0.1 -p 5432 -U read_only_user -d postgres
# Password: readonly456
```

Then try these SQL commands:
```sql
-- Should SUCCEED (has SELECT grant)
SELECT * FROM public.public_info;

-- Should FAIL (no INSERT grant)
INSERT INTO public.public_info (title) VALUES ('Test');
-- Error: permission denied

-- Should FAIL (no access to sensitive_data)
SELECT * FROM public.sensitive_data;
-- Error: permission denied
```

#### Test as app_user:
```bash
psql -h 127.0.0.1 -p 5432 -U app_user -d postgres
# Password: new_secret_password_2024 (changed in test_auth_rbac.sql)
```

```sql
-- Should SUCCEED
SELECT * FROM app_data.orders;

-- Should SUCCEED (has INSERT grant)
INSERT INTO app_data.orders (customer_name, order_total, status) 
VALUES ('Test Customer', 100.00, 'pending');

-- Should FAIL (no DELETE grant)
DELETE FROM app_data.orders;
-- Error: permission denied

-- Should FAIL (no access to sensitive_data)
SELECT * FROM public.sensitive_data;
-- Error: permission denied
```

#### Test as admin_user:
```bash
psql -h 127.0.0.1 -p 5432 -U admin_user -d postgres
# Password: admin789!
```

```sql
-- Should SUCCEED (has ALL privileges)
SELECT * FROM public.sensitive_data;

-- Should SUCCEED
INSERT INTO public.sensitive_data (account_number, ssn, salary) 
VALUES ('ACC-999', '111-22-3333', 99999.99);

-- Should SUCCEED
DELETE FROM public.sensitive_data WHERE account_number = 'ACC-999';
```

## SQL Commands Reference

### Create User with Password
```sql
CREATE USER new_user WITH LOGIN PASSWORD 'secret123';
```

### Create User without Password
```sql
CREATE USER no_pass_user WITH LOGIN;
```

### Change Password
```sql
ALTER USER existing_user WITH PASSWORD 'new_password';
```

### Grant Privileges
```sql
-- Grant SELECT
GRANT SELECT ON table_name TO user_name;

-- Grant multiple privileges
GRANT SELECT, INSERT, UPDATE ON table_name TO user_name;

-- Grant ALL privileges
GRANT ALL ON table_name TO user_name;

-- Grant schema usage
GRANT USAGE ON SCHEMA schema_name TO user_name;
```

### Revoke Privileges
```sql
-- Revoke specific privilege
REVOKE UPDATE ON table_name FROM user_name;

-- Revoke all privileges
REVOKE ALL ON table_name FROM user_name;
```

### Create Role (not a login user)
```sql
CREATE ROLE data_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO data_reader;
GRANT data_reader TO some_user;
```

### Check User Password Status
```sql
SELECT 
    rolname,
    rolcanlogin,
    CASE 
        WHEN rolpassword IS NULL THEN 'NO PASSWORD'
        WHEN rolpassword LIKE 'md5%' THEN 'MD5 HASHED'
        ELSE 'PLAINTEXT'
    END as password_status
FROM __pg_authid__
WHERE rolname = 'your_user';
```

### Check Table Permissions
```sql
-- From pg_class
SELECT relname, relacl 
FROM pg_class 
WHERE relname = 'your_table';

-- From information_schema
SELECT * FROM information_schema.table_privileges 
WHERE table_name = 'your_table';
```

## Password Hash Format

PGQT uses PostgreSQL-compatible MD5 hashing:

- Format: `md5<32-character-hex>`
- Calculation: `MD5(password + username)`
- Example: `MD5('secret' + 'postgres')` = `md5c...`

You can verify hashes manually:
```bash
# On macOS/Linux
echo -n "passwordusername" | md5sum
# Then prepend "md5" to the result
```

## Troubleshooting

### "password authentication failed"
- Wrong password provided
- User has a password but none was supplied
- Check password status: `SELECT rolname, rolpassword IS NOT NULL FROM __pg_authid__ WHERE rolname = 'user';`

### "no password supplied"
- User has a password but connection attempted without one
- Use `--trust-mode` when starting pgqt to bypass (not recommended for production)

### "permission denied"
- User doesn't have the required privilege
- Check grants: `SELECT * FROM information_schema.table_privileges WHERE grantee = 'user_name';`

### Auto-created users
- First connection as a non-existent user creates them automatically
- New users get superuser privileges (for backward compatibility)
- Subsequent connections will use the stored (or no) password

## Running with Trust Mode

For testing without passwords, start pgqt with `--trust-mode`:

```bash
./target/release/pgqt --port 5432 --database test.db --trust-mode
```

All connections will be accepted regardless of password.

## Cleanup

To remove test users and tables:

```sql
DROP USER IF EXISTS app_user;
DROP USER IF EXISTS read_only_user;
DROP USER IF EXISTS admin_user;
DROP USER IF EXISTS no_pass_user;
DROP USER IF EXISTS test_pw_user;
DROP USER IF EXISTS test_no_pw;
DROP ROLE IF EXISTS data_reader;
DROP SCHEMA IF EXISTS app_data CASCADE;
DROP TABLE IF EXISTS auth_test_table CASCADE;
```