#!/usr/bin/env python3
"""
End-to-end tests for schema-qualified function support.
Tests creating and calling SQL and PL/pgSQL functions in different schemas,
including cross-schema function calls.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_schema_functions_e2e.db"


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
        try:
            os.remove(DB_PATH)
        except:
            pass


def test_create_schemas():
    """Test creating multiple schemas."""
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
        
        # Create schemas
        cur.execute("CREATE SCHEMA auth")
        cur.execute("CREATE SCHEMA api")
        cur.execute("CREATE SCHEMA app")
        conn.commit()
        
        # Verify schemas exist
        cur.execute("SELECT nspname FROM __pg_namespace__ WHERE nspname IN ('auth', 'api', 'app') ORDER BY nspname")
        result = cur.fetchall()
        
        assert len(result) == 3, f"Expected 3 schemas, got {len(result)}"
        assert result[0][0] == 'api', f"Expected 'api', got {result[0][0]}"
        assert result[1][0] == 'app', f"Expected 'app', got {result[1][0]}"
        assert result[2][0] == 'auth', f"Expected 'auth', got {result[2][0]}"
        
        cur.close()
        conn.close()
        print("test_create_schemas: PASSED")
    finally:
        stop_proxy(proc)


def test_sql_function_in_schema():
    """Test creating and calling SQL functions in non-public schemas."""
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
        
        # Create schema and function
        cur.execute("CREATE SCHEMA auth")
        cur.execute("""
            CREATE FUNCTION auth.uid() RETURNS TEXT AS $$
                SELECT 'user-123'
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Call the function
        cur.execute("SELECT auth.uid()")
        result = cur.fetchone()
        
        assert result[0] == 'user-123', f"Expected 'user-123', got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_sql_function_in_schema: PASSED")
    finally:
        stop_proxy(proc)


def test_plpgsql_function_in_schema():
    """Test creating and calling PL/pgSQL functions in non-public schemas."""
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
        
        # Create schema and PL/pgSQL function
        cur.execute("CREATE SCHEMA auth")
        cur.execute("""
            CREATE FUNCTION auth.jwt_claim(claim_name TEXT) RETURNS TEXT AS $$
            BEGIN
                RETURN 'claim-' || claim_name;
            END;
            $$ LANGUAGE plpgsql
        """)
        conn.commit()
        
        # Call the function
        cur.execute("SELECT auth.jwt_claim('sub')")
        result = cur.fetchone()
        
        assert result[0] == 'claim-sub', f"Expected 'claim-sub', got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_plpgsql_function_in_schema: PASSED")
    finally:
        stop_proxy(proc)


