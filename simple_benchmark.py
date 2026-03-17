#!/usr/bin/env python3
"""
Simple benchmark comparing PGQT and pgsqlite performance.
Works with any PostgreSQL-compatible server since it uses psycopg2.
"""

import time
import sys
import argparse
import psycopg2
from contextlib import contextmanager
import statistics

# Benchmark configuration
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 5432
DEFAULT_USER = "postgres"
DEFAULT_PASSWORD = "postgres"
DEFAULT_DATABASE = "postgres"

# Number of iterations for each test
WARMUP_ITERATIONS = 100
BENCHMARK_ITERATIONS = 1000


@contextmanager
def get_connection(host, port, user, password, database):
    """Get a database connection."""
    conn = psycopg2.connect(
        host=host,
        port=port,
        user=user,
        password=password,
        database=database
    )
    conn.autocommit = True
    try:
        yield conn
    finally:
        conn.close()


def setup_tables(cur):
    """Create benchmark tables."""
    # Clean up any existing tables
    cur.execute("DROP TABLE IF EXISTS benchmark_orders")
    cur.execute("DROP TABLE IF EXISTS benchmark_users")
    
    # Create users table - use simpler defaults for maximum compatibility
    cur.execute("""
        CREATE TABLE benchmark_users (
            id SERIAL PRIMARY KEY,
            username VARCHAR(100) NOT NULL,
            email VARCHAR(255) UNIQUE NOT NULL,
            created_at TEXT,
            active BOOLEAN DEFAULT true,
            score REAL DEFAULT 0.0
        )
    """)
    
    # Create orders table with foreign key
    cur.execute("""
        CREATE TABLE benchmark_orders (
            id SERIAL PRIMARY KEY,
            user_id INTEGER REFERENCES benchmark_users(id),
            amount REAL NOT NULL,
            status VARCHAR(50) DEFAULT 'pending',
            created_at TEXT
        )
    """)
    
    # Create indexes
    cur.execute("CREATE INDEX idx_users_active ON benchmark_users(active)")
    cur.execute("CREATE INDEX idx_orders_user_id ON benchmark_orders(user_id)")
    cur.execute("CREATE INDEX idx_orders_status ON benchmark_orders(status)")


def teardown_tables(cur):
    """Drop benchmark tables."""
    cur.execute("DROP TABLE IF EXISTS benchmark_orders")
    cur.execute("DROP TABLE IF EXISTS benchmark_users")


def benchmark_insert_simple(cur, iterations, offset=0):
    """Benchmark simple INSERT statements."""
    times = []
    for i in range(iterations):
        idx = offset + i
        start = time.perf_counter()
        cur.execute(
            "INSERT INTO benchmark_users (username, email, score) VALUES (%s, %s, %s)",
            (f"user_{idx}", f"user_{idx}@example.com", float(idx))
        )
        end = time.perf_counter()
        times.append((end - start) * 1000)  # Convert to ms
        # Small delay to avoid rate limiting on some servers
        if i % 100 == 0:
            time.sleep(0.001)
    return times


def benchmark_insert_batch(cur, iterations, batch_size=100, offset=0):
    """Benchmark batch INSERT using executemany."""
    times = []
    data = [
        (f"batch_user_{offset + i}", f"batch_{offset + i}@example.com", float(offset + i))
        for i in range(iterations * batch_size)
    ]
    
    for i in range(0, len(data), batch_size):
        batch = data[i:i + batch_size]
        start = time.perf_counter()
        cur.executemany(
            "INSERT INTO benchmark_users (username, email, score) VALUES (%s, %s, %s)",
            batch
        )
        end = time.perf_counter()
        times.append((end - start) * 1000)  # Convert to ms
    
    return times


def benchmark_select_simple(cur, iterations, offset=0):
    """Benchmark simple SELECT by primary key."""
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        cur.execute("SELECT * FROM benchmark_users WHERE id = %s", (i + 1,))
        cur.fetchall()
        end = time.perf_counter()
        times.append((end - start) * 1000)
    return times


def benchmark_select_range(cur, iterations, offset=0):
    """Benchmark SELECT with range scan."""
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        cur.execute(
            "SELECT * FROM benchmark_users WHERE score BETWEEN %s AND %s",
            (float(i), float(i + 100))
        )
        cur.fetchall()
        end = time.perf_counter()
        times.append((end - start) * 1000)
    return times


def benchmark_select_join(cur, iterations, offset=0):
    """Benchmark SELECT with JOIN."""
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        cur.execute("""
            SELECT u.*, o.id as order_id, o.amount
            FROM benchmark_users u
            LEFT JOIN benchmark_orders o ON u.id = o.user_id
            WHERE u.id = %s
        """, (i + 1,))
        cur.fetchall()
        end = time.perf_counter()
        times.append((end - start) * 1000)
    return times


def benchmark_update(cur, iterations, offset=0):
    """Benchmark UPDATE statements."""
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        cur.execute(
            "UPDATE benchmark_users SET score = score + 1 WHERE id = %s",
            (i + 1,)
        )
        end = time.perf_counter()
        times.append((end - start) * 1000)
    return times


def benchmark_delete_insert(cur, iterations, offset=0):
    """Benchmark DELETE + INSERT pattern."""
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        cur.execute("DELETE FROM benchmark_orders WHERE user_id = %s", (i + 1,))
        cur.execute(
            "INSERT INTO benchmark_orders (user_id, amount, status) VALUES (%s, %s, %s)",
            (i + 1, float(i * 10), 'completed')
        )
        end = time.perf_counter()
        times.append((end - start) * 1000)
    return times


def benchmark_transaction(cur, iterations, offset=0):
    """Benchmark transactions with multiple operations."""
    times = []
    for i in range(iterations):
        idx = offset + i
        start = time.perf_counter()
        cur.execute("BEGIN")
        cur.execute(
            "INSERT INTO benchmark_users (username, email, score) VALUES (%s, %s, %s)",
            (f"tx_user_{idx}", f"tx_{idx}@example.com", float(idx))
        )
        cur.execute(
            "UPDATE benchmark_users SET active = false WHERE id = %s",
            (idx + 1,)
        )
        cur.execute("COMMIT")
        end = time.perf_counter()
        times.append((end - start) * 1000)
    return times


def run_benchmark(name, benchmark_func, cur, iterations, warmup=True):
    """Run a benchmark with warmup."""
    print(f"\n  Running {name}...")
    
    # Warmup - use a very large offset to avoid conflicts
    if warmup:
        print(f"    Warming up ({WARMUP_ITERATIONS} iterations)...")
        benchmark_func(cur, WARMUP_ITERATIONS, 900000)
    
    # Actual benchmark - use a different offset
    print(f"    Benchmarking ({iterations} iterations)...")
    times = benchmark_func(cur, iterations, 1000000)
    
    # Calculate statistics
    avg_time = statistics.mean(times)
    median_time = statistics.median(times)
    min_time = min(times)
    max_time = max(times)
    p95 = sorted(times)[int(len(times) * 0.95)]
    p99 = sorted(times)[int(len(times) * 0.99)]
    
    ops_per_sec = 1000.0 / avg_time if avg_time > 0 else float('inf')
    
    return {
        'name': name,
        'avg_ms': avg_time,
        'median_ms': median_time,
        'min_ms': min_time,
        'max_ms': max_time,
        'p95_ms': p95,
        'p99_ms': p99,
        'ops_per_sec': ops_per_sec,
        'total_time_ms': sum(times)
    }


def print_results(results, server_name):
    """Print benchmark results in a formatted table."""
    print(f"\n{'='*80}")
    print(f"Results for {server_name}")
    print(f"{'='*80}")
    print(f"{'Benchmark':<25} {'Avg (ms)':<12} {'Median (ms)':<12} {'P95 (ms)':<12} {'Ops/sec':<12}")
    print(f"{'-'*80}")
    
    for r in results:
        print(f"{r['name']:<25} {r['avg_ms']:>10.3f}  {r['median_ms']:>10.3f}  "
              f"{r['p95_ms']:>10.3f}  {r['ops_per_sec']:>10.1f}")
    
    print(f"{'='*80}")


def main():
    parser = argparse.ArgumentParser(description='Simple benchmark for PostgreSQL-compatible servers')
    parser.add_argument('--host', default=DEFAULT_HOST, help='Server host')
    parser.add_argument('--port', type=int, default=DEFAULT_PORT, help='Server port')
    parser.add_argument('--user', default=DEFAULT_USER, help='Username')
    parser.add_argument('--password', default=DEFAULT_PASSWORD, help='Password')
    parser.add_argument('--database', default=DEFAULT_DATABASE, help='Database name')
    parser.add_argument('--iterations', type=int, default=BENCHMARK_ITERATIONS, 
                        help=f'Number of iterations (default: {BENCHMARK_ITERATIONS})')
    parser.add_argument('--no-warmup', action='store_true', help='Skip warmup phase')
    parser.add_argument('--name', default='Server', help='Server name for results')
    parser.add_argument('--tests', default='all', 
                        help='Comma-separated list of tests to run: insert_simple,insert_batch,select_simple,select_range,select_join,update,delete_insert,transaction')
    
    args = parser.parse_args()
    
    print(f"Connecting to {args.host}:{args.port}...")
    
    try:
        with get_connection(args.host, args.port, args.user, args.password, args.database) as conn:
            cur = conn.cursor()
            
            # Setup
            print("Setting up benchmark tables...")
            setup_tables(cur)
            
            # Pre-populate with some data for SELECT/UPDATE/DELETE tests
            print("Pre-populating data...")
            for i in range(args.iterations):
                cur.execute(
                    "INSERT INTO benchmark_users (username, email, score) VALUES (%s, %s, %s)",
                    (f"pre_user_{i}", f"pre_{i}@example.com", float(i))
                )
            
            # Define available benchmarks
            # Functions accept (cur, iterations, offset) - offset is used by run_benchmark
            benchmarks = {
                'insert_simple': ('Simple INSERT', benchmark_insert_simple),
                'insert_batch': ('Batch INSERT (100)', lambda c, i, o: benchmark_insert_batch(c, i, batch_size=100, offset=o)),
                'select_simple': ('SELECT by PK', benchmark_select_simple),
                'select_range': ('SELECT range scan', benchmark_select_range),
                'select_join': ('SELECT with JOIN', benchmark_select_join),
                'update': ('UPDATE', benchmark_update),
                'delete_insert': ('DELETE + INSERT', benchmark_delete_insert),
                'transaction': ('Transaction (multi-op)', benchmark_transaction),
            }
            
            # Determine which tests to run
            if args.tests == 'all':
                tests_to_run = list(benchmarks.keys())
            else:
                tests_to_run = args.tests.split(',')
            
            # Run benchmarks
            results = []
            for test_name in tests_to_run:
                if test_name not in benchmarks:
                    print(f"Warning: Unknown test '{test_name}', skipping")
                    continue
                
                display_name, func = benchmarks[test_name]
                result = run_benchmark(
                    display_name, 
                    func, 
                    cur, 
                    args.iterations,
                    warmup=not args.no_warmup
                )
                results.append(result)
            
            # Cleanup
            print("\nCleaning up...")
            teardown_tables(cur)
            
            # Print results
            print_results(results, args.name)
            
    except psycopg2.Error as e:
        print(f"Database error: {e}")
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)


if __name__ == '__main__':
    main()
