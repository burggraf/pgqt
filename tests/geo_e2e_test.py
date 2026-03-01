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

def test_point_type(conn):
    """Test point type creation, insertion, and distance."""
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
    print("✓ test_point_type passed")

def test_box_operators(conn):
    """Test box type and its spatial operators."""
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
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1, 2], f"Expected [1, 2], got {ids}"
    
    # Test contains (@>)
    # Box 1: (0,0),(2,2) contains (0.5,0.5),(1.5,1.5) - yes
    # Box 2: (1,1),(3,3) does NOT contain (0.5,0.5),(1.5,1.5) because 0.5 < 1
    cur.execute("SELECT id FROM boxes WHERE b @> '(0.5,0.5),(1.5,1.5)' ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1], f"Expected [1], got {ids}"
    
    # Test contained in (<@)
    cur.execute("SELECT id FROM boxes WHERE '(0,0),(10,10)' @> b ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1, 2, 3], f"Expected [1, 2, 3], got {ids}"

    # Test left (<<) - strictly left means box.high.x < other.low.x
    # Box 1: high.x=2, Box 2: high.x=3, Box 3: high.x=5
    # Query box: (3.5,0),(6,6) has low.x=3.5
    # Box 1 (high.x=2) < 3.5 - yes
    # Box 2 (high.x=3) < 3.5 - yes  
    cur.execute("SELECT id FROM boxes WHERE b << '(3.5,0),(6,6)' ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1, 2], f"Expected [1, 2], got {ids}"
    
    # Test right (>>) - strictly right means box.low.x > other.high.x
    # Query box: (0,0),(3.5,3.5) has high.x=3.5
    # Box 3: low.x=4 > 3.5 - yes
    cur.execute("SELECT id FROM boxes WHERE b >> '(0,0),(3.5,3.5)' ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [3], f"Expected [3], got {ids}"

    cur.close()
    print("✓ test_box_operators passed")

def test_below_above_operators(conn):
    """Test below (<<|) and above (|>>) operators for boxes."""
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS boxes_y")
    cur.execute("CREATE TABLE boxes_y (id SERIAL PRIMARY KEY, b BOX)")
    cur.execute("INSERT INTO boxes_y (b) VALUES ('(0,0),(2,2)'), ('(0,4),(2,6)')")
    conn.commit()
    
    # Test below (<<|) - strictly below means box.high.y < other.low.y
    # Box 1: high.y=2, Box 2: high.y=6
    # Query box: (0,3),(2,3.5) has low.y=3
    # Box 1 (high.y=2) < 3 - yes
    cur.execute("SELECT id FROM boxes_y WHERE b <<| '(0,3),(2,3.5)'")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1], f"Expected [1], got {ids}"
    
    # Test above (|>>) - strictly above means box.low.y > other.high.y
    # Query box: (0,3),(2,3.5) has high.y=3.5
    # Box 2: low.y=4 > 3.5 - yes
    cur.execute("SELECT id FROM boxes_y WHERE b |>> '(0,3),(2,3.5)'")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [2], f"Expected [2], got {ids}"
    
    cur.close()
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
        stderr=subprocess.STDOUT,
        text=True
    )
    
    # Wait for server to start
    print("Waiting for server to initialize...")
    time.sleep(10)
    
    # In another thread, print server output
    import threading
    def log_server():
        for line in server_proc.stdout:
            print(f"SERVER: {line.strip()}")
    threading.Thread(target=log_server, daemon=True).start()
    
    try:
        conn = get_connection()
        print("Connected to proxy.")
        test_point_type(conn)
        test_box_operators(conn)
        test_below_above_operators(conn)
        conn.close()
        print("\nAll E2E geometric tests passed!")
    except Exception as e:
        print(f"\nTests failed: {e}")
        # traceback
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        server_proc.terminate()
        server_proc.wait()
        if os.path.exists(db_file):
            os.remove(db_file)

if __name__ == "__main__":
    main()
