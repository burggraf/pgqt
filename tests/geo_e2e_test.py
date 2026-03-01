#!/usr/bin/env python3
"""
End-to-end tests for geometric types functionality.
Requires psycopg2 and a running pglite-proxy server.

Usage:
    # Start the server first:
    cargo run -- --port 5433 --database /tmp/test_geo_e2e.db
    
    # Run tests:
    python3 tests/geo_e2e_test.py
"""

import sys
import os
import time
import subprocess

try:
    import psycopg2
except ImportError:
    print("psycopg2 not installed. Run: pip install psycopg2-binary")
    sys.exit(1)

# Configuration
DB_HOST = "127.0.0.1"
DB_PORT = 5433
DB_USER = "postgres"
DB_NAME = "test"

def get_connection():
    """Get a database connection."""
    return psycopg2.connect(
        host=DB_HOST,
        port=DB_PORT,
        user=DB_USER,
        database=DB_NAME
    )

def test_point_type():
    """Test point type creation, insertion, and distance."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS points")
    cur.execute("CREATE TABLE points (id SERIAL PRIMARY KEY, p POINT)")
    cur.execute("INSERT INTO points (p) VALUES ('(1, 2)'), ('(4, 6)')")
    conn.commit()
    
    cur.execute("SELECT p <-> '(1, 2)' FROM points ORDER BY id")
    distances = [float(row[0]) for row in cur.fetchall()]
    
    assert distances[0] == 0.0, f"Expected 0.0, got {distances[0]}"
    # distance between (1,2) and (4,6) is sqrt(3^2 + 4^2) = 5.0
    assert distances[1] == 5.0, f"Expected 5.0, got {distances[1]}"
    
    cur.close()
    conn.close()
    print("✓ test_point_type passed")

def test_box_operators():
    """Test box type and its spatial operators."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS boxes")
    cur.execute("CREATE TABLE boxes (id SERIAL PRIMARY KEY, b BOX)")
    # Box 1: (0,0) to (2,2) -> ((2,2),(0,0))
    # Box 2: (1,1) to (3,3) -> ((3,3),(1,1))
    # Box 3: (4,4) to (5,5) -> ((5,5),(4,4))
    cur.execute("INSERT INTO boxes (b) VALUES ('(0,0),(2,2)'), ('(1,1),(3,3)'), ('(4,4),(5,5)')")
    conn.commit()
    
    # Test overlaps (&&)
    cur.execute("SELECT id FROM boxes WHERE b && '(1,1),(2,2)' ORDER BY id")
    ids = [row[0] for row in cur.fetchall()]
    assert ids == [1, 2], f"Expected [1, 2], got {ids}"
    
    # Test contains (@>)
    cur.execute("SELECT id FROM boxes WHERE b @> '(0.5,0.5),(1.5,1.5)' ORDER BY id")
    ids = [row[0] for row in cur.fetchall()]
    assert ids == [1, 2], f"Expected [1, 2], got {ids}"
    
    # Test contained in (<@)
    cur.execute("SELECT id FROM boxes WHERE '(0,0),(10,10)' @> b ORDER BY id")
    ids = [row[0] for row in cur.fetchall()]
    assert ids == [1, 2, 3], f"Expected [1, 2, 3], got {ids}"

    # Test left (<<)
    cur.execute("SELECT id FROM boxes WHERE b << '(3.5,0),(6,6)' ORDER BY id")
    ids = [row[0] for row in cur.fetchall()]
    assert ids == [1, 2], f"Expected [1, 2], got {ids}"
    
    # Test right (>>)
    cur.execute("SELECT id FROM boxes WHERE b >> '(0,0),(3.5,3.5)' ORDER BY id")
    ids = [row[0] for row in cur.fetchall()]
    assert ids == [3], f"Expected [3], got {ids}"

    cur.close()
    conn.close()
    print("✓ test_box_operators passed")

def test_below_above_operators():
    """Test below (<<|) and above (|>>) operators for boxes."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS boxes_y")
    cur.execute("CREATE TABLE boxes_y (id SERIAL PRIMARY KEY, b BOX)")
    cur.execute("INSERT INTO boxes_y (b) VALUES ('(0,0),(2,2)'), ('(0,4),(2,6)')")
    conn.commit()
    
    # Test below (<<|)
    cur.execute("SELECT id FROM boxes_y WHERE b <<| '(0,3),(2,3.5)'")
    ids = [row[0] for row in cur.fetchall()]
    assert ids == [1], f"Expected [1], got {ids}"
    
    # Test above (|>>)
    cur.execute("SELECT id FROM boxes_y WHERE b |>> '(0,3),(2,3.5)'")
    ids = [row[0] for row in cur.fetchall()]
    assert ids == [2], f"Expected [2], got {ids}"
    
    cur.close()
    conn.close()
    print("✓ test_below_above_operators passed")

def main():
    db_file = "/tmp/test_geo_e2e_direct.db"
    if os.path.exists(db_file):
        os.remove(db_file)
        
    # Start proxy server
    print(f"Starting pglite-proxy on port {DB_PORT}...")
    server_proc = subprocess.Popen(
        ["cargo", "run", "--", "--port", str(DB_PORT), "--database", db_file],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    
    # Wait for server to start
    print("Waiting for server to initialize...")
    time.sleep(10)
    
    try:
        test_point_type()
        test_box_operators()
        test_below_above_operators()
        print("\nAll E2E geometric tests passed!")
    except Exception as e:
        print(f"\nTests failed: {e}")
        # Print server output for debugging
        stdout, stderr = server_proc.communicate(timeout=1)
        print("Server stdout:", stdout.decode())
        print("Server stderr:", stderr.decode())
        sys.exit(1)
    finally:
        server_proc.terminate()
        server_proc.wait()
        if os.path.exists(db_file):
            os.remove(db_file)

if __name__ == "__main__":
    main()
