#!/usr/bin/env python3
"""
End-to-end tests for PostgreSQL introspection commands (\d, \df, \du).
"""
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test

def test_introspection(proxy):
    """Test various psql-style introspection queries."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # 1. Test \dt (list tables)
    cur.execute("DROP TABLE IF EXISTS intro_test")
    cur.execute("CREATE TABLE intro_test (id SERIAL PRIMARY KEY, name TEXT)")
    conn.commit()
    
    # This is what \dt roughly does
    cur.execute("""
        SELECT n.nspname as "Schema",
          c.relname as "Name",
          CASE c.relkind WHEN 'r' THEN 'table' WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized view' WHEN 'i' THEN 'index' WHEN 'S' THEN 'sequence' WHEN 's' THEN 'special' WHEN 'f' THEN 'foreign table' END as "Type",
          pg_catalog.pg_get_userbyid(c.relowner) as "Owner"
        FROM pg_catalog.pg_class c
             LEFT JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind IN ('r','p','')
              AND n.nspname <> 'pg_catalog'
              AND n.nspname <> 'information_schema'
              AND n.nspname !~ '^pg_toast'
          AND pg_catalog.pg_table_is_visible(c.oid)
        ORDER BY 1,2;
    """)
    rows = cur.fetchall()
    print(f"Tables: {rows}")
    assert any(r[1] == 'intro_test' for r in rows)
    
    # 2. Test \df (list functions)
    cur.execute("CREATE OR REPLACE FUNCTION intro_func(a int, b text) RETURNS text AS $$ SELECT b || a::text $$ LANGUAGE sql;")
    conn.commit()
    
    cur.execute("""
        SELECT n.nspname as "Schema",
          p.proname as "Name",
          pg_catalog.pg_get_function_result(p.oid) as "Result data type",
          COALESCE(pg_catalog.pg_get_function_arguments(p.oid), '') as "Argument data types",
          CASE p.prokind
            WHEN 'a' THEN 'agg'
            WHEN 'w' THEN 'window'
            WHEN 'p' THEN 'proc'
            ELSE 'func'
          END as "Type"
        FROM pg_catalog.pg_proc p
             LEFT JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname <> 'pg_catalog'
              AND n.nspname <> 'information_schema'
          AND pg_catalog.pg_function_is_visible(p.oid)
        ORDER BY 1, 2, 4;
    """)
    rows = cur.fetchall()
    print(f"Functions: {rows}")
    assert any(r[1] == 'intro_func' for r in rows)
    
    # 3. Test \du (list roles)
    cur.execute("""
        SELECT r.rolname, r.rolsuper, r.rolinherit,
          r.rolcreaterole, r.rolcreatedb, r.rolcanlogin,
          r.rolconnlimit, r.rolvaliduntil,
          NULL as memberof
        , r.rolreplication
        , r.rolbypassrls
        FROM pg_catalog.pg_roles r
        WHERE r.rolname !~ '^pg_'
        ORDER BY 1;
    """)
    rows = cur.fetchall()
    print(f"Roles: {rows}")
    assert any(r[0] == 'postgres' for r in rows)
    
    # 4. Test comments visible in pg_description
    cur.execute("COMMENT ON TABLE intro_test IS 'This is a test table'")
    conn.commit()
    
    cur.execute("""
        SELECT description FROM pg_description d
        JOIN pg_class c ON d.objoid = c.oid
        WHERE c.relname = 'intro_test'
    """)
    row = cur.fetchone()
    print(f"Comment: {row}")
    assert row[0] == 'This is a test table'
    
    print("✓ Introspection tests passed")
    cur.close()
    conn.close()

if __name__ == "__main__":
    run_e2e_test("introspection", test_introspection)
