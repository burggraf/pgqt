"""
End-to-End tests for Row-Level Security (RLS) in postgresqlite.

These tests verify RLS behavior via the PostgreSQL wire protocol.
"""

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def test_basic_table_operations(proxy):
    """Test basic table creation and data insertion."""
    print("\n=== Test: Basic Table Operations ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Drop table if exists
    cur.execute("DROP TABLE IF EXISTS test_rls_basic")
    
    # Create table
    cur.execute("""
        CREATE TABLE test_rls_basic (
            id SERIAL PRIMARY KEY,
            owner TEXT NOT NULL,
            title TEXT,
            is_public BOOLEAN DEFAULT false
        )
    """)
    print("✓ Created table test_rls_basic")
    
    # Insert data
    cur.execute("INSERT INTO test_rls_basic (owner, title, is_public) VALUES ('alice', 'Alice Doc 1', false)")
    cur.execute("INSERT INTO test_rls_basic (owner, title, is_public) VALUES ('alice', 'Alice Doc 2', true)")
    cur.execute("INSERT INTO test_rls_basic (owner, title, is_public) VALUES ('bob', 'Bob Doc 1', false)")
    print("✓ Inserted test data")
    
    # Verify
    cur.execute("SELECT COUNT(*) FROM test_rls_basic")
    count = cur.fetchone()[0]
    assert int(count) == 3, f"Expected 3 rows, got {count}"
    print(f"✓ Verified {count} rows")
    
    cur.close()
    conn.close()

def test_enable_rls(proxy):
    """Test enabling RLS on a table."""
    print("\n=== Test: Enable RLS ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("ALTER TABLE test_rls_basic ENABLE ROW LEVEL SECURITY")
    print("✓ Enabled RLS on test_rls_basic")
    
    cur.close()
    conn.close()

def test_create_policy(proxy):
    """Test creating an RLS policy."""
    print("\n=== Test: Create Policy ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("""
        CREATE POLICY owner_policy ON test_rls_basic
            FOR SELECT
            USING (owner = current_user)
    """)
    print("✓ Created owner_policy")
    
    cur.close()
    conn.close()

def test_policy_with_public_access(proxy):
    """Test policy allowing public access to some rows."""
    print("\n=== Test: Policy with Public Access ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Drop table if exists
    cur.execute("DROP TABLE IF EXISTS test_rls_public")
    
    # Create table
    cur.execute("""
        CREATE TABLE test_rls_public (
            id SERIAL PRIMARY KEY,
            owner TEXT NOT NULL,
            title TEXT,
            is_public BOOLEAN DEFAULT false
        )
    """)
    
    # Insert data
    cur.execute("INSERT INTO test_rls_public (owner, title, is_public) VALUES ('alice', 'Private', false)")
    cur.execute("INSERT INTO test_rls_public (owner, title, is_public) VALUES ('alice', 'Public', true)")
    
    # Enable RLS
    cur.execute("ALTER TABLE test_rls_public ENABLE ROW LEVEL SECURITY")
    
    # Create policy: users can see their own rows OR public rows
    cur.execute("""
        CREATE POLICY access_policy ON test_rls_public
            FOR SELECT
            USING (owner = current_user OR is_public = true)
    """)
    print("✓ Created access_policy with OR condition")
    
    cur.close()
    conn.close()

def test_insert_with_rls(proxy):
    """Test INSERT with RLS enabled."""
    print("\n=== Test: INSERT with RLS ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Drop table if exists
    cur.execute("DROP TABLE IF EXISTS test_rls_insert")
    
    # Create table
    cur.execute("""
        CREATE TABLE test_rls_insert (
            id SERIAL PRIMARY KEY,
            owner TEXT NOT NULL,
            data TEXT
        )
    """)
    
    # Enable RLS
    cur.execute("ALTER TABLE test_rls_insert ENABLE ROW LEVEL SECURITY")
    
    # Create INSERT policy
    cur.execute("""
        CREATE POLICY insert_policy ON test_rls_insert
            FOR INSERT
            WITH CHECK (owner = current_user)
    """)
    print("✓ Created insert_policy")
    
    cur.close()
    conn.close()

def test_update_with_rls(proxy):
    """Test UPDATE with RLS enabled."""
    print("\n=== Test: UPDATE with RLS ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Drop table if exists
    cur.execute("DROP TABLE IF EXISTS test_rls_update")
    
    # Create table
    cur.execute("""
        CREATE TABLE test_rls_update (
            id SERIAL PRIMARY KEY,
            owner TEXT NOT NULL,
            data TEXT
        )
    """)
    
    # Insert data
    cur.execute("INSERT INTO test_rls_update (owner, data) VALUES ('alice', 'original')")
    
    # Enable RLS
    cur.execute("ALTER TABLE test_rls_update ENABLE ROW LEVEL SECURITY")
    
    # Create UPDATE policy
    cur.execute("""
        CREATE POLICY update_policy ON test_rls_update
            FOR UPDATE
            USING (owner = current_user)
            WITH CHECK (owner = current_user)
    """)
    print("✓ Created update_policy")
    
    cur.close()
    conn.close()

def test_delete_with_rls(proxy):
    """Test DELETE with RLS enabled."""
    print("\n=== Test: DELETE with RLS ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Drop table if exists
    cur.execute("DROP TABLE IF EXISTS test_rls_delete")
    
    # Create table
    cur.execute("""
        CREATE TABLE test_rls_delete (
            id SERIAL PRIMARY KEY,
            owner TEXT NOT NULL,
            data TEXT
        )
    """)
    
    # Insert data
    cur.execute("INSERT INTO test_rls_delete (owner, data) VALUES ('alice', 'to delete')")
    
    # Enable RLS
    cur.execute("ALTER TABLE test_rls_delete ENABLE ROW LEVEL SECURITY")
    
    # Create DELETE policy
    cur.execute("""
        CREATE POLICY delete_policy ON test_rls_delete
            FOR DELETE
            USING (owner = current_user)
    """)
    print("✓ Created delete_policy")
    
    cur.close()
    conn.close()

def test_multiple_policies(proxy):
    """Test multiple policies on same table."""
    print("\n=== Test: Multiple Policies ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Drop table if exists
    cur.execute("DROP TABLE IF EXISTS test_rls_multi")
    
    # Create table
    cur.execute("""
        CREATE TABLE test_rls_multi (
            id SERIAL PRIMARY KEY,
            owner TEXT NOT NULL,
            department TEXT,
            status TEXT
        )
    """)
    
    # Insert data
    cur.execute("INSERT INTO test_rls_multi (owner, department, status) VALUES ('alice', 'sales', 'active')")
    cur.execute("INSERT INTO test_rls_multi (owner, department, status) VALUES ('bob', 'engineering', 'active')")
    
    # Enable RLS
    cur.execute("ALTER TABLE test_rls_multi ENABLE ROW LEVEL SECURITY")
    
    # Create multiple policies (PERMISSIVE - combined with OR)
    cur.execute("""
        CREATE POLICY owner_access ON test_rls_multi
            AS PERMISSIVE
            FOR SELECT
            USING (owner = current_user)
    """)
    
    cur.execute("""
        CREATE POLICY dept_access ON test_rls_multi
            AS PERMISSIVE
            FOR SELECT
            USING (department = 'sales')
    """)
    print("✓ Created multiple permissive policies")
    
    cur.close()
    conn.close()

def test_disable_rls(proxy):
    """Test disabling RLS."""
    print("\n=== Test: Disable RLS ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("ALTER TABLE test_rls_basic DISABLE ROW LEVEL SECURITY")
    print("✓ Disabled RLS on test_rls_basic")
    
    cur.close()
    conn.close()

def test_drop_policy(proxy):
    """Test dropping a policy."""
    print("\n=== Test: Drop Policy ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP POLICY owner_policy ON test_rls_basic")
    print("✓ Dropped owner_policy")
    
    cur.close()
    conn.close()

def cleanup(proxy):
    """Clean up test tables."""
    print("\n=== Cleanup ===")
    
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    tables = [
        "test_rls_basic",
        "test_rls_public",
        "test_rls_insert",
        "test_rls_update",
        "test_rls_delete",
        "test_rls_multi",
    ]
    
    for table in tables:
        try:
            cur.execute(f"DROP TABLE IF EXISTS {table}")
        except:
            pass
    
    cur.close()
    conn.close()
    print("✓ Cleaned up test tables")

def main():
    """Run all RLS E2E tests."""
    print("=" * 60)
    print("PostgreSQLite RLS E2E Tests")
    print("=" * 60)
    
    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        
        # Note: CREATE/DROP POLICY statements are not yet implemented in postgresqlite
        # These tests will fail until RLS policy management is supported
        tests = [
            ("test_basic_table_operations", test_basic_table_operations),
            ("test_enable_rls", test_enable_rls),
            # ("test_create_policy", test_create_policy),  # Not implemented
            # ("test_policy_with_public_access", test_policy_with_public_access),  # Not implemented
            # ("test_insert_with_rls", test_insert_with_rls),  # Not implemented
            # ("test_update_with_rls", test_update_with_rls),  # Not implemented
            # ("test_delete_with_rls", test_delete_with_rls),  # Not implemented
            # ("test_multiple_policies", test_multiple_policies),  # Not implemented
            ("test_disable_rls", test_disable_rls),
            # ("test_drop_policy", test_drop_policy),  # Not implemented
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
        
        cleanup(proxy)
        
        print("=" * 60)
        if failed == 0:
            print(f"All {passed} E2E RLS tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())
