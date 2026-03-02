"""
End-to-end tests for vector search functionality.
"""

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import ProxyManager

def test_create_vector_table(proxy):
    """Test creating a table with VECTOR type."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS vectors")
    cur.execute("""
        CREATE TABLE vectors (
            id SERIAL PRIMARY KEY,
            name TEXT,
            embedding VECTOR(3)
        )
    """)
    conn.commit()
    
    # Verify table was created
    cur.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='vectors'")
    result = cur.fetchone()
    assert result is not None, "Table 'vectors' was not created"
    
    cur.close()
    conn.close()
    print("✓ test_create_vector_table passed")

def test_insert_and_select_vectors(proxy):
    """Test inserting and selecting vectors."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DELETE FROM vectors")
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("item1", "[1,2,3]"))
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("item2", "[4,5,6]"))
    conn.commit()
    
    cur.execute("SELECT name, embedding FROM vectors ORDER BY id")
    rows = cur.fetchall()
    
    assert len(rows) == 2, f"Expected 2 rows, got {len(rows)}"
    assert rows[0][0] == "item1", f"Expected 'item1', got {rows[0][0]}"
    assert rows[1][0] == "item2", f"Expected 'item2', got {rows[1][0]}"
    
    cur.close()
    conn.close()
    print("✓ test_insert_and_select_vectors passed")

def main():
    """Run all vector E2E tests."""
    print("=" * 60)
    print("PostgreSQLite Vector E2E Tests")
    print("=" * 60)
    
    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        
        # Note: Vector distance functions and operators are not fully implemented yet
        tests = [
            ("test_create_vector_table", test_create_vector_table),
            ("test_insert_and_select_vectors", test_insert_and_select_vectors),
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
            print(f"All {passed} E2E vector tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())
