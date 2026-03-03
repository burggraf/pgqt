#!/usr/bin/env python3
"""
End-to-end tests for PostgreSQLite function support.
Tests CREATE FUNCTION, DROP FUNCTION, and function execution through wire protocol.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_function_e2e.db"

def start_proxy():
    """Start the pgqt proxy in the background."""
    subprocess.run("pkill -f pgqt", shell=True)
    if os.path.exists(DB_PATH):
        try:
            os.remove(DB_PATH)
        except:
            pass
    
    proxy_cmd = f"./target/release/pgqt --port {PROXY_PORT} --database {DB_PATH}"
    proc = subprocess.Popen(
        proxy_cmd,
        shell=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        preexec_fn=os.setsid
    )
    
    time.sleep(2)
    return proc

def stop_proxy(proc):
    """Stop the pgqt proxy."""
    try:
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        proc.wait(timeout=5)
    except:
        pass
    
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

def test_simple_scalar_function():
    """Test creating and calling a simple scalar function."""
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
        
        # Create function
        cur.execute("""
            CREATE FUNCTION add_numbers(a integer, b integer)
            RETURNS integer
            LANGUAGE sql
            AS $$
                SELECT a + b
            $$;
        """)
        
        # Call function
        cur.execute("SELECT add_numbers(5, 3)")
        result = cur.fetchone()
        # Result may be returned as string, convert to int for comparison
        assert int(result[0]) == 8, f"Expected 8, got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_simple_scalar_function: PASSED")
    finally:
        stop_proxy(proc)

def test_function_with_out_params():
    """Test function with OUT parameters."""
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
        
        # Create table
        cur.execute("CREATE TABLE users (id INTEGER, username TEXT, email TEXT)")
        cur.execute("INSERT INTO users VALUES (1, 'alice', 'alice@example.com')")
        
        # Create function with OUT params
        cur.execute("""
            CREATE FUNCTION get_user_info(user_id integer, OUT username text, OUT email text)
            LANGUAGE sql
            AS $$
                SELECT username, email FROM users WHERE id = user_id
            $$;
        """)
        
        # Call function
        cur.execute("SELECT * FROM get_user_info(1)")
        result = cur.fetchone()
        assert result[0] == 'alice', f"Expected 'alice', got {result[0]}"
        assert result[1] == 'alice@example.com', f"Expected email, got {result[1]}"
        
        cur.close()
        conn.close()
        print("test_function_with_out_params: PASSED")
    finally:
        stop_proxy(proc)

def test_strict_function():
    """Test STRICT attribute behavior."""
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
        
        # Create STRICT function
        cur.execute("""
            CREATE FUNCTION square(x integer)
            RETURNS integer
            LANGUAGE sql
            STRICT
            AS $$
                SELECT x * x
            $$;
        """)
        
        # Call with NULL - should return NULL
        cur.execute("SELECT square(NULL)")
        result = cur.fetchone()
        assert result[0] is None, f"Expected NULL for STRICT function with NULL input, got {result[0]}"
        
        # Call with value - should work
        cur.execute("SELECT square(5)")
        result = cur.fetchone()
        assert int(result[0]) == 25, f"Expected 25, got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_strict_function: PASSED")
    finally:
        stop_proxy(proc)

def test_returns_table_function():
    """Test RETURNS TABLE function."""
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
        
        # Create table
        cur.execute("CREATE TABLE products (id INTEGER, name TEXT, price REAL)")
        cur.execute("INSERT INTO products VALUES (1, 'Widget', 10.5), (2, 'Gadget', 20.0)")
        
        # Create RETURNS TABLE function
        cur.execute("""
            CREATE FUNCTION get_active_products()
            RETURNS TABLE(id integer, name text, price real)
            LANGUAGE sql
            AS $$
                SELECT id, name, price FROM products
            $$;
        """)
        
        # Call function
        cur.execute("SELECT * FROM get_active_products()")
        results = cur.fetchall()
        assert len(results) == 2, f"Expected 2 rows, got {len(results)}"
        
        cur.close()
        conn.close()
        print("test_returns_table_function: PASSED")
    finally:
        stop_proxy(proc)

def test_drop_function():
    """Test DROP FUNCTION."""
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
        
        # Create function
        cur.execute("""
            CREATE FUNCTION test_func(x integer)
            RETURNS integer
            LANGUAGE sql
            AS $$
                SELECT x * 2
            $$;
        """)
        
        # Call it
        cur.execute("SELECT test_func(5)")
        result = cur.fetchone()
        assert int(result[0]) == 10
        
        # Drop it
        cur.execute("DROP FUNCTION test_func")
        
        # Should fail now
        try:
            cur.execute("SELECT test_func(5)")
            assert False, "Should have failed after DROP FUNCTION"
        except Exception as e:
            pass  # Expected
        
        cur.close()
        conn.close()
        print("test_drop_function: PASSED")
    finally:
        stop_proxy(proc)

def test_create_or_replace():
    """Test CREATE OR REPLACE FUNCTION."""
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
        
        # Create function
        cur.execute("""
            CREATE FUNCTION test_func(x integer)
            RETURNS integer
            LANGUAGE sql
            AS $$
                SELECT x * 2
            $$;
        """)
        
        # Call it
        cur.execute("SELECT test_func(5)")
        result = cur.fetchone()
        assert int(result[0]) == 10
        
        # Replace it
        cur.execute("""
            CREATE OR REPLACE FUNCTION test_func(x integer)
            RETURNS integer
            LANGUAGE sql
            AS $$
                SELECT x * 3
            $$;
        """)
        
        # Should use new implementation
        cur.execute("SELECT test_func(5)")
        result = cur.fetchone()
        assert int(result[0]) == 15, f"Expected 15 after REPLACE, got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_create_or_replace: PASSED")
    finally:
        stop_proxy(proc)

def test_function_in_where_clause():
    """Test using function in WHERE clause."""
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
        
        # Create table
        cur.execute("DROP TABLE IF EXISTS numbers")
        cur.execute("CREATE TABLE numbers (value INTEGER)")
        cur.execute("INSERT INTO numbers VALUES (1), (2), (3), (4), (5)")
        
        # Create function
        cur.execute("""
            CREATE FUNCTION is_even(x integer)
            RETURNS boolean
            LANGUAGE sql
            AS $$
                SELECT x % 2 = 0
            $$;
        """)
        
        # Use in WHERE clause
        cur.execute("SELECT value FROM numbers WHERE is_even(value)")
        results = cur.fetchall()
        even_values = [int(r[0]) for r in results]
        assert even_values == [2, 4], f"Expected [2, 4], got {even_values}"
        
        cur.close()
        conn.close()
        print("test_function_in_where_clause: PASSED")
    finally:
        stop_proxy(proc)

def test_void_function():
    """Test VOID function."""
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
        
        # Create table for side effects
        cur.execute("CREATE TABLE logs (msg TEXT)")
        
        # Create VOID function
        cur.execute("""
            CREATE FUNCTION log_it(p_msg text)
            RETURNS void
            LANGUAGE sql
            AS $$
                INSERT INTO logs(msg) VALUES(p_msg)
            $$;
        """)
        
        # Call function via SELECT (simple function call execution)
        cur.execute("SELECT log_it('hello world')")
        result = cur.fetchone()
        assert result[0] is None, f"Expected NULL for VOID function, got {result[0]}"
        
        # Verify side effect
        cur.execute("SELECT msg FROM logs")
        result = cur.fetchone()
        assert result[0] == 'hello world', f"Expected 'hello world' in logs, got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_void_function: PASSED")
    finally:
        stop_proxy(proc)

def test_setof_function_in_select_list():
    """Test SETOF function in SELECT list."""
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
        
        # Create table
        cur.execute("CREATE TABLE items (id INTEGER, vals TEXT)")
        cur.execute("INSERT INTO items VALUES (1, '[10, 20, 30]')")
        
        # Create SETOF function using json_each
        cur.execute("""
            CREATE FUNCTION get_vals(j text)
            RETURNS SETOF integer
            LANGUAGE sql
            AS $$
                SELECT value FROM json_each($1)
            $$;
        """)
        
        # Call function in SELECT list
        # In SQLite, this will only return the FIRST value (10) for each row
        cur.execute("SELECT id, get_vals(vals) FROM items")
        result = cur.fetchone()
        assert int(result[0]) == 1, f"Expected 1, got {result[0]}"
        assert int(result[1]) == 10, f"Expected 10, got {result[1]}"
        
        cur.close()
        conn.close()
        print("test_setof_function_in_select_list: PASSED")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    test_simple_scalar_function()
    test_function_with_out_params()
    test_strict_function()
    test_returns_table_function()
    test_void_function()
    test_setof_function_in_select_list()
    test_drop_function()
    test_create_or_replace()
    test_function_in_where_clause()
    print("\n✅ All E2E tests passed!")
