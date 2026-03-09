#!/usr/bin/env python3
"""
End-to-End tests for PGQT DISTINCT ON transpilation to SQLite.

This test suite verifies that DISTINCT ON is correctly transpiled
from PostgreSQL syntax to SQLite using ROW_NUMBER() and produces correct results.
"""

import os
import sys
import psycopg2
import pytest
import subprocess
import time

# Test database file
TEST_DB = "/tmp/pglite_distinct_on_test.db"
TEST_PORT = 55433


@pytest.fixture(scope="module")
def proxy_server():
    """Start the PGlite proxy server for testing."""
    # Build the proxy if needed
    subprocess.run(["cargo", "build", "--release"], check=True, capture_output=True)
    
    # Remove old test database
    if os.path.exists(TEST_DB):
        os.remove(TEST_DB)
    
    # Start the proxy server
    env = os.environ.copy()
    env["PG_LITE_DB"] = TEST_DB
    env["PG_LITE_PORT"] = str(TEST_PORT)
    
    proc = subprocess.Popen(
        ["./target/release/pgqt"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    
    # Wait for server to start
    time.sleep(2)
    
    yield proc
    
    # Cleanup
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    
    if os.path.exists(TEST_DB):
        os.remove(TEST_DB)


@pytest.fixture(scope="module")
def db_connection(proxy_server):
    """Create a database connection and set up test data."""
    conn = psycopg2.connect(
        host="127.0.0.1",
        port=TEST_PORT,
        user="postgres",
        password="postgres",
        database="test"
    )
    conn.autocommit = True
    
    # Create test tables
    with conn.cursor() as cur:
        # Orders table - classic DISTINCT ON use case
        cur.execute("""
            CREATE TABLE orders (
                id SERIAL PRIMARY KEY,
                customer_id INT,
                order_date DATE,
                amount DECIMAL(10, 2),
                status VARCHAR(20)
            )
        """)
        
        # Insert test data - multiple orders per customer
        cur.execute("""
            INSERT INTO orders (customer_id, order_date, amount, status) VALUES
                (1, '2023-01-01', 100.00, 'completed'),
                (1, '2023-01-15', 200.00, 'completed'),
                (1, '2023-02-01', 150.00, 'pending'),
                (2, '2023-01-02', 300.00, 'completed'),
                (2, '2023-01-20', 250.00, 'completed'),
                (3, '2023-01-03', 50.00, 'completed'),
                (3, '2023-01-10', 75.00, 'cancelled'),
                (3, '2023-02-05', 100.00, 'completed')
        """)
        
        # Employees table for department tests
        cur.execute("""
            CREATE TABLE employees (
                id SERIAL PRIMARY KEY,
                name VARCHAR(100),
                department VARCHAR(50),
                role VARCHAR(50),
                salary DECIMAL(10, 2),
                hire_date DATE
            )
        """)
        
        cur.execute("""
            INSERT INTO employees (name, department, role, salary, hire_date) VALUES
                ('Alice', 'Engineering', 'Senior', 100000, '2020-01-15'),
                ('Bob', 'Engineering', 'Junior', 70000, '2021-03-20'),
                ('Charlie', 'Engineering', 'Senior', 95000, '2019-06-10'),
                ('Diana', 'Sales', 'Manager', 90000, '2020-09-01'),
                ('Eve', 'Sales', 'Associate', 65000, '2021-07-15'),
                ('Frank', 'Sales', 'Senior', 85000, '2020-04-22'),
                ('Grace', 'Marketing', 'Manager', 80000, '2022-01-10'),
                ('Henry', 'Marketing', 'Associate', 60000, '2021-11-05')
        """)
    
    yield conn
    
    conn.close()


class TestBasicDistinctOn:
    """Tests for basic DISTINCT ON functionality."""
    
    def test_distinct_on_single_column(self, db_connection):
        """Test DISTINCT ON with single column - get first order per customer."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
                FROM orders
                ORDER BY customer_id, order_date
            """)
            rows = cur.fetchall()
            
            # Should get exactly one row per customer (3 customers)
            assert len(rows) == 3
            
            # Each customer should appear exactly once (values may be strings)
            customer_ids = [int(r[0]) for r in rows]
            assert sorted(customer_ids) == [1, 2, 3]
            
            # Should be the earliest order for each customer
            for row in rows:
                customer_id = int(row[0])
                if customer_id == 1:
                    assert str(row[1]) == '2023-01-01'  # Earliest
                elif customer_id == 2:
                    assert str(row[1]) == '2023-01-02'  # Earliest
                elif customer_id == 3:
                    assert str(row[1]) == '2023-01-03'  # Earliest
    
    def test_distinct_on_latest_per_group(self, db_connection):
        """Test DISTINCT ON to get latest order per customer."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
                FROM orders
                ORDER BY customer_id, order_date DESC
            """)
            rows = cur.fetchall()
            
            assert len(rows) == 3
            
            # Should be the latest order for each customer
            for row in rows:
                customer_id = int(row[0])
                if customer_id == 1:
                    assert str(row[1]) == '2023-02-01'  # Latest
                elif customer_id == 2:
                    assert str(row[1]) == '2023-01-20'  # Latest
                elif customer_id == 3:
                    assert str(row[1]) == '2023-02-05'  # Latest
    
    def test_distinct_on_with_where(self, db_connection):
        """Test DISTINCT ON with WHERE clause."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date, amount
                FROM orders
                WHERE status = 'completed'
                ORDER BY customer_id, order_date DESC
            """)
            rows = cur.fetchall()
            
            # Customer 3 has a later cancelled order, so latest completed is 2023-02-05
            assert len(rows) == 3
            
            for row in rows:
                customer_id = int(row[0])
                if customer_id == 3:
                    # Latest completed order (not the cancelled one)
                    assert str(row[1]) == '2023-02-05'


class TestDistinctOnMultipleColumns:
    """Tests for DISTINCT ON with multiple columns."""
    
    def test_distinct_on_two_columns(self, db_connection):
        """Test DISTINCT ON with multiple columns."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (department, role) department, role, name, salary
                FROM employees
                ORDER BY department, role, salary DESC
            """)
            rows = cur.fetchall()
            
            # Should get one row per (department, role) combination
            dept_roles = set((r[0], r[1]) for r in rows)
            
            # Engineering: Senior (2), Junior (1)
            # Sales: Manager (1), Senior (1), Associate (1)
            # Marketing: Manager (1), Associate (1)
            # Total: 6 combinations (but we may have 7 due to data)
            assert len(dept_roles) >= 6
            
            # For Engineering/Senior, should get highest salary (Alice: 100000)
            eng_senior = [r for r in rows if r[0] == 'Engineering' and r[1] == 'Senior']
            assert len(eng_senior) == 1
            assert float(eng_senior[0][3]) == 100000.0


class TestDistinctOnWithLimit:
    """Tests for DISTINCT ON with LIMIT and OFFSET."""
    
    def test_distinct_on_with_limit(self, db_connection):
        """Test DISTINCT ON with LIMIT."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date
                FROM orders
                ORDER BY customer_id, order_date
                LIMIT 2
            """)
            rows = cur.fetchall()
            
            # Should get only 2 rows
            assert len(rows) == 2
            
            # Should be first 2 customers
            customer_ids = sorted([int(r[0]) for r in rows])
            assert customer_ids == [1, 2]
    
    def test_distinct_on_with_limit_offset(self, db_connection):
        """Test DISTINCT ON with LIMIT and OFFSET."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, order_date
                FROM orders
                ORDER BY customer_id, order_date
                LIMIT 1 OFFSET 1
            """)
            rows = cur.fetchall()
            
            # Should get only 1 row
            assert len(rows) == 1
            
            # Should be customer 2 (skipping customer 1)
            assert int(rows[0][0]) == 2


class TestDistinctOnNoOrderBy:
    """Tests for DISTINCT ON without explicit ORDER BY."""
    
    def test_distinct_on_without_order_by(self, db_connection):
        """Test DISTINCT ON without ORDER BY - uses DISTINCT ON columns as implicit ORDER BY."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT ON (customer_id) customer_id, amount
                FROM orders
            """)
            rows = cur.fetchall()
            
            # Should still get one row per customer
            assert len(rows) == 3
            
            # Each customer should appear exactly once
            customer_ids = [int(r[0]) for r in rows]
            assert sorted(customer_ids) == [1, 2, 3]


class TestRegularDistinct:
    """Tests to ensure regular DISTINCT still works correctly."""
    
    def test_regular_distinct_single_column(self, db_connection):
        """Test regular DISTINCT on single column."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT customer_id FROM orders
            """)
            rows = cur.fetchall()
            
            # Should get 3 unique customer_ids
            assert len(rows) == 3
            customer_ids = sorted([int(r[0]) for r in rows])
            assert customer_ids == [1, 2, 3]
    
    def test_regular_distinct_multiple_columns(self, db_connection):
        """Test regular DISTINCT on multiple columns."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT DISTINCT department, role FROM employees
            """)
            rows = cur.fetchall()
            
            # Engineering/Senior, Engineering/Junior, Sales/Manager, 
            # Sales/Senior, Sales/Associate, Marketing/Manager, Marketing/Associate
            # Note: actual count depends on data
            assert len(rows) >= 6


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v", "-s"]))
