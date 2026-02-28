import psycopg2
import time
import subprocess

def run_test(name, sql, expected_results=None, check_meta=False):
    print(f"--- Running Test: {name} ---")
    conn = None
    try:
        conn = psycopg2.connect(host="127.0.0.1", port=5432, user="postgres", dbname="test.db")
        cur = conn.cursor()
        
        cur.execute(sql)
        print(f"✅ SQL Executed: {sql}")
        
        if expected_results:
            rows = cur.fetchall()
            print(f"   Results: {rows}")
            assert len(rows) == len(expected_results), f"Expected {len(expected_results)} rows, got {len(rows)}"
        
        conn.commit()
        cur.close()
        conn.close()
        
        if check_meta:
            # Check the shadow catalog using sqlite3 directly
            table_name = "test_table"
            cmd = f"sqlite3 test.db \"SELECT column_name, original_type FROM __pg_meta__ WHERE table_name = '{table_name}';\""
            result = subprocess.check_output(cmd, shell=True).decode()
            print(f"📦 Metadata in __pg_meta__:\n{result}")
            assert "VARCHAR(10)" in result
            assert "SERIAL" in result
            assert "TIMESTAMPTZ" in result
            
    except Exception as e:
        print(f"❌ Test Failed: {e}")
        if conn: conn.close()
        exit(1)

if __name__ == "__main__":
    # 1. Clean up old DB
    subprocess.run("rm -f test.db", shell=True)
    
    # 2. Test Basic Connection & Schema Creation (Task 2 & 3)
    run_test("Schema & Metadata", 
             "CREATE TABLE test_table (id SERIAL, name VARCHAR(10), created_at TIMESTAMPTZ);",
             check_meta=True)
    
    # 3. Test Public Schema Mapping (Task 2)
    run_test("Public Schema Mapping",
             "SELECT * FROM public.test_table;")

    # 4. Test Data Insertion & Types (Phase 1.5 & Task 3)
    run_test("Data Insertion",
             "INSERT INTO test_table (name, created_at) VALUES ('alice', now());")
    
    # 5. Test Query with Types
    run_test("Query Results",
             "SELECT id, name FROM test_table;",
             expected_results=[(1, 'alice')])

    print("\n✨ ALL TESTS PASSED ✨")
