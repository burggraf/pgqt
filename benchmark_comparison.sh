#!/bin/bash
# Benchmark comparison script for PGQT vs pgsqlite
# This script starts both servers, runs benchmarks, and compares results

set -e

# Configuration
PGQT_PORT=5436
PGSQLITE_PORT=5437
PGQT_DB="/tmp/pgqt_benchmark.db"
PGSQLITE_DB="/tmp/pgsqlite_benchmark.db"
ITERATIONS=500

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  PGQT vs pgsqlite Benchmark${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Check if binaries exist
if [ ! -f "./target/release/pgqt" ]; then
    echo -e "${RED}Error: pgqt binary not found at ./target/release/pgqt${NC}"
    echo "Please build with: cargo build --release"
    exit 1
fi

if [ ! -f "./target/release/pgsqlite" ]; then
    echo -e "${RED}Error: pgsqlite binary not found at ./target/release/pgsqlite${NC}"
    exit 1
fi

# Clean up old database files
rm -f "$PGQT_DB" "$PGSQLITE_DB"

# Function to cleanup processes on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    pkill -f "pgqt --port $PGQT_PORT" 2>/dev/null || true
    pkill -f "pgsqlite --port $PGSQLITE_PORT" 2>/dev/null || true
    rm -f "$PGQT_DB" "$PGSQLITE_DB"
}
trap cleanup EXIT

# Start PGQT
echo -e "${GREEN}Starting PGQT on port $PGQT_PORT...${NC}"
./target/release/pgqt --port $PGQT_PORT --database "$PGQT_DB" --trust-mode --output NULL &
PGQT_PID=$!
sleep 2

# Check if PGQT started
if ! kill -0 $PGQT_PID 2>/dev/null; then
    echo -e "${RED}Error: PGQT failed to start${NC}"
    exit 1
fi
echo -e "${GREEN}PGQT started (PID: $PGQT_PID)${NC}"
echo ""

# Start pgsqlite
echo -e "${GREEN}Starting pgsqlite on port $PGSQLITE_PORT...${NC}"
./target/release/pgsqlite --port $PGSQLITE_PORT --database "$PGSQLITE_DB" &
PGSQLITE_PID=$!
sleep 2

# Check if pgsqlite started
if ! kill -0 $PGSQLITE_PID 2>/dev/null; then
    echo -e "${RED}Error: pgsqlite failed to start${NC}"
    exit 1
fi
echo -e "${GREEN}pgsqlite started (PID: $PGSQLITE_PID)${NC}"
echo ""

# Wait for servers to be ready
echo "Waiting for servers to be ready..."
sleep 3

# Run benchmarks
echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Running PGQT Benchmark${NC}"
echo -e "${BLUE}========================================${NC}"
python3 simple_benchmark.py \
    --host 127.0.0.1 \
    --port $PGQT_PORT \
    --name "PGQT" \
    --iterations $ITERATIONS \
    --user postgres \
    --password postgres \
    --database postgres

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Running pgsqlite Benchmark${NC}"
echo -e "${BLUE}========================================${NC}"
python3 simple_benchmark.py \
    --host 127.0.0.1 \
    --port $PGSQLITE_PORT \
    --name "pgsqlite" \
    --iterations $ITERATIONS \
    --user postgres \
    --password postgres \
    --database postgres

echo ""
echo -e "${GREEN}Benchmark complete!${NC}"
