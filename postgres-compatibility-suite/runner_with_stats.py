#!/usr/bin/env python3
"""
PGQT Compatibility Test Runner with Statement-Level Statistics

This runner executes SQL statements against both PostgreSQL (reference) and PGQT (test),
comparing results and reporting detailed pass/fail statistics at the STATEMENT level
(rather than just file level).

Usage:
    python runner_with_stats.py [--verbose] [--fail-fast]

Output:
    - Per-file statement pass rates
    - Overall statement pass rate
    - Categorized failures by error type
"""

import psycopg2
import os
import re
import sys
import subprocess
import time
import json
from dataclasses import dataclass, field
from typing import Optional
from datetime import datetime

SQL_DIR = os.path.join(os.path.dirname(__file__), "sql")
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DB_PATH = os.path.join(PROJECT_ROOT, "postgres-compatibility-suite", "test_db.db")
PGQT_BINARY = os.path.join(PROJECT_ROOT, "target", "release", "pgqt")
PG_DSN = os.environ.get("PG_DSN", "host=localhost port=5432 user=postgres password=postgres dbname=postgres")
PROXY_PORT = 5435

@dataclass
class StatementResult:
    """Result of a single statement execution."""
    statement: str
    passed: bool
    error_type: Optional[str] = None
    error_message: Optional[str] = None
    pg_error: Optional[str] = None

@dataclass
class FileResult:
    """Results for a single SQL file."""
    filename: str
    total_statements: int = 0
    passed_statements: int = 0
    failed_statements: int = 0
    skipped_statements: int = 0
    failures: list = field(default_factory=list)
    
    @property
    def pass_rate(self) -> float:
        if self.total_statements == 0:
            return 0.0
        return (self.passed_statements / self.total_statements) * 100

def get_sql_files():
    """Recursively list all SQL files in the sql directory."""
    sql_files = []
    for root, _, files in os.walk(SQL_DIR):
        for f in files:
            if f.endswith(".sql") or f.endswith(".sqltest"):
                sql_files.append(os.path.join(root, f))
    return sorted(sql_files)

def parse_sql(content, is_sqltest=False):
    """Parse SQL content into individual statements."""
    # Skip placeholder/error files
    if content.strip().startswith("404:") or len(content.strip()) < 10:
        return []
    
    if is_sqltest:
        statements = []
        for part in content.split('---'):
            m = re.search(r'sql:\s*(.*)', part, re.DOTALL)
            if m:
                stmt = m.group(1).strip()
                if stmt:
                    statements.append(stmt)
        return statements
    
    # Regular SQL file parsing
    statements = []
    content = re.sub(r'--.*', '', content)
    content = re.sub(r'/\*.*?\*/', '', content, flags=re.DOTALL)
    
    for s in content.split(';'):
        s = s.strip()
        if s:
            statements.append(s)
    return statements

def should_skip_statement(stmt: str) -> tuple[bool, str]:
    """Check if a statement should be skipped and return reason."""
    upper = stmt.upper()
    skip_keywords = {
        "COPY": "COPY command not supported",
        "CREATE EXTENSION": "CREATE EXTENSION not supported",
        "CLUSTER": "CLUSTER command not supported",
        "ANALYZE": "ANALYZE command not supported",
        "VACUUM": "VACUUM command not supported",
    }
    for keyword, reason in skip_keywords.items():
        if keyword in upper:
            return True, reason
    return False, ""

def categorize_error(err_msg: str, err_code: Optional[str] = None) -> str:
    """Categorize an error by type for reporting."""
    if err_code:
        error_categories = {
            "42601": "Syntax Error",
            "42P01": "Undefined Table",
            "42703": "Undefined Column",
            "42883": "Undefined Function",
            "22001": "String Data Right Truncation",
            "22003": "Numeric Value Out of Range",
            "22008": "DateTime Out of Range",
            "22P02": "Invalid Text Representation",
        }
        if err_code in error_categories:
            return error_categories[err_code]
    
    # Pattern-based categorization
    lower_msg = err_msg.lower()
    if "no such table" in lower_msg or "undefined table" in lower_msg:
        return "Missing Table/View"
    elif "no such column" in lower_msg or "undefined column" in lower_msg:
        return "Missing Column"
    elif "no such function" in lower_msg or "undefined function" in lower_msg:
        return "Missing Function"
    elif "syntax error" in lower_msg:
        return "Syntax Error"
    elif "type" in lower_msg and ("does not exist" in lower_msg or "invalid" in lower_msg):
        return "Type Error"
    elif "column mismatch" in lower_msg:
        return "Column Mismatch"
    elif "row count mismatch" in lower_msg:
        return "Row Count Mismatch"
    elif "should have failed" in lower_msg:
        return "Error Handling Gap"
    else:
        return "Other"

