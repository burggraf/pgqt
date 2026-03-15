#!/bin/bash
set -e

# Configuration
PROXY_PORT=5434
DB_PATH="northwind_pagila_test.db"
TEST_DIR="Northwind-Pagila-Tests"
LOG_FILE="$TEST_DIR/test_run.log"

# URLs
NORTHWIND_URL="https://raw.githubusercontent.com/pthom/northwind_psql/master/northwind.sql"
PAGILA_SCHEMA_URL="https://raw.githubusercontent.com/devrimgunduz/pagila/master/pagila-schema.sql"
PAGILA_DATA_URL="https://raw.githubusercontent.com/devrimgunduz/pagila/master/pagila-data.sql"

mkdir -p "$TEST_DIR"

echo "=== Downloading Test Scripts ==="
if [ ! -f "$TEST_DIR/northwind.sql" ]; then
    echo "Downloading Northwind..."
    curl -L "$NORTHWIND_URL" -o "$TEST_DIR/northwind.sql"
fi

if [ ! -f "$TEST_DIR/pagila-schema.sql" ]; then
    echo "Downloading Pagila Schema..."
    curl -L "$PAGILA_SCHEMA_URL" -o "$TEST_DIR/pagila-schema.sql"
fi

if [ ! -f "$TEST_DIR/pagila-data.sql" ]; then
    echo "Downloading Pagila Data..."
    curl -L "$PAGILA_DATA_URL" -o "$TEST_DIR/pagila-data.sql"
fi

echo "=== Preparing Combined Pagila Script ==="
cat "$TEST_DIR/pagila-schema.sql" "$TEST_DIR/pagila-data.sql" > "$TEST_DIR/pagila-all.sql"

echo "=== Building pgqt ==="
cargo build --release

echo "=== Starting PGQT Proxy ==="
# Cleanup old DB
rm -f "$DB_PATH"

# Start proxy in background via pi process tool if available, but for a script we'll use a standard background process
./target/release/pgqt --port "$PROXY_PORT" --database "$DB_PATH" > "$LOG_FILE" 2>&1 &
PROXY_PID=$!

# Ensure proxy is killed on exit
trap "kill $PROXY_PID 2>/dev/null || true" EXIT

echo "Waiting for proxy to start..."
sleep 2

run_sql() {
    local file=$1
    local name=$2
    echo "--- Running $name ---"
    PGPASSWORD=postgres psql -h 127.0.0.1 -p "$PROXY_PORT" -U postgres -d postgres -f "$file" >> "$LOG_FILE" 2>&1
    if [ $? -eq 0 ]; then
        echo "$name: SUCCESS"
    else
        echo "$name: FAILED (Check $LOG_FILE for details)"
    fi
}

echo "=== Running Northwind Tests ==="
run_sql "$TEST_DIR/northwind.sql" "Northwind (Full)"

echo "=== Running Pagila Tests ==="
# Pagila is more complex and might have many unsupported features
run_sql "$TEST_DIR/pagila-all.sql" "Pagila (Full)"

echo "=== Running Custom Feature Harness ==="
cat <<EOF > "$TEST_DIR/harness.sql"
-- Sanity select from Northwind
SELECT COUNT(*) AS customers_cnt FROM customers;

-- Join + aggregation
SELECT c.customer_id, COUNT(o.order_id) AS order_count
FROM customers c
JOIN orders o ON c.customer_id = o.customer_id
GROUP BY c.customer_id
ORDER BY order_count DESC
LIMIT 10;

-- Subquery + HAVING
SELECT c.company_name
FROM customers c
WHERE c.customer_id IN (
  SELECT o.customer_id
  FROM orders o
  GROUP BY o.customer_id
  HAVING COUNT(*) > 5
);
EOF

run_sql "$TEST_DIR/harness.sql" "Feature Harness"

echo "=== Test Summary ==="
grep -E "SUCCESS|FAILED" "$LOG_FILE" | tail -n 5

echo "Full logs available in $LOG_FILE"
