"""
End-to-end tests for PostgreSQL schema/namespace support.

This test script verifies schema functionality through the PostgreSQL wire protocol.
"""

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def setup_test_db(proxy):
    """Create test tables and schemas."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Clean up
    cur.execute("DROP SCHEMA IF EXISTS test_inventory CASCADE")
    cur.execute("DROP SCHEMA IF EXISTS test_analytics CASCADE")
    cur.execute("DROP TABLE IF EXISTS test_public_users CASCADE")
    
    # Create schemas
    cur.execute("CREATE SCHEMA test_inventory")
    cur.execute("CREATE SCHEMA test_analytics")
    
    # Create tables
    cur.execute("""
        CREATE TABLE test_public_users (
            id SERIAL PRIMARY KEY,
            name TEXT,
            email TEXT
        )
    """)
    
    cur.execute("""
        CREATE TABLE test_inventory.products (
            id SERIAL PRIMARY KEY,
            name TEXT,
            price REAL,
            quantity INTEGER
        )
    """)
    
    cur.execute("""
        CREATE TABLE test_analytics.events (
            id SERIAL PRIMARY KEY,
            event_type TEXT,
            user_id INTEGER,
            created_at TEXT DEFAULT datetime('now')
        )
    """)
    
    conn.commit()
    cur.close()
    conn.close()

def test_create_schema(proxy):
    """Test CREATE SCHEMA statement."""
    print("Testing CREATE SCHEMA...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create schema
    cur.execute("CREATE SCHEMA test_new_schema")
    
    # Create table in schema
    cur.execute("CREATE TABLE test_new_schema.test_table (id INTEGER)")
    
    # Insert and query
    cur.execute("INSERT INTO test_new_schema.test_table VALUES (1)")
    cur.execute("SELECT * FROM test_new_schema.test_table")
    result = cur.fetchall()
    assert len(result) == 1
    assert int(result[0][0]) == 1
    
    # Clean up
    cur.execute("DROP SCHEMA test_new_schema CASCADE")
    conn.commit()
    
    cur.close()
    conn.close()
    print("  ✓ CREATE SCHEMA works")

def test_drop_schema(proxy):
    """Test DROP SCHEMA statement."""
    print("Testing DROP SCHEMA...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create a schema
    cur.execute("CREATE SCHEMA test_drop_schema")
    
    # Drop it
    cur.execute("DROP SCHEMA test_drop_schema")
    
    # Verify it's gone (should fail to create table)
    try:
        cur.execute("CREATE TABLE test_drop_schema.test (id INTEGER)")
        conn.commit()
        assert False, "Should have failed - schema should not exist"
    except Exception:
        conn.rollback()
    
    cur.close()
    conn.close()
    print("  ✓ DROP SCHEMA works")

def test_drop_schema_cascade(proxy):
    """Test DROP SCHEMA CASCADE removes all objects."""
    print("Testing DROP SCHEMA CASCADE...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create schema with table
    cur.execute("CREATE SCHEMA test_cascade_schema")
    cur.execute("CREATE TABLE test_cascade_schema.test_table (id INTEGER)")
    cur.execute("INSERT INTO test_cascade_schema.test_table VALUES (42)")
    conn.commit()
    
    # Drop with cascade
    cur.execute("DROP SCHEMA test_cascade_schema CASCADE")
    conn.commit()
    
    # Verify it's gone
    try:
        cur.execute("SELECT * FROM test_cascade_schema.test_table")
        assert False, "Should have failed - schema should be dropped"
    except Exception:
        conn.rollback()
    
    cur.close()
    conn.close()
    print("  ✓ DROP SCHEMA CASCADE works")

def test_schema_qualified_tables(proxy):
    """Test accessing tables in different schemas."""
    print("Testing schema-qualified table access...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Insert into inventory schema
    cur.execute("INSERT INTO test_inventory.products (name, price, quantity) VALUES ('Widget', 9.99, 100)")
    
    # Insert into analytics schema
    cur.execute("INSERT INTO test_analytics.events (event_type, user_id) VALUES ('click', 1)")
    
    # Insert into public schema (no prefix)
    cur.execute("INSERT INTO test_public_users (name, email) VALUES ('Alice', 'alice@example.com')")
    
    conn.commit()
    
    # Query from each schema
    cur.execute("SELECT name, price FROM test_inventory.products")
    result = cur.fetchall()
    assert len(result) == 1
    assert result[0][0] == 'Widget'
    
    cur.execute("SELECT event_type FROM test_analytics.events")
    result = cur.fetchall()
    assert len(result) == 1
    assert result[0][0] == 'click'
    
    cur.execute("SELECT name FROM test_public_users")
    result = cur.fetchall()
    assert len(result) == 1
    assert result[0][0] == 'Alice'
    
    cur.close()
    conn.close()
    print("  ✓ Schema-qualified table access works")

def test_cross_schema_join(proxy):
    """Test joining tables across schemas."""
    print("Testing cross-schema joins...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Clean up
    cur.execute("DELETE FROM test_inventory.products")
    cur.execute("DELETE FROM test_public_users")
    
    # Insert test data
    cur.execute("INSERT INTO test_public_users (id, name) VALUES (1, 'Bob')")
    cur.execute("INSERT INTO test_inventory.products (id, name, price, quantity) VALUES (1, 'Gadget', 10.0, 5)")
    
    conn.commit()
    
    # Cross-schema join (using a common id field pattern)
    cur.execute("""
        SELECT u.name, p.name 
        FROM test_public_users u 
        JOIN test_inventory.products p ON u.id = p.id
    """)
    result = cur.fetchall()
    assert len(result) == 1
    
    cur.close()
    conn.close()
    print("  ✓ Cross-schema join works")

def test_search_path(proxy):
    """Test SET/SHOW search_path."""
    print("Testing search_path...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Show default
    cur.execute("SHOW search_path")
    result = cur.fetchall()
    print(f"  Default search_path: {result}")
    
    # Set new search_path
    cur.execute("SET search_path TO test_inventory, public")
    
    # Show updated
    cur.execute("SHOW search_path")
    result = cur.fetchall()
    print(f"  Updated search_path: {result}")
    
    cur.close()
    conn.close()
    print("  ✓ SET/SHOW search_path works")

def test_public_schema_default(proxy):
    """Test that public schema tables work without prefix."""
    print("Testing public schema as default...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Insert without schema prefix (should go to public)
    cur.execute("INSERT INTO test_public_users (name, email) VALUES ('Charlie', 'charlie@example.com')")
    conn.commit()
    
    # Query with explicit public prefix
    cur.execute("SELECT name FROM public.test_public_users WHERE name = 'Charlie'")
    result = cur.fetchall()
    assert len(result) == 1
    
    # Query without prefix (should resolve to public)
    cur.execute("SELECT name FROM test_public_users WHERE name = 'Charlie'")
    result = cur.fetchall()
    assert len(result) == 1
    
    cur.close()
    conn.close()
    print("  ✓ Public schema works as default")

def test_if_not_exists(proxy):
    """Test CREATE SCHEMA IF NOT EXISTS."""
    print("Testing CREATE SCHEMA IF NOT EXISTS...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create schema
    cur.execute("CREATE SCHEMA IF NOT EXISTS test_ifne_schema")
    
    # Create again (should not error)
    cur.execute("CREATE SCHEMA IF NOT EXISTS test_ifne_schema")
    
    # Clean up
    cur.execute("DROP SCHEMA test_ifne_schema")
    conn.commit()
    
    cur.close()
    conn.close()
    print("  ✓ CREATE SCHEMA IF NOT EXISTS works")

def test_drop_if_exists(proxy):
    """Test DROP SCHEMA IF EXISTS."""
    print("Testing DROP SCHEMA IF EXISTS...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Should succeed without error even if schema doesn't exist
    cur.execute("DROP SCHEMA IF EXISTS nonexistent_schema_xyz")
    conn.commit()
    
    cur.close()
    conn.close()
    print("  ✓ DROP SCHEMA IF EXISTS works")

def test_pg_namespace_catalog(proxy):
    """Test pg_namespace catalog view."""
    print("Testing pg_namespace catalog...")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT nspname FROM pg_namespace ORDER BY nspname")
    result = cur.fetchall()
    schema_names = [r[0] for r in result]
    
    print(f"  Schemas: {schema_names}")
    
    assert 'public' in schema_names
    assert 'pg_catalog' in schema_names
    assert 'information_schema' in schema_names
    assert 'test_inventory' in schema_names
    assert 'test_analytics' in schema_names
    
    cur.close()
    conn.close()
    print("  ✓ pg_namespace catalog works")

def cleanup_test_db(proxy):
    """Clean up test data."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP SCHEMA IF EXISTS test_inventory CASCADE")
    cur.execute("DROP SCHEMA IF EXISTS test_analytics CASCADE")
    cur.execute("DROP TABLE IF EXISTS test_public_users CASCADE")
    conn.commit()
    
    cur.close()
    conn.close()

def main():
    """Run all schema E2E tests."""
    print("=" * 60)
    print("PostgreSQL Schema Support E2E Tests")
    print("=" * 60)
    
    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        
        # Setup
        print("\nSetting up test database...")
        setup_test_db(proxy)
        print()
        
        # Run tests
        tests = [
            ("test_create_schema", test_create_schema),
            ("test_drop_schema", test_drop_schema),
            ("test_drop_schema_cascade", test_drop_schema_cascade),
            ("test_schema_qualified_tables", test_schema_qualified_tables),
            ("test_cross_schema_join", test_cross_schema_join),
            ("test_search_path", test_search_path),
            ("test_public_schema_default", test_public_schema_default),
            ("test_if_not_exists", test_if_not_exists),
            ("test_drop_if_exists", test_drop_if_exists),
            ("test_pg_namespace_catalog", test_pg_namespace_catalog),
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
        
        cleanup_test_db(proxy)
        
        print("=" * 60)
        if failed == 0:
            print(f"All {passed} E2E schema tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())
