# PGQT Password Authentication & RBAC Test Procedure

## Prerequisites

- PGQT built: `cargo build --release`
- psql client installed
- Terminal with two windows/tabs (one for server, one for tests)

---

## Step 1: Start PGQT Server (Password Auth Mode)

In Terminal 1:

```bash
# Kill any existing server
pkill -f pgqt 2>/dev/null || true
sleep 1

# Build and run with password auth enabled (NO --trust-mode flag)
cargo build --release
./target/release/pgqt --port 5432 --database test.db
```

Expected output:
```
Server listening on 127.0.0.1:5432
Using database: test.db
```

**Leave this terminal running!**

---

## Step 2: Create Test Users

In Terminal 2, run:

```bash
psql -h 127.0.0.1 -p 5432 -U postgres -d postgres << 'EOF'
-- Clean up existing test users
DROP USER IF EXISTS test_pw_user;
DROP USER IF EXISTS test_no_pw;

-- Create user with password
CREATE USER test_pw_user WITH LOGIN PASSWORD 'correct_password';

-- Create user without password
CREATE USER test_no_pw WITH LOGIN;

-- Create test table
DROP TABLE IF EXISTS auth_test_table;
CREATE TABLE auth_test_table (
    id SERIAL PRIMARY KEY,
    data TEXT
);

-- Grant access
GRANT SELECT ON auth_test_table TO test_pw_user;
GRANT SELECT ON auth_test_table TO test_no_pw;
INSERT INTO auth_test_table (data) VALUES ('test data');

-- Verify
SELECT rolname, rolcanlogin 
FROM pg_roles 
WHERE rolname IN ('test_pw_user', 'test_no_pw');
EOF
```

Expected: Two users created, both with `rolcanlogin = true`

---

## Step 3: Test Password Authentication

### Test 3.1: Correct Password (Should Succeed)

```bash
psql "postgresql://test_pw_user:correct_password@127.0.0.1:5432/postgres" \
  -c "SELECT current_user;"
```

**Expected:**
```
 current_user 
--------------
 test_pw_user
(1 row)
```

---

### Test 3.2: Wrong Password (Should Fail)

```bash
psql "postgresql://test_pw_user:wrong_password@127.0.0.1:5432/postgres" \
  -c "SELECT current_user;" 2>&1 || echo "TEST PASSED - Connection rejected"
```

**Expected:**
```
psql: error: connection to server at "127.0.0.1", port 5432 failed: 
FATAL:  password authentication failed
TEST PASSED - Connection rejected
```

---

### Test 3.3: No Password When Required (Should Fail)

```bash
psql "postgresql://test_pw_user:@127.0.0.1:5432/postgres" \
  -c "SELECT current_user;" 2>&1 || echo "TEST PASSED - Connection rejected"
```

**Expected:**
```
psql: error: connection to server at "127.0.0.1", port 5432 failed: 
fe_sendauth: no password supplied
TEST PASSED - Connection rejected
```

---

### Test 3.4: User Without Password (Should Succeed)

```bash
psql "postgresql://test_no_pw:@127.0.0.1:5432/postgres" \
  -c "SELECT current_user;"
```

**Expected:**
```
 current_user 
--------------
 test_no_pw
(1 row)
```

---

### Test 3.5: Interactive Mode

```bash
psql -h 127.0.0.1 -p 5432 -U test_pw_user -d postgres
```

When prompted:
```
Password: correct_password
```

Then run:
```sql
SELECT current_user;
\q
```

**Expected:**
```
 current_user 
--------------
 test_pw_user
(1 row)
```

---

## Step 4: Test RBAC (Grants and Revokes)

### Step 4.1: Create Users and Tables

