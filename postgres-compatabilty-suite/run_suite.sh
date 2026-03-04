#!/bin/bash
# test-suite/run_suite.sh
# Comprehensive runner for the PGQT compatibility test suite.

set -e

# Configuration
export PG_DSN=${PG_DSN:-"host=localhost port=5432 user=postgres password=postgres dbname=postgres"}
export PROXY_PORT=5435
export DB_PATH="test-suite/test_db.db"

# 1. Ensure we are in the project root
if [ ! -d "test-suite" ]; then
    echo "Error: Must run from project root (where test-suite/ exists)"
    exit 1
fi

# 2. Activate virtual environment
if [ ! -d "test-suite/venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv test-suite/venv
    source test-suite/venv/bin/activate
    pip install -r test-suite/requirements.txt
else
    source test-suite/venv/bin/activate
fi

# 3. Build PGQT
echo "Building PGQT in release mode..."
cargo build --release

# 4. Check if Reference Postgres is ready
echo "Checking reference PostgreSQL ($PG_DSN)..."
if ! command -v pg_isready &> /dev/null; then
    echo "Warning: pg_isready not found. Attempting connection anyway..."
else
    if ! pg_isready -h localhost -p 5432 > /dev/null; then
        echo "Error: Reference PostgreSQL not found. Start Postgres to use the ground-truth comparison."
        echo "If using Docker: docker run --name pg-test -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres"
        exit 1
    fi
fi

# 5. Run the suite
echo "Starting Compatibility Test Suite..."
pytest test-suite/runner.py "$@"

# 6. Summary
echo "Test run complete."
