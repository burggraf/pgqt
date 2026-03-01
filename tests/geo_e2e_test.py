#!/usr/bin/env python3
"""
End-to-end tests for geometric types functionality.
"""

import sys
import os

# Add tests directory to path for importing helper
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager


def test_point_type(proxy):
    """Test point type creation, insertion, and distance."""
    conn = proxy.get_connection()
    conn.autocommit = True
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS points")
    cur.execute("CREATE TABLE points (id SERIAL PRIMARY KEY, p POINT)")
    cur.execute("INSERT INTO points (p) VALUES ('(1, 2)'), ('(4, 6)')")
    
    cur.execute("SELECT p <-> '(1, 2)' FROM points ORDER BY id")
    distances = [float(row[0]) for row in cur.fetchall()]
    
    assert distances[0] == 0.0, f"Expected 0.0, got {distances[0]}"
    assert distances[1] == 5.0, f"Expected 5.0, got {distances[1]}"
    
    cur.close()
    conn.close()


def test_box_operators(proxy):
    """Test box type and its spatial operators."""
    conn = proxy.get_connection()
    conn.autocommit = True
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS boxes")
    cur.execute("CREATE TABLE boxes (id SERIAL PRIMARY KEY, b BOX)")
    cur.execute("INSERT INTO boxes (b) VALUES ('(0,0),(2,2)'), ('(1,1),(3,3)'), ('(4,4),(5,5)')")
    
    # Test overlaps (&&)
    cur.execute("SELECT id FROM boxes WHERE b && '(1,1),(2,2)' ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1, 2], f"Expected [1, 2], got {ids}"
    
    # Test contains (@>)
    cur.execute("SELECT id FROM boxes WHERE b @> '(0.5,0.5),(1.5,1.5)' ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1], f"Expected [1], got {ids}"
    
    # Test contained in (<@)
    cur.execute("SELECT id FROM boxes WHERE '(0,0),(10,10)' @> b ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1, 2, 3], f"Expected [1, 2, 3], got {ids}"

    # Test left (<<)
    cur.execute("SELECT id FROM boxes WHERE b << '(3.5,0),(6,6)' ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1, 2], f"Expected [1, 2], got {ids}"
    
    # Test right (>>)
    cur.execute("SELECT id FROM boxes WHERE b >> '(0,0),(3.5,3.5)' ORDER BY id")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [3], f"Expected [3], got {ids}"

    cur.close()
    conn.close()


def test_below_above_operators(proxy):
    """Test below (<<|) and above (|>>) operators for boxes."""
    conn = proxy.get_connection()
    conn.autocommit = True
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS boxes_y")
    cur.execute("CREATE TABLE boxes_y (id SERIAL PRIMARY KEY, b BOX)")
    cur.execute("INSERT INTO boxes_y (b) VALUES ('(0,0),(2,2)'), ('(0,4),(2,6)')")
    
    # Test below (<<|)
    cur.execute("SELECT id FROM boxes_y WHERE b <<| '(0,3),(2,3.5)'")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [1], f"Expected [1], got {ids}"
    
    # Test above (|>>)
    cur.execute("SELECT id FROM boxes_y WHERE b |>> '(0,3),(2,3.5)'")
    ids = [int(row[0]) for row in cur.fetchall()]
    assert ids == [2], f"Expected [2], got {ids}"
    
    cur.close()
    conn.close()


def main():
    """Run all geo E2E tests."""
    print("=" * 60)
    print("PostgreSQLite Geo E2E Tests")
    print("=" * 60)
    
    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        
        tests = [
            ("test_point_type", test_point_type),
            ("test_box_operators", test_box_operators),
            ("test_below_above_operators", test_below_above_operators),
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
            print(f"All {passed} E2E geometric tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1


if __name__ == "__main__":
    import sys
    sys.exit(main())
