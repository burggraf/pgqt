#!/usr/bin/env python3
"""
End-to-end tests for PostgreSQL introspection commands using psql.
Tests: \\d (list tables), \\df (list functions), \\du (list roles)
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_introspection_psql_e2e.db"

def start_proxy():
    """Start the PGQT proxy server."""
    # Clean up old db
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    env = os.environ.copy()
    env["RUST_LOG"] = "error"
    
    proc = subprocess.Popen(
        ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        preexec_fn=os.setsid if hasattr(os, 'setsid') else None,
        env=env,
        cwd="/Users/markb/dev/pgqt"
    )
    
    # Wait for proxy to start
    time.sleep(2)
    
    # Verify it's running
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        conn.close()
    except Exception as e:
        proc.kill()
        raise RuntimeError(f"Failed to connect to proxy: {e}")
    
    return proc

def stop_proxy(proc):
    """Stop the PGQT proxy server."""
    if hasattr(os, 'killpg'):
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
    else:
        proc.terminate()
    proc.wait()
    
    # Clean up
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

def test_psql_d_command():
    """Test psql \\d command works without errors."""
    proc = start_proxy()
    try:
        # Run psql \d command
        result = subprocess.run(
            ["psql", "-h", PROXY_HOST, "-p", str(PROXY_PORT), 
             "-U", "postgres", "-d", "postgres", "-c", "\\d"],
            capture_output=True,
            text=True,
            env={**os.environ, "PGPASSWORD": "postgres"}
        )
        
        print(f"psql \\d stdout: {result.stdout}")
        print(f"psql \\d stderr: {result.stderr}")
        print(f"psql \\d return code: {result.returncode}")
        
        # Should not have any "Did not find any" column errors
        assert result.returncode == 0, f"psql \\d failed with code {result.returncode}: {result.stderr}"
        assert "column" not in result.stderr.lower() or "did not find any" not in result.stderr.lower(), \
            f"psql \\d reported missing columns: {result.stderr}"
        
        print("test_psql_d_command: PASSED")
    finally:
        stop_proxy(proc)

def test_psql_df_command():
    """Test psql \\df command works without errors."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create a test function
        cur.execute("CREATE OR REPLACE FUNCTION test_add(a int, b int) RETURNS int AS $$ SELECT a + b $$ LANGUAGE sql")
        conn.commit()
        cur.close()
        conn.close()
        
        # Run psql \df command
        result = subprocess.run(
            ["psql", "-h", PROXY_HOST, "-p", str(PROXY_PORT), 
             "-U", "postgres", "-d", "postgres", "-c", "\\df"],
            capture_output=True,
            text=True,
            env={**os.environ, "PGPASSWORD": "postgres"}
        )
        
        print(f"psql \\df stdout: {result.stdout}")
        print(f"psql \\df stderr: {result.stderr}")
        print(f"psql \\df return code: {result.returncode}")
        
        # Should not have any "Did not find any" column errors
        assert result.returncode == 0, f"psql \\df failed with code {result.returncode}: {result.stderr}"
        assert "column" not in result.stderr.lower() or "did not find any" not in result.stderr.lower(), \
            f"psql \\df reported missing columns: {result.stderr}"
        
        print("test_psql_df_command: PASSED")
    finally:
        stop_proxy(proc)

def test_psql_du_command():
    """Test psql \\du command works without errors."""
    proc = start_proxy()
    try:
        # Run psql \du command
        result = subprocess.run(
            ["psql", "-h", PROXY_HOST, "-p", str(PROXY_PORT), 
             "-U", "postgres", "-d", "postgres", "-c", "\\du"],
            capture_output=True,
            text=True,
            env={**os.environ, "PGPASSWORD": "postgres"}
        )
        
        print(f"psql \\du stdout: {result.stdout}")
        print(f"psql \\du stderr: {result.stderr}")
        print(f"psql \\du return code: {result.returncode}")
        
        # Should not have any "Did not find any" column errors
        assert result.returncode == 0, f"psql \\du failed with code {result.returncode}: {result.stderr}"
        assert "column" not in result.stderr.lower() or "did not find any" not in result.stderr.lower(), \
            f"psql \\du reported missing columns: {result.stderr}"
        
        print("test_psql_du_command: PASSED")
    finally:
        stop_proxy(proc)

def test_pg_proc_proargtypes():
    """Test that pg_proc.proargtypes returns proper type OIDs."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create a test function with typed arguments
        cur.execute("CREATE OR REPLACE FUNCTION test_typed(a int, b text) RETURNS text AS $$ SELECT b || a::text $$ LANGUAGE sql")
        conn.commit()
        
        # Query pg_proc for the function
        cur.execute("""
            SELECT proname, proargtypes 
            FROM pg_proc 
            WHERE proname = 'test_typed'
        """)
        row = cur.fetchone()
        print(f"test_typed function: proname={row[0]}, proargtypes={row[1]}")
        
        # proargtypes should not be NULL, it should contain OIDs
        assert row[1] is not None, "proargtypes should not be NULL"
        assert row[1] != '', "proargtypes should not be empty for functions with arguments"
        
        cur.close()
        conn.close()
        print("test_pg_proc_proargtypes: PASSED")
    finally:
        stop_proxy(proc)

def test_pg_proc_proacl():
    """Test that pg_proc.proacl returns ACL string."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Query pg_proc for built-in functions
        cur.execute("""
            SELECT proname, proacl 
            FROM pg_proc 
            WHERE proname IN ('now', 'current_timestamp')
            LIMIT 2
        """)
        rows = cur.fetchall()
        
        for row in rows:
            print(f"Function {row[0]}: proacl={row[1]}")
            # proacl should not be NULL, it should be '{}' or an ACL string
            assert row[1] is not None, f"proacl for {row[0]} should not be NULL"
            assert row[1] == '{}', f"proacl for built-in {row[0]} should be '{{}}'"
        
        cur.close()
        conn.close()
        print("test_pg_proc_proacl: PASSED")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    test_psql_d_command()
    test_psql_df_command()
    test_psql_du_command()
    test_pg_proc_proargtypes()
    test_pg_proc_proacl()