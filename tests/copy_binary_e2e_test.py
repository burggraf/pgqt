#!/usr/bin/env python3
"""
End-to-end tests for COPY BINARY format support.
Tests binary COPY FROM and COPY TO operations.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal
import struct

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_copy_binary_e2e.db"

def start_proxy():
    """Start the pgqt proxy server."""
    # Clean up any existing database
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    proc = subprocess.Popen(
        ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    # Wait for server to start
    time.sleep(1)
    return proc

def stop_proxy(proc):
    """Stop the proxy server."""
    proc.send_signal(signal.SIGTERM)
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    # Clean up
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

def create_binary_copy_data():
    """Create binary COPY data for testing various types."""
    data = bytearray()
    
    # File header
    data.extend(b"PGCOPY\n\xff\r\n\0")  # Signature (11 bytes)
    data.extend(struct.pack('>i', 0))    # Flags (4 bytes)
    data.extend(struct.pack('>i', 0))    # Header extension length (4 bytes)
    
    # Row 1: (1, 'hello', true, 3.14, NULL)
    data.extend(struct.pack('>h', 5))    # 5 columns
    # Column 1: id (int4) = 1
    data.extend(struct.pack('>i', 4))    # length = 4
    data.extend(struct.pack('>i', 1))    # value = 1
    # Column 2: name (text) = 'hello'
    data.extend(struct.pack('>i', 5))    # length = 5
    data.extend(b'hello')                 # value
    # Column 3: active (bool) = true
    data.extend(struct.pack('>i', 1))    # length = 1
    data.extend(struct.pack('b', 1))     # value = true
    # Column 4: score (float8) = 3.14
    data.extend(struct.pack('>i', 8))    # length = 8
    data.extend(struct.pack('>d', 3.14)) # value = 3.14
    # Column 5: notes (text) = NULL
    data.extend(struct.pack('>i', -1))   # length = -1 (NULL)
    
    # Row 2: (2, 'world', false, 2.71, 'test')
    data.extend(struct.pack('>h', 5))    # 5 columns
    # Column 1: id (int4) = 2
    data.extend(struct.pack('>i', 4))    # length = 4
    data.extend(struct.pack('>i', 2))    # value = 2
    # Column 2: name (text) = 'world'
    data.extend(struct.pack('>i', 5))    # length = 5
    data.extend(b'world')                 # value
    # Column 3: active (bool) = false
    data.extend(struct.pack('>i', 1))    # length = 1
    data.extend(struct.pack('b', 0))     # value = false
    # Column 4: score (float8) = 2.71
    data.extend(struct.pack('>i', 8))    # length = 8
    data.extend(struct.pack('>d', 2.71)) # value = 2.71
    # Column 5: notes (text) = 'test'
    data.extend(struct.pack('>i', 4))    # length = 4
    data.extend(b'test')                  # value
    
    # Trailer
    data.extend(struct.pack('>h', -1))   # End marker
    
    return bytes(data)

def test_binary_copy_from():
    """Test COPY FROM with binary format."""
    print("test_binary_copy_from: Starting...")
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
        cur.execute("""
            CREATE TABLE test_binary (
                id INT,
                name TEXT,
                active BOOLEAN,
                score FLOAT8,
                notes TEXT
            )
        """)
        conn.commit()
        
        # Create binary COPY data
        binary_data = create_binary_copy_data()
        
        # Import using binary COPY
        import io
        cur.copy_expert("COPY test_binary FROM STDIN WITH (FORMAT binary)", io.BytesIO(binary_data))
        conn.commit()
        
        # Verify data
        cur.execute("SELECT COUNT(*) FROM test_binary")
        count = cur.fetchone()[0]
        assert int(count) == 2, f"Expected 2 rows, got {count}"
        
        # Verify row 1
        cur.execute("SELECT id, name, active, score, notes FROM test_binary WHERE id = 1")
        row = cur.fetchone()
        assert row is not None, "Row 1 not found"
        assert row[0] == 1, f"Expected id=1, got {row[0]}"
        assert row[1] == 'hello', f"Expected name='hello', got {row[1]}"
        assert row[2] == True, f"Expected active=True, got {row[2]}"
        assert abs(row[3] - 3.14) < 0.001, f"Expected score=3.14, got {row[3]}"
        assert row[4] is None, f"Expected notes=None, got {row[4]}"
        
        # Verify row 2
        cur.execute("SELECT id, name, active, score, notes FROM test_binary WHERE id = 2")
        row = cur.fetchone()
        assert row is not None, "Row 2 not found"
        assert row[0] == 2, f"Expected id=2, got {row[0]}"
        assert row[1] == 'world', f"Expected name='world', got {row[1]}"
        assert row[2] == False, f"Expected active=False, got {row[2]}"
        assert abs(row[3] - 2.71) < 0.001, f"Expected score=2.71, got {row[3]}"
        assert row[4] == 'test', f"Expected notes='test', got {row[4]}"
        
        cur.close()
        conn.close()
        print("test_binary_copy_from: PASSED")
    finally:
        stop_proxy(proc)

def test_binary_copy_int_types():
    """Test binary COPY with various integer types."""
    print("test_binary_copy_int_types: Starting...")
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
        
        # Create test table with different int types
        cur.execute("""
            CREATE TABLE test_ints (
                small_val INT2,
                int_val INT4,
                big_val INT8
            )
        """)
        conn.commit()
        
        # Create binary COPY data with int types
        data = bytearray()
        data.extend(b"PGCOPY\n\xff\r\n\0")
        data.extend(struct.pack('>i', 0))
        data.extend(struct.pack('>i', 0))
        
        # Row 1: (100, 100000, 10000000000)
        data.extend(struct.pack('>h', 3))
        # int2 = 100
        data.extend(struct.pack('>i', 2))
        data.extend(struct.pack('>h', 100))
        # int4 = 100000
        data.extend(struct.pack('>i', 4))
        data.extend(struct.pack('>i', 100000))
        # int8 = 10000000000
        data.extend(struct.pack('>i', 8))
        data.extend(struct.pack('>q', 10000000000))
        
        # Row 2: (-100, -100000, -10000000000)
        data.extend(struct.pack('>h', 3))
        # int2 = -100
        data.extend(struct.pack('>i', 2))
        data.extend(struct.pack('>h', -100))
        # int4 = -100000
        data.extend(struct.pack('>i', 4))
        data.extend(struct.pack('>i', -100000))
        # int8 = -10000000000
        data.extend(struct.pack('>i', 8))
        data.extend(struct.pack('>q', -10000000000))
        
        # Trailer
        data.extend(struct.pack('>h', -1))
        
        # Import
        import io
        cur.copy_expert("COPY test_ints FROM STDIN WITH (FORMAT binary)", io.BytesIO(bytes(data)))
        conn.commit()
        
        # Verify
        cur.execute("SELECT * FROM test_ints ORDER BY small_val")
        rows = cur.fetchall()
        assert len(rows) == 2
        
        # Row 1 (negative values)
        assert rows[0][0] == -100
        assert rows[0][1] == -100000
        assert rows[0][2] == -10000000000
        
        # Row 2 (positive values)
        assert rows[1][0] == 100
        assert rows[1][1] == 100000
        assert rows[1][2] == 10000000000
        
        cur.close()
        conn.close()
        print("test_binary_copy_int_types: PASSED")
    finally:
        stop_proxy(proc)

def test_binary_copy_to():
    """Test COPY TO with binary format."""
    print("test_binary_copy_to: Starting...")
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
        
        # Create test table and insert data
        cur.execute("CREATE TABLE test_copy_to (id INT, name TEXT)")
        cur.execute("INSERT INTO test_copy_to VALUES (1, 'alice'), (2, 'bob')")
        conn.commit()
        
        # Export to binary format
        import io
        output = io.BytesIO()
        cur.copy_expert("COPY test_copy_to TO STDOUT WITH (FORMAT binary)", output)
        binary_data = output.getvalue()
        
        # Verify binary header
        assert binary_data[:11] == b"PGCOPY\n\xff\r\n\0", "Invalid binary signature"
        
        # Verify we got some data (at least header + rows + trailer)
        assert len(binary_data) > 19, f"Binary data too short: {len(binary_data)} bytes"
        
        cur.close()
        conn.close()
        print("test_binary_copy_to: PASSED")
    finally:
        stop_proxy(proc)

def test_binary_copy_roundtrip():
    """Test round-trip: COPY TO binary then COPY FROM binary."""
    print("test_binary_copy_roundtrip: Starting...")
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
        
        # Create test table and insert data
        cur.execute("CREATE TABLE test_roundtrip (id INT, name TEXT, active BOOLEAN)")
        cur.execute("INSERT INTO test_roundtrip VALUES (1, 'test1', true), (2, 'test2', false)")
        conn.commit()
        
        # Export to binary format
        import io
        output = io.BytesIO()
        cur.copy_expert("COPY test_roundtrip TO STDOUT WITH (FORMAT binary)", output)
        binary_data = output.getvalue()
        
        # Clear table
        cur.execute("DELETE FROM test_roundtrip")
        conn.commit()
        
        # Import back from binary
        import io
        cur.copy_expert("COPY test_roundtrip FROM STDIN WITH (FORMAT binary)", io.BytesIO(binary_data))
        conn.commit()
        
        # Verify data
        cur.execute("SELECT * FROM test_roundtrip ORDER BY id")
        rows = cur.fetchall()
        assert len(rows) == 2
        assert rows[0] == (1, 'test1', True)
        assert rows[1] == (2, 'test2', False)
        
        cur.close()
        conn.close()
        print("test_binary_copy_roundtrip: PASSED")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    # Build release first
    print("Building release binary...")
    result = subprocess.run(["cargo", "build", "--release"], cwd="/Users/markb/dev/pgqt")
    if result.returncode != 0:
        print("Build failed!")
        sys.exit(1)
    
    test_binary_copy_from()
    test_binary_copy_int_types()
    test_binary_copy_to()
    test_binary_copy_roundtrip()
    print("\nAll binary COPY tests passed!")
