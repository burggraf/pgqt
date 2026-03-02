"""
End-to-end tests for PostgreSQL LATERAL joins support.
"""

import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def test_json_each_lateral(proxy):
    """Test LATERAL json_each join."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS test_jsonb")
    cur.execute("""
        CREATE TABLE test_jsonb (
            id SERIAL PRIMARY KEY,
            name TEXT,
            props JSONB
        )
    """)
    conn.commit()
    
    # Insert JSON data
    cur.execute("""
        INSERT INTO test_jsonb (name, props) VALUES 
            ('item1', '{"a": 1, "b": 2}'),
            ('item2', '{"c": 3}')
    """)
    conn.commit()
    
    # Test LATERAL with jsonb_each
    # PG: SELECT name, key, value FROM test_jsonb, LATERAL jsonb_each(props) AS x(key, value)
    cur.execute("""
        SELECT name, key, value 
        FROM test_jsonb, LATERAL jsonb_each(props) AS x(key, value)
        ORDER BY name, key
    """)
    rows = cur.fetchall()
    print(f"LATERAL jsonb_each rows: {rows}")
    
    assert len(rows) == 3
    assert rows[0][0] == 'item1'
    assert rows[0][1] == 'a'
    assert str(rows[0][2]) == '1'
    
    print("✓ LATERAL jsonb_each works")
    
    cur.close()
    conn.close()

def test_json_array_elements_lateral(proxy):
    """Test LATERAL jsonb_array_elements join."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS test_json_array")
    cur.execute("""
        CREATE TABLE test_json_array (
            id SERIAL PRIMARY KEY,
            name TEXT,
            tags JSONB
        )
    """)
    conn.commit()
    
    # Insert JSON array data
    cur.execute("""
        INSERT INTO test_json_array (name, tags) VALUES 
            ('item1', '["admin", "user"]'),
            ('item2', '["editor"]')
    """)
    conn.commit()
    
    # Test LATERAL with jsonb_array_elements
    # PG maps jsonb_array_elements to json_each.
    # In SQLite, json_each(tags) returns (key, value, ...).
    # For an array, value is in the 'value' column.
    cur.execute("""
        SELECT name, value 
        FROM test_json_array, LATERAL jsonb_array_elements(tags)
        ORDER BY name, value
    """)
    rows = cur.fetchall()
    print(f"LATERAL jsonb_array_elements rows: {rows}")
    
    assert len(rows) == 3
    assert rows[0][0] == 'item1'
    assert rows[0][1] == 'admin'
    
    print("✓ LATERAL jsonb_array_elements works")
    
    cur.close()
    conn.close()

def test_unsupported_lateral_subquery(proxy):
    """Test that unsupported lateral subqueries report an error (or fail gracefully)."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    try:
        cur.execute("""
            SELECT * FROM (SELECT 1 as x) a, LATERAL (SELECT a.x + 1 as y) b
        """)
        cur.fetchall()
    except Exception as e:
        print(f"✓ Unsupported lateral subquery failed as expected: {e}")
    
    cur.close()
    conn.close()

def run_all_tests(proxy):
    test_json_each_lateral(proxy)
    test_json_array_elements_lateral(proxy)
    test_unsupported_lateral_subquery(proxy)

if __name__ == "__main__":
    run_e2e_test("lateral_joins", run_all_tests)