def execute_and_compare(conn_ref, conn_test, sql_stmt: str, verbose: bool = False) -> StatementResult:
    """Executes a single statement on both connections and compares results."""
    res_ref = None
    err_ref = None
    err_ref_code = None
    res_test = None
    err_test = None
    err_test_code = None
    
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
    except psycopg2.Error as e:
        err_ref_code = e.pgcode
        err_ref = f"{e.pgcode}: {e.pgerror}" if e.pgcode else str(e)
        try:
            conn_ref.rollback()
        except:
            pass
    except Exception as e:
        err_ref = str(e)
        try:
            conn_ref.rollback()
        except:
            pass
        
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
    except psycopg2.Error as e:
        err_test_code = e.pgcode
        err_test = f"{e.pgcode}: {e.pgerror}" if e.pgcode else str(e)
        try:
            conn_test.rollback()
        except:
            pass
    except Exception as e:
        err_test = str(e)
        try:
            conn_test.rollback()
        except:
            pass
    
    # Comparison logic
    if err_ref:
        # If ref failed, test should also fail
        if not err_test:
            error_msg = f"Statement should have failed with: {err_ref}"
            return StatementResult(
                statement=sql_stmt,
                passed=False,
                error_type="Error Handling Gap",
                error_message=error_msg,
                pg_error=err_ref
            )
        # Both failed - this is acceptable behavior match
        return StatementResult(statement=sql_stmt, passed=True)
    else:
        if err_test:
            error_type = categorize_error(err_test, err_test_code)
            return StatementResult(
                statement=sql_stmt,
                passed=False,
                error_type=error_type,
                error_message=err_test,
                pg_error=None
            )
        
        # Compare results if any
        if res_ref:
            if not res_test:
                return StatementResult(
                    statement=sql_stmt,
                    passed=False,
                    error_type="Missing Results",
                    error_message="Reference returned results, but test did not."
                )
            
            # Compare columns
            if res_ref["cols"] != res_test["cols"]:
                return StatementResult(
                    statement=sql_stmt,
                    passed=False,
                    error_type="Column Mismatch",
                    error_message=f"Expected columns: {res_ref['cols']}, got: {res_test['cols']}"
                )
            
            # Compare row counts
            if len(res_ref["rows"]) != len(res_test["rows"]):
                return StatementResult(
                    statement=sql_stmt,
                    passed=False,
                    error_type="Row Count Mismatch",
                    error_message=f"Expected {len(res_ref['rows'])} rows, got {len(res_test['rows'])}"
                )
        
        return StatementResult(statement=sql_stmt, passed=True)

def start_proxy():
    """Start the PGQT proxy."""
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    # Ensure binary is built
    print("Building PGQT...")
    result = subprocess.run(["cargo", "build", "--release"], 
                          capture_output=True, text=True, cwd=PROJECT_ROOT)
    if result.returncode != 0:
        print(f"Build failed:\n{result.stderr}")
        sys.exit(1)
    
    cmd = [PGQT_BINARY, "--port", str(PROXY_PORT), "--database", DB_PATH]
    err_log_path = os.path.join(PROJECT_ROOT, "postgres-compatibility-suite", "test_db.db.error.log")
    
    with open(err_log_path, "w") as err_log:
        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=err_log)
    
    # Wait for proxy to start
    time.sleep(2)
    return proc

def stop_proxy(proc):
    """Stop the PGQT proxy."""
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

def drop_all_tables(conn, catalog_table):
    """Drop all tables from a database connection."""
    try:
        cur = conn.cursor()
        
        if catalog_table == "pg_catalog":
            cur.execute("""
                SELECT tablename FROM pg_tables
                WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
            """)
            tables = [row[0] for row in cur.fetchall()]
            
            cur.execute("""
                SELECT nspname FROM pg_namespace
                WHERE nspname NOT IN ('pg_catalog', 'information_schema', 'public')
                AND nspname NOT LIKE 'pg_toast%'
                AND nspname NOT LIKE 'pg_temp_%'
            """)
            schemas = [row[0] for row in cur.fetchall()]
            
            for schema in schemas:
                try:
                    cur.execute(f'DROP SCHEMA IF EXISTS "{schema}" CASCADE')
                except Exception:
                    pass
            
            for table in tables:
                try:
                    cur.execute(f'DROP TABLE IF EXISTS "{table}" CASCADE')
                except Exception:
                    pass
        else:
            cur.execute("""
                SELECT name, type FROM sqlite_master
                WHERE type IN ('table', 'view')
                AND name NOT LIKE 'sqlite_%'
                AND name NOT LIKE '__pg_%'
            """)
            items = cur.fetchall()
            
            for name, type in items:
                try:
                    if type == 'table':
                        cur.execute(f'DROP TABLE IF EXISTS "{name}"')
                    else:
                        cur.execute(f'DROP VIEW IF EXISTS "{name}"')
                except Exception:
                    pass
        
        cur.close()
    except Exception as e:
        print(f"Warning: Cleanup failed: {e}")

