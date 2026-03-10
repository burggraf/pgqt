#!/bin/bash
#
# PGQT PostgreSQL Compatibility Test Suite Runner
#
# This script runs the enhanced compatibility test suite that compares
# PGQT behavior against a reference PostgreSQL instance.
#
# Usage:
#   ./run_compatibility_suite.sh [options]
#
# Options:
#   -v, --verbose       Show detailed output for each statement
#   -f, --fail-fast     Stop on first failure
#   -j, --json          Output results as JSON only
#   -s, --summary       Show summary only (no per-file progress)
#   -h, --help          Show this help message
#
# Environment Variables:
#   PG_DSN              PostgreSQL connection string (default: host=localhost port=5432 user=postgres password=postgres dbname=postgres)
#   PROXY_PORT          Port for PGQT proxy (default: 5435)
#
# Examples:
#   ./run_compatibility_suite.sh                    # Run full suite with default settings
#   ./run_compatibility_suite.sh -v                 # Run with verbose output
#   ./run_compatibility_suite.sh -f -v              # Fail fast with verbose output
#   PG_DSN="host=192.168.1.100 port=5432 user=pg password=secret dbname=test" ./run_compatibility_suite.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
VERBOSE=""
FAIL_FAST=""
JSON_OUTPUT=false
SUMMARY_ONLY=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -v|--verbose)
            VERBOSE="--verbose"
            shift
            ;;
        -f|--fail-fast)
            FAIL_FAST="--fail-fast"
            shift
            ;;
        -j|--json)
            JSON_OUTPUT=true
            shift
            ;;
        -s|--summary)
            SUMMARY_ONLY=true
            shift
            ;;
        -h|--help)
            echo "PGQT PostgreSQL Compatibility Test Suite Runner"
            echo ""
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  -v, --verbose       Show detailed output for each statement"
            echo "  -f, --fail-fast     Stop on first failure"
            echo "  -j, --json          Output results as JSON only"
            echo "  -s, --summary       Show summary only (no per-file progress)"
            echo "  -h, --help          Show this help message"
            echo ""
            echo "Environment Variables:"
            echo "  PG_DSN              PostgreSQL connection string"
            echo "  PROXY_PORT          Port for PGQT proxy"
            echo ""
            echo "Examples:"
            echo "  $0                          # Run full suite"
            echo "  $0 -v                       # Run with verbose output"
            echo "  $0 -f -v                    # Fail fast with verbose output"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUITE_DIR="$SCRIPT_DIR/postgres-compatibility-suite"

# Check if we're in the right directory
if [ ! -d "$SUITE_DIR" ]; then
    echo -e "${RED}Error: Compatibility suite directory not found at $SUITE_DIR${NC}"
    echo "Make sure you're running this script from the project root."
    exit 1
fi

# Check if Python is available
if ! command -v python3 &> /dev/null; then
    echo -e "${RED}Error: python3 is required but not installed${NC}"
    exit 1
fi

# Check if the runner exists
RUNNER_SCRIPT="$SUITE_DIR/runner_with_stats.py"
if [ ! -f "$RUNNER_SCRIPT" ]; then
    echo -e "${RED}Error: Runner script not found at $RUNNER_SCRIPT${NC}"
    exit 1
fi

# Check PostgreSQL availability
echo -e "${BLUE}Checking PostgreSQL availability...${NC}"
if ! command -v pg_isready &> /dev/null; then
    echo -e "${YELLOW}Warning: pg_isready not found. Will attempt connection anyway.${NC}"
else
    PG_HOST=$(echo "$PG_DSN" | grep -o 'host=[^ ]*' | cut -d= -f2 || echo "localhost")
    PG_PORT=$(echo "$PG_DSN" | grep -o 'port=[^ ]*' | cut -d= -f2 || echo "5432")
    
    if ! pg_isready -h "$PG_HOST" -p "$PG_PORT" > /dev/null 2>&1; then
        echo -e "${RED}Error: PostgreSQL is not running at $PG_HOST:$PG_PORT${NC}"
        echo ""
        echo "To start PostgreSQL with Docker:"
        echo "  docker run --name pg-test -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres"
        echo ""
        echo "Or set PG_DSN to point to your PostgreSQL instance:"
        echo "  export PG_DSN=\"host=your-host port=5432 user=postgres password=secret dbname=postgres\""
        exit 1
    fi
    echo -e "${GREEN}✓ PostgreSQL is available${NC}"
fi

# Build PGQT if needed
echo -e "${BLUE}Checking PGQT binary...${NC}"
PGQT_BINARY="$SCRIPT_DIR/target/release/pgqt"
if [ ! -f "$PGQT_BINARY" ]; then
    echo "Building PGQT in release mode..."
    cd "$SCRIPT_DIR"
    cargo build --release
fi
echo -e "${GREEN}✓ PGQT binary ready${NC}"

# Run the tests
echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  PGQT Compatibility Test Suite${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

cd "$SUITE_DIR"

if [ "$JSON_OUTPUT" = true ]; then
    # JSON output only
    python3 runner_with_stats.py $VERBOSE $FAIL_FAST 2>&1 | tail -n 1 | python3 -m json.tool 2>/dev/null || \
    python3 runner_with_stats.py $VERBOSE $FAIL_FAST 2>&1 | grep -A 1000 "^Detailed results saved"
else
    # Normal output
    if [ "$SUMMARY_ONLY" = true ]; then
        # Show only the summary section
        python3 runner_with_stats.py $VERBOSE $FAIL_FAST 2>&1 | grep -A 1000 "^===* PGQT COMPATIBILITY"
    else
        # Full output
        python3 runner_with_stats.py $VERBOSE $FAIL_FAST 2>&1
    fi
fi

# Capture exit code
EXIT_CODE=${PIPESTATUS[0]}

# Final message
echo ""
if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✓ Compatibility test suite completed successfully${NC}"
else
    echo -e "${YELLOW}⚠ Compatibility test suite completed with issues${NC}"
fi

exit $EXIT_CODE
