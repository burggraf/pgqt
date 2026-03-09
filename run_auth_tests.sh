#!/bin/bash
# PGQT Password Auth & RBAC Automated Test Script
# Run this after starting PGQT server

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

# Check if server is running
echo -n "Checking PGQT server... "
if ! pg_isready -h 127.0.0.1 -p 5432 > /dev/null 2>&1; then
    echo -e "${RED}NOT RUNNING${NC}"
    echo "Start PGQT first: ./target/release/pgqt --port 5432 --database test.db"
    exit 1
fi
echo -e "${GREEN}OK${NC}"
echo ""

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
echo "Running tests..."
echo ""

echo -n "Test 1: Correct password              "
psql "postgresql://test_pw_user:correct_password@127.0.0.1:5432/postgres" \
  -c "SELECT 1;" > /dev/null 2>&1
check_result

echo -n "Test 2: Wrong password (should fail)  "
if psql "postgresql://test_pw_user:wrong@127.0.0.1:5432/postgres" \
  -c "SELECT 1;" > /dev/null 2>&1; then
    echo -e "${RED}✗ FAIL${NC} - Should have rejected"
    ((FAIL++))
else
    echo -e "${GREEN}✓ PASS${NC} - Correctly rejected"
    ((PASS++))
fi

echo -n "Test 3: No-password user              "
psql "postgresql://test_no_pw:@127.0.0.1:5432/postgres" \
  -c "SELECT 1;" > /dev/null 2>&1
check_result

echo -n "Test 4: read_only SELECT              "
psql "postgresql://read_only:readonly123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM public_data;" > /dev/null 2>&1
check_result

echo -n "Test 5: read_only INSERT (rejected)   "
if psql "postgresql://read_only:readonly123@127.0.0.1:5432/postgres" \
  -c "INSERT INTO public_data VALUES (2,'x');" > /dev/null 2>&1; then
    echo -e "${RED}✗ FAIL${NC} - Should have rejected"
    ((FAIL++))
else
    echo -e "${GREEN}✓ PASS${NC} - Correctly rejected"
    ((PASS++))
fi

echo -n "Test 6: app_user INSERT               "
psql "postgresql://app_user:app123@127.0.0.1:5432/postgres" \
  -c "INSERT INTO public_data VALUES (3,'from app');" > /dev/null 2>&1
check_result

echo -n "Test 7: admin_user sensitive access   "
psql "postgresql://admin_user:admin123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM sensitive_data;" > /dev/null 2>&1
check_result

echo -n "Test 8: app_user denied sensitive     "
if psql "postgresql://app_user:app123@127.0.0.1:5432/postgres" \
  -c "SELECT * FROM sensitive_data;" > /dev/null 2>&1; then
    echo -e "${RED}✗ FAIL${NC} - Should have rejected"
    ((FAIL++))
else
    echo -e "${GREEN}✓ PASS${NC} - Correctly rejected"
    ((PASS++))
fi

echo ""
echo "=========================================="
echo "Results: $PASS passed, $FAIL failed"
echo "=========================================="

if [ $FAIL -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi