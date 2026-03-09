#!/bin/bash
# =====================================================
# Password Authentication & RBAC Test Script
# Run this to automatically test the auth system
# =====================================================

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
PG_HOST="${PG_HOST:-127.0.0.1}"
PG_PORT="${PG_PORT:-5432}"
PG_USER="${PG_USER:-postgres}"
PG_PASS="${PG_PASS:-postgres}"

echo "====================================================="
echo "PGQT Password Authentication Test Suite"
echo "====================================================="
echo ""
echo "Using: $PG_HOST:$PG_PORT"
echo ""

# Function to run SQL file
run_sql() {
    local file=$1
    echo "Running: $file"
    PGPASSWORD=$PG_PASS psql -h $PG_HOST -p $PG_PORT -U $PG_USER -d postgres -f "$file"
}

# Function to test connection
test_connection() {
    local user=$1
    local pass=$2
    local expected=$3
    local desc=$4
    
    echo -n "Testing: $desc... "
    
    if PGPASSWORD="$pass" psql "postgresql://$user@$PG_HOST:$PG_PORT/postgres" -c "SELECT 1" > /dev/null 2>&1; then
        if [ "$expected" = "success" ]; then
            echo -e "${GREEN}✓ PASS${NC}"
            return 0
        else
            echo -e "${RED}✗ FAIL${NC} (expected failure but succeeded)"
            return 1
        fi
    else
        if [ "$expected" = "fail" ]; then
            echo -e "${GREEN}✓ PASS${NC} (correctly rejected)"
            return 0
        else
            echo -e "${RED}✗ FAIL${NC} (expected success but failed)"
            return 1
        fi
    fi
}

# Check if server is running
echo -n "Checking if PGQT server is running... "
if ! pg_isready -h $PG_HOST -p $PG_PORT > /dev/null 2>&1; then
    echo -e "${RED}NOT RUNNING${NC}"
    echo ""
    echo "Please start PGQT first:"
    echo "  cargo run --release -- --port $PG_PORT --database test.db"
    echo ""
    exit 1
fi
echo -e "${GREEN}OK${NC}"
echo ""

# Step 1: Run setup
echo "====================================================="
echo "Step 1: Running Setup Script"
echo "====================================================="
run_sql test_auth_setup.sql
echo ""

# Step 2: Run password auth tests
echo "====================================================="
echo "Step 2: Running Password Auth Setup"
echo "====================================================="
run_sql test_password_auth.sql
echo ""

# Step 3: Run RBAC tests
echo "====================================================="
echo "Step 3: Running RBAC Setup"
echo "====================================================="
run_sql test_auth_rbac.sql
echo ""

# Step 4: Connection tests
echo "====================================================="
echo "Step 4: Testing Password Connections"
echo "====================================================="

PASS_COUNT=0
FAIL_COUNT=0

# Test 1: Correct password
if test_connection "test_pw_user" "correct_password" "success" "test_pw_user with correct password"; then
    ((PASS_COUNT++))
else
    ((FAIL_COUNT++))
fi

# Test 2: Wrong password
if test_connection "test_pw_user" "wrong_password" "fail" "test_pw_user with wrong password"; then
    ((PASS_COUNT++))
else
    ((FAIL_COUNT++))
fi

# Test 3: User without password (empty)
if test_connection "test_no_pw" "" "success" "test_no_pw with no password"; then
    ((PASS_COUNT++))
else
    ((FAIL_COUNT++))
fi

# Test 4: Postgres superuser
if test_connection "postgres" "postgres" "success" "postgres with correct password"; then
    ((PASS_COUNT++))
else
    ((FAIL_COUNT++))
fi

echo ""

# Step 5: RBAC query tests
echo "====================================================="
echo "Step 5: Testing RBAC Permissions"
echo "====================================================="

echo -n "Testing: read_only_user can SELECT public_info... "
if PGPASSWORD="readonly456" psql "postgresql://read_only_user@$PG_HOST:$PG_PORT/postgres" -c "SELECT * FROM public.public_info LIMIT 1" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC}"
    ((PASS_COUNT++))
else
    echo -e "${RED}✗ FAIL${NC}"
    ((FAIL_COUNT++))
fi

echo -n "Testing: read_only_user cannot INSERT public_info... "
if ! PGPASSWORD="readonly456" psql "postgresql://read_only_user@$PG_HOST:$PG_PORT/postgres" -c "INSERT INTO public.public_info (title) VALUES ('Test')" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC} (correctly rejected)"
    ((PASS_COUNT++))
else
    echo -e "${RED}✗ FAIL${NC}"
    ((FAIL_COUNT++))
fi

echo -n "Testing: app_user can SELECT app_data.orders... "
if PGPASSWORD="new_secret_password_2024" psql "postgresql://app_user@$PG_HOST:$PG_PORT/postgres" -c "SELECT * FROM app_data.orders LIMIT 1" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC}"
    ((PASS_COUNT++))
else
    echo -e "${RED}✗ FAIL${NC}"
    ((FAIL_COUNT++))
fi

echo -n "Testing: app_user cannot access sensitive_data... "
if ! PGPASSWORD="new_secret_password_2024" psql "postgresql://app_user@$PG_HOST:$PG_PORT/postgres" -c "SELECT * FROM public.sensitive_data LIMIT 1" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC} (correctly rejected)"
    ((PASS_COUNT++))
else
    echo -e "${RED}✗ FAIL${NC}"
    ((FAIL_COUNT++))
fi

echo ""
echo "====================================================="
echo "Test Summary"
echo "====================================================="
echo -e "Passed: ${GREEN}$PASS_COUNT${NC}"
echo -e "Failed: ${RED}$FAIL_COUNT${NC}"
echo ""

if [ $FAIL_COUNT -eq 0 ]; then
    echo -e "${GREEN}All tests passed! ✓${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed! ✗${NC}"
    exit 1
fi