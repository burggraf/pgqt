"""
End-to-end tests for PostgreSQL SHOW command support.

This test script verifies SHOW functionality through the PostgreSQL wire protocol.
"""

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def test_show_search_path(proxy):
    """Test SHOW search_path command."""
    print("Testing SHOW search_path...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    # Show default search_path
    cur.execute("SHOW search_path")
    result = cur.fetchall()
    print(f"  Default search_path: {result}")
    assert len(result) > 0

    # Set a custom search_path
    cur.execute("SET search_path TO public, pg_catalog")

    # Show updated search_path
    cur.execute("SHOW search_path")
    result = cur.fetchall()
    print(f"  Updated search_path: {result}")
    assert len(result) > 0

    cur.close()
    conn.close()
    print("  ✓ SHOW search_path works")

def test_show_server_version(proxy):
    """Test SHOW server_version command."""
    print("Testing SHOW server_version...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    cur.execute("SHOW server_version")
    result = cur.fetchall()
    print(f"  Server version: {result}")
    assert len(result) > 0
    # Version should contain a number
    assert len(result[0][0]) > 0

    cur.close()
    conn.close()
    print("  ✓ SHOW server_version works")

def test_show_server_encoding(proxy):
    """Test SHOW server_encoding command."""
    print("Testing SHOW server_encoding...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    cur.execute("SHOW server_encoding")
    result = cur.fetchall()
    print(f"  Server encoding: {result}")
    assert len(result) > 0

    cur.close()
    conn.close()
    print("  ✓ SHOW server_encoding works")

def test_show_client_encoding(proxy):
    """Test SHOW client_encoding command."""
    print("Testing SHOW client_encoding...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    cur.execute("SHOW client_encoding")
    result = cur.fetchall()
    print(f"  Client encoding: {result}")
    assert len(result) > 0

    cur.close()
    conn.close()
    print("  ✓ SHOW client_encoding works")

def test_show_timezone(proxy):
    """Test SHOW timezone command."""
    print("Testing SHOW timezone...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    cur.execute("SHOW timezone")
    result = cur.fetchall()
    print(f"  Timezone: {result}")
    assert len(result) > 0

    cur.close()
    conn.close()
    print("  ✓ SHOW timezone works")

def test_show_transaction_isolation(proxy):
    """Test SHOW transaction_isolation command."""
    print("Testing SHOW transaction_isolation...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    cur.execute("SHOW transaction_isolation")
    result = cur.fetchall()
    print(f"  Transaction isolation: {result}")
    assert len(result) > 0

    cur.close()
    conn.close()
    print("  ✓ SHOW transaction_isolation works")

def test_show_all(proxy):
    """Test SHOW ALL command."""
    print("Testing SHOW ALL...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    cur.execute("SHOW ALL")
    result = cur.fetchall()
    print(f"  SHOW ALL returned {len(result)} configuration parameters")

    # Verify we get multiple rows
    assert len(result) > 0

    # Each row should have 3 columns: name, setting, description
    for row in result:
        assert len(row) == 3, f"Expected 3 columns, got {len(row)}"

    cur.close()
    conn.close()
    print("  ✓ SHOW ALL works")

def test_show_multiple_configs(proxy):
    """Test multiple SHOW commands in sequence."""
    print("Testing multiple SHOW commands...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    configs = [
        "DateStyle",
        "transaction_read_only",
        "statement_timeout",
        "max_connections",
        "work_mem",
    ]

    for config in configs:
        cur.execute(f"SHOW {config}")
        result = cur.fetchall()
        print(f"  {config}: {result[0][0]}")
        assert len(result) > 0

    cur.close()
    conn.close()
    print("  ✓ Multiple SHOW commands work")

def test_show_with_schema(proxy):
    """Test SHOW in combination with schema operations."""
    print("Testing SHOW with schema operations...")

    conn = proxy.get_connection()
    cur = conn.cursor()

    # Create a schema
    cur.execute("CREATE SCHEMA test_show_schema")
    conn.commit()

    # Set search_path to include the new schema
    cur.execute("SET search_path TO test_show_schema, public")

    # Verify the search_path
    cur.execute("SHOW search_path")
    result = cur.fetchall()
    print(f"  search_path with new schema: {result}")
    assert len(result) > 0
    # The result should contain our new schema
    assert "test_show_schema" in str(result).lower()

    # Clean up
    cur.execute("DROP SCHEMA test_show_schema CASCADE")
    conn.commit()

    cur.close()
    conn.close()
    print("  ✓ SHOW with schema operations works")

def main():
    """Run all SHOW command E2E tests."""
    print("=" * 60)
    print("PostgreSQL SHOW Command E2E Tests")
    print("=" * 60)

    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")

        tests = [
            ("test_show_search_path", test_show_search_path),
            ("test_show_server_version", test_show_server_version),
            ("test_show_server_encoding", test_show_server_encoding),
            ("test_show_client_encoding", test_show_client_encoding),
            ("test_show_timezone", test_show_timezone),
            ("test_show_transaction_isolation", test_show_transaction_isolation),
            ("test_show_all", test_show_all),
            ("test_show_multiple_configs", test_show_multiple_configs),
            ("test_show_with_schema", test_show_with_schema),
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
            print(f"All {passed} SHOW command E2E tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1

if __name__ == "__main__":
    sys.exit(main())