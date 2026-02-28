#!/usr/bin/env python3
"""
End-to-end tests for vector search functionality.
Requires psycopg2 and a running pglite-proxy server.

Usage:
    # Start the server first:
    cargo run -- --port 5433 --database /tmp/test_vector_e2e.db
    
    # Run tests:
    python3 tests/vector_e2e_test.py
"""

import sys
import os

try:
    import psycopg2
except ImportError:
    print("psycopg2 not installed. Run: pip install psycopg2-binary")
    sys.exit(1)

# Configuration
DB_HOST = "127.0.0.1"
DB_PORT = int(os.environ.get("PG_LITE_PORT", "5433"))
DB_USER = "postgres"
DB_NAME = "test"

def get_connection():
    """Get a database connection."""
    return psycopg2.connect(
        host=DB_HOST,
        port=DB_PORT,
        user=DB_USER,
        database=DB_NAME
    )

def test_create_vector_table():
    """Test creating a table with VECTOR type."""
    conn = get_connection()
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

def test_insert_and_select_vectors():
    """Test inserting and selecting vectors."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("DELETE FROM vectors")
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("item1", "[1, 2, 3]"))
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("item2", "[4, 5, 6]"))
    conn.commit()
    
    cur.execute("SELECT name, embedding FROM vectors ORDER BY id")
    rows = cur.fetchall()
    
    assert len(rows) == 2, f"Expected 2 rows, got {len(rows)}"
    assert rows[0][0] == "item1", f"Expected 'item1', got {rows[0][0]}"
    assert rows[1][0] == "item2", f"Expected 'item2', got {rows[1][0]}"
    
    cur.close()
    conn.close()
    print("✓ test_insert_and_select_vectors passed")

def test_l2_distance():
    """Test L2 distance function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l2_distance('[1, 2, 3]', '[4, 5, 6]')")
    result = float(cur.fetchone()[0])
    
    # L2 distance: sqrt((4-1)^2 + (5-2)^2 + (6-3)^2) = sqrt(27) ≈ 5.196
    expected = 5.196152422706632
    assert abs(result - expected) < 0.01, f"Expected {expected}, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_l2_distance passed")

def test_cosine_distance():
    """Test cosine distance function."""
    conn = get_connection()
    cur = conn.cursor()
    
    # Identical vectors should have distance 0
    cur.execute("SELECT cosine_distance('[1, 2, 3]', '[1, 2, 3]')")
    result = float(cur.fetchone()[0])
    assert abs(result) < 0.0001, f"Expected 0, got {result}"
    
    # Orthogonal vectors should have distance 1
    cur.execute("SELECT cosine_distance('[1, 0]', '[0, 1]')")
    result = float(cur.fetchone()[0])
    assert abs(result - 1.0) < 0.0001, f"Expected 1, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_cosine_distance passed")

def test_inner_product():
    """Test inner product function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT inner_product('[1, 2, 3]', '[4, 5, 6]')")
    result = float(cur.fetchone()[0])
    
    # 1*4 + 2*5 + 3*6 = 32
    assert abs(result - 32.0) < 0.0001, f"Expected 32, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_inner_product passed")

def test_l1_distance():
    """Test L1 distance function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l1_distance('[1, 2, 3]', '[4, 5, 6]')")
    result = float(cur.fetchone()[0])
    
    # |1-4| + |2-5| + |3-6| = 9
    assert abs(result - 9.0) < 0.0001, f"Expected 9, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_l1_distance passed")

def test_vector_dims():
    """Test vector_dims function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT vector_dims('[1, 2, 3, 4, 5]')")
    result = int(cur.fetchone()[0])
    
    assert result == 5, f"Expected 5, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_vector_dims passed")

def test_l2_norm():
    """Test l2_norm function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l2_norm('[3, 4]')")
    result = float(cur.fetchone()[0])
    
    # sqrt(9 + 16) = 5
    assert abs(result - 5.0) < 0.0001, f"Expected 5, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_l2_norm passed")

def test_l2_normalize():
    """Test l2_normalize function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l2_normalize('[3, 4]')")
    result = cur.fetchone()[0]
    
    # Should be [0.6, 0.8]
    import json
    vals = json.loads(result) if isinstance(result, str) else result
    assert abs(vals[0] - 0.6) < 0.0001, f"Expected 0.6, got {vals[0]}"
    assert abs(vals[1] - 0.8) < 0.0001, f"Expected 0.8, got {vals[1]}"
    
    cur.close()
    conn.close()
    print("✓ test_l2_normalize passed")

def test_subvector():
    """Test subvector function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT subvector('[1, 2, 3, 4, 5]', 1, 3)")
    result = cur.fetchone()[0]
    
    import json
    vals = json.loads(result) if isinstance(result, str) else result
    assert len(vals) == 3, f"Expected 3 elements, got {len(vals)}"
    assert vals[0] == 1, f"Expected first element to be 1, got {vals[0]}"
    assert vals[1] == 2, f"Expected second element to be 2, got {vals[1]}"
    assert vals[2] == 3, f"Expected third element to be 3, got {vals[2]}"
    
    cur.close()
    conn.close()
    print("✓ test_subvector passed")

def test_vector_search_query():
    """Test vector search with ORDER BY distance."""
    conn = get_connection()
    cur = conn.cursor()
    
    # Clear and insert test vectors
    cur.execute("DELETE FROM vectors")
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("close", "[1, 1, 1]"))
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("far", "[100, 100, 100]"))
    conn.commit()
    
    # Query for nearest to [1, 1, 1]
    cur.execute("""
        SELECT name, l2_distance(embedding, '[1, 1, 1]') AS dist
        FROM vectors
        ORDER BY dist
        LIMIT 1
    """)
    result = cur.fetchone()
    
    assert result[0] == "close", f"Expected 'close', got {result[0]}"
    
    cur.close()
    conn.close()
    print("✓ test_vector_search_query passed")

def test_vector_operators():
    """Test pgvector-compatible operators."""
    conn = get_connection()
    cur = conn.cursor()
    
    # Test <-> (L2 distance)
    cur.execute("SELECT embedding <-> '[1, 2, 3]' FROM vectors WHERE name = 'close'")
    result = cur.fetchone()
    assert result is not None, "L2 distance operator failed"
    
    # Test <=> (cosine distance)
    cur.execute("SELECT embedding <=> '[1, 2, 3]' FROM vectors WHERE name = 'close'")
    result = cur.fetchone()
    assert result is not None, "Cosine distance operator failed"
    
    # Test <#> (inner product)
    cur.execute("SELECT embedding <#> '[1, 2, 3]' FROM vectors WHERE name = 'close'")
    result = cur.fetchone()
    assert result is not None, "Inner product operator failed"
    
    # Test <+> (L1 distance)
    cur.execute("SELECT embedding <+> '[1, 2, 3]' FROM vectors WHERE name = 'close'")
    result = cur.fetchone()
    assert result is not None, "L1 distance operator failed"
    
    cur.close()
    conn.close()
    print("✓ test_vector_operators passed")

def test_vector_arithmetic():
    """Test vector arithmetic functions."""
    conn = get_connection()
    cur = conn.cursor()
    
    # Test vector_add
    cur.execute("SELECT vector_add('[1, 2, 3]', '[4, 5, 6]')")
    result = cur.fetchone()[0]
    import json
    vals = json.loads(result) if isinstance(result, str) else result
    assert vals[0] == 5, f"vector_add: Expected 5, got {vals[0]}"
    
    # Test vector_sub
    cur.execute("SELECT vector_sub('[4, 5, 6]', '[1, 2, 3]')")
    result = cur.fetchone()[0]
    vals = json.loads(result) if isinstance(result, str) else result
    assert vals[0] == 3, f"vector_sub: Expected 3, got {vals[0]}"
    
    cur.close()
    conn.close()
    print("✓ test_vector_arithmetic passed")

def run_all_tests():
    """Run all E2E tests."""
    print(f"Connecting to {DB_HOST}:{DB_PORT}...")
    
    try:
        # Test connection first
        conn = get_connection()
        conn.close()
    except Exception as e:
        print(f"\n❌ Cannot connect to server: {e}")
        print("\nMake sure the server is running:")
        print("  cargo run -- --port 5433 --database /tmp/test_vector_e2e.db")
        return False
    
    print("Running vector search E2E tests...\n")
    
    tests = [
        test_create_vector_table,
        test_insert_and_select_vectors,
        test_l2_distance,
        test_cosine_distance,
        test_inner_product,
        test_l1_distance,
        test_vector_dims,
        test_l2_norm,
        test_l2_normalize,
        test_subvector,
        test_vector_search_query,
        test_vector_operators,
        test_vector_arithmetic,
    ]
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            print(f"✗ {test.__name__} failed: {e}")
            failed += 1
    
    print(f"\n{'='*50}")
    print(f"Results: {passed} passed, {failed} failed")
    
    return failed == 0

if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)
