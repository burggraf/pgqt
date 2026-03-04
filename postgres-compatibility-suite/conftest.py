import pytest
import psycopg2
import subprocess
import time
import os
import signal

# Configuration
PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5435  # Use a different port than default to avoid collisions
# Determine paths relative to project root
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DB_PATH = os.path.join(PROJECT_ROOT, "postgres-compatibility-suite", "test_db.db")
PGQT_BINARY = os.path.join(PROJECT_ROOT, "target", "release", "pgqt")
PG_DSN = os.environ.get("PG_DSN", "host=localhost port=5432 user=postgres password=postgres dbname=postgres")

@pytest.fixture(scope="session")
def pgqt_proxy():
    """Starts the PGQT proxy as a background process."""
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

    # Ensure binary is built
    subprocess.run(["cargo", "build", "--release"], check=True)

    cmd = [PGQT_BINARY, "--port", str(PROXY_PORT), "--database", DB_PATH]
    # Use DEVNULL to avoid buffer filling issues that cause hangs
    with open(os.path.join(PROJECT_ROOT, "postgres-compatibility-suite", "test_db.db.error.log"), "w") as err_log:
        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=err_log)
    
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

@pytest.fixture(autouse=True)
def cleanup_databases(pg_conn, proxy_conn):
    """Auto-use fixture that drops all tables before each test to ensure isolation."""
    # Drop tables from PostgreSQL reference
    drop_all_tables(pg_conn, "pg_catalog")

    # Drop tables from PGQT proxy
    drop_all_tables(proxy_conn, "sqlite_master")

def drop_all_tables(conn, catalog_table):
    """Drop all tables from a database connection."""
    try:
        cur = conn.cursor()

        if catalog_table == "pg_catalog":
            # PostgreSQL: get tables from pg_tables
            cur.execute("""
                SELECT tablename FROM pg_tables
                WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
            """)
            tables = [row[0] for row in cur.fetchall()]

            # Also drop schemas if any
            cur.execute("""
                SELECT nspname FROM pg_namespace
                WHERE nspname NOT IN ('pg_catalog', 'information_schema', 'public')
                AND nspname NOT LIKE 'pg_toast%'
                AND nspname NOT LIKE 'pg_temp_%'
            """)
            schemas = [row[0] for row in cur.fetchall()]

            # Drop schemas first (which cascades to tables)
            for schema in schemas:
                try:
                    cur.execute(f'DROP SCHEMA IF EXISTS "{schema}" CASCADE')
                except Exception:
                    pass

            # Drop any remaining tables
            for table in tables:
                try:
                    cur.execute(f'DROP TABLE IF EXISTS "{table}" CASCADE')
                except Exception:
                    pass

        else:
            # SQLite/PGQT: get tables from sqlite_master
            cur.execute("""
                SELECT name FROM sqlite_master
                WHERE type = 'table'
                AND name NOT LIKE 'sqlite_%'
                AND name NOT LIKE '__pg_%'
            """)
            tables = [row[0] for row in cur.fetchall()]

            for table in tables:
                try:
                    cur.execute(f'DROP TABLE IF EXISTS "{table}"')
                except Exception:
                    pass

        cur.close()
    except Exception as e:
        # Don't fail tests if cleanup fails, just log it
        print(f"Warning: Cleanup failed: {e}")
