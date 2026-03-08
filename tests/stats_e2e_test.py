#!/usr/bin/env python3
"""End-to-end tests for statistical aggregates."""
import subprocess
import time
import psycopg2
import os
import sys
import signal
import math

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_stats_e2e.db"

def start_proxy():
    # Build first to ensure target exists
    subprocess.run(["cargo", "build"], check=True, capture_output=True)
    
    env = os.environ.copy()
    env["RUST_LOG"] = "info"
    proc = subprocess.Popen(
        ["./target/debug/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    # Wait for proxy to start by attempting to connect
    max_retries = 10
    for i in range(max_retries):
        try:
            conn = psycopg2.connect(
                host=PROXY_HOST,
                port=PROXY_PORT,
                database="postgres",
                user="postgres",
                password="postgres",
                connect_timeout=1
            )
            conn.close()
            return proc
        except psycopg2.OperationalError:
            time.sleep(1)
    
    # If we get here, the proxy failed to start
    stdout, stderr = proc.communicate(timeout=1)
    print(f"Proxy failed to start. Stdout: {stdout.decode()}\nStderr: {stderr.decode()}")
    raise RuntimeError("Proxy failed to start")

def stop_proxy(proc):
    proc.send_signal(signal.SIGTERM)
    proc.wait()

def test_regr_functions():
    """Test linear regression functions."""
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
        
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
        
        # Create test table
        cur.execute("CREATE TABLE data (x FLOAT, y FLOAT)")
        cur.execute("INSERT INTO data VALUES (1, 2), (2, 3), (3, 5), (4, 4)")
        conn.commit()
        
        # Test regr_count
        cur.execute("SELECT regr_count(y, x) FROM data")
        result = cur.fetchone()[0]
        assert int(result) == 4, f"Expected 4, got {result} (type: {type(result)})"
        
        # Test regr_slope
        # X: 1,2,3,4 -> sxx = 5.0
        # Y: 2,3,5,4 -> sxy = 4.0
        # slope = 4.0 / 5.0 = 0.8
        cur.execute("SELECT regr_slope(y, x) FROM data")
        result = float(cur.fetchone()[0])
        expected = 0.8
        assert abs(result - expected) < 0.0001, f"Expected {expected}, got {result}"
        
        # Test corr
        # sxx = 5.0, syy = 5.0, sxy = 4.0
        # corr = 4.0 / sqrt(5.0 * 5.0) = 0.8
        cur.execute("SELECT corr(y, x) FROM data")
        result = float(cur.fetchone()[0])
        expected = 0.8
        assert abs(result - expected) < 0.0001, f"Expected {expected}, got {result}"

        # Test regr_intercept
        # avg_y = 3.5, avg_x = 2.5, slope = 0.8
        # intercept = 3.5 - 0.8 * 2.5 = 1.5
        cur.execute("SELECT regr_intercept(y, x) FROM data")
        result = float(cur.fetchone()[0])
        expected = 1.5
        assert abs(result - expected) < 0.0001, f"Expected {expected}, got {result}"
        
        cur.close()
        conn.close()
        print("test_regr_functions: PASSED")
    finally:
        stop_proxy(proc)
        if os.path.exists(DB_PATH):
            os.remove(DB_PATH)

if __name__ == "__main__":
    test_regr_functions()
