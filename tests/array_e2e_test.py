"""
End-to-end tests for PostgreSQL array support.

This test suite validates array operators and functions through actual
PostgreSQL wire protocol connections.
"""

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def test_basic_array_operations(proxy):
    """Test basic array creation and retrieval."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
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
    
    # Query
    cur.execute("SELECT name, tags FROM items ORDER BY id")
    rows = cur.fetchall()
    
    assert len(rows) == 3
    assert rows[0][0] == 'item1'
    assert rows[1][0] == 'item2'
    assert rows[2][0] == 'item3'
    
    print("✓ Basic array operations work")
    
    cur.close()
    conn.close()

def test_array_overlap_operator(proxy):
    """Test array overlap operator (&&)."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS users")
    cur.execute("""
        CREATE TABLE users (
            id SERIAL PRIMARY KEY,
            name TEXT,
            roles TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("""
        INSERT INTO users (name, roles) VALUES
            ('Alice', '{"admin","editor"}'),
            ('Bob', '{"viewer"}'),
            ('Charlie', '{"editor","viewer"}')
    """)
    conn.commit()
    
    # Test overlap operator
    cur.execute("""
        SELECT name FROM users 
        WHERE roles && '{"admin","editor"}' 
        ORDER BY name
    """)
    results = [row[0] for row in cur.fetchall()]
    assert results == ['Alice', 'Charlie'], f"Expected ['Alice', 'Charlie'], got {results}"
    
    print("✓ Array overlap operator (&&) works")
    
    cur.close()
    conn.close()

def test_array_contains_operator(proxy):
    """Test array contains operator (@>)."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS users")
    cur.execute("""
        CREATE TABLE users (
            id SERIAL PRIMARY KEY,
            name TEXT,
            roles TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("""
        INSERT INTO users (name, roles) VALUES
            ('Alice', '{"admin","editor","viewer"}'),
            ('Bob', '{"viewer"}'),
            ('Charlie', '{"editor","viewer"}')
    """)
    conn.commit()
    
    # Test contains operator
    cur.execute("""
        SELECT name FROM users 
        WHERE roles @> '{"admin","editor"}' 
        ORDER BY name
    """)
    results = [row[0] for row in cur.fetchall()]
    assert results == ['Alice'], f"Expected ['Alice'], got {results}"
    
    print("✓ Array contains operator (@>) works")
    
    cur.close()
    conn.close()

def test_array_contained_operator(proxy):
    """Test array contained operator (<@)."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS users")
    cur.execute("""
        CREATE TABLE users (
            id SERIAL PRIMARY KEY,
            name TEXT,
            roles TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("""
        INSERT INTO users (name, roles) VALUES
            ('Alice', '{"admin","editor"}'),
            ('Bob', '{"viewer"}'),
            ('Charlie', '{"editor","viewer"}')
    """)
    conn.commit()
    
    # Test contained operator
    cur.execute("""
        SELECT name FROM users 
        WHERE roles <@ '{"admin","editor","viewer"}' 
        ORDER BY name
    """)
    results = [row[0] for row in cur.fetchall()]
    assert results == ['Alice', 'Bob', 'Charlie'], f"Expected all users, got {results}"
    
    print("✓ Array contained operator (<@) works")
    
    cur.close()
    conn.close()

def test_array_append_function(proxy):
    """Test array_append function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\"}')")
    conn.commit()
    
    # Test array_append
    cur.execute("SELECT array_append(tags, 'c') FROM items")
    result = cur.fetchone()[0]
    assert 'c' in result, f"Expected 'c' in result, got {result}"
    
    print("✓ array_append function works")
    
    cur.close()
    conn.close()

def test_array_prepend_function(proxy):
    """Test array_prepend function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_prepend
    cur.execute("SELECT array_prepend('a', tags) FROM items")
    result = cur.fetchone()[0]
    assert 'a' in result, f"Expected 'a' in result, got {result}"
    
    print("✓ array_prepend function works")
    
    cur.close()
    conn.close()

def test_array_cat_function(proxy):
    """Test array_cat function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\"}')")
    conn.commit()
    
    # Test array_cat
    cur.execute("SELECT array_cat(tags, '{\"c\",\"d\"}') FROM items")
    result = cur.fetchone()[0]
    assert 'c' in result and 'd' in result, f"Expected 'c' and 'd' in result, got {result}"
    
    print("✓ array_cat function works")
    
    cur.close()
    conn.close()

