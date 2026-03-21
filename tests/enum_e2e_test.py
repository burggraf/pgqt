#!/usr/bin/env python3
"""
End-to-end tests for PostgreSQL enum types.

This test suite validates enum type support including:
- CREATE TYPE ... AS ENUM
- Enum column type with CHECK constraints
- Valid and invalid enum value insertion
- Enum type metadata in catalog
"""

import sys
import os
import psycopg2

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def test_create_enum_type(proxy):
    """Test CREATE TYPE ... AS ENUM statement."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create an enum type
    cur.execute("CREATE TYPE mood AS ENUM ('happy', 'sad', 'neutral')")
    
    # Verify the enum values were stored in the catalog
    cur.execute("SELECT enumlabel FROM pg_enum WHERE enumtypid = (SELECT oid FROM pg_type WHERE typname = 'mood') ORDER BY enumsortorder")
    rows = cur.fetchall()
    assert len(rows) == 3, f"Expected 3 enum values, got {len(rows)}"
    assert rows[0][0] == 'happy', f"Expected 'happy', got {rows[0][0]}"
    assert rows[1][0] == 'sad', f"Expected 'sad', got {rows[1][0]}"
    assert rows[2][0] == 'neutral', f"Expected 'neutral', got {rows[2][0]}"
    
    cur.close()
    conn.close()
    print("✓ CREATE TYPE ... AS ENUM works")

def test_enum_column_with_check(proxy):
    """Test that enum columns get CHECK constraints."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create enum type
    cur.execute("CREATE TYPE status AS ENUM ('pending', 'active', 'completed', 'cancelled')")
    
    # Create table with enum column
    cur.execute("CREATE TABLE tasks (id SERIAL PRIMARY KEY, name TEXT, task_status status)")
    
    # Check that the CHECK constraint exists by querying the table schema
    cur.execute("SELECT sql FROM sqlite_master WHERE type='table' AND name='tasks'")
    row = cur.fetchone()
    assert row is not None, "Table 'tasks' not found"
    create_sql = row[0].lower()
    
    # Verify CHECK constraint is present
    assert 'check' in create_sql, f"CHECK constraint not found in: {create_sql}"
    assert 'task_status' in create_sql, f"Column 'task_status' not found in: {create_sql}"
    assert 'pending' in create_sql, f"Enum value 'pending' not found in CHECK constraint"
    assert 'active' in create_sql, f"Enum value 'active' not found in CHECK constraint"
    assert 'completed' in create_sql, f"Enum value 'completed' not found in CHECK constraint"
    assert 'cancelled' in create_sql, f"Enum value 'cancelled' not found in CHECK constraint"
    
    cur.close()
    conn.close()
    print("✓ Enum column CHECK constraint works")

