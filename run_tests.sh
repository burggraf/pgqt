#!/bin/bash
#
# PGQT Test Runner
# Runs all unit tests, integration tests, and e2e tests
#

set -e  # Exit on error

echo "=========================================="
echo "PGQT Test Suite"
echo "=========================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track results
UNIT_PASSED=0
UNIT_FAILED=0
INTEGRATION_PASSED=0
INTEGRATION_FAILED=0
E2E_PASSED=0
E2E_FAILED=0

# Function to run cargo tests
run_unit_tests() {
    echo -e "${YELLOW}Running Unit Tests (cargo test)...${NC}"
    echo "------------------------------------------"
    
    if cargo test --quiet 2>&1; then
        # Count passed tests (portable, no -P flag)
        UNIT_PASSED=$(cargo test --quiet 2>&1 | grep -oE '[0-9]+ passed' | grep -oE '[0-9]+' | awk '{s+=$1} END {print s}')
        if [ -z "$UNIT_PASSED" ]; then
            UNIT_PASSED=0
        fi
        echo -e "${GREEN}✓ Unit tests passed${NC}"
    else
        echo -e "${RED}✗ Unit tests failed${NC}"
        UNIT_FAILED=1
    fi
    echo ""
}

# Function to run a specific integration test
run_integration_test() {
    local test_file=$1
    local test_name=$(basename "$test_file" .rs)
    
    echo -n "Testing $test_name... "
    if cargo test --test "$test_name" --quiet 2>&1 | grep -q "test result: ok"; then
        echo -e "${GREEN}✓${NC}"
        INTEGRATION_PASSED=$((INTEGRATION_PASSED + 1))
    else
        echo -e "${RED}✗${NC}"
        INTEGRATION_FAILED=$((INTEGRATION_FAILED + 1))
    fi
}

# Function to run all integration tests
run_integration_tests() {
    echo -e "${YELLOW}Running Integration Tests...${NC}"
    echo "------------------------------------------"
    
    for test_file in tests/*.rs; do
        if [ -f "$test_file" ]; then
            run_integration_test "$test_file"
        fi
    done
    echo ""
}

# Function to run Python e2e tests
run_e2e_tests() {
    echo -e "${YELLOW}Running E2E Tests (Python)...${NC}"
    echo "------------------------------------------"
    
    # Check if Python and psycopg2 are available
    if ! command -v python3 &> /dev/null; then
        echo -e "${YELLOW}⚠ python3 not found, skipping e2e tests${NC}"
        return
    fi
    
    if ! python3 -c "import psycopg2" 2>/dev/null; then
        echo -e "${YELLOW}⚠ psycopg2 not installed, skipping e2e tests${NC}"
        echo "   Install with: pip install psycopg2-binary"
        return
    fi
    
        # Run tests one by one to avoid complex proxy management issues
    echo "Running tests individually..."
    echo ""
    
    for test_file in tests/*_e2e_test.py; do
        if [ -f "$test_file" ]; then
            test_name=$(basename "$test_file")
            echo -n "Testing $test_name... "
            
            # Ensure no stale proxy is running
            pkill -f pgqt > /dev/null 2>&1 || true
            
            if python3 "$test_file" > /tmp/e2e_out.log 2>&1; then
                echo -e "${GREEN}✓${NC}"
                E2E_PASSED=$((E2E_PASSED + 1))
            else
                echo -e "${RED}✗${NC}"
                cat /tmp/e2e_out.log
                E2E_FAILED=$((E2E_FAILED + 1))
            fi
        fi
    done
    echo ""
}

# Function to print summary
print_summary() {
    echo "=========================================="
    echo "Test Summary"
    echo "=========================================="
    echo -e "Unit Tests:       ${GREEN}$UNIT_PASSED passed${NC}"
    echo -e "Integration Tests: ${GREEN}$INTEGRATION_PASSED passed${NC} ${RED}$INTEGRATION_FAILED failed${NC}"
    echo -e "E2E Tests:        ${GREEN}$E2E_PASSED passed${NC} ${RED}$E2E_FAILED failed${NC}"
    echo ""
    
    TOTAL_FAILED=$((UNIT_FAILED + INTEGRATION_FAILED + E2E_FAILED))
    if [ $TOTAL_FAILED -eq 0 ]; then
        echo -e "${GREEN}All tests passed! ✓${NC}"
        exit 0
    else
        echo -e "${RED}Some tests failed. ✗${NC}"
        exit 1
    fi
}

# Main execution
main() {
    # Parse arguments
    RUN_UNIT=true
    RUN_INTEGRATION=true
    RUN_E2E=true
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            --unit-only)
                RUN_INTEGRATION=false
                RUN_E2E=false
                shift
                ;;
            --integration-only)
                RUN_UNIT=false
                RUN_E2E=false
                shift
                ;;
            --e2e-only)
                RUN_UNIT=false
                RUN_INTEGRATION=false
                shift
                ;;
            --no-e2e)
                RUN_E2E=false
                shift
                ;;
            -h|--help)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --unit-only          Run only unit tests"
                echo "  --integration-only   Run only integration tests"
                echo "  --e2e-only           Run only e2e tests"
                echo "  --no-e2e             Skip e2e tests"
                echo "  -h, --help           Show this help message"
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    # Run tests
    if [ "$RUN_UNIT" = true ]; then
        run_unit_tests
    fi
    
    if [ "$RUN_INTEGRATION" = true ]; then
        run_integration_tests
    fi
    
    if [ "$RUN_E2E" = true ]; then
        run_e2e_tests
    fi
    
    print_summary
}

main "$@"