def run_tests(verbose: bool = False, fail_fast: bool = False) -> list[FileResult]:
    """Run all compatibility tests and return results."""
    
    # Check if Reference Postgres is ready
    print("Checking reference PostgreSQL...")
    try:
        pg_conn = psycopg2.connect(PG_DSN)
        pg_conn.autocommit = True
        pg_conn.close()
    except Exception as e:
        print(f"Error: Reference PostgreSQL not available: {e}")
        print("Start Postgres to use the ground-truth comparison.")
        print("If using Docker: docker run --name pg-test -e POSTGRES_PASSWORD=postgres -p 5432:5432 -d postgres")
        sys.exit(1)
    
    # Start proxy
    print("Starting PGQT proxy...")
    proxy_proc = start_proxy()
    
    try:
        # Connect to both databases
        pg_conn = psycopg2.connect(PG_DSN)
        pg_conn.autocommit = True
        
        proxy_dsn = f"host=127.0.0.1 port={PROXY_PORT} user=postgres password=postgres dbname=postgres"
        proxy_conn = psycopg2.connect(proxy_dsn)
        proxy_conn.autocommit = True
        
        sql_files = get_sql_files()
        file_results = []
        
        print(f"\nRunning compatibility tests on {len(sql_files)} files...\n")
        
        for sql_file in sql_files:
            filename = os.path.basename(sql_file)
            rel_path = os.path.relpath(sql_file, SQL_DIR)
            
            # Clean up before each file
            drop_all_tables(pg_conn, "pg_catalog")
            drop_all_tables(proxy_conn, "sqlite_master")
            
            with open(sql_file, 'r') as f:
                content = f.read()
            
            is_sqltest = sql_file.endswith(".sqltest")
            statements = parse_sql(content, is_sqltest)
            
            file_result = FileResult(filename=rel_path)
            
            for stmt in statements:
                # Check if should skip
                should_skip, skip_reason = should_skip_statement(stmt)
                if should_skip:
                    file_result.skipped_statements += 1
                    if verbose:
                        print(f"  [SKIP] {skip_reason}")
                    continue
                
                file_result.total_statements += 1
                
                result = execute_and_compare(pg_conn, proxy_conn, stmt, verbose)
                
                if result.passed:
                    file_result.passed_statements += 1
                    if verbose:
                        print(f"  [PASS] {stmt[:80]}...")
                else:
                    file_result.failed_statements += 1
                    file_result.failures.append(result)
                    if verbose:
                        print(f"  [FAIL] {result.error_type}: {result.error_message[:80]}...")
                    
                    if fail_fast:
                        print(f"\n[FAIL FAST] Stopping on first failure.")
                        break
            
            file_results.append(file_result)
            
            # Skip empty files in progress output
            if file_result.total_statements == 0:
                print(f"  {rel_path}: [empty/placeholder file, skipped]")
            else:
                status = "PASS" if file_result.pass_rate == 100 else f"{file_result.pass_rate:.1f}%"
                print(f"  {rel_path}: {file_result.passed_statements}/{file_result.total_statements} passed ({status})")
            
            if fail_fast and file_result.failed_statements > 0:
                break
        
        pg_conn.close()
        proxy_conn.close()
        
        return file_results
        
    finally:
        stop_proxy(proxy_proc)

