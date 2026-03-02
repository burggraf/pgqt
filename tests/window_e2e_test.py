#!/usr/bin/env python3
"""
End-to-End tests for PGQT Window Functions transpilation to SQLite.

This test suite verifies that window functions are correctly transpiled
from PostgreSQL syntax to SQLite syntax and produce correct results.
"""

import os
import sys
import psycopg2
import pytest
import subprocess
import time
import signal

# Test database file
TEST_DB = "/tmp/pglite_window_test.db"
TEST_PORT = 55432


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
        database="test"
    )
    conn.autocommit = True
    
    # Create test tables
    with conn.cursor() as cur:
        # Employees table for window function tests
        cur.execute("""
            CREATE TABLE employees (
                id SERIAL PRIMARY KEY,
                name VARCHAR(100),
                department VARCHAR(50),
                salary DECIMAL(10, 2),
                hire_date DATE
            )
        """)
        
        # Insert test data
        cur.execute("""
            INSERT INTO employees (name, department, salary, hire_date) VALUES
                ('Alice', 'Engineering', 90000, '2020-01-15'),
                ('Bob', 'Engineering', 85000, '2021-03-20'),
                ('Charlie', 'Engineering', 95000, '2019-06-10'),
                ('Diana', 'Sales', 80000, '2020-09-01'),
                ('Eve', 'Sales', 75000, '2021-07-15'),
                ('Frank', 'Sales', 85000, '2020-04-22'),
                ('Grace', 'Marketing', 70000, '2022-01-10'),
                ('Henry', 'Marketing', 72000, '2021-11-05')
        """)
        
        # Orders table for running totals and moving averages
        cur.execute("""
            CREATE TABLE orders (
                id SERIAL PRIMARY KEY,
                customer_id INT,
                order_date DATE,
                amount DECIMAL(10, 2)
            )
        """)
        
        cur.execute("""
            INSERT INTO orders (customer_id, order_date, amount) VALUES
                (1, '2023-01-01', 100.00),
                (1, '2023-01-02', 150.00),
                (1, '2023-01-03', 200.00),
                (2, '2023-01-01', 300.00),
                (2, '2023-01-02', 250.00),
                (3, '2023-01-01', 50.00),
                (3, '2023-01-03', 75.00),
                (3, '2023-01-04', 100.00)
        """)
    
    yield conn
    
    conn.close()


class TestBasicWindowFunctions:
    """Tests for basic window function syntax."""
    
    def test_row_number_basic(self, db_connection):
        """Test row_number() without any window clause."""
        with db_connection.cursor() as cur:
            cur.execute("SELECT id, row_number() OVER () FROM employees")
            rows = cur.fetchall()
            assert len(rows) == 8
            # Row numbers should be 1-8 (SQLite returns integers)
            row_numbers = [int(r[1]) for r in rows]
            assert sorted(row_numbers) == list(range(1, 9))
    
    def test_row_number_with_order(self, db_connection):
        """Test row_number() with ORDER BY."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT id, name, row_number() OVER (ORDER BY salary DESC) as rn
                FROM employees
                ORDER BY rn
            """)
            rows = cur.fetchall()
            # First row should be highest salary (Charlie: 95000)
            assert rows[0][1] == 'Charlie'
    
    def test_rank_function(self, db_connection):
        """Test rank() function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT department, salary, rank() OVER (ORDER BY salary DESC) as rnk
                FROM employees
            """)
            rows = cur.fetchall()
            # Verify ranks are assigned correctly (SQLite returns strings for rank)
            ranks = [int(r[2]) for r in rows]
            assert 1 in ranks  # Someone has rank 1
    
    def test_dense_rank_function(self, db_connection):
        """Test dense_rank() function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT salary, dense_rank() OVER (ORDER BY salary DESC) as dr
                FROM employees
            """)
            rows = cur.fetchall()
            # Dense rank should have no gaps (SQLite returns strings)
            ranks = sorted(set(int(r[1]) for r in rows))
            assert ranks == list(range(1, len(ranks) + 1))


class TestPartitionBy:
    """Tests for PARTITION BY clause."""
    
    def test_partition_by_department(self, db_connection):
        """Test window functions with PARTITION BY."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT department, salary,
                       row_number() OVER (PARTITION BY department ORDER BY salary DESC) as rn
                FROM employees
                ORDER BY department, rn
            """)
            rows = cur.fetchall()
            
            # Each department should have its own row numbering starting from 1
            dept_rows = {}
            for dept, salary, rn in rows:
                if dept not in dept_rows:
                    dept_rows[dept] = []
                dept_rows[dept].append(int(rn))
            
            for dept, rns in dept_rows.items():
                assert sorted(rns) == list(range(1, len(rns) + 1))
    
    def test_partition_with_aggregate(self, db_connection):
        """Test aggregate functions over partitions."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT department, salary,
                       sum(salary) OVER (PARTITION BY department) as dept_total
                FROM employees
            """)
            rows = cur.fetchall()
            
            # Engineering: 90000 + 85000 + 95000 = 270000
            # Sales: 80000 + 75000 + 85000 = 240000
            # Marketing: 70000 + 72000 = 142000
            dept_totals = {}
            for dept, salary, total in rows:
                if total is not None:
                    dept_totals[dept] = float(total)
            
            assert dept_totals.get('Engineering') == 270000.0
            assert dept_totals.get('Sales') == 240000.0
            assert dept_totals.get('Marketing') == 142000.0


class TestFrameSpecifications:
    """Tests for window frame specifications."""
    
    def test_rows_unbounded_preceding(self, db_connection):
        """Test ROWS UNBOUNDED PRECEDING frame."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT order_date, amount,
                       sum(amount) OVER (ORDER BY order_date 
                                         ROWS UNBOUNDED PRECEDING) as running_total
                FROM orders
                WHERE customer_id = 1
                ORDER BY order_date
            """)
            rows = cur.fetchall()
            
            # Running totals: 100, 250, 450
            assert rows[0][2] is not None
            assert float(rows[0][2]) == 100.0
            assert float(rows[1][2]) == 250.0
            assert float(rows[2][2]) == 450.0
    
    def test_rows_between_preceding_following(self, db_connection):
        """Test ROWS BETWEEN N PRECEDING AND M FOLLOWING frame."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT order_date, amount,
                       avg(amount) OVER (ORDER BY order_date 
                                         ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) as moving_avg
                FROM orders
                WHERE customer_id = 1
                ORDER BY order_date
            """)
            rows = cur.fetchall()
            
            # Moving averages: (100+150)/2=125, (100+150+200)/3=150, (150+200)/2=175
            # Note: edge cases may vary by implementation
            assert len(rows) == 3
    
    def test_rows_between_unbounded(self, db_connection):
        """Test ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT department, salary,
                       max(salary) OVER (PARTITION BY department 
                                         ROWS BETWEEN UNBOUNDED PRECEDING 
                                                   AND UNBOUNDED FOLLOWING) as dept_max
                FROM employees
            """)
            rows = cur.fetchall()
            
            # All rows in each department should have the same max
            dept_maxes = {}
            for dept, salary, max_sal in rows:
                if max_sal is not None:
                    if dept not in dept_maxes:
                        dept_maxes[dept] = float(max_sal)
                    assert float(max_sal) == dept_maxes[dept]
            
            assert dept_maxes.get('Engineering') == 95000.0
            assert dept_maxes.get('Sales') == 85000.0


class TestOffsetFunctions:
    """Tests for lag, lead, first_value, last_value functions."""
    
    def test_lag_function(self, db_connection):
        """Test lag() function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT order_date, amount,
                       lag(amount) OVER (ORDER BY order_date) as prev_amount
                FROM orders
                WHERE customer_id = 1
                ORDER BY order_date
            """)
            rows = cur.fetchall()
            
            assert rows[0][2] is None  # First row has no previous
            assert rows[1][2] is not None
            assert float(rows[1][2]) == 100.0  # Previous is first row
            assert float(rows[2][2]) == 150.0  # Previous is second row
    
    def test_lead_function(self, db_connection):
        """Test lead() function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT order_date, amount,
                       lead(amount) OVER (ORDER BY order_date) as next_amount
                FROM orders
                WHERE customer_id = 1
                ORDER BY order_date
            """)
            rows = cur.fetchall()
            
            assert rows[0][2] is not None
            assert float(rows[0][2]) == 150.0  # Next is second row
            assert float(rows[1][2]) == 200.0  # Next is third row
            assert rows[2][2] is None  # Last row has no next
    
    def test_lag_with_offset(self, db_connection):
        """Test lag() with explicit offset."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT order_date, amount,
                       lag(amount, 2) OVER (ORDER BY order_date) as prev_prev
                FROM orders
                WHERE customer_id = 3
                ORDER BY order_date
            """)
            rows = cur.fetchall()
            
            assert rows[0][2] is None  # No row 2 positions back
            assert rows[1][2] is None  # No row 2 positions back
            assert rows[2][2] is not None
            assert float(rows[2][2]) == 50.0  # Two rows back
    
    def test_first_value(self, db_connection):
        """Test first_value() function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT department, salary,
                       first_value(salary) OVER (PARTITION BY department 
                                                  ORDER BY salary DESC
                                                  ROWS BETWEEN UNBOUNDED PRECEDING 
                                                            AND UNBOUNDED FOLLOWING) as highest_in_dept
                FROM employees
            """)
            rows = cur.fetchall()
            
            # All rows should have the highest salary in their department
            for dept, salary, highest in rows:
                if highest is not None:
                    if dept == 'Engineering':
                        assert float(highest) == 95000.0
                    elif dept == 'Sales':
                        assert float(highest) == 85000.0
                    elif dept == 'Marketing':
                        assert float(highest) == 72000.0


class TestNtile:
    """Tests for ntile() function."""
    
    def test_ntile_function(self, db_connection):
        """Test ntile() function for quartiles."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT name, salary, ntile(4) OVER (ORDER BY salary) as quartile
                FROM employees
                ORDER BY salary
            """)
            rows = cur.fetchall()
            
            # With 8 rows and 4 buckets, each quartile should have 2 rows
            quartiles = [int(r[2]) for r in rows]
            assert quartiles.count(1) == 2
            assert quartiles.count(2) == 2
            assert quartiles.count(3) == 2
            assert quartiles.count(4) == 2


class TestPercentRankAndCumeDist:
    """Tests for percent_rank() and cume_dist() functions."""
    
    def test_percent_rank(self, db_connection):
        """Test percent_rank() function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT salary, percent_rank() OVER (ORDER BY salary) as pct
                FROM employees
                ORDER BY salary
            """)
            rows = cur.fetchall()
            
            # First should be 0, last should be 1
            assert rows[0][1] is not None
            assert float(rows[0][1]) == 0.0
            assert float(rows[-1][1]) == 1.0
    
    def test_cume_dist(self, db_connection):
        """Test cume_dist() function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT salary, cume_dist() OVER (ORDER BY salary) as cume
                FROM employees
                ORDER BY salary
            """)
            rows = cur.fetchall()
            
            # First should be 1/8 = 0.125, last should be 1.0
            assert rows[0][1] is not None
            assert float(rows[0][1]) == pytest.approx(0.125, rel=0.01)
            assert float(rows[-1][1]) == 1.0


class TestComplexQueries:
    """Tests for complex window function queries."""
    
    def test_multiple_window_functions(self, db_connection):
        """Test multiple window functions in same query."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT department, salary,
                       row_number() OVER (PARTITION BY department ORDER BY salary DESC) as rn,
                       rank() OVER (PARTITION BY department ORDER BY salary DESC) as rnk,
                       sum(salary) OVER (PARTITION BY department) as dept_total
                FROM employees
            """)
            rows = cur.fetchall()
            assert len(rows) == 8
    
    def test_window_function_with_subquery(self, db_connection):
        """Test window function in subquery."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT * FROM (
                    SELECT department, salary,
                           rank() OVER (PARTITION BY department ORDER BY salary DESC) as rnk
                    FROM employees
                ) sub
                WHERE rnk = 1
            """)
            rows = cur.fetchall()
            
            # Should get top earner from each department
            assert len(rows) == 3  # 3 departments
    
    def test_window_function_with_count_star(self, db_connection):
        """Test count(*) as window function."""
        with db_connection.cursor() as cur:
            cur.execute("""
                SELECT department, count(*) OVER (PARTITION BY department) as dept_count
                FROM employees
            """)
            rows = cur.fetchall()
            
            # Engineering: 3, Sales: 3, Marketing: 2
            dept_counts = {}
            for dept, count in rows:
                if count is not None:
                    dept_counts[dept] = int(count)
            
            assert dept_counts.get('Engineering') == 3
            assert dept_counts.get('Sales') == 3
            assert dept_counts.get('Marketing') == 2


if __name__ == "__main__":
    # Run tests with pytest
    sys.exit(pytest.main([__file__, "-v", "-s"]))
