#!/bin/bash
set -e

# Directory for SQL files
SQL_DIR="postgres-compatability-suite/sql/pg_regress"
EXPECT_DIR="postgres-compatability-suite/expected/pg_regress"

mkdir -p "$SQL_DIR"
mkdir -p "$EXPECT_DIR"

# Base URL for raw GitHub files
PG_RAW_URL="https://raw.githubusercontent.com/postgres/postgres/master/src/test/regress"

# List of test files to download initially
TESTS=(
    "boolean"
    "int2"
    "int4"
    "int8"
    "float4"
    "float8"
    "numeric"
    "strings"
    "char"
    "varchar"
    "text"
    "uuid"
    "json"
    "jsonb"
    "arrays"
    "date"
    "time"
    "timestamp"
    "timestamptz"
    "interval"
    "create_table"
    "insert"
    "update"
    "delete"
    "select"
    "select_distinct"
    "union"
    "case"
    "join"
    "aggregates"
    "window"
    "limit"
    "with"
    "subselect"
)

echo "Downloading PostgreSQL regression tests..."

for test in "${TESTS[@]}"; do
    if [[ -f "$SQL_DIR/$test.sql" && -f "$EXPECT_DIR/$test.out" ]]; then
        echo "  - $test (exists)"
        continue
    fi
    echo "  - $test"
    curl -s -o "$SQL_DIR/$test.sql" "$PG_RAW_URL/sql/$test.sql"
    curl -s -o "$EXPECT_DIR/$test.out" "$PG_RAW_URL/expected/$test.out"
    sleep 1
done

echo "Done."
