#!/usr/bin/env python3
"""
Performance benchmark for COPY command optimizations.
Compares COPY performance against individual INSERTs.
"""

import sys
import os
import io
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test

def test_copy_performance(proxy):
    """Benchmark COPY vs individual INSERTs."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create test table
    cur.execute("CREATE TABLE perf_test (id INT, name TEXT, email TEXT, score REAL)")
    conn.commit()
    
    # Generate test data
    num_rows = 10000
    csv_data = "\n".join([
        f"{i},user_{i},user_{i}@example.com,{i * 1.5}"
        for i in range(num_rows)
    ])
    
    # Benchmark COPY FROM
    print(f"\nBenchmarking COPY FROM with {num_rows} rows...")
    start = time.perf_counter()
    
    f = io.StringIO(csv_data)
    cur.copy_expert("COPY perf_test FROM STDIN WITH (FORMAT CSV)", f)
    conn.commit()
    
    copy_time = time.perf_counter() - start
    copy_rate = num_rows / copy_time
    
    print(f"  COPY FROM: {copy_time:.2f}s ({copy_rate:,.0f} rows/sec)")
    
    # Clear table
    cur.execute("DELETE FROM perf_test")
    conn.commit()
    
    # Benchmark individual INSERTs (sample - don't run full 10k)
    sample_size = min(1000, num_rows)
    print(f"\nBenchmarking individual INSERTs with {sample_size} rows...")
    start = time.perf_counter()
    
    for i in range(sample_size):
        cur.execute(
            "INSERT INTO perf_test VALUES (%s, %s, %s, %s)",
            (i, f"user_{i}", f"user_{i}@example.com", i * 1.5)
        )
    conn.commit()
    
    insert_time = time.perf_counter() - start
    insert_rate = sample_size / insert_time
    
    print(f"  Individual INSERTs: {insert_time:.2f}s ({insert_rate:,.0f} rows/sec)")
    
    # Calculate speedup
    speedup = copy_rate / insert_rate
    print(f"\n  COPY is {speedup:.1f}x faster than individual INSERTs")
    
    # Verify data integrity
    cur.execute("SELECT COUNT(*) FROM perf_test")
    count = cur.fetchone()[0]
    # Note: We inserted sample_size rows via INSERTs, not num_rows
    # Convert count to int in case it's a different type
    count = int(count)
    assert count == sample_size, f"Expected {sample_size} rows, got {count} (type: {type(count)})"
    
    # Verify some data
    cur.execute("SELECT * FROM perf_test WHERE id = 500")
    row = cur.fetchone()
    assert row == (500, 'user_500', 'user_500@example.com', 750.0)
    
    print("\n✓ Performance test passed")
    print(f"  Data integrity verified ({count} rows)")
    
    cur.close()
    conn.close()

def test_copy_large_dataset(proxy):
    """Test COPY with larger dataset to verify batching works correctly."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create test table with many columns to test batch size calculation
    cur.execute("""
        CREATE TABLE wide_table (
            c1 TEXT, c2 TEXT, c3 TEXT, c4 TEXT, c5 TEXT,
            c6 TEXT, c7 TEXT, c8 TEXT, c9 TEXT, c10 TEXT
        )
    """)
    conn.commit()
    
    # Generate data - 5000 rows with 10 columns = 50,000 values
    # Batch size should be 99 (999 / 10)
    num_rows = 5000
    rows = []
    for i in range(num_rows):
        values = [f"val_{i}_{j}" for j in range(10)]
        rows.append(",".join(values))
    csv_data = "\n".join(rows)
    
    print(f"\nTesting COPY with {num_rows} rows, 10 columns...")
    print(f"  Expected batch size: {999 // 10} rows")
    print(f"  Expected batches: {num_rows // (999 // 10) + 1}")
    
    start = time.perf_counter()
    
    f = io.StringIO(csv_data)
    cur.copy_expert("COPY wide_table FROM STDIN WITH (FORMAT CSV)", f)
    conn.commit()
    
    elapsed = time.perf_counter() - start
    rate = num_rows / elapsed
    
    print(f"  Completed in {elapsed:.2f}s ({rate:,.0f} rows/sec)")
    
    # Verify count
    cur.execute("SELECT COUNT(*) FROM wide_table")
    count = int(cur.fetchone()[0])
    assert count == num_rows, f"Expected {num_rows} rows, got {count}"
    
    # Verify sample data
    cur.execute("SELECT * FROM wide_table WHERE rowid = 2500")
    row = cur.fetchone()
    assert row[0] == "val_2499_0", f"Data mismatch: {row[0]}"
    
    print(f"\n✓ Large dataset test passed ({count} rows verified)")
    
    cur.close()
    conn.close()

if __name__ == "__main__":
    def run_all(proxy):
        test_copy_performance(proxy)
        test_copy_large_dataset(proxy)
        
    from e2e_helper import run_e2e_test
    run_e2e_test("copy_performance", run_all)