def print_summary(file_results: list[FileResult]):
    """Print a detailed summary of test results."""
    
    # Filter out empty files from statistics
    non_empty_files = [r for r in file_results if r.total_statements > 0]
    empty_file_count = len(file_results) - len(non_empty_files)
    
    # Calculate overall stats
    total_files = len(non_empty_files)
    total_statements = sum(r.total_statements for r in non_empty_files)
    total_passed = sum(r.passed_statements for r in non_empty_files)
    total_failed = sum(r.failed_statements for r in non_empty_files)
    total_skipped = sum(r.skipped_statements for r in non_empty_files)
    
    overall_pass_rate = (total_passed / total_statements * 100) if total_statements > 0 else 0
    
    # Count files by pass rate
    perfect_files = sum(1 for r in non_empty_files if r.pass_rate == 100)
    good_files = sum(1 for r in non_empty_files if 50 <= r.pass_rate < 100)
    poor_files = sum(1 for r in non_empty_files if 0 < r.pass_rate < 50)
    failed_files = sum(1 for r in non_empty_files if r.pass_rate == 0 and r.total_statements > 0)
    
    # Collect error categories (only from non-empty files)
    error_counts = {}
    for file_result in non_empty_files:
        for failure in file_result.failures:
            error_type = failure.error_type or "Unknown"
            error_counts[error_type] = error_counts.get(error_type, 0) + 1
    
    # Print summary header
    print("\n" + "=" * 80)
    print("PGQT COMPATIBILITY TEST SUMMARY")
    print("=" * 80)
    print(f"\nTest Run: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    
    # Overall statistics
    print("\n" + "-" * 40)
    print("OVERALL STATISTICS")
    print("-" * 40)
    print(f"  Total SQL Files:        {total_files}")
    print(f"  Total Statements:       {total_statements}")
    print(f"  Passed Statements:      {total_passed}")
    print(f"  Failed Statements:      {total_failed}")
    print(f"  Skipped Statements:     {total_skipped}")
    print(f"\n  OVERALL PASS RATE:      {overall_pass_rate:.2f}%")
    
    # File-level summary
    print("\n" + "-" * 40)
    print("FILE-LEVEL SUMMARY")
    print("-" * 40)
    print(f"  Perfect (100%):         {perfect_files} files")
    print(f"  Good (50-99%):          {good_files} files")
    print(f"  Poor (1-49%):           {poor_files} files")
    print(f"  Failed (0%):            {failed_files} files")
    
    # Error breakdown
    if error_counts:
        print("\n" + "-" * 40)
        print("FAILURE BREAKDOWN BY CATEGORY")
        print("-" * 40)
        for error_type, count in sorted(error_counts.items(), key=lambda x: -x[1]):
            percentage = (count / total_failed * 100) if total_failed > 0 else 0
            print(f"  {error_type:.<30} {count:>3} ({percentage:>5.1f}%)")
    
    # Detailed file results
    print("\n" + "-" * 40)
    print("DETAILED FILE RESULTS")
    print("-" * 40)
    
    # Sort by pass rate (worst first) - only non-empty files
    sorted_results = sorted(non_empty_files, key=lambda r: (r.pass_rate, -r.total_statements))
    
    for result in sorted_results:
        status_symbol = "✓" if result.pass_rate == 100 else "✗" if result.pass_rate == 0 else "~"
        print(f"\n  [{status_symbol}] {result.filename}")
        print(f"      Statements: {result.total_statements}, Passed: {result.passed_statements}, "
              f"Failed: {result.failed_statements}, Skipped: {result.skipped_statements}")
        print(f"      Pass Rate: {result.pass_rate:.1f}%")
        
        # Show first few failures for this file
        if result.failures and len(result.failures) <= 3:
            for failure in result.failures[:3]:
                print(f"      - {failure.error_type}: {failure.error_message[:60]}...")
        elif result.failures:
            print(f"      - ({len(result.failures)} failures, use --verbose for details)")
    
    # Show sample failures by category (only from non-empty files)
    if any(r.failures for r in non_empty_files):
        print("\n" + "-" * 40)
        print("SAMPLE FAILURES BY CATEGORY")
        print("-" * 40)
        
        shown_categories = set()
        for file_result in non_empty_files:
            for failure in file_result.failures:
                if failure.error_type not in shown_categories and len(shown_categories) < 5:
                    shown_categories.add(failure.error_type)
                    print(f"\n  [{failure.error_type}]")
                    print(f"    File: {file_result.filename}")
                    print(f"    Statement: {failure.statement[:100]}...")
                    print(f"    Error: {failure.error_message[:100]}...")
    
    print("\n" + "=" * 80)
    print(f"FINAL RESULT: {overall_pass_rate:.2f}% statement compatibility")
    print("=" * 80 + "\n")
    
    return overall_pass_rate

def main():
    """Main entry point."""
    verbose = "--verbose" in sys.argv or "-v" in sys.argv
    fail_fast = "--fail-fast" in sys.argv
    
    if "--help" in sys.argv or "-h" in sys.argv:
        print(__doc__)
        print("\nOptions:")
        print("  --verbose, -v     Show detailed output for each statement")
        print("  --fail-fast       Stop on first failure")
        print("  --help, -h        Show this help message")
        sys.exit(0)
    
    file_results = run_tests(verbose=verbose, fail_fast=fail_fast)
    overall_pass_rate = print_summary(file_results)
    
    # Save results to JSON for further analysis
    results_json = {
        "timestamp": datetime.now().isoformat(),
        "overall_pass_rate": overall_pass_rate,
        "files": [
            {
                "filename": r.filename,
                "total": r.total_statements,
                "passed": r.passed_statements,
                "failed": r.failed_statements,
                "skipped": r.skipped_statements,
                "pass_rate": r.pass_rate,
            }
            for r in file_results
        ]
    }
    
    results_path = os.path.join(os.path.dirname(__file__), "test_results.json")
    with open(results_path, 'w') as f:
        json.dump(results_json, f, indent=2)
    print(f"Detailed results saved to: {results_path}")
    
    # Exit with appropriate code
    sys.exit(0 if overall_pass_rate >= 90 else 1)

if __name__ == "__main__":
    main()