def test_multiple_schemas_with_functions():
    """Test functions in multiple schemas don't conflict."""
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
        
        # Create schemas
        cur.execute("CREATE SCHEMA auth")
        cur.execute("CREATE SCHEMA api")
        conn.commit()
        
        # Create functions with same name in different schemas
        cur.execute("""
            CREATE FUNCTION auth.get_context() RETURNS TEXT AS $$
                SELECT 'auth-context'
            $$ LANGUAGE sql
        """)
        cur.execute("""
            CREATE FUNCTION api.get_context() RETURNS TEXT AS $$
                SELECT 'api-context'
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Call both functions
        cur.execute("SELECT auth.get_context()")
        auth_result = cur.fetchone()[0]
        
        cur.execute("SELECT api.get_context()")
        api_result = cur.fetchone()[0]
        
        assert auth_result == 'auth-context', f"Expected 'auth-context', got {auth_result}"
        assert api_result == 'api-context', f"Expected 'api-context', got {api_result}"
        
        cur.close()
        conn.close()
        print("test_multiple_schemas_with_functions: PASSED")
    finally:
        stop_proxy(proc)


def test_function_in_where_clause():
    """Test using schema-qualified function in WHERE clause (RLS pattern)."""
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
        
        # Setup
        cur.execute("CREATE SCHEMA auth")
        cur.execute("""
            CREATE FUNCTION auth.uid() RETURNS TEXT AS $$
                SELECT 'user-123'
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Create table and insert data
        cur.execute("CREATE TABLE items (id INT, owner_id TEXT, name TEXT)")
        cur.execute("INSERT INTO items VALUES (1, 'user-123', 'Item 1')")
        cur.execute("INSERT INTO items VALUES (2, 'other-user', 'Item 2')")
        conn.commit()
        
        # RLS-style query
        cur.execute("SELECT * FROM items WHERE owner_id = auth.uid()")
        result = cur.fetchall()
        
        assert len(result) == 1, f"Expected 1 row, got {len(result)}"
        assert result[0][2] == 'Item 1', f"Expected 'Item 1', got {result[0][2]}"
        
        cur.close()
        conn.close()
        print("test_function_in_where_clause: PASSED")
    finally:
        stop_proxy(proc)


def test_function_in_insert():
    """Test using schema-qualified function in INSERT statement."""
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
        
        # Setup
        cur.execute("CREATE SCHEMA auth")
        cur.execute("""
            CREATE FUNCTION auth.uid() RETURNS TEXT AS $$
                SELECT 'user-123'
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Create table
        cur.execute("CREATE TABLE users (id TEXT, name TEXT)")
        conn.commit()
        
        # Insert using function
        cur.execute("INSERT INTO users (id, name) VALUES (auth.uid(), 'test-user')")
        conn.commit()
        
        # Verify
        cur.execute("SELECT * FROM users")
        result = cur.fetchone()
        
        assert result[0] == 'user-123', f"Expected 'user-123', got {result[0]}"
        assert result[1] == 'test-user', f"Expected 'test-user', got {result[1]}"
        
        cur.close()
        conn.close()
        print("test_function_in_insert: PASSED")
    finally:
        stop_proxy(proc)


def test_sql_function_calling_sql_function():
    """Test SQL function calling another SQL function in same schema."""
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
        
        # Setup
        cur.execute("CREATE SCHEMA auth")
        cur.execute("""
            CREATE FUNCTION auth.uid() RETURNS TEXT AS $$
                SELECT 'user-123'
            $$ LANGUAGE sql
        """)
        cur.execute("""
            CREATE FUNCTION auth.current_user_id() RETURNS TEXT AS $$
                SELECT auth.uid()
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Call nested function
        cur.execute("SELECT auth.current_user_id()")
        result = cur.fetchone()
        
        assert result[0] == 'user-123', f"Expected 'user-123', got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_sql_function_calling_sql_function: PASSED")
    finally:
        stop_proxy(proc)


def test_cross_schema_function_call():
    """Test function in one schema calling function in another schema."""
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
        
        # Setup
        cur.execute("CREATE SCHEMA auth")
        cur.execute("CREATE SCHEMA api")
        
        # Create function in auth schema
        cur.execute("""
            CREATE FUNCTION auth.email() RETURNS TEXT AS $$
                SELECT 'user@example.com'
            $$ LANGUAGE sql
        """)
        
        # Create function in api schema that calls auth.email()
        cur.execute("""
            CREATE FUNCTION api.user_email() RETURNS TEXT AS $$
                SELECT auth.email()
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Call cross-schema function
        cur.execute("SELECT api.user_email()")
        result = cur.fetchone()
        
        assert result[0] == 'user@example.com', f"Expected 'user@example.com', got {result[0]}"
        
        cur.close()
        conn.close()
        print("test_cross_schema_function_call: PASSED")
    finally:
        stop_proxy(proc)


def test_function_with_arguments():
    """Test schema-qualified function with arguments."""
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
        
        # Setup
        cur.execute("CREATE SCHEMA auth")
        cur.execute("""
            CREATE FUNCTION auth.has_role(role_name TEXT) RETURNS BOOLEAN AS $$
                SELECT role_name = 'admin'
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Call with different arguments
        cur.execute("SELECT auth.has_role('admin')")
        result1 = cur.fetchone()[0]
        
        cur.execute("SELECT auth.has_role('user')")
        result2 = cur.fetchone()[0]
        
        # Note: SQLite stores BOOLEAN as INTEGER (1/0) or string '1'/'0'
        assert result1 in (1, '1', True), f"Expected True for admin, got {result1}"
        assert result2 in (0, '0', False), f"Expected False for user, got {result2}"
        
        cur.close()
        conn.close()
        print("test_function_with_arguments: PASSED")
    finally:
        stop_proxy(proc)


def test_supabase_pattern():
    """Test typical Supabase auth.uid() pattern for RLS."""
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
        
        # Create auth schema and uid function (Supabase pattern)
        cur.execute("CREATE SCHEMA auth")
        cur.execute("""
            CREATE FUNCTION auth.uid() RETURNS TEXT AS $$
                SELECT 'authenticated-user-id'
            $$ LANGUAGE sql
        """)
        conn.commit()
        
        # Create table with owner_id
        cur.execute("""
            CREATE TABLE posts (
                id SERIAL PRIMARY KEY,
                title TEXT,
                owner_id TEXT
            )
        """)
        cur.execute("INSERT INTO posts (title, owner_id) VALUES ('Post 1', 'authenticated-user-id')")
        cur.execute("INSERT INTO posts (title, owner_id) VALUES ('Post 2', 'other-user')")
        conn.commit()
        
        # RLS-style query (what Supabase would generate)
        cur.execute("SELECT * FROM posts WHERE owner_id = auth.uid()")
        result = cur.fetchall()
        
        assert len(result) == 1, f"Expected 1 row, got {len(result)}"
        assert result[0][1] == 'Post 1', f"Expected 'Post 1', got {result[0][1]}"
        
        # Test in combination with other conditions
        cur.execute("SELECT * FROM posts WHERE owner_id = auth.uid() AND title = 'Post 1'")
        result2 = cur.fetchall()
        assert len(result2) == 1, f"Expected 1 row with combined conditions, got {len(result2)}"
        
        cur.close()
        conn.close()
        print("test_supabase_pattern: PASSED")
    finally:
        stop_proxy(proc)


def run_all_tests():
    """Run all schema function tests."""
    tests = [
        test_create_schemas,
        test_sql_function_in_schema,
        test_plpgsql_function_in_schema,
        test_multiple_schemas_with_functions,
        test_function_in_where_clause,
        test_function_in_insert,
        test_sql_function_calling_sql_function,
        test_cross_schema_function_call,
        test_function_with_arguments,
        test_supabase_pattern,
    ]
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            print(f"{test.__name__}: FAILED - {e}")
            import traceback
            traceback.print_exc()
            failed += 1
    
    print(f"\n{'='*60}")
    print(f"Schema Functions E2E Test Results: {passed} passed, {failed} failed")
    print(f"{'='*60}")
    
    return failed == 0


if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)