import pytest
import psycopg2
import subprocess
import time
import os
import signal

# Configuration
PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5435  # Use a different port than default to avoid collisions
DB_PATH = "test-suite/test_db.db"
PG_DSN = os.environ.get("PG_DSN", "host=localhost port=5432 user=postgres password=postgres dbname=postgres")

@pytest.fixture(scope="session")
def pgqt_proxy():
    """Starts the PGQT proxy as a background process."""
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    # Ensure binary is built
    subprocess.run(["cargo", "build", "--release"], check=True)
    
    cmd = ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    
    # Wait for proxy to start
    time.sleep(2)
    yield proc
    
    # Cleanup
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

@pytest.fixture
def pg_conn():
    """Connection to the reference PostgreSQL instance."""
    try:
        conn = psycopg2.connect(PG_DSN)
        conn.autocommit = True
        yield conn
        conn.close()
    except Exception as e:
        pytest.skip(f"Reference PostgreSQL not available: {e}")

@pytest.fixture
def proxy_conn(pgqt_proxy):
    """Connection to the PGQT proxy."""
    dsn = f"host={PROXY_HOST} port={PROXY_PORT} user=postgres password=postgres dbname=postgres"
    conn = psycopg2.connect(dsn)
    conn.autocommit = True
    yield conn
    conn.close()