```bash
psql -h 127.0.0.1 -p 5432 -U postgres -d postgres << 'EOF'
-- Create RBAC test users
DROP USER IF EXISTS app_user, read_only, admin_user;
CREATE USER app_user WITH LOGIN PASSWORD 'app123';
CREATE USER read_only WITH LOGIN PASSWORD 'readonly123';
CREATE USER admin_user WITH LOGIN PASSWORD 'admin123';

-- Create test tables
DROP TABLE IF EXISTS public_data CASCADE;
DROP TABLE IF EXISTS sensitive_data CASCADE;

CREATE TABLE public_data (
    id SERIAL PRIMARY KEY,
    info TEXT
);

CREATE TABLE sensitive_data (
    id SERIAL PRIMARY KEY,
    secret TEXT
);

INSERT INTO public_data VALUES (1, 'Public information');
INSERT INTO sensitive_data VALUES (1, 'Top secret data');

-- Grant permissions
GRANT SELECT ON public_data TO read_only;
GRANT SELECT, INSERT, UPDATE ON public_data TO app_user;
GRANT USAGE, SELECT ON SEQUENCE public_data_id_seq TO app_user;
GRANT ALL ON sensitive_data TO admin_user;
GRANT USAGE, SELECT ON SEQUENCE sensitive_data_id_seq TO admin_user;

-- Verify grants
SELECT relname, relacl::text 
FROM pg_class 
WHERE relname IN ('public_data', 'sensitive_data');
EOF
```

---

### Step 4.2: Test read_only User

**Should succeed (SELECT granted):**
```bash
psql "postgresql://read_only:readonly123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM public_data;"
```

**Expected:**
```
 id |       info
----+--------------------
  1 | Public information
(1 row)
```

**Should fail (no INSERT permission):**
```bash
psql "postgresql://read_only:readonly123@127.0.0.1:5432/postgres" \
  -c "INSERT INTO public_data VALUES (2, 'test');" 2>&1 || echo "TEST PASSED - Insert rejected"
```

**Expected:** Permission denied error

---

### Step 4.3: Test app_user

**Should succeed (INSERT granted):**
```bash
psql "postgresql://app_user:app123@127.0.0.1:5432/postgres" \
  -c "INSERT INTO public_data VALUES (2, 'Added by app_user'); SELECT * FROM public_data;"
```

**Expected:** Two rows returned

**Should fail (no access to sensitive_data):**
```bash
psql "postgresql://app_user:app123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM sensitive_data;" 2>&1 || echo "TEST PASSED - Access denied"
```

**Expected:** Permission denied error

---

### Step 4.4: Test admin_user

**Should succeed (ALL privileges):**
```bash
psql "postgresql://admin_user:admin123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM sensitive_data;"
```

**Expected:**
```
 id |      secret
----+------------------
  1 | Top secret data
(1 row)
```

**Insert should also succeed:**
```bash
psql "postgresql://admin_user:admin123@127.0.0.1:5432/postgres" \
  -c "INSERT INTO sensitive_data VALUES (2, 'More secrets'); SELECT * FROM sensitive_data;"
```

**Expected:** Two rows of secret data

---

## Step 5: Test ALTER USER (Password Change)

### Step 5.1: Change Password

```bash
psql -h 127.0.0.1 -p 5432 -U postgres -d postgres \
  -c "ALTER USER test_pw_user WITH PASSWORD 'new_password';"
```

---

### Step 5.2: Test New Password (Should Work)

```bash
psql "postgresql://test_pw_user:new_password@127.0.0.1:5432/postgres" \
  -c "SELECT current_user;"
```

**Expected:** `test_pw_user`

---

### Step 5.3: Test Old Password (Should Fail)

```bash
psql "postgresql://test_pw_user:correct_password@127.0.0.1:5432/postgres" \
  -c "SELECT current_user;" 2>&1 || echo "TEST PASSED - Old password rejected"
```

**Expected:** Password authentication failed

---

## Quick Automated Test Script

Save this as `run_auth_tests.sh`:

