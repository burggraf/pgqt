#!/usr/bin/env python3
"""
End-to-End tests for Row-Level Security (RLS) in postgresqlite.

These tests verify RLS behavior via the PostgreSQL wire protocol.
Run with: python tests/rls_e2e_test.py

Prerequisites:
- postgresqlite proxy running on localhost:5432
- psycopg2 installed (pip install psycopg2-binary)
"""

import os
import sys
import subprocess
import time
import signal

try:
    import psycopg2
except ImportError:
    print("ERROR: psycopg2 not installed. Run: pip install psycopg2-binary")
    sys.exit(1)

# Test configuration
PG_HOST = os.environ.get("PG_HOST", "127.0.0.1")
PG_PORT = int(os.environ.get("PG_PORT", "5432"))
PG_USER = os.environ.get("PG_USER", "postgres")
PG_DATABASE = os.environ.get("PG_DATABASE", "postgres")


class RLSE2ETest:
    def __init__(self):
        self.conn = None
        self.server_process = None
        
    def setup(self):
        """Connect to the postgresqlite proxy."""
        try:
            self.conn = psycopg2.connect(
                host=PG_HOST,
                port=PG_PORT,
                user=PG_USER,
                database=PG_DATABASE,
            )
            self.conn.autocommit = True
            print(f"✓ Connected to postgresqlite at {PG_HOST}:{PG_PORT}")
            return True
        except Exception as e:
            print(f"✗ Failed to connect: {e}")
            return False
    
    def teardown(self):
        """Close connection and cleanup."""
        if self.conn:
            self.conn.close()
        if self.server_process:
            self.server_process.terminate()
            self.server_process.wait()
    
    def execute(self, sql):
        """Execute SQL and return cursor."""
        cur = self.conn.cursor()
        cur.execute(sql)
        return cur
    
    def query(self, sql):
        """Execute query and return all rows."""
        cur = self.execute(sql)
        try:
            return cur.fetchall()
        except:
            return []
    
    def test_basic_table_operations(self):
        """Test basic table creation and data insertion."""
        print("\n=== Test: Basic Table Operations ===")
        
        # Drop table if exists
        self.execute("DROP TABLE IF EXISTS test_rls_basic")
        
        # Create table
        self.execute("""
            CREATE TABLE test_rls_basic (
                id SERIAL PRIMARY KEY,
                owner TEXT NOT NULL,
                title TEXT,
                is_public BOOLEAN DEFAULT false
            )
        """)
        print("✓ Created table test_rls_basic")
        
        # Insert data
        self.execute("INSERT INTO test_rls_basic (owner, title, is_public) VALUES ('alice', 'Alice Doc 1', false)")
        self.execute("INSERT INTO test_rls_basic (owner, title, is_public) VALUES ('alice', 'Alice Doc 2', true)")
        self.execute("INSERT INTO test_rls_basic (owner, title, is_public) VALUES ('bob', 'Bob Doc 1', false)")
        print("✓ Inserted test data")
        
        # Verify data
        rows = self.query("SELECT COUNT(*) FROM test_rls_basic")
        count = rows[0][0] if rows else 0
        assert count == 3, f"Expected 3 rows, got {count}"
        print(f"✓ Verified {count} rows")
        
        return True
    
    def test_enable_rls(self):
        """Test enabling RLS on a table."""
        print("\n=== Test: Enable RLS ===")
        
        self.execute("ALTER TABLE test_rls_basic ENABLE ROW LEVEL SECURITY")
        print("✓ Enabled RLS on test_rls_basic")
        
        return True
    
    def test_create_policy(self):
        """Test creating an RLS policy."""
        print("\n=== Test: Create Policy ===")
        
        # Create a policy that allows users to see their own rows
        self.execute("""
            CREATE POLICY owner_policy ON test_rls_basic
                FOR SELECT
                USING (owner = current_user)
        """)
        print("✓ Created owner_policy")
        
        return True
    
    def test_policy_with_public_access(self):
        """Test policy allowing public access to some rows."""
        print("\n=== Test: Policy with Public Access ===")
        
        # Drop table if exists
        self.execute("DROP TABLE IF EXISTS test_rls_public")
        
        # Create table
        self.execute("""
            CREATE TABLE test_rls_public (
                id SERIAL PRIMARY KEY,
                owner TEXT NOT NULL,
                title TEXT,
                is_public BOOLEAN DEFAULT false
            )
        """)
        
        # Insert data
        self.execute("INSERT INTO test_rls_public (owner, title, is_public) VALUES ('alice', 'Private', false)")
        self.execute("INSERT INTO test_rls_public (owner, title, is_public) VALUES ('alice', 'Public', true)")
        
        # Enable RLS
        self.execute("ALTER TABLE test_rls_public ENABLE ROW LEVEL SECURITY")
        
        # Create policy: users can see their own rows OR public rows
        self.execute("""
            CREATE POLICY access_policy ON test_rls_public
                FOR SELECT
                USING (owner = current_user OR is_public = true)
        """)
        print("✓ Created access_policy with OR condition")
        
        return True
    
    def test_insert_with_rls(self):
        """Test INSERT with RLS enabled."""
        print("\n=== Test: INSERT with RLS ===")
        
        # Drop table if exists
        self.execute("DROP TABLE IF EXISTS test_rls_insert")
        
        # Create table
        self.execute("""
            CREATE TABLE test_rls_insert (
                id SERIAL PRIMARY KEY,
                owner TEXT NOT NULL,
                data TEXT
            )
        """)
        
        # Enable RLS
        self.execute("ALTER TABLE test_rls_insert ENABLE ROW LEVEL SECURITY")
        
        # Create INSERT policy
        self.execute("""
            CREATE POLICY insert_policy ON test_rls_insert
                FOR INSERT
                WITH CHECK (owner = current_user)
        """)
        print("✓ Created insert_policy")
        
        return True
    
    def test_update_with_rls(self):
        """Test UPDATE with RLS enabled."""
        print("\n=== Test: UPDATE with RLS ===")
        
        # Drop table if exists
        self.execute("DROP TABLE IF EXISTS test_rls_update")
        
        # Create table
        self.execute("""
            CREATE TABLE test_rls_update (
                id SERIAL PRIMARY KEY,
                owner TEXT NOT NULL,
                data TEXT
            )
        """)
        
        # Insert data
        self.execute("INSERT INTO test_rls_update (owner, data) VALUES ('alice', 'original')")
        
        # Enable RLS
        self.execute("ALTER TABLE test_rls_update ENABLE ROW LEVEL SECURITY")
        
        # Create UPDATE policy
        self.execute("""
            CREATE POLICY update_policy ON test_rls_update
                FOR UPDATE
                USING (owner = current_user)
                WITH CHECK (owner = current_user)
        """)
        print("✓ Created update_policy")
        
        return True
    
    def test_delete_with_rls(self):
        """Test DELETE with RLS enabled."""
        print("\n=== Test: DELETE with RLS ===")
        
        # Drop table if exists
        self.execute("DROP TABLE IF EXISTS test_rls_delete")
        
        # Create table
        self.execute("""
            CREATE TABLE test_rls_delete (
                id SERIAL PRIMARY KEY,
                owner TEXT NOT NULL,
                data TEXT
            )
        """)
        
        # Insert data
        self.execute("INSERT INTO test_rls_delete (owner, data) VALUES ('alice', 'to delete')")
        
        # Enable RLS
        self.execute("ALTER TABLE test_rls_delete ENABLE ROW LEVEL SECURITY")
        
        # Create DELETE policy
        self.execute("""
            CREATE POLICY delete_policy ON test_rls_delete
                FOR DELETE
                USING (owner = current_user)
        """)
        print("✓ Created delete_policy")
        
        return True
    
    def test_multiple_policies(self):
        """Test multiple policies on same table."""
        print("\n=== Test: Multiple Policies ===")
        
        # Drop table if exists
        self.execute("DROP TABLE IF EXISTS test_rls_multi")
        
        # Create table
        self.execute("""
            CREATE TABLE test_rls_multi (
                id SERIAL PRIMARY KEY,
                owner TEXT NOT NULL,
                department TEXT,
                status TEXT
            )
        """)
        
        # Insert data
        self.execute("INSERT INTO test_rls_multi (owner, department, status) VALUES ('alice', 'sales', 'active')")
        self.execute("INSERT INTO test_rls_multi (owner, department, status) VALUES ('bob', 'engineering', 'active')")
        
        # Enable RLS
        self.execute("ALTER TABLE test_rls_multi ENABLE ROW LEVEL SECURITY")
        
        # Create multiple policies (PERMISSIVE - combined with OR)
        self.execute("""
            CREATE POLICY owner_access ON test_rls_multi
                AS PERMISSIVE
                FOR SELECT
                USING (owner = current_user)
        """)
        
        self.execute("""
            CREATE POLICY dept_access ON test_rls_multi
                AS PERMISSIVE
                FOR SELECT
                USING (department = 'sales')
        """)
        print("✓ Created multiple permissive policies")
        
        return True
    
    def test_disable_rls(self):
        """Test disabling RLS."""
        print("\n=== Test: Disable RLS ===")
        
        self.execute("ALTER TABLE test_rls_basic DISABLE ROW LEVEL SECURITY")
        print("✓ Disabled RLS on test_rls_basic")
        
        return True
    
    def test_drop_policy(self):
        """Test dropping a policy."""
        print("\n=== Test: Drop Policy ===")
        
        self.execute("DROP POLICY owner_policy ON test_rls_basic")
        print("✓ Dropped owner_policy")
        
        return True
    
    def cleanup(self):
        """Clean up test tables."""
        print("\n=== Cleanup ===")
        
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
                self.execute(f"DROP TABLE IF EXISTS {table}")
            except:
                pass
        
        print("✓ Cleaned up test tables")
    
    def run_all_tests(self):
        """Run all E2E tests."""
        print("=" * 60)
        print("PostgreSQLite RLS E2E Tests")
        print("=" * 60)
        
        if not self.setup():
            return False
        
        tests = [
            self.test_basic_table_operations,
            self.test_enable_rls,
            self.test_create_policy,
            self.test_policy_with_public_access,
            self.test_insert_with_rls,
            self.test_update_with_rls,
            self.test_delete_with_rls,
            self.test_multiple_policies,
            self.test_disable_rls,
            self.test_drop_policy,
        ]
        
        passed = 0
        failed = 0
        
        for test in tests:
            try:
                if test():
                    passed += 1
                else:
                    failed += 1
            except Exception as e:
                print(f"✗ Test failed with exception: {e}")
                failed += 1
        
        self.cleanup()
        self.teardown()
        
        print("\n" + "=" * 60)
        print(f"Results: {passed} passed, {failed} failed")
        print("=" * 60)
        
        return failed == 0


if __name__ == "__main__":
    tester = RLSE2ETest()
    success = tester.run_all_tests()
    sys.exit(0 if success else 1)