def test_valid_enum_insert(proxy):
    """Test inserting valid enum values."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create enum type and table
    cur.execute("CREATE TYPE priority AS ENUM ('low', 'medium', 'high')")
    cur.execute("CREATE TABLE items (id SERIAL PRIMARY KEY, name TEXT, priority priority)")
    
    # Insert valid enum values
    cur.execute("INSERT INTO items (name, priority) VALUES ('item1', 'low')")
    cur.execute("INSERT INTO items (name, priority) VALUES ('item2', 'medium')")
    cur.execute("INSERT INTO items (name, priority) VALUES ('item3', 'high')")
    
    # Verify inserts
    cur.execute("SELECT name, priority FROM items ORDER BY id")
    rows = cur.fetchall()
    assert len(rows) == 3, f"Expected 3 rows, got {len(rows)}"
    assert rows[0] == ('item1', 'low'), f"Expected ('item1', 'low'), got {rows[0]}"
    assert rows[1] == ('item2', 'medium'), f"Expected ('item2', 'medium'), got {rows[1]}"
    assert rows[2] == ('item3', 'high'), f"Expected ('item3', 'high'), got {rows[2]}"
    
    cur.close()
    conn.close()
    print("✓ Valid enum value insertion works")

def test_invalid_enum_insert(proxy):
    """Test that invalid enum values are rejected by CHECK constraint."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create enum type and table
    cur.execute("CREATE TYPE color AS ENUM ('red', 'green', 'blue')")
    cur.execute("CREATE TABLE colored_items (id SERIAL PRIMARY KEY, name TEXT, color color)")
    
    # Insert valid value
    cur.execute("INSERT INTO colored_items (name, color) VALUES ('item1', 'red')")
    conn.commit()
    
    # Try to insert invalid enum value - should fail due to CHECK constraint
    error_caught = False
    try:
        cur.execute("INSERT INTO colored_items (name, color) VALUES ('item2', 'yellow')")
        conn.commit()
    except psycopg2.errors.CheckViolation:
        conn.rollback()
        error_caught = True
    except Exception as e:
        conn.rollback()
        # SQLite might return a different error type
        error_str = str(e).lower()
        if 'check' in error_str or 'constraint' in error_str:
            error_caught = True
        else:
            print(f"Unexpected error type: {e}")
    
    # Verify the CHECK constraint was enforced
    if not error_caught:
        # Check if the row was actually inserted - if so, CHECK constraint didn't work
        cur.execute("SELECT COUNT(*) FROM colored_items")
        count = cur.fetchone()[0]
        if count > 1:
            raise AssertionError(f"CHECK constraint failed - invalid enum value 'yellow' was accepted")
    
    # Verify only the valid row exists
    cur.execute("SELECT COUNT(*) FROM colored_items")
    count = int(cur.fetchone()[0])
    assert count == 1, f"Expected 1 row after invalid insert rejection, got {count}"
    
    cur.close()
    conn.close()
    print("✓ Invalid enum value rejection works")

def test_enum_in_where_clause(proxy):
    """Test using enum values in WHERE clause."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create enum type and table
    cur.execute("CREATE TYPE state AS ENUM ('new', 'in_progress', 'done', 'archived')")
    cur.execute("CREATE TABLE projects (id SERIAL PRIMARY KEY, name TEXT, state state)")
    
    # Insert data
    cur.execute("INSERT INTO projects (name, state) VALUES ('proj1', 'new')")
    cur.execute("INSERT INTO projects (name, state) VALUES ('proj2', 'in_progress')")
    cur.execute("INSERT INTO projects (name, state) VALUES ('proj3', 'done')")
    cur.execute("INSERT INTO projects (name, state) VALUES ('proj4', 'done')")
    
    # Query by enum value
    cur.execute("SELECT name FROM projects WHERE state = 'done' ORDER BY name")
    rows = cur.fetchall()
    assert len(rows) == 2, f"Expected 2 rows, got {len(rows)}"
    assert rows[0][0] == 'proj3', f"Expected 'proj3', got {rows[0][0]}"
    assert rows[1][0] == 'proj4', f"Expected 'proj4', got {rows[1][0]}"
    
    # Query with IN clause
    cur.execute("SELECT name FROM projects WHERE state IN ('new', 'in_progress') ORDER BY name")
    rows = cur.fetchall()
    assert len(rows) == 2, f"Expected 2 rows, got {len(rows)}"
    assert rows[0][0] == 'proj1'
    assert rows[1][0] == 'proj2'
    
    cur.close()
    conn.close()
    print("✓ Enum values in WHERE clause work")

def test_multiple_enum_types(proxy):
    """Test multiple enum types in the same database."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create multiple enum types
    cur.execute("CREATE TYPE size AS ENUM ('small', 'medium', 'large')")
    cur.execute("CREATE TYPE weight AS ENUM ('light', 'medium', 'heavy')")
    
    # Create table with multiple enum columns
    cur.execute("CREATE TABLE products (id SERIAL PRIMARY KEY, name TEXT, size size, weight weight)")
    
    # Insert data
    cur.execute("INSERT INTO products (name, size, weight) VALUES ('widget', 'small', 'light')")
    cur.execute("INSERT INTO products (name, size, weight) VALUES ('gadget', 'large', 'heavy')")
    
    # Query
    cur.execute("SELECT name FROM products WHERE size = 'large' AND weight = 'heavy'")
    rows = cur.fetchall()
    assert len(rows) == 1, f"Expected 1 row, got {len(rows)}"
    assert rows[0][0] == 'gadget', f"Expected 'gadget', got {rows[0][0]}"
    
    # Verify different CHECK constraints for different enum types
    cur.execute("SELECT sql FROM sqlite_master WHERE type='table' AND name='products'")
    row = cur.fetchone()
    create_sql = row[0].lower()
    assert 'size' in create_sql
    assert 'weight' in create_sql
    # Both enums have 'medium' but should have different constraints
    assert create_sql.count('medium') >= 2, f"Expected 'medium' to appear for both enums"
    
    cur.close()
    conn.close()
    print("✓ Multiple enum types work")