```bash
#!/bin/bash
set -e

echo "=========================================="
echo "PGQT Password Auth & RBAC Tests"
echo "=========================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

PASS=0
FAIL=0

check_result() {
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ PASS${NC}"
        ((PASS++))
    else
        echo -e "${RED}✗ FAIL${NC}"
        ((FAIL++))
    fi
}

# Setup
echo "Setting up test users..."
psql -h 127.0.0.1 -p 5432 -U postgres -d postgres << 'EOF' 2>/dev/null
DROP USER IF EXISTS test_pw_user, test_no_pw, app_user, read_only, admin_user;
CREATE USER test_pw_user WITH LOGIN PASSWORD 'correct_password';
CREATE USER test_no_pw WITH LOGIN;
CREATE USER app_user WITH LOGIN PASSWORD 'app123';
CREATE USER read_only WITH LOGIN PASSWORD 'readonly123';
CREATE USER admin_user WITH LOGIN PASSWORD 'admin123';

DROP TABLE IF EXISTS public_data, sensitive_data;
CREATE TABLE public_data (id SERIAL PRIMARY KEY, info TEXT);
CREATE TABLE sensitive_data (id SERIAL PRIMARY KEY, secret TEXT);
INSERT INTO public_data VALUES (1, 'Public');
INSERT INTO sensitive_data VALUES (1, 'Secret');

GRANT SELECT ON public_data TO read_only;
GRANT SELECT, INSERT ON public_data TO app_user;
GRANT ALL ON sensitive_data TO admin_user;
EOF

echo ""
echo "Test 1: Correct password"
psql "postgresql://test_pw_user:correct_password@127.0.0.1:5432/postgres" \
  -c "SELECT 1;" > /dev/null 2>&1
check_result

echo "Test 2: Wrong password (should fail)"
if psql "postgresql://test_pw_user:wrong@127.0.0.1:5432/postgres" \
  -c "SELECT 1;" > /dev/null 2>&1; then
    echo -e "${RED}✗ FAIL${NC} - Should have rejected"
    ((FAIL++))
else
    echo -e "${GREEN}✓ PASS${NC} - Correctly rejected"
    ((PASS++))
fi

echo "Test 3: No-password user"
psql "postgresql://test_no_pw:@127.0.0.1:5432/postgres" \
  -c "SELECT 1;" > /dev/null 2>&1
check_result

echo "Test 4: read_only SELECT (should succeed)"
psql "postgresql://read_only:readonly123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM public_data;" > /dev/null 2>&1
check_result

echo "Test 5: read_only INSERT (should fail)"
if psql "postgresql://read_only:readonly123@127.0.0.1:5432/postgres" \
  -c "INSERT INTO public_data VALUES (2,'x');" > /dev/null 2>&1; then
    echo -e "${RED}✗ FAIL${NC} - Should have rejected"
    ((FAIL++))
else
    echo -e "${GREEN}✓ PASS${NC} - Correctly rejected"
    ((PASS++))
fi

echo "Test 6: admin_user access to sensitive_data"
psql "postgresql://admin_user:admin123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM sensitive_data;" > /dev/null 2>&1
check_result

echo ""
echo "=========================================="
echo "Results: $PASS passed, $FAIL failed"
echo "=========================================="

if [ $FAIL -eq 0 ]; then
    exit 0
else
    exit 1
fi
```

Make it executable:
```bash
chmod +x run_auth_tests.sh
./run_auth_tests.sh
```

---

## Cleanup

When done testing:

```bash
# Stop PGQT
pkill -f pgqt

# Clean up database file
rm -f test.db test.db.error.log
```

---

## Troubleshooting

### "Connection refused"
- PGQT server is not running
- Wrong port number

### "password authentication failed"
- Wrong password provided
- User doesn't exist (run setup again)

### "fe_sendauth: no password supplied"
- User has a password but you didn't provide one
- Add password to connection string

### "permission denied"
- User doesn't have the required privilege
- Check grants with: `\z table_name` in psql