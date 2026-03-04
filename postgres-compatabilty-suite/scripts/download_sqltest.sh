#!/bin/bash
set -e

# Directory for SQL files
SQLTEST_DIR="postgres-compatability-suite/sql/sqltest"
mkdir -p "$SQLTEST_DIR"

# Base URL for elliotchance/sqltest
SQLTEST_RAW_URL="https://raw.githubusercontent.com/elliotchance/sqltest/master/tests"

# List of test categories to download
CATEGORIES=(
    "aggregates"
    "alter_table"
    "case"
    "cast"
    "create_index"
    "create_table"
    "delete"
    "drop_table"
    "insert"
    "join"
    "select"
    "subqueries"
    "update"
)

echo "Downloading sqltest templates..."

for cat in "${CATEGORIES[@]}"; do
    echo "  - $cat"
    curl -s -o "$SQLTEST_DIR/$cat.sqltest" "$SQLTEST_RAW_URL/$cat.sqltest"
    sleep 1
done

echo "Done."
