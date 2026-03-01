#!/usr/bin/env python3
"""
End-to-end tests for pg_catalog system views.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_catalog_e2e.db"

def start_proxy():
    """Start the PostgreSQL proxy server."""
    # Clean up old DB
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    proc = subprocess.Popen(
        ["./target/release/postgresqlite", "-d", DB_PATH, "-p", str(PROXY_PORT)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    time.sleep(1)
    return proc

def stop_proxy(proc):
    """Stop the proxy server."""
    proc.send_signal(signal.SIGTERM)
    proc.wait()
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

def test_pg_class():
    """Test pg_class catalog view."""
    print("Testing pg_class...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        # Create test table
        cur.execute("CREATE TABLE test_users (id SERIAL PRIMARY KEY, name TEXT)")
        conn.commit()
        
        # Query pg_class
        cur.execute("SELECT relname, relkind FROM pg_class WHERE relname = 'test_users'")
        result = cur.fetchone()
        assert result is not None, "test_users should be in pg_class"
        assert result[0] == 'test_users'
        assert result[1] == 'r'  # regular table
        
        cur.close()
        conn.close()
        print("  ✓ pg_class works")
    finally:
        stop_proxy(proc)

def test_pg_attribute():
    """Test pg_attribute catalog view."""
    print("Testing pg_attribute...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("CREATE TABLE test_attrs (id INTEGER PRIMARY KEY, email TEXT NOT NULL)")
        conn.commit()
        
        # Query pg_attribute
        cur.execute("""
            SELECT attname, attnotnull, attnum 
            FROM pg_attribute a
            JOIN pg_class c ON a.attrelid = c.oid
            WHERE c.relname = 'test_attrs'
            ORDER BY attnum
        """)
        results = cur.fetchall()
        assert len(results) >= 2, "Should have at least 2 columns"
        
        col_names = [r[0] for r in results]
        assert 'id' in col_names
        assert 'email' in col_names
        
        cur.close()
        conn.close()
        print("  ✓ pg_attribute works")
    finally:
        stop_proxy(proc)

def test_pg_type():
    """Test pg_type catalog view."""
    print("Testing pg_type...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("SELECT typname, typtype FROM pg_type WHERE typname = 'int4'")
        result = cur.fetchone()
        assert result is not None, "int4 type should exist"
        assert result[0] == 'int4'
        assert result[1] == 'b'  # base type
        
        cur.close()
        conn.close()
        print("  ✓ pg_type works")
    finally:
        stop_proxy(proc)

def test_pg_tables():
    """Test pg_tables view."""
    print("Testing pg_tables...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("CREATE TABLE test_pg_tables (id INTEGER PRIMARY KEY)")
        conn.commit()
        
        cur.execute("SELECT tablename FROM pg_tables WHERE tablename = 'test_pg_tables'")
        result = cur.fetchone()
        assert result is not None, "test_pg_tables should be in pg_tables"
        
        cur.close()
        conn.close()
        print("  ✓ pg_tables works")
    finally:
        stop_proxy(proc)

def test_pg_namespace():
    """Test pg_namespace catalog view."""
    print("Testing pg_namespace...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("SELECT nspname FROM pg_namespace ORDER BY nspname")
        results = cur.fetchall()
        schema_names = [r[0] for r in results]
        
        assert 'public' in schema_names
        assert 'pg_catalog' in schema_names
        
        cur.close()
        conn.close()
        print("  ✓ pg_namespace works")
    finally:
        stop_proxy(proc)

def test_pg_roles():
    """Test pg_roles view."""
    print("Testing pg_roles...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("SELECT rolname, rolsuper FROM pg_roles WHERE rolname = 'postgres'")
        result = cur.fetchone()
        assert result is not None, "postgres role should exist"
        assert result[0] == 'postgres'
        assert result[1] in (True, 1, '1')  # superuser (SQLite returns 0/1)
        
        cur.close()
        conn.close()
        print("  ✓ pg_roles works")
    finally:
        stop_proxy(proc)

def test_pg_database():
    """Test pg_database view."""
    print("Testing pg_database...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("SELECT datname, encoding FROM pg_database LIMIT 1")
        result = cur.fetchone()
        assert result is not None
        assert result[0] == 'postgres'
        
        cur.close()
        conn.close()
        print("  ✓ pg_database works")
    finally:
        stop_proxy(proc)

def test_pg_settings():
    """Test pg_settings view."""
    print("Testing pg_settings...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("SELECT name, setting FROM pg_settings WHERE name = 'server_version'")
        result = cur.fetchone()
        assert result is not None
        assert result[0] == 'server_version'
        assert result[1] == '15.0'
        
        cur.close()
        conn.close()
        print("  ✓ pg_settings works")
    finally:
        stop_proxy(proc)

def test_pg_extension():
    """Test pg_extension view."""
    print("Testing pg_extension...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("SELECT extname FROM pg_extension WHERE extname = 'plpgsql'")
        result = cur.fetchone()
        assert result is not None
        assert result[0] == 'plpgsql'
        
        cur.close()
        conn.close()
        print("  ✓ pg_extension works")
    finally:
        stop_proxy(proc)

def test_orm_introspection_query():
    """Test a typical ORM introspection query."""
    print("Testing ORM introspection query...")
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST, port=PROXY_PORT,
            database="postgres", user="postgres", password="postgres"
        )
        cur = conn.cursor()
        
        # Create test schema
        cur.execute("""
            CREATE TABLE test_orm_table (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                email TEXT UNIQUE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.commit()
        
        # Query similar to what Prisma/TypeORM would do
        cur.execute("""
            SELECT 
                c.relname as table_name,
                a.attname as column_name,
                t.typname as data_type,
                a.attnotnull as is_nullable,
                a.attnum as ordinal_position
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_attribute a ON a.attrelid = c.oid
            JOIN pg_type t ON t.oid = a.atttypid
            WHERE n.nspname = 'public'
              AND c.relkind = 'r'
              AND a.attnum > 0
              AND NOT a.attisdropped
            ORDER BY c.relname, a.attnum
        """)
        
        results = cur.fetchall()
        assert len(results) >= 4, "Should have at least 4 columns"
        
        col_names = [r[1] for r in results if r[0] == 'test_orm_table']
        assert 'id' in col_names
        assert 'name' in col_names
        assert 'email' in col_names
        assert 'created_at' in col_names
        
        cur.close()
        conn.close()
        print("  ✓ ORM introspection query works")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    test_pg_class()
    test_pg_attribute()
    test_pg_type()
    test_pg_tables()
    test_pg_namespace()
    test_pg_roles()
    test_pg_database()
    test_pg_settings()
    test_pg_extension()
    test_orm_introspection_query()
    print("\n✅ All catalog E2E tests passed!")
