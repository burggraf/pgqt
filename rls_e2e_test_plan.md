# RLS E2E Test Plan

## Overview
This document outlines the end-to-end (E2E testing strategy for Row-Level Security (RLS) implementation in postgresqlite. Tests will use `psycopg2` to connect to the proxy and verify PostgreSQL-compatible RLS behavior.

---

## 1. Test Infrastructure

### 1.1 Test Harness
- **Language**: Python 3
- **Driver**: `psycopg2`
- **Connection**: Connect to postgresqlite proxy on `127.0.0.1:5432`
- **Database**: Fresh SQLite database for each test suite

### 1.2 Test Script Structure
```
tests/e2e/
├── conftest.py           # Pytest fixtures for DB setup/teardown
├── test_rls_basic.py     # Basic RLS enable/disable tests
├── test_rls_policies.py  # Policy CRUD and evaluation
├── test_rls_multiuser.py # Multi-user session scenarios
├── test_rls_edge_cases.py# Edge cases and bypass attempts
├── test_rls_compatibility.py # PostgreSQL compatibility verification
└── utils.py              # Helper functions
```

---

## 2. Test Scenarios

### 2.1 Basic RLS Operations (`test_rls_basic.py`)

| Test ID | Scenario | Steps | Expected Result |
|---------|----------|-------|-----------------|
| B01 | Enable RLS on table | `ALTER TABLE docs ENABLE ROW LEVEL SECURITY` | RLS metadata shows enabled |
| B02 | Disable RLS on table | `ALTER TABLE docs DISABLE ROW LEVEL SECURITY` | RLS metadata shows disabled |
| B03 | Force RLS for owner | `ALTER TABLE docs FORCE ROW LEVEL SECURITY` | Owner subject to RLS |
| B04 | No Force RLS for owner | `ALTER TABLE docs NO FORCE ROW LEVEL SECURITY` | Owner bypasses RLS |
| B05 | Default state | Create table, check RLS status | RLS disabled by default |

### 2.2 Policy Management (`test_rls_policies.py`)

| Test ID | Scenario | Steps | Expected Result |
|---------|----------|-------|-----------------|
| P01 | Create SELECT policy | `CREATE POLICY sel_policy ON docs FOR SELECT USING (owner = current_user)` | Policy stored in metadata |
| P02 | Create INSERT policy | `CREATE POLICY ins_policy ON docs FOR INSERT WITH CHECK (owner = current_user)` | Policy stored |
| P03 | Create UPDATE policy | `CREATE POLICY upd_policy ON docs FOR UPDATE USING (owner = current_user) WITH CHECK (owner = current_user)` | Both clauses stored |
| P04 | Create DELETE policy | `CREATE POLICY del_policy ON docs FOR DELETE USING (owner = current_user)` | Policy stored |
| P05 | Create ALL policy | `CREATE POLICY all_policy ON docs FOR ALL USING (owner = current_user)` | Applies to all commands |
| P06 | Create PERMISSIVE policy | `CREATE POLICY p1 ON docs AS PERMISSIVE FOR SELECT USING (status = 'public')` | Policy marked permissive |
| P07 | Create RESTRICTIVE policy | `CREATE POLICY r1 ON docs AS RESTRICTIVE FOR SELECT USING (status != 'deleted')` | Policy marked restrictive |
| P08 | Policy for specific role | `CREATE POLICY admin_pol ON docs TO admin_role FOR SELECT USING (true)` | Role stored in policy |
| P09 | Policy for PUBLIC | `CREATE POLICY public_pol ON docs TO PUBLIC FOR SELECT USING (status = 'public')` | PUBLIC role stored |
| P10 | Drop policy | `DROP POLICY p1 ON docs` | Policy removed from metadata |
| P11 | Alter policy | `ALTER POLICY p1 ON docs USING (new_expr)` | Policy updated |

### 2.3 Policy Evaluation (`test_rls_policies.py` continued)

| Test ID | Scenario | Steps | Expected Result |
|---------|----------|-------|-----------------|
| E01 | Single permissive policy | User selects from table with one permissive policy | Only matching rows returned |
| E02 | Multiple permissive policies | Two permissive policies, data matches both | Rows matching EITHER policy returned (OR) |
| E03 | Multiple restrictive policies | Two restrictive policies | Rows matching BOTH policies returned (AND) |
| E04 | Mixed permissive/restrictive | One permissive, one restrictive | Rows must pass both: `(permissive OR...) AND (restrictive AND...)` |
| E05 | No policies, RLS enabled | Enable RLS without policies, user queries | No rows returned (default deny) |
| E06 | Policy with current_user | Policy uses `current_user` function | Correctly filters by session user |
| E07 | Policy with session_user | Policy uses `session_user` function | Correctly filters by session user |

### 2.4 Multi-User Scenarios (`test_rls_multiuser.py`)

| Test ID | Scenario | Steps | Expected Result |
|---------|----------|-------|-----------------|
| M01 | User isolation | Two users with different data, RLS enabled | Each user sees only their data |
| M02 | Shared data access | Policy allows PUBLIC access to some rows | All users see shared rows |
| M03 | Role-based access | Policy for specific role, user has that role | User can access role-protected data |
| M04 | Role hierarchy | User has multiple roles | Policies for any role apply |
| M05 | Owner bypass | Table owner queries without FORCE RLS | Owner sees all rows |
| M06 | Owner with FORCE RLS | Table owner queries with FORCE RLS | Owner subject to policies |
| M07 | Superuser bypass | Superuser queries RLS-protected table | Superuser sees all rows |

### 2.5 DML Operations with RLS (`test_rls_policies.py`)

| Test ID | Scenario | Steps | Expected Result |
|---------|----------|-------|-----------------|
| D01 | SELECT with RLS | User queries table with SELECT policy | Only visible rows returned |
| D02 | INSERT with RLS | User inserts row, WITH CHECK policy | Insert allowed if policy passes |
| D03 | INSERT blocked by RLS | User inserts row that fails WITH CHECK | Insert rejected |
| D04 | UPDATE with RLS | User updates row they can see | Update allowed if USING and WITH CHECK pass |
| D05 | UPDATE blocked by RLS | User updates row they can't see | Update silently affects 0 rows |
| D06 | DELETE with RLS | User deletes row they can see | Delete allowed |
| D07 | DELETE blocked by RLS | User deletes row they can't see | Delete silently affects 0 rows |

### 2.6 Edge Cases & Bypass Attempts (`test_rls_edge_cases.py`)

| Test ID | Scenario | Steps | Expected Result |
|---------|----------|-------|-----------------|
| X01 | Subquery bypass attempt | Subquery references RLS-protected table | RLS applied to subquery |
| X02 | JOIN bypass attempt | JOIN with RLS-protected table | RLS applied to joined table |
| X03 | CTE bypass attempt | CTE references RLS table | RLS applied within CTE |
| X04 | View on RLS table | Create view on RLS-protected table | RLS applied through view |
| X05 | Function with RLS table | Function queries RLS table | RLS applied in function context |
| X06 | Empty policy expression | Policy with `USING (true)` | All rows visible |
| X07 | Always-false policy | Policy with `USING (false)` | No rows visible |
| X08 | Complex policy expression | Policy with subquery in USING | Expression evaluated correctly |
| X09 | Self-referential policy | Policy references same table | Handled without infinite loop |
| X10 | NULL in policy expression | Policy compares nullable column | NULL handling matches PostgreSQL |

---

## 3. PostgreSQL Compatibility Verification

### 3.1 Compatibility Test Strategy (`test_rls_compatibility.py`)

For each scenario, run against both PostgreSQL and postgresqlite to verify identical behavior:

```python
def test_compatibility_select_rls():
    """Verify SELECT with RLS returns same results as PostgreSQL"""
    # Setup: Create table, enable RLS, create policy, insert test data
    
    # Test against PostgreSQL
    pg_results = run_on_postgresql("SELECT * FROM docs")
    
    # Test against postgresqlite
    sqlite_results = run_on_postgresqlite("SELECT * FROM docs")
    
    # Verify identical results
    assert pg_results == sqlite_results
```

### 3.2 Key Compatibility Scenarios

| Test ID | Scenario | Verification Method |
|---------|----------|---------------------|
| C01 | Policy combination semantics | Compare OR/AND behavior with PostgreSQL |
| C02 | Default deny behavior | Verify no policies = no access |
| C03 | current_user function | Verify returns same value as PostgreSQL |
| C04 | session_user function | Verify returns same value as PostgreSQL |
| C05 | Role membership | Verify role resolution matches PostgreSQL |
| C06 | Command type matching | Verify FOR SELECT/INSERT/UPDATE/DELETE/ALL behavior |
| C07 | NULL handling in policies | Verify NULL comparisons match PostgreSQL |

---

## 4. Python Test Script Structure

### 4.1 conftest.py (Fixtures)

```python
import pytest
import psycopg2
import subprocess
import time
import os

@pytest.fixture(scope="module")
def proxy_server():
    """Start the postgresqlite proxy server"""
    db_path = "/tmp/rls_test.db"
    
    # Clean up any existing database
    if os.path.exists(db_path):
        os.remove(db_path)
    
    # Start proxy
    proc = subprocess.Popen(
        ["cargo", "run", "--release", "--", "--db", db_path, "--port", "5433"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )
    time.sleep(3)  # Wait for server to start
    
    yield {"host": "127.0.0.1", "port": 5433, "db": db_path}
    
    # Cleanup
    proc.terminate()
    proc.wait()

@pytest.fixture
def db_connection(proxy_server):
    """Provide a fresh database connection"""
    conn = psycopg2.connect(
        host=proxy_server["host"],
        port=proxy_server["port"],
        user="postgres",
        dbname="rls_test"
    )
    conn.autocommit = True
    yield conn
    conn.close()

@pytest.fixture
def rls_test_table(db_connection):
    """Create a standard test table for RLS tests"""
    cur = db_connection.cursor()
    cur.execute("""
        CREATE TABLE documents (
            id SERIAL PRIMARY KEY,
            title TEXT,
            owner TEXT,
            status TEXT,
            content TEXT
        )
    """)
    cur.execute("""
        INSERT INTO documents (title, owner, status, content) VALUES
            ('Doc 1', 'alice', 'public', 'Content 1'),
            ('Doc 2', 'alice', 'private', 'Content 2'),
            ('Doc 3', 'bob', 'public', 'Content 3'),
            ('Doc 4', 'bob', 'private', 'Content 4')
    """)
    yield "documents"
    cur.execute("DROP TABLE IF EXISTS documents")
```

### 4.2 utils.py (Helper Functions)

```python
def set_session_user(conn, username):
    """Set the current session user for RLS context"""
    cur = conn.cursor()
    # Execute SET SESSION AUTHORIZATION or equivalent
    cur.execute(f"SET SESSION AUTHORIZATION '{username}'")
    
def get_visible_rows(conn, table):
    """Get all visible rows from a table"""
    cur = conn.cursor()
    cur.execute(f"SELECT * FROM {table} ORDER BY id")
    return cur.fetchall()

def count_visible_rows(conn, table):
    """Count visible rows from a table"""
    cur = conn.cursor()
    cur.execute(f"SELECT COUNT(*) FROM {table}")
    return cur.fetchone()[0]
```

---

## 5. Test Execution

### 5.1 Running Tests

```bash
# Start proxy manually (if needed)
cargo run --release -- --db test_rls.db --port 5432 &

# Run all E2E tests
pytest tests/e2e/ -v

# Run specific test file
pytest tests/e2e/test_rls_basic.py -v

# Run with PostgreSQL comparison (requires running PostgreSQL)
pytest tests/e2e/test_rls_compatibility.py -v --pg-host=localhost --pg-port=5432
```

### 5.2 Test Report

Tests should produce:
- Pass/Fail status for each scenario
- Actual vs expected row counts for data tests
- Error messages for blocked operations
- Timing information for performance baseline

---

## 6. Dependencies

### 6.1 Python Requirements
```
psycopg2-binary>=2.9.0
pytest>=7.0.0
pytest-cov>=4.0.0
```

### 6.2 System Requirements
- postgresqlite proxy running and accepting connections
- PostgreSQL (optional, for compatibility comparison tests)

---

## 7. Success Criteria

The E2E test suite passes when:

1. **Basic Operations**: All B01-B05 tests pass
2. **Policy Management**: All P01-P11 tests pass  
3. **Policy Evaluation**: All E01-E07 tests pass
4. **Multi-User**: All M01-M07 tests pass
5. **DML Operations**: All D01-D07 tests pass
6. **Edge Cases**: All X01-X10 tests pass
7. **Compatibility**: C01-C07 tests show identical behavior to PostgreSQL

---

## 8. Implementation Notes

### 8.1 Current Gaps to Address

Based on research findings, these areas need attention before E2E tests will pass:

1. **AST Injection**: Current string-based `WHERE` injection is fragile
2. **current_user() Function**: Need SQLite custom function for policy expressions
3. **Default Deny**: Empty permissive policies should deny access
4. **WITH CHECK for INSERT**: May need trigger-based implementation

### 8.2 Test Data Patterns

Use consistent test data across scenarios:
- Users: `alice`, `bob`, `admin`
- Roles: `PUBLIC`, `admin_role`, `editor_role`
- Status values: `public`, `private`, `deleted`

---

## 9. Communication with Researcher

Need to confirm with `researcher-new`:

1. Exact semantics of `current_user` vs `session_user` in context of RLS
2. How role membership should be resolved (recursive? cached?)
3. Expected behavior for policies referencing non-existent columns
4. Behavior when policy expression raises an error

---

*Document Version: 1.0*
*Created: 2026-02-28*
*Author: e2e-tester-v2*
