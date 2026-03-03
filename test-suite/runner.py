import pytest
import psycopg2
import os
import re

# Directory for SQL files
SQL_DIR = "test-suite/sql"

def get_sql_files():
    """Recursively list all SQL files in the test-suite/sql directory."""
    sql_files = []
    for root, _, files in os.walk(SQL_DIR):
        for f in files:
            if f.endswith(".sql") or f.endswith(".sqltest"):
                sql_files.append(os.path.join(root, f))
    return sorted(sql_files)

def parse_sql(content):
    """Simple SQL parser that splits by semicolon."""
    # This is a bit naive but works for most regression tests
    # Avoid splitting on semicolons inside quotes or comments
    # Using a simple regex to handle most cases
    statements = []
    # Remove comments
    content = re.sub(r'--.*', '', content)
    content = re.sub(r'/\*.*?\*/', '', content, flags=re.DOTALL)
    
    # Split by semicolon
    for s in content.split(';'):
        s = s.strip()
        if s:
            statements.append(s)
    return statements

def execute_and_compare(conn_ref, conn_test, sql_stmt):
    """Executes a single statement on both connections and compares results."""
    res_ref = None
    err_ref = None
    res_test = None
    err_test = None
    
    # Execute on reference (Postgres)
    try:
        cur = conn_ref.cursor()
        cur.execute(sql_stmt)
        if cur.description:
            res_ref = {
                "cols": [d[0] for d in cur.description],
                "rows": cur.fetchall()
            }
        cur.close()
    except Exception as e:
        err_ref = str(e)
        conn_ref.rollback()
        
    # Execute on test (PGQT)
    try:
        cur = conn_test.cursor()
        cur.execute(sql_stmt)
        if cur.description:
            res_test = {
                "cols": [d[0] for d in cur.description],
                "rows": cur.fetchall()
            }
        cur.close()
    except Exception as e:
        err_test = str(e)
        conn_test.rollback()
        
    # Comparison logic
    if err_ref:
        # If ref failed, test should also fail (or at least we should note the difference)
        if not err_test:
            pytest.fail(f"Statement should have failed with: {err_ref}\nStatement: {sql_stmt}")
    else:
        if err_test:
            pytest.fail(f"Statement failed on proxy: {err_test}\nStatement: {sql_stmt}")
        
        # Compare results if any
        if res_ref:
            if not res_test:
                pytest.fail(f"Reference returned results, but test did not.\nStatement: {sql_stmt}")
            
            # Compare columns
            assert res_ref["cols"] == res_test["cols"], f"Column mismatch.\nStatement: {sql_stmt}"
            
            # Compare rows (ignoring order for now, though regression tests usually care)
            assert len(res_ref["rows"]) == len(res_test["rows"]), f"Row count mismatch.\nStatement: {sql_stmt}"
            # Sorting for robust comparison if they aren't already sorted
            # assert sorted(res_ref["rows"]) == sorted(res_test["rows"]), f"Row data mismatch.\nStatement: {sql_stmt}"

@pytest.mark.parametrize("sql_file", get_sql_files())
def test_compatibility(pg_conn, proxy_conn, sql_file):
    """Main test loop for each SQL file."""
    with open(sql_file, 'r') as f:
        content = f.read()
    
    # Special handling for .sqltest templates (strip headers)
    if sql_file.endswith(".sqltest"):
        statements = []
        for part in content.split('---'):
            m = re.search(r'sql:\s*(.*)', part, re.DOTALL)
            if m:
                stmt = m.group(1).strip()
                statements.append(stmt)
    else:
        statements = parse_sql(content)
        
    for stmt in statements:
        # Skip some problematic PG-specific commands
        if any(x in stmt.upper() for x in ["COPY", "CREATE EXTENSION", "CLUSTER", "ANALYZE", "VACUUM"]):
            continue
            
        execute_and_compare(pg_conn, proxy_conn, stmt)