def test_array_remove_function(proxy):
    """Test array_remove function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_remove
    cur.execute("SELECT array_remove(tags, 'b') FROM items")
    result = cur.fetchone()[0]
    assert 'b' not in result, f"Expected 'b' not in result, got {result}"
    
    print("✓ array_remove function works")
    
    cur.close()
    conn.close()

def test_array_replace_function(proxy):
    """Test array_replace function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_replace
    cur.execute("SELECT array_replace(tags, 'b', 'x') FROM items")
    result = cur.fetchone()[0]
    assert 'b' not in result and 'x' in result, f"Expected 'b' replaced with 'x', got {result}"
    
    print("✓ array_replace function works")
    
    cur.close()
    conn.close()

def test_array_length_function(proxy):
    """Test array_length function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_length
    cur.execute("SELECT array_length(tags, 1) FROM items")
    result = cur.fetchone()[0]
    assert int(result) == 3, f"Expected 3, got {result}"
    
    print("✓ array_length function works")
    
    cur.close()
    conn.close()

def test_cardinality_function(proxy):
    """Test cardinality function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test cardinality
    cur.execute("SELECT cardinality(tags) FROM items")
    result = cur.fetchone()[0]
    assert int(result) == 3, f"Expected 3, got {result}"
    
    print("✓ cardinality function works")
    
    cur.close()
    conn.close()

def test_array_position_function(proxy):
    """Test array_position function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_position
    cur.execute("SELECT array_position(tags, 'b') FROM items")
    result = cur.fetchone()[0]
    assert int(result) == 2, f"Expected 2, got {result}"
    
    print("✓ array_position function works")
    
    cur.close()
    conn.close()

def test_array_positions_function(proxy):
    """Test array_positions function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"a\",\"c\"}')")
    conn.commit()
    
    # Test array_positions
    cur.execute("SELECT array_positions(tags, 'a') FROM items")
    result = cur.fetchone()[0]
    # PostgreSQL returns array as {1,3} format
    assert result == '{1,3}' or result == [1, 3], f"Expected [1, 3] or '{{1,3}}', got {result}"
    
    print("✓ array_positions function works")
    
    cur.close()
    conn.close()

def test_array_to_string_function(proxy):
    """Test array_to_string function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_to_string
    cur.execute("SELECT array_to_string(tags, ',') FROM items")
    result = cur.fetchone()[0]
    assert result == 'a,b,c', f"Expected 'a,b,c', got {result}"
    
    print("✓ array_to_string function works")
    
    cur.close()
    conn.close()

def test_string_to_array_function(proxy):
    """Test string_to_array function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('a,b,c')")
    conn.commit()
    
    # Test string_to_array
    cur.execute("SELECT string_to_array(tags, ',') FROM items")
    result = cur.fetchone()[0]
    assert 'b' in result, f"Expected 'b' in result, got {result}"
    
    print("✓ string_to_array function works")
    
    cur.close()
    conn.close()

def test_array_ndims_function(proxy):
    """Test array_ndims function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_ndims
    cur.execute("SELECT array_ndims(tags) FROM items")
    result = cur.fetchone()[0]
    assert int(result) == 1, f"Expected 1, got {result}"
    
    print("✓ array_ndims function works")
    
    cur.close()
    conn.close()

def test_array_dims_function(proxy):
    """Test array_dims function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\"}')")
    conn.commit()
    
    # Test array_dims
    cur.execute("SELECT array_dims(tags) FROM items")
    result = cur.fetchone()[0]
    assert result == '[1:3]', f"Expected '[1:3]', got {result}"
    
    print("✓ array_dims function works")
    
    cur.close()
    conn.close()

