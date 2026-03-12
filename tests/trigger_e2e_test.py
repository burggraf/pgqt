#!/usr/bin/env python3
"""
End-to-end tests for PostgreSQL trigger functionality.
Tests CREATE TRIGGER, trigger execution, and trigger effects through wire protocol.

NOTE: These tests verify the trigger infrastructure is working. Some tests are
simplified due to limitations in the PL/pgSQL transpiler (e.g., assignments in
trigger functions are not fully supported yet).
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_trigger_e2e.db"

def start_proxy():
    """Start the pgqt proxy in the background."""
    subprocess.run("pkill -f pgqt", shell=True)
    if os.path.exists(DB_PATH):
        try:
            os.remove(DB_PATH)
        except:
            pass
    
    proxy_cmd = f"./target/release/pgqt --port {PROXY_PORT} --database {DB_PATH}"
    proc = subprocess.Popen(
        proxy_cmd,
        shell=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        preexec_fn=os.setsid
    )
    
    time.sleep(2)
    return proc

def stop_proxy(proc):
    """Stop the pgqt proxy."""
    try:
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        proc.wait(timeout=5)
    except:
        pass
    
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

def test_before_insert_trigger():
    """Test BEFORE INSERT trigger that allows the insert."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table
        cur.execute("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT)")
        
        # Create PL/pgSQL function that just returns NEW
        cur.execute("""
            CREATE FUNCTION allow_insert() RETURNS TRIGGER AS $$
            BEGIN
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)
        
        # Create trigger
        cur.execute("""
            CREATE TRIGGER before_insert_users
            BEFORE INSERT ON users
            FOR EACH ROW
            EXECUTE FUNCTION allow_insert();
        """)
        
        # Insert data
        cur.execute("INSERT INTO users (name) VALUES ('Alice')")
        conn.commit()
        
        # Verify trigger fired (row was inserted)
        cur.execute("SELECT name FROM users WHERE name = 'Alice'")
        row = cur.fetchone()
        assert row is not None, "Row should exist"
        assert row[0] == 'Alice'
        
        cur.close()
        conn.close()
        print("test_before_insert_trigger: PASSED")
    finally:
        stop_proxy(proc)

def test_before_insert_trigger_abort():
    """Test BEFORE INSERT trigger that aborts the insert by returning NULL."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table
        cur.execute("CREATE TABLE blocked_users (id SERIAL PRIMARY KEY, name TEXT)")
        
        # Create PL/pgSQL function that returns NULL to abort
        cur.execute("""
            CREATE FUNCTION block_insert() RETURNS TRIGGER AS $$
            BEGIN
                RETURN NULL;
            END;
            $$ LANGUAGE plpgsql;
        """)
        
        # Create trigger
        cur.execute("""
            CREATE TRIGGER before_insert_blocked
            BEFORE INSERT ON blocked_users
            FOR EACH ROW
            EXECUTE FUNCTION block_insert();
        """)
        
        # Try to insert data (should be blocked by trigger)
        cur.execute("INSERT INTO blocked_users (name) VALUES ('Bob')")
        conn.commit()
        
        # Verify no row was inserted
        cur.execute("SELECT COUNT(*) FROM blocked_users")
        count = int(cur.fetchone()[0])
        assert count == 0, f"Expected 0 rows but got {count}"
        
        cur.close()
        conn.close()
        print("test_before_insert_trigger_abort: PASSED")
    finally:
        stop_proxy(proc)

def test_after_insert_trigger():
    """Test AFTER INSERT trigger fires (basic verification)."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create table
        cur.execute("CREATE TABLE orders (id SERIAL PRIMARY KEY, total REAL)")
        
        # Create function that just returns NEW
        cur.execute("""
            CREATE FUNCTION after_order_insert() RETURNS TRIGGER AS $$
            BEGIN
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)
        
        # Create AFTER trigger
        cur.execute("""
            CREATE TRIGGER after_insert_orders
            AFTER INSERT ON orders
            FOR EACH ROW
            EXECUTE FUNCTION after_order_insert();
        """)
        
        # Insert data
        cur.execute("INSERT INTO orders (total) VALUES (100.0)")
        conn.commit()
        
        # Verify row was inserted (trigger didn't block it)
        cur.execute("SELECT total FROM orders")
        row = cur.fetchone()
        assert row is not None, "Row should exist"
        assert float(row[0]) == 100.0
        
        cur.close()
        conn.close()
        print("test_after_insert_trigger: PASSED")
    finally:
        stop_proxy(proc)

def test_drop_trigger():
    """Test DROP TRIGGER."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table
        cur.execute("CREATE TABLE test_table (id SERIAL PRIMARY KEY, value TEXT)")
        
        # Create function that blocks inserts
        cur.execute("""
            CREATE FUNCTION block_all() RETURNS TRIGGER AS $$
            BEGIN
                RETURN NULL;
            END;
            $$ LANGUAGE plpgsql;
        """)
        
        # Create trigger
        cur.execute("""
            CREATE TRIGGER block_trigger
            BEFORE INSERT ON test_table
            FOR EACH ROW
            EXECUTE FUNCTION block_all();
        """)
        
        # Try to insert with trigger active - should be blocked
        cur.execute("INSERT INTO test_table (value) VALUES ('test')")
        conn.commit()
        
        cur.execute("SELECT COUNT(*) FROM test_table")
        count = int(cur.fetchone()[0])
        assert count == 0, f"No rows should exist with trigger active, got {count}"
        
        # Drop trigger
        cur.execute("DROP TRIGGER block_trigger ON test_table")
        
        # Insert without trigger - should succeed
        cur.execute("INSERT INTO test_table (value) VALUES ('test')")
        conn.commit()
        
        cur.execute("SELECT COUNT(*) FROM test_table")
        count = int(cur.fetchone()[0])
        assert count == 1, f"One row should exist after dropping trigger, got {count}"
        
        cur.close()
        conn.close()
        print("test_drop_trigger: PASSED")
    finally:
        stop_proxy(proc)

def test_multiple_triggers():
    """Test multiple triggers on the same table/event."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table with counter
        cur.execute("CREATE TABLE items (id SERIAL PRIMARY KEY, name TEXT)")
        
        # Create first trigger function (just returns NEW)
        cur.execute("""
            CREATE FUNCTION trigger1() RETURNS TRIGGER AS $$
            BEGIN
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)
        
        # Create second trigger function (just returns NEW)
        cur.execute("""
            CREATE FUNCTION trigger2() RETURNS TRIGGER AS $$
            BEGIN
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)
        
        # Create both triggers
        cur.execute("""
            CREATE TRIGGER first_trigger
            BEFORE INSERT ON items
            FOR EACH ROW
            EXECUTE FUNCTION trigger1();
        """)
        
        cur.execute("""
            CREATE TRIGGER second_trigger
            BEFORE INSERT ON items
            FOR EACH ROW
            EXECUTE FUNCTION trigger2();
        """)
        
        # Insert data
        cur.execute("INSERT INTO items (name) VALUES ('widget')")
        conn.commit()
        
        # Verify row was inserted (both triggers allowed it)
        cur.execute("SELECT name FROM items")
        row = cur.fetchone()
        assert row is not None, "Row should exist"
        assert row[0] == 'widget'
        
        cur.close()
        conn.close()
        print("test_multiple_triggers: PASSED")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    test_before_insert_trigger()
    test_before_insert_trigger_abort()
    test_after_insert_trigger()
    test_drop_trigger()
    test_multiple_triggers()
    print("\n✅ All E2E tests passed!")
