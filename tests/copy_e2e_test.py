#!/usr/bin/env python3
"""
End-to-end tests for PostgreSQL COPY command support.
"""

import sys
import os
import io

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test

def test_copy_from_stdin_text(proxy):
    """Test COPY FROM STDIN with default text format."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE copy_text (id INT, name TEXT)")
    
    # Data in text format (tab-delimited)
    data = "1\talice\n2\tbob\n3\tcharlie\n"
    f = io.StringIO(data)
    
    cur.copy_from(f, 'copy_text')
    conn.commit()
    
    cur.execute("SELECT * FROM copy_text ORDER BY id")
    rows = cur.fetchall()
    # Proxy currently returns everything as TEXT
    assert rows == [('1', 'alice'), ('2', 'bob'), ('3', 'charlie')]
    
    print("✓ COPY FROM STDIN (text) works")
    cur.close()
    conn.close()

def test_copy_from_stdin_csv(proxy):
    """Test COPY FROM STDIN with CSV format."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE copy_csv (id INT, name TEXT, city TEXT)")
    
    # Data in CSV format
    data = "1,alice,New York\n2,bob,London\n3,charlie,Paris\n"
    f = io.StringIO(data)
    
    cur.copy_expert("COPY copy_csv FROM STDIN WITH (FORMAT CSV)", f)
    conn.commit()
    
    cur.execute("SELECT * FROM copy_csv ORDER BY id")
    rows = cur.fetchall()
    assert rows == [('1', 'alice', 'New York'), ('2', 'bob', 'London'), ('3', 'charlie', 'Paris')]
    
    print("✓ COPY FROM STDIN (CSV) works")
    cur.close()
    conn.close()

def test_copy_to_stdout_text(proxy):
    """Test COPY TO STDOUT with default text format."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE export_text (id INT, name TEXT)")
    cur.execute("INSERT INTO export_text VALUES (1, 'alice'), (2, 'bob')")
    conn.commit()
    
    f = io.StringIO()
    cur.copy_to(f, 'export_text')
    
    result = f.getvalue()
    assert result == "1\talice\n2\tbob\n"
    
    print("✓ COPY TO STDOUT (text) works")
    cur.close()
    conn.close()

def test_copy_to_stdout_csv(proxy):
    """Test COPY TO STDOUT with CSV format and HEADER."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE export_csv (id INT, name TEXT)")
    cur.execute("INSERT INTO export_csv VALUES (1, 'alice'), (2, 'bob')")
    conn.commit()
    
    f = io.StringIO()
    cur.copy_expert("COPY export_csv TO STDOUT WITH (FORMAT CSV, HEADER)", f)
    
    result = f.getvalue()
    # Note: Column names might be uppercase depending on SQLite/proxy behavior
    # Our proxy currently preserves case or uses lowercase. 
    # Let's check what it actually returns.
    lines = result.splitlines()
    assert len(lines) == 3
    assert lines[0].lower() == "id,name"
    assert lines[1] == "1,alice"
    assert lines[2] == "2,bob"
    
    print("✓ COPY TO STDOUT (CSV) works")
    cur.close()
    conn.close()

def test_copy_to_stdout_query(proxy):
    """Test COPY (SELECT ...) TO STDOUT."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE query_table (id INT, name TEXT, val INT)")
    cur.execute("INSERT INTO query_table VALUES (1, 'a', 10), (2, 'b', 20), (3, 'c', 30)")
    conn.commit()
    
    f = io.StringIO()
    cur.copy_expert("COPY (SELECT name, val FROM query_table WHERE val > 15) TO STDOUT", f)
    
    result = f.getvalue()
    assert result == "b\t20\nc\t30\n"
    
    print("✓ COPY (QUERY) TO STDOUT works")
    cur.close()
    conn.close()

def test_copy_binary(proxy):
    """Test COPY with binary format."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE copy_binary (id INT, name TEXT)")
    
    # Insert some data first
    cur.execute("INSERT INTO copy_binary VALUES (1, 'alice'), (2, 'bob')")
    conn.commit()
    
    # Export to binary
    f = io.BytesIO()
    cur.copy_expert("COPY copy_binary TO STDOUT WITH (FORMAT BINARY)", f)
    binary_data = f.getvalue()
    
    # Check signature
    assert binary_data.startswith(b"PGCOPY\n\xff\r\n\0")
    
    # Import from binary into a new table
    cur.execute("CREATE TABLE copy_binary_import (id INT, name TEXT)")
    f.seek(0)
    cur.copy_expert("COPY copy_binary_import FROM STDIN WITH (FORMAT BINARY)", f)
    conn.commit()
    
    cur.execute("SELECT * FROM copy_binary_import ORDER BY id")
    rows = cur.fetchall()
    assert rows == [('1', 'alice'), ('2', 'bob')]
    
    print("✓ COPY (BINARY) works")
    cur.close()
    conn.close()

if __name__ == "__main__":
    import sys
    
    def run_all(proxy):
        test_copy_from_stdin_text(proxy)
        test_copy_from_stdin_csv(proxy)
        test_copy_to_stdout_text(proxy)
        test_copy_to_stdout_csv(proxy)
        test_copy_to_stdout_query(proxy)
        test_copy_binary(proxy)
        
    from e2e_helper import run_e2e_test
    run_e2e_test("copy_e2e", run_all)
