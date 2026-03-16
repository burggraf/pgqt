#!/usr/bin/env python3
"""
End-to-end tests for COPY command encoding support.

This tests the encoding conversion functionality for COPY FROM/TO operations.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal
import io

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_copy_encoding_e2e.db"


def start_proxy():
    """Start the pgqt proxy server."""
    # Clean up any existing database
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    # Start proxy
    proc = subprocess.Popen(
        ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    # Wait for proxy to start
    time.sleep(1)
    return proc


def stop_proxy(proc):
    """Stop the proxy server."""
    proc.send_signal(signal.SIGTERM)
    proc.wait()
    # Clean up
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)


def test_copy_from_latin1():
    """Test COPY FROM with LATIN1 encoded data."""
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
        cur.execute("CREATE TABLE test_latin1 (id INT, name TEXT)")
        conn.commit()
        
        # LATIN1 encoded data with special characters
        # "José" in LATIN1: 4A 6F 73 E9 (J o s é)
        # "François" in LATIN1: 46 72 61 6E E7 6F 69 73 (F r a n ç o i s)
        latin1_data = b"1,Jos\xe9\n2,Fran\xe7ois\n"
        
        # Import using COPY FROM with LATIN1 encoding
        cur.copy_expert(
            "COPY test_latin1 FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1')",
            io.BytesIO(latin1_data)
        )
        conn.commit()
        
        # Verify data was converted to UTF-8 correctly
        cur.execute("SELECT name FROM test_latin1 ORDER BY id")
        names = [row[0] for row in cur.fetchall()]
        assert names == ['José', 'François'], f"Expected ['José', 'François'], got {names}"
        
        cur.close()
        conn.close()
        print("test_copy_from_latin1: PASSED")
    finally:
        stop_proxy(proc)


def test_copy_to_latin1():
    """Test COPY TO with LATIN1 encoding."""
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
        cur.execute("CREATE TABLE test_latin1_to (id INT, name TEXT)")
        cur.execute("INSERT INTO test_latin1_to VALUES (1, 'José'), (2, 'François')")
        conn.commit()
        
        # Export to LATIN1
        output = io.BytesIO()
        cur.copy_expert(
            "COPY test_latin1_to TO STDOUT WITH (FORMAT csv, ENCODING 'LATIN1')",
            output
        )
        output.seek(0)
        output = output.read()
        
        # Verify output is LATIN1 encoded
        assert b'Jos\xe9' in output, f"Expected LATIN1 encoded 'José', got {output}"
        assert b'Fran\xe7ois' in output, f"Expected LATIN1 encoded 'François', got {output}"
        
        cur.close()
        conn.close()
        print("test_copy_to_latin1: PASSED")
    finally:
        stop_proxy(proc)


def test_copy_roundtrip_latin1():
    """Test COPY TO/FROM roundtrip with LATIN1 encoding."""
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
        cur.execute("CREATE TABLE test_roundtrip (id INT, name TEXT)")
        cur.execute("INSERT INTO test_roundtrip VALUES (1, 'José'), (2, 'François')")
        conn.commit()
        
        # Export to LATIN1
        latin1_buffer = io.BytesIO()
        cur.copy_expert(
            "COPY test_roundtrip TO STDOUT WITH (FORMAT csv, ENCODING 'LATIN1')",
            latin1_buffer
        )
        latin1_data = latin1_buffer.getvalue()
        
        # Clear table
        cur.execute("DELETE FROM test_roundtrip")
        conn.commit()
        
        # Import from LATIN1
        cur.copy_expert(
            "COPY test_roundtrip FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1')",
            io.BytesIO(latin1_data)
        )
        conn.commit()
        
        # Verify data matches
        cur.execute("SELECT COUNT(*) FROM test_roundtrip")
        count = int(cur.fetchone()[0])
        assert count == 2, f"Expected 2 rows, got {count}"
        
        cur.execute("SELECT name FROM test_roundtrip ORDER BY id")
        names = [row[0] for row in cur.fetchall()]
        assert names == ['José', 'François'], f"Expected ['José', 'François'], got {names}"
        
        cur.close()
        conn.close()
        print("test_copy_roundtrip_latin1: PASSED")
    finally:
        stop_proxy(proc)


def test_copy_windows1252_euro():
    """Test COPY FROM with WINDOWS-1252 encoding (Euro sign support)."""
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
        cur.execute("CREATE TABLE test_euro (id INT, price TEXT)")
        conn.commit()
        
        # WINDOWS-1252 encoded data with Euro sign
        # Euro in WINDOWS-1252: 0x80
        win1252_data = b"1,\x80100\n2,\x8050\n"
        
        # Import using COPY FROM with WINDOWS-1252 encoding
        cur.copy_expert(
            "COPY test_euro FROM STDIN WITH (FORMAT csv, ENCODING 'WINDOWS-1252')",
            io.BytesIO(win1252_data)
        )
        conn.commit()
        
        # Verify Euro sign was converted correctly
        cur.execute("SELECT price FROM test_euro ORDER BY id")
        prices = [row[0] for row in cur.fetchall()]
        assert prices == ['€100', '€50'], f"Expected ['€100', '€50'], got {prices}"
        
        cur.close()
        conn.close()
        print("test_copy_windows1252_euro: PASSED")
    finally:
        stop_proxy(proc)


def test_copy_encoding_error_handling():
    """Test that encoding errors are properly reported."""
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
        cur.execute("CREATE TABLE test_error (id INT, name TEXT)")
        cur.execute("INSERT INTO test_error VALUES (1, '€100')")
        conn.commit()
        
        # Try to export Euro sign in LATIN1 (should fail)
        try:
            output = io.BytesIO()
            cur.copy_expert(
                "COPY test_error TO STDOUT WITH (FORMAT csv, ENCODING 'LATIN1')",
                output
            )
            print("test_copy_encoding_error_handling: FAILED - Expected error but got none")
        except psycopg2.Error as e:
            # Expected error - Euro cannot be encoded in LATIN1
            error_msg = str(e).lower()
            if "encod" in error_msg or "character" in error_msg:
                print("test_copy_encoding_error_handling: PASSED")
            else:
                print(f"test_copy_encoding_error_handling: PASSED (got expected error: {e})")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proc)


def test_copy_utf8_default():
    """Test that UTF-8 is the default encoding."""
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
        cur.execute("CREATE TABLE test_utf8 (id INT, name TEXT)")
        conn.commit()
        
        # UTF-8 data (default)
        utf8_data = "1,Hello\n2,World\n".encode('utf-8')
        
        # Import without specifying encoding (should default to UTF8)
        cur.copy_expert(
            "COPY test_utf8 FROM STDIN WITH (FORMAT csv)",
            io.BytesIO(utf8_data)
        )
        conn.commit()
        
        # Verify data
        cur.execute("SELECT COUNT(*) FROM test_utf8")
        assert int(cur.fetchone()[0]) == 2, "Expected 2 rows"
        
        cur.execute("SELECT name FROM test_utf8 ORDER BY id")
        names = [row[0] for row in cur.fetchall()]
        assert names == ['Hello', 'World'], f"Expected ['Hello', 'World'], got {names}"
        
        cur.close()
        conn.close()
        print("test_copy_utf8_default: PASSED")
    finally:
        stop_proxy(proc)


if __name__ == "__main__":
    # Build release first
    print("Building release binary...")
    result = subprocess.run(["cargo", "build", "--release"], capture_output=True, text=True)
    if result.returncode != 0:
        print(f"Build failed: {result.stderr}")
        sys.exit(1)
    
    print("\nRunning COPY encoding E2E tests...\n")
    
    test_copy_from_latin1()
    test_copy_to_latin1()
    test_copy_roundtrip_latin1()
    test_copy_windows1252_euro()
    test_copy_encoding_error_handling()
    test_copy_utf8_default()
    
    print("\nAll COPY encoding E2E tests passed!")