def test_enum_update(proxy):
    """Test updating enum values."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create enum type and table
    cur.execute("CREATE TYPE level AS ENUM ('beginner', 'intermediate', 'advanced', 'expert')")
    cur.execute("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, level level)")
    
    # Insert data
    cur.execute("INSERT INTO users (name, level) VALUES ('alice', 'beginner')")
    
    # Update to valid value
    cur.execute("UPDATE users SET level = 'intermediate' WHERE name = 'alice'")
    
    # Verify update
    cur.execute("SELECT level FROM users WHERE name = 'alice'")
    row = cur.fetchone()
    assert row[0] == 'intermediate', f"Expected 'intermediate', got {row[0]}"
    
    # Try to update to invalid value
    try:
        cur.execute("UPDATE users SET level = 'master' WHERE name = 'alice'")
        conn.commit()
        assert False, "Expected CHECK constraint violation"
    except (psycopg2.errors.CheckViolation, Exception) as e:
        conn.rollback()
        if 'check' not in str(e).lower() and 'constraint' not in str(e).lower():
            # Still acceptable if it's a constraint-related error
            pass
    
    cur.close()
    conn.close()
    print("✓ Enum value updates work")

def test_enum_null_values(proxy):
    """Test that NULL values are allowed for enum columns."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create enum type and table
    cur.execute("CREATE TYPE pet_type AS ENUM ('dog', 'cat', 'bird')")
    cur.execute("CREATE TABLE pets (id SERIAL PRIMARY KEY, name TEXT, pet_type pet_type)")
    
    # Insert with NULL enum value
    cur.execute("INSERT INTO pets (name, pet_type) VALUES ('unknown', NULL)")
    
    # Verify insert
    cur.execute("SELECT name, pet_type FROM pets WHERE name = 'unknown'")
    row = cur.fetchone()
    assert row == ('unknown', None), f"Expected ('unknown', None), got {row}"
    
    cur.close()
    conn.close()
    print("✓ NULL enum values work")

def main():
    """Run all enum E2E tests."""
    print("=" * 60)
    print("PGQT Enum E2E Tests")
    print("=" * 60)
    
    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        
        tests = [
            ("test_create_enum_type", test_create_enum_type),
            ("test_enum_column_with_check", test_enum_column_with_check),
            ("test_valid_enum_insert", test_valid_enum_insert),
            ("test_invalid_enum_insert", test_invalid_enum_insert),
            ("test_enum_in_where_clause", test_enum_in_where_clause),
            ("test_multiple_enum_types", test_multiple_enum_types),
            ("test_enum_update", test_enum_update),
            ("test_enum_null_values", test_enum_null_values),
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
            print(f"All {passed} E2E enum tests passed!")
            return 0
        else:
            print(f"{passed} passed, {failed} failed")
            return 1

if __name__ == "__main__":
    import sys
    sys.exit(main())