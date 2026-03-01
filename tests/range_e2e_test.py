import psycopg2
import unittest
import time
import subprocess
import os

class TestRangeTypes(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # We assume the proxy is already running on port 5435
        pass

    @classmethod
    def tearDownClass(cls):
        pass

    def get_conn(self):
        conn = psycopg2.connect(
            host="127.0.0.1",
            port=5435,
            user="postgres",
            password="password",
            database="postgres"
        )
        conn.autocommit = True
        return conn

    def test_range_table(self):
        conn = self.get_conn()
        cur = conn.cursor()
        
        cur.execute("DROP TABLE IF EXISTS test_ranges")
        cur.execute("CREATE TABLE test_ranges (id SERIAL PRIMARY KEY, r INT4RANGE)")
        
        # Insert ranges
        cur.execute("INSERT INTO test_ranges (r) VALUES ('[10, 20]')")
        cur.execute("INSERT INTO test_ranges (r) VALUES ('(30, 40)')")
        cur.execute("INSERT INTO test_ranges (r) VALUES ('empty')")
        
        # Check canonicalization (discrete)
        cur.execute("SELECT r FROM test_ranges ORDER BY id")
        rows = cur.fetchall()
        self.assertEqual(rows[0][0], "[10,21)")
        self.assertEqual(rows[1][0], "[31,40)")
        self.assertEqual(rows[2][0], "empty")
        
        # Test @> operator with string
        cur.execute("SELECT id FROM test_ranges WHERE r @> '15'")
        rows = cur.fetchall()
        self.assertEqual(len(rows), 1)
        self.assertEqual(int(rows[0][0]), 1)
        
        # Test && operator
        cur.execute("SELECT id FROM test_ranges WHERE r && '[15, 25)'::int4range")
        rows = cur.fetchall()
        self.assertEqual(len(rows), 1)
        self.assertEqual(int(rows[0][0]), 1)
        
        # Test range metadata functions
        cur.execute("SELECT lower(r), upper(r), isempty(r) FROM test_ranges WHERE id = 1")
        row = cur.fetchone()
        self.assertEqual(row[0], "10")
        self.assertEqual(row[1], "21")
        self.assertEqual(bool(int(row[2])), False)
        
        cur.close()
        conn.close()

    def test_range_constructors(self):
        conn = self.get_conn()
        cur = conn.cursor()
        
        cur.execute("SELECT int4range(10, 20)")
        self.assertEqual(cur.fetchone()[0], "[10,20)")
        
        cur.execute("SELECT int4range(10, 20, '[]')")
        self.assertEqual(cur.fetchone()[0], "[10,21)")
        
        cur.close()
        conn.close()

    def test_daterange(self):
        conn = self.get_conn()
        cur = conn.cursor()
        
        cur.execute("SELECT daterange('2023-01-01', '2023-01-01', '[]')")
        self.assertEqual(cur.fetchone()[0], "[2023-01-01,2023-01-02)")
        
        cur.close()
        conn.close()

if __name__ == "__main__":
    unittest.main()
