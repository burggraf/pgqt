#!/usr/bin/env python3
"""
E2E tests for multi-row triggers.
"""
import psycopg2
import sys
import os

# Add parent directory to path to import e2e_helper
sys.path.append(os.path.dirname(os.path.abspath(__file__)))
from e2e_helper import ProxyManager

def test_multi_row_update_trigger(proxy):
    """Test that UPDATE triggers fire for each affected row."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS items")
    cur.execute("CREATE TABLE items (id SERIAL PRIMARY KEY, val INTEGER, log TEXT)")
    cur.execute("INSERT INTO items (val) VALUES (10), (20), (30)")
    conn.commit()
    
    # Create trigger function
    cur.execute("""
        CREATE OR REPLACE FUNCTION log_update() RETURNS TRIGGER AS $$
        BEGIN
            NEW.log = 'updated ' || NEW.val;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    """)
    
    # Create trigger
    cur.execute("""
        CREATE TRIGGER items_before_update
        BEFORE UPDATE ON items
        FOR EACH ROW EXECUTE FUNCTION log_update();
    """)
    conn.commit()
    
    # Perform multi-row update
    cur.execute("UPDATE items SET val = val + 1 WHERE val > 15")
    conn.commit()
    
    # Verify results
    cur.execute("SELECT id, val, log FROM items ORDER BY id")
    rows = cur.fetchall()
    
    assert rows[0] == (1, 10, None), f"Expected (1, 10, None), got {rows[0]}"
    assert rows[1] == (2, 21, 'updated 21'), f"Expected (2, 21, 'updated 21'), got {rows[1]}"
    assert rows[2] == (3, 31, 'updated 31'), f"Expected (3, 31, 'updated 31'), got {rows[2]}"
    
    print("✓ multi_row_update_trigger PASSED")
    cur.close()
    conn.close()

def test_multi_row_delete_trigger(proxy):
    """Test that DELETE triggers fire for each affected row."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("DROP TABLE IF EXISTS audit_log")
    cur.execute("CREATE TABLE audit_log (msg TEXT)")
    cur.execute("DROP TABLE IF EXISTS items_del")
    cur.execute("CREATE TABLE items_del (id SERIAL PRIMARY KEY, name TEXT)")
    cur.execute("INSERT INTO items_del (name) VALUES ('a'), ('b'), ('c')")
    conn.commit()
    
    # Create trigger function to audit deletes
    cur.execute("""
        CREATE OR REPLACE FUNCTION skip_b() RETURNS TRIGGER AS $$
        BEGIN
            IF OLD.name = 'b' THEN
                RETURN NULL;
            END IF;
            RETURN OLD;
        END;
        $$ LANGUAGE plpgsql;
    """)
    
    cur.execute("""
        CREATE TRIGGER items_before_delete
        BEFORE DELETE ON items_del
        FOR EACH ROW EXECUTE FUNCTION skip_b();
    """)
    conn.commit()
    
    # Perform multi-row delete
    cur.execute("DELETE FROM items_del")
    conn.commit()
    
    # Verify results
    cur.execute("SELECT name FROM items_del")
    rows = cur.fetchall()
    assert rows == [('b',)], f"Expected only 'b' to remain, got {rows}"
    
    print("✓ multi_row_delete_trigger PASSED")
    cur.close()
    conn.close()

if __name__ == "__main__":
    with ProxyManager() as proxy:
        try:
            test_multi_row_update_trigger(proxy)
            test_multi_row_delete_trigger(proxy)
            print("\n✅ All multi-row trigger tests PASSED!")
        except Exception as e:
            print(f"\n❌ Test FAILED: {e}")
            import traceback
            traceback.print_exc()
            sys.exit(1)
