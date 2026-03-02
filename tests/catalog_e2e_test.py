"""
End-to-end tests for pg_catalog system views.
"""
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def test_pg_class(proxy):
    """Test pg_class catalog view."""
    print("Testing pg_class...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create test table
    cur.execute("CREATE TABLE test_users (id SERIAL PRIMARY KEY, name TEXT)")
    conn.commit()
    
    # Query pg_class
    cur.execute("SELECT relname, relkind FROM pg_class WHERE relname = 'test_users'")
    result = cur.fetchone()
    assert result is not None, "test_users should be in pg_class"
    assert result[0] == 'test_users'
    assert result[1] == 'r'  # regular table
    
    cur.close()
    conn.close()
    print("  ✓ pg_class works")

def test_pg_attribute(proxy):
    """Test pg_attribute catalog view."""
    print("Testing pg_attribute...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE test_attrs (id INTEGER PRIMARY KEY, email TEXT NOT NULL)")
    conn.commit()
    
    # Query pg_attribute
    cur.execute("""
        SELECT attname, attnotnull, attnum 
        FROM pg_attribute a
        JOIN pg_class c ON a.attrelid = c.oid
        WHERE c.relname = 'test_attrs'
        ORDER BY attnum
    """)
    results = cur.fetchall()
    assert len(results) >= 2, "Should have at least 2 columns"
    
    col_names = [r[0] for r in results]
    assert 'id' in col_names
    assert 'email' in col_names
    
    cur.close()
    conn.close()
    print("  ✓ pg_attribute works")

def test_pg_type(proxy):
    """Test pg_type catalog view."""
    print("Testing pg_type...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT typname, typtype FROM pg_type WHERE typname = 'int4'")
    result = cur.fetchone()
    assert result is not None, "int4 type should exist"
    assert result[0] == 'int4'
    assert result[1] == 'b'  # base type
    
    cur.close()
    conn.close()
    print("  ✓ pg_type works")

def test_pg_tables(proxy):
    """Test pg_tables view."""
    print("Testing pg_tables...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE test_pg_tables (id INTEGER PRIMARY KEY)")
    conn.commit()
    
    cur.execute("SELECT tablename FROM pg_tables WHERE tablename = 'test_pg_tables'")
    result = cur.fetchone()
    assert result is not None, "test_pg_tables should be in pg_tables"
    
    cur.close()
    conn.close()
    print("  ✓ pg_tables works")

def test_pg_namespace(proxy):
    """Test pg_namespace catalog view."""
    print("Testing pg_namespace...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT nspname FROM pg_namespace ORDER BY nspname")
    results = cur.fetchall()
    schema_names = [r[0] for r in results]
    
    assert 'public' in schema_names
    assert 'pg_catalog' in schema_names
    
    cur.close()
    conn.close()
    print("  ✓ pg_namespace works")

def test_pg_roles(proxy):
    """Test pg_roles view."""
    print("Testing pg_roles...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT rolname, rolsuper FROM pg_roles WHERE rolname = 'postgres'")
    result = cur.fetchone()
    assert result is not None, "postgres role should exist"
    assert result[0] == 'postgres'
    assert result[1] in (True, 1, '1')  
    
    cur.close()
    conn.close()
    print("  ✓ pg_roles works")

def test_pg_database(proxy):
    """Test pg_database view."""
    print("Testing pg_database...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT datname, encoding FROM pg_database LIMIT 1")
    result = cur.fetchone()
    assert result is not None
    assert result[0] == 'postgres'
    
    cur.close()
    conn.close()
    print("  ✓ pg_database works")

def test_pg_settings(proxy):
    """Test pg_settings view."""
    print("Testing pg_settings...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT name, setting FROM pg_settings WHERE name = 'server_version'")
    result = cur.fetchone()
    assert result is not None
    assert result[0] == 'server_version'
    assert result[1] == '15.0'
    
    cur.close()
    conn.close()
    print("  ✓ pg_settings works")

def test_pg_extension(proxy):
    """Test pg_extension view."""
    print("Testing pg_extension...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT extname FROM pg_extension WHERE extname = 'plpgsql'")
    result = cur.fetchone()
    assert result is not None
    assert result[0] == 'plpgsql'
    
    cur.close()
    conn.close()
    print("  ✓ pg_extension works")

def test_orm_introspection_query(proxy):
    """Test a typical ORM introspection query."""
    print("Testing ORM introspection query...")
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create test schema
    cur.execute("""
        CREATE TABLE test_orm_table (
            id SERIAL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            email TEXT UNIQUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
    """)
    conn.commit()
    
    # Typical ORM introspection query
    cur.execute("""
        SELECT 
            c.relname as table_name,
            a.attname as column_name,
            t.typname as data_type,
            a.attnotnull as is_nullable,
            a.attnum as ordinal_position
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
          AND a.attnum > 0
          AND NOT a.attisdropped
        ORDER BY c.relname, a.attnum
    """)
    
    results = cur.fetchall()
    assert len(results) >= 4, "Should have at least 4 columns"
    
    col_names = [r[1] for r in results if r[0] == 'test_orm_table']
    assert 'id' in col_names
    assert 'name' in col_names
    assert 'email' in col_names
    assert 'created_at' in col_names
    
    cur.close()
    conn.close()
    print("  ✓ ORM introspection query works")

def main():
    """Run all catalog E2E tests."""
    print("=" * 60)
    print("PostgreSQLite Catalog E2E Tests")
    print("=" * 60)
    
    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        
        tests = [
            ("test_pg_class", test_pg_class),
            ("test_pg_attribute", test_pg_attribute),
            ("test_pg_type", test_pg_type),
            ("test_pg_tables", test_pg_tables),
            ("test_pg_namespace", test_pg_namespace),
            ("test_pg_roles", test_pg_roles),
            ("test_pg_database", test_pg_database),
            ("test_pg_settings", test_pg_settings),
            ("test_pg_extension", test_pg_extension),
            ("test_orm_introspection_query", test_orm_introspection_query),
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
            print(f"All {passed} E2E catalog tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())
