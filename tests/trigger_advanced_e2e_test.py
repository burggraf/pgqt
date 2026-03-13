#!/usr/bin/env python3
"""
End-to-end tests for advanced PostgreSQL trigger functionality.
Tests trigger assignments, NEW/OLD row access, and built-in functions.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_trigger_advanced_e2e.db"


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


def test_trigger_with_now_function():
    """Test trigger that uses NOW() function to set timestamp."""
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

        # Create test table with created_at column
        cur.execute("""
            CREATE TABLE events (
                id SERIAL PRIMARY KEY,
                name TEXT,
                created_at TIMESTAMP
            )
        """)

        # Create trigger function that sets created_at using NOW()
        cur.execute("""
            CREATE FUNCTION set_created_at() RETURNS TRIGGER AS $$
            BEGIN
                NEW.created_at = NOW();
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Create trigger
        cur.execute("""
            CREATE TRIGGER before_insert_events
            BEFORE INSERT ON events
            FOR EACH ROW
            EXECUTE FUNCTION set_created_at();
        """)

        # Insert data
        cur.execute("INSERT INTO events (name) VALUES ('Test Event')")
        conn.commit()

        # Verify the row was inserted with a timestamp
        cur.execute("SELECT name, created_at FROM events WHERE name = 'Test Event'")
        row = cur.fetchone()
        assert row is not None, "Row should exist"
        assert row[0] == 'Test Event'
        # created_at should be set (not None)
        assert row[1] is not None, "created_at should be set by trigger"

        cur.close()
        conn.close()
        print("test_trigger_with_now_function: PASSED")
    finally:
        stop_proxy(proc)


def test_trigger_validates_new_values():
    """Test trigger that validates NEW values and aborts if invalid."""
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
        cur.execute("""
            CREATE TABLE products (
                id SERIAL PRIMARY KEY,
                name TEXT,
                price REAL
            )
        """)

        # Create trigger function that validates price
        cur.execute("""
            CREATE FUNCTION check_price() RETURNS TRIGGER AS $$
            BEGIN
                IF NEW.price < 0 THEN
                    RETURN NULL;
                END IF;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Create trigger
        cur.execute("""
            CREATE TRIGGER before_insert_products
            BEFORE INSERT ON products
            FOR EACH ROW
            EXECUTE FUNCTION check_price();
        """)

        # Insert valid data (should succeed)
        cur.execute("INSERT INTO products (name, price) VALUES ('Widget', 10.00)")
        conn.commit()

        # Verify valid row was inserted
        cur.execute("SELECT COUNT(*) FROM products WHERE name = 'Widget'")
        count = int(cur.fetchone()[0])
        assert count == 1, f"Valid row should be inserted, got {count}"

        # Try to insert invalid data (should be blocked by trigger)
        cur.execute("INSERT INTO products (name, price) VALUES ('Invalid', -5.00)")
        conn.commit()

        # Verify invalid row was NOT inserted
        cur.execute("SELECT COUNT(*) FROM products WHERE name = 'Invalid'")
        count = int(cur.fetchone()[0])
        assert count == 0, f"Invalid row should be blocked by trigger, got {count}"

        cur.close()
        conn.close()
        print("test_trigger_validates_new_values: PASSED")
    finally:
        stop_proxy(proc)


def test_trigger_modifies_multiple_columns():
    """Test trigger that modifies multiple NEW columns."""
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
        cur.execute("""
            CREATE TABLE articles (
                id SERIAL PRIMARY KEY,
                title TEXT,
                slug TEXT,
                created_at TIMESTAMP
            )
        """)

        # Create trigger function that sets slug and created_at
        cur.execute("""
            CREATE FUNCTION prepare_article() RETURNS TRIGGER AS $$
            BEGIN
                NEW.slug = LOWER(NEW.title);
                NEW.created_at = CURRENT_TIMESTAMP;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Create trigger
        cur.execute("""
            CREATE TRIGGER before_insert_articles
            BEFORE INSERT ON articles
            FOR EACH ROW
            EXECUTE FUNCTION prepare_article();
        """)

        # Insert data
        cur.execute("INSERT INTO articles (title) VALUES ('Hello World')")
        conn.commit()

        # Verify both columns were set
        cur.execute("SELECT title, slug, created_at FROM articles WHERE title = 'Hello World'")
        row = cur.fetchone()
        assert row is not None, "Row should exist"
        assert row[0] == 'Hello World'
        assert row[1] == 'hello world', f"slug should be lowercase, got {row[1]}"
        assert row[2] is not None, "created_at should be set"

        cur.close()
        conn.close()
        print("test_trigger_modifies_multiple_columns: PASSED")
    finally:
        stop_proxy(proc)


if __name__ == "__main__":
    test_trigger_with_now_function()
    test_trigger_validates_new_values()
    test_trigger_modifies_multiple_columns()
    print("\n✅ All advanced trigger E2E tests passed!")
