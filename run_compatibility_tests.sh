#!/bin/bash
#
# PGQT PostgreSQL Compatibility Test Suite Runner
# Automatically runs the postgres-compatibility-suite and generates a report
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
PG_DSN="${PG_DSN:-host=localhost port=5432 user=postgres password=postgres dbname=postgres}"
PROXY_PORT="${PROXY_PORT:-5435}"
TEST_DB="${TEST_DB:-/tmp/pgqt_compat_test.db}"
RESULTS_DIR="${RESULTS_DIR:-./compatibility_results}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_FILE="$RESULTS_DIR/results_$TIMESTAMP.log"
SUMMARY_FILE="$RESULTS_DIR/summary_$TIMESTAMP.md"

# Create results directory
mkdir -p "$RESULTS_DIR"

echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}PGQT Compatibility Test Suite Runner${NC}"
echo -e "${BLUE}==========================================${NC}"
echo ""

# Check prerequisites
echo -e "${YELLOW}Checking prerequisites...${NC}"

# Check Python 3
if ! command -v python3 &> /dev/null; then
    echo -e "${RED}Error: python3 not found${NC}"
    exit 1
fi

# Check if we're in the right directory
if [ ! -d "postgres-compatibility-suite" ]; then
    echo -e "${RED}Error: postgres-compatibility-suite directory not found${NC}"
    echo "Please run this script from the pgqt root directory"
    exit 1
fi

# Check if pgqt binary exists
if [ ! -f "target/release/pgqt" ]; then
    echo -e "${YELLOW}Building pgqt release binary...${NC}"
    cargo build --release
fi

# Setup Python virtual environment
cd postgres-compatibility-suite

if [ ! -d "venv" ]; then
    echo -e "${YELLOW}Creating Python virtual environment...${NC}"
    python3 -m venv venv
fi

source venv/bin/activate

# Install dependencies
if ! python3 -c "import psycopg2" 2>/dev/null; then
    echo -e "${YELLOW}Installing psycopg2...${NC}"
    pip install psycopg2-binary
fi

if ! python3 -c "import pytest" 2>/dev/null; then
    echo -e "${YELLOW}Installing pytest...${NC}"
    pip install pytest
fi

cd ..

# Clean up old test database
rm -f "$TEST_DB"

echo -e "${GREEN}✓ Prerequisites check complete${NC}"
echo ""

# Start PGQT proxy
echo -e "${YELLOW}Starting PGQT proxy on port $PROXY_PORT...${NC}"
./target/release/pgqt --port "$PROXY_PORT" --database "$TEST_DB" &
PROXY_PID=$!

# Wait for proxy to start
sleep 2

# Check if proxy is running
if ! kill -0 $PROXY_PID 2>/dev/null; then
    echo -e "${RED}Error: Failed to start PGQT proxy${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Proxy started (PID: $PROXY_PID)${NC}"
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    if kill -0 $PROXY_PID 2>/dev/null; then
        kill $PROXY_PID
        wait $PROXY_PID 2>/dev/null || true
    fi
    rm -f "$TEST_DB"
    echo -e "${GREEN}✓ Cleanup complete${NC}"
}
trap cleanup EXIT

# Run tests
echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}Running Compatibility Tests${NC}"
echo -e "${BLUE}==========================================${NC}"
echo ""
echo "PostgreSQL DSN: $PG_DSN"
echo "PGQT Proxy: 127.0.0.1:$PROXY_PORT"
echo "Results: $RESULTS_FILE"
echo ""

# Export for the test runner
export PG_DSN
export PGQT_PORT="$PROXY_PORT"

# Run the tests (from the compatibility suite directory, but write results to parent)
(cd postgres-compatibility-suite && pytest runner.py -v --tb=short 2>&1) | tee "$RESULTS_FILE"
if [ "${PIPESTATUS[0]}" -eq 0 ]; then
    TEST_STATUS="PASSED"
else
    TEST_STATUS="FAILED"
fi

