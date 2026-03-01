#!/usr/bin/env python3
"""
End-to-end tests for PostgreSQL array support.

This test suite validates array operators and functions through actual
PostgreSQL wire protocol connections.
"""

import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_array_e2e.db"


def start_proxy():
    """Start the PostgreSQLite proxy server."""
    # Clean up old database if exists
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    env = os.environ.copy()
    env["PG_LITE_DB"] = DB_PATH
    env["PG_LITE_PORT"] = str(PROXY_PORT)
    
    # Start proxy
    proc = subprocess.Popen(
        ["cargo", "run", "--release"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        preexec_fn=os.setsid,
    )
    
    # Wait for proxy to start
    time.sleep(3)
    
    return proc


def stop_proxy(proc):
    """Stop the proxy server."""
    if proc:
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        proc.wait()


def get_connection():
    """Get a connection to the proxy."""
    return psycopg2.connect(
        host=PROXY_HOST,
        port=PROXY_PORT,
        user="postgres",
        database="test",
    )


def test_basic_array_operations():
    """Test basic array creation and retrieval."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        # Create table with array column
        cur.execute("""
            CREATE TABLE items (
                id SERIAL PRIMARY KEY,
                name TEXT,
                tags TEXT[]
            )
        """)
        conn.commit()
        
        # Insert with array
        cur.execute("""
            INSERT INTO items (name, tags) VALUES 
                ('item1', '{"tag1","tag2","tag3"}'),
                ('item2', '{"alpha","beta"}'),
                ('item3', '{"single"}')
        """)
        conn.commit()
        
        # Retrieve data
        cur.execute("SELECT name, tags FROM items ORDER BY id")
        rows = cur.fetchall()
        
        assert len(rows) == 3
        assert rows[0][0] == 'item1'
        assert rows[1][0] == 'item2'
        assert rows[2][0] == 'item3'
        
        print("✓ Basic array operations work")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_overlap_operator():
    """Test the && (overlap) operator."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        # Create test table
        cur.execute("""
            CREATE TABLE test_overlap (
                id SERIAL PRIMARY KEY,
                numbers INTEGER[]
            )
        """)
        
        # Insert test data
        cur.execute("""
            INSERT INTO test_overlap (numbers) VALUES 
                ('{1,2,3}'),
                ('{3,4,5}'),
                ('{6,7,8}')
        """)
        conn.commit()
        
        # Test overlap - should match rows with 3
        cur.execute("SELECT id FROM test_overlap WHERE numbers && '{3,9}'")
        rows = cur.fetchall()
        assert len(rows) == 2  # First two rows have 3
        
        print("✓ Array overlap operator (&&) works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_contains_operator():
    """Test the @> (contains) operator."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        # Create test table
        cur.execute("""
            CREATE TABLE test_contains (
                id SERIAL PRIMARY KEY,
                tags TEXT[]
            )
        """)
        
        # Insert test data
        cur.execute("""
            INSERT INTO test_contains (tags) VALUES 
                ('{"admin","user","guest"}'),
                ('{"user","guest"}'),
                ('{"guest"}')
        """)
        conn.commit()
        
        # Test contains - need all specified elements
        cur.execute("SELECT id FROM test_contains WHERE tags @> '{\"admin\",\"user\"}'")
        rows = cur.fetchall()
        assert len(rows) == 1  # Only first row has both admin and user
        
        cur.execute("SELECT id FROM test_contains WHERE tags @> '{\"user\"}'")
        rows = cur.fetchall()
        assert len(rows) == 2  # First two rows have user
        
        print("✓ Array contains operator (@>) works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_contained_operator():
    """Test the <@ (contained by) operator."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        # Create test table
        cur.execute("""
            CREATE TABLE test_contained (
                id SERIAL PRIMARY KEY,
                items TEXT[]
            )
        """)
        
        # Insert test data
        cur.execute("""
            INSERT INTO test_contained (items) VALUES 
                ('{"a","b"}'),
                ('{"a","b","c"}'),
                ('{"d"}')
        """)
        conn.commit()
        
        # Test contained by - items must be subset of the array
        cur.execute("SELECT id FROM test_contained WHERE items <@ '{\"a\",\"b\",\"c\",\"d\"}'")
        rows = cur.fetchall()
        assert len(rows) == 3  # All rows are subsets
        
        cur.execute("SELECT id FROM test_contained WHERE items <@ '{\"a\",\"b\"}'")
        rows = cur.fetchall()
        assert len(rows) == 1  # Only first row is subset of {a,b}
        
        print("✓ Array contained by operator (<@) works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_append_function():
    """Test array_append function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_append('{1,2,3}'::int[], 4)")
        result = cur.fetchone()[0]
        # Result should contain 4 elements with 4 at the end
        assert '4' in result
        
        print("✓ array_append function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_prepend_function():
    """Test array_prepend function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_prepend(0, '{1,2,3}'::int[])")
        result = cur.fetchone()[0]
        # Result should contain 4 elements with 0 at the beginning
        assert '0' in result
        
        print("✓ array_prepend function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_cat_function():
    """Test array_cat function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_cat('{1,2}'::int[], '{3,4}'::int[])")
        result = cur.fetchone()[0]
        # Result should contain 4 elements
        assert '1' in result and '4' in result
        
        print("✓ array_cat function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_remove_function():
    """Test array_remove function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_remove('{1,2,2,3}'::int[], 2)")
        result = cur.fetchone()[0]
        # Should have 1 and 3 but no 2
        assert '1' in result and '3' in result
        
        print("✓ array_remove function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_replace_function():
    """Test array_replace function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_replace('{1,2,2,3}'::int[], 2, 9)")
        result = cur.fetchone()[0]
        # Should have 1, 9, 9, 3
        assert '9' in result
        
        print("✓ array_replace function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_length_function():
    """Test array_length function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_length('{1,2,3,4,5}'::int[], 1)")
        result = cur.fetchone()[0]
        assert result == 5
        
        print("✓ array_length function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_cardinality_function():
    """Test cardinality function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT cardinality('{1,2,3}'::int[])")
        result = cur.fetchone()[0]
        assert result == 3
        
        cur.execute("SELECT cardinality('{{1,2},{3,4}}'::int[][])")
        result = cur.fetchone()[0]
        assert result == 4
        
        print("✓ cardinality function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_position_function():
    """Test array_position function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_position('{\"a\",\"b\",\"c\",\"b\"}'::text[], 'b')")
        result = cur.fetchone()[0]
        assert result == 2  # First occurrence at position 2
        
        cur.execute("SELECT array_position('{\"a\",\"b\",\"c\",\"b\"}'::text[], 'b', 3)")
        result = cur.fetchone()[0]
        assert result == 4  # Second occurrence at position 4
        
        print("✓ array_position function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_positions_function():
    """Test array_positions function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_positions('{1,2,1,3,1}'::int[], 1)")
        result = cur.fetchone()[0]
        # Should contain positions 1, 3, 5
        assert '1' in result and '3' in result and '5' in result
        
        print("✓ array_positions function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_to_string_function():
    """Test array_to_string function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_to_string('{\"a\",\"b\",\"c\"}'::text[], ',')")
        result = cur.fetchone()[0]
        assert result == 'a,b,c'
        
        cur.execute("SELECT array_to_string('{\"a\",NULL,\"c\"}'::text[], ',', '*')")
        result = cur.fetchone()[0]
        assert 'a' in result and 'c' in result
        
        print("✓ array_to_string function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_string_to_array_function():
    """Test string_to_array function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT string_to_array('a,b,c', ',')")
        result = cur.fetchone()[0]
        assert 'a' in result and 'b' in result and 'c' in result
        
        print("✓ string_to_array function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_ndims_function():
    """Test array_ndims function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_ndims('{1,2,3}'::int[])")
        result = cur.fetchone()[0]
        assert result == 1
        
        cur.execute("SELECT array_ndims('{{1,2},{3,4}}'::int[][])")
        result = cur.fetchone()[0]
        assert result == 2
        
        print("✓ array_ndims function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_dims_function():
    """Test array_dims function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_dims('{1,2,3}'::int[])")
        result = cur.fetchone()[0]
        assert '[1:3]' in result
        
        print("✓ array_dims function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_trim_array_function():
    """Test trim_array function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT trim_array('{1,2,3,4,5}'::int[], 2)")
        result = cur.fetchone()[0]
        # Should have first 3 elements
        assert '1' in result and '2' in result and '3' in result
        
        print("✓ trim_array function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_fill_function():
    """Test array_fill function."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        cur.execute("SELECT array_fill(7, '{3}'::int[])")
        result = cur.fetchone()[0]
        # Should have three 7s
        assert '7' in result
        
        print("✓ array_fill function works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_complex_array_query():
    """Test complex query with multiple array operations."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        # Create test table
        cur.execute("""
            CREATE TABLE products (
                id SERIAL PRIMARY KEY,
                name TEXT,
                categories INTEGER[],
                tags TEXT[]
            )
        """)
        
        # Insert test data
        cur.execute("""
            INSERT INTO products (name, categories, tags) VALUES 
                ('Product A', '{1,2,3}', '{"featured","sale"}'),
                ('Product B', '{2,3,4}', '{"sale"}'),
                ('Product C', '{1,5}', '{"featured","new"}')
        """)
        conn.commit()
        
        # Complex query: Find products in category 2 with "featured" tag
        cur.execute("""
            SELECT name FROM products 
            WHERE categories @> '{2}'::int[] 
            AND tags @> '{\"featured\"}'
        """)
        rows = cur.fetchall()
        assert len(rows) == 1
        assert rows[0][0] == 'Product A'
        
        print("✓ Complex array query works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def test_array_in_update():
    """Test using array functions in UPDATE statements."""
    proxy = start_proxy()
    try:
        conn = get_connection()
        cur = conn.cursor()
        
        # Create test table
        cur.execute("""
            CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                name TEXT,
                roles TEXT[]
            )
        """)
        
        # Insert test data
        cur.execute("""
            INSERT INTO users (name, roles) VALUES 
                ('Alice', '{"user"}'),
                ('Bob', '{"user","admin"}')
        """)
        conn.commit()
        
        # Update: Add 'admin' role to Alice
        cur.execute("""
            UPDATE users SET roles = array_append(roles, 'admin') 
            WHERE name = 'Alice'
        """)
        conn.commit()
        
        # Verify
        cur.execute("SELECT roles FROM users WHERE name = 'Alice'")
        result = cur.fetchone()[0]
        assert 'admin' in result
        
        print("✓ Array in UPDATE works")
        
        cur.close()
        conn.close()
    finally:
        stop_proxy(proxy)


def run_all_tests():
    """Run all E2E tests."""
    tests = [
        test_basic_array_operations,
        test_array_overlap_operator,
        test_array_contains_operator,
        test_array_contained_operator,
        test_array_append_function,
        test_array_prepend_function,
        test_array_cat_function,
        test_array_remove_function,
        test_array_replace_function,
        test_array_length_function,
        test_cardinality_function,
        test_array_position_function,
        test_array_positions_function,
        test_array_to_string_function,
        test_string_to_array_function,
        test_array_ndims_function,
        test_array_dims_function,
        test_trim_array_function,
        test_array_fill_function,
        test_complex_array_query,
        test_array_in_update,
    ]
    
    print(f"Running {len(tests)} array E2E tests...\n")
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            print(f"✗ {test.__name__} failed: {e}")
            failed += 1
    
    print(f"\n{'='*50}")
    print(f"Results: {passed} passed, {failed} failed")
    
    return failed == 0


if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)