def test_trim_array_function(proxy):
    """Test trim_array function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            tags TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (tags) VALUES ('{\"a\",\"b\",\"c\",\"d\"}')")
    conn.commit()
    
    # Test trim_array
    cur.execute("SELECT trim_array(tags, 1) FROM items")
    result = cur.fetchone()[0]
    assert 'd' not in result, f"Expected 'd' removed, got {result}"
    
    print("✓ trim_array function works")
    
    cur.close()
    conn.close()

def test_array_fill_function(proxy):
    """Test array_fill function."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("""
        CREATE TABLE items (
            id SERIAL PRIMARY KEY,
            value INTEGER
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO items (value) VALUES (1)")
    conn.commit()
    
    # Test array_fill
    cur.execute("SELECT array_fill('x', ARRAY[3])")
    result = cur.fetchone()[0]
    assert len(result) >= 3, f"Expected array with 3 elements, got {result}"
    
    print("✓ array_fill function works")
    
    cur.close()
    conn.close()

def test_complex_array_query(proxy):
    """Test complex query with multiple array operations."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS users")
    cur.execute("""
        CREATE TABLE users (
            id SERIAL PRIMARY KEY,
            name TEXT,
            roles TEXT[],
            permissions TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("""
        INSERT INTO users (name, roles, permissions) VALUES
            ('Alice', '{"admin","editor"}', '{"read","write","delete"}'),
            ('Bob', '{"viewer"}', '{"read"}'),
            ('Charlie', '{"editor"}', '{"read","write"}')
    """)
    conn.commit()
    
    # Complex query
    cur.execute("""
        SELECT name FROM users 
        WHERE roles && '{"admin","editor"}' 
        AND permissions @> '{"read","write"}'
        ORDER BY name
    """)
    results = [row[0] for row in cur.fetchall()]
    assert results == ['Alice', 'Charlie'], f"Expected ['Alice', 'Charlie'], got {results}"
    
    print("✓ Complex array query works")
    
    cur.close()
    conn.close()

def test_array_in_update(proxy):
    """Test array operations in UPDATE statements."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS users")
    cur.execute("""
        CREATE TABLE users (
            id SERIAL PRIMARY KEY,
            name TEXT,
            roles TEXT[]
        )
    """)
    conn.commit()
    
    # Insert test data
    cur.execute("INSERT INTO users (name, roles) VALUES ('Alice', '{\"user\"}')")
    conn.commit()
    
    # Update with array operation
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

def main():
    """Run all array E2E tests."""
    print("=" * 60)
    print("PostgreSQLite Array E2E Tests")
    print("=" * 60)
    
    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        
        tests = [
            ("test_basic_array_operations", test_basic_array_operations),
            ("test_array_overlap_operator", test_array_overlap_operator),
            ("test_array_contains_operator", test_array_contains_operator),
            ("test_array_contained_operator", test_array_contained_operator),
            ("test_array_append_function", test_array_append_function),
            ("test_array_prepend_function", test_array_prepend_function),
            ("test_array_cat_function", test_array_cat_function),
            ("test_array_remove_function", test_array_remove_function),
            ("test_array_replace_function", test_array_replace_function),
            ("test_array_length_function", test_array_length_function),
            ("test_cardinality_function", test_cardinality_function),
            ("test_array_position_function", test_array_position_function),
            ("test_array_positions_function", test_array_positions_function),
            ("test_array_to_string_function", test_array_to_string_function),
            ("test_string_to_array_function", test_string_to_array_function),
            ("test_array_ndims_function", test_array_ndims_function),
            ("test_array_dims_function", test_array_dims_function),
            ("test_trim_array_function", test_trim_array_function),
            ("test_array_fill_function", test_array_fill_function),
            ("test_complex_array_query", test_complex_array_query),
            ("test_array_in_update", test_array_in_update),
        ]
        
        passed = 0
        failed = 0
        
        for test_name, test_func in tests:
            try:
                test_func(proxy)
                print(f"✓ {test_name} passed")
                passed += 1
            except Exception as e:
                print(f"✗ {test_name} failed: {e}")
                import traceback
                traceback.print_exc()
                failed += 1
        
        print("=" * 60)
        if failed == 0:
            print(f"All {passed} E2E array tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())