# Generate summary
echo ""
echo -e "${BLUE}==========================================${NC}"
echo -e "${BLUE}Test Summary${NC}"
echo -e "${BLUE}==========================================${NC}"
echo ""

# Parse results (only count lines that start with runner.py:: to avoid double-counting from summary)
TOTAL_TESTS=$(grep -c "^runner.py::test_compatibility" "$RESULTS_FILE" || echo "0")
PASSED_TESTS=$(grep "^runner.py::test_compatibility" "$RESULTS_FILE" | grep -c "PASSED" || echo "0")
FAILED_TESTS=$(grep "^runner.py::test_compatibility" "$RESULTS_FILE" | grep -c "FAILED" || echo "0")

# Calculate percentage
if [ "$TOTAL_TESTS" -gt 0 ]; then
    PASS_RATE=$(echo "scale=1; $PASSED_TESTS * 100 / $TOTAL_TESTS" | bc)
else
    PASS_RATE="0.0"
fi

echo "Total Tests: $TOTAL_TESTS"
echo -e "${GREEN}Passed: $PASSED_TESTS${NC}"
echo -e "${RED}Failed: $FAILED_TESTS${NC}"
echo "Pass Rate: $PASS_RATE%"
echo ""

# Generate markdown summary
cat > "$SUMMARY_FILE" << EOF
# PGQT Compatibility Test Results

**Date:** $(date)
**Commit:** $(git rev-parse --short HEAD)
**PostgreSQL DSN:** $PG_DSN

## Summary

| Metric | Value |
|--------|-------|
| Total Tests | $TOTAL_TESTS |
| Passed | $PASSED_TESTS |
| Failed | $FAILED_TESTS |
| **Pass Rate** | **$PASS_RATE%** |

## Test Status

### Passing Tests

EOF

grep "^runner.py::test_compatibility" "$RESULTS_FILE" | grep "PASSED" | sed 's/.*::test_compatibility\[/\* /; s/\] PASSED.*//' >> "$SUMMARY_FILE"

cat >> "$SUMMARY_FILE" << EOF

### Failing Tests

EOF

grep "^runner.py::test_compatibility" "$RESULTS_FILE" | grep "FAILED" | sed 's/.*::test_compatibility\[/\* /; s/\] FAILED.*//' >> "$SUMMARY_FILE"

cat >> "$SUMMARY_FILE" << EOF

## Next Steps

Based on the test results, consider:

1. **Review failing tests** in the detailed log: \`$RESULTS_FILE\`
2. **Prioritize fixes** based on:
   - Type validation issues
   - Missing built-in functions
   - Column alias preservation
   - System catalog access

## Files

- Detailed log: \`$RESULTS_FILE\`
- This summary: \`$SUMMARY_FILE\`
EOF

echo -e "${GREEN}Summary saved to: $SUMMARY_FILE${NC}"
echo ""

# Show quick analysis of failure categories
echo -e "${YELLOW}Quick Failure Analysis:${NC}"
echo ""

if grep -q "value too long for type" "$RESULTS_FILE"; then
    echo -e "${RED}• Type validation issues (VARCHAR/CHAR length)${NC}"
fi

if grep -q "no such table: pg_class" "$RESULTS_FILE"; then
    echo -e "${RED}• System catalog access issues${NC}"
fi

if grep -q "Column mismatch" "$RESULTS_FILE"; then
    echo -e "${RED}• Column alias/naming issues${NC}"
fi

if grep -q "out of range" "$RESULTS_FILE"; then
    echo -e "${RED}• Numeric range validation issues${NC}"
fi

if grep -q "date/time field value out of range" "$RESULTS_FILE"; then
    echo -e "${RED}• Date validation issues${NC}"
fi

if grep -q "Invalid function" "$RESULTS_FILE"; then
    echo -e "${RED}• Missing function issues${NC}"
fi

echo ""
echo -e "${BLUE}==========================================${NC}"

if [ "$TEST_STATUS" = "PASSED" ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${YELLOW}Some tests failed. Review the results above.${NC}"
    exit 1
fi
