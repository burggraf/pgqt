# Investigation Plan: Timestamp/Time.sql Regression

## Executive Summary

**Estimated Improvement:** +18.5-26.5%  
**Actual Result:** -1.14% (57.13% → 55.99%)  
**Status:** REGRESSION - We went backwards

## What Was Implemented

### Phase 4 Tasks (8 total)

1. **INTERVAL Type Support** - Added datetime + interval arithmetic
2. **String Functions** - chr(), lpad(), rpad(), translate(), format()
3. **UUID Functions** - uuidv4(), uuidv7(), uuid_extract_version(), uuid_extract_timestamp()
4. **EXPLAIN Tests** - Added tests for EXPLAIN queries
5. **SHOW Commands** - Added support for timezone, transaction_isolation_level, etc.
6. **Date/Time Functions** - clock_timestamp(), statement_timestamp(), transaction_timestamp()
7. **Error Handling Docs** - Documented known compatibility gaps
8. **Validation Functions** - pg_input_is_valid(), pg_input_error_info()

### Files Modified
- `src/transpiler/expr/operators.rs` - INTERVAL arithmetic, 96 lines added
- `src/transpiler/mod.rs` - SHOW ALL handling, 30 lines modified
- `src/transpiler/registry.rs` - Function aliases, 3 lines added
- `src/handler/mod.rs` - String, UUID, validation functions, 300+ lines added
- `Cargo.toml` - Added uuid crate dependency
- `docs/KNOWN_ISSUES.md` - New documentation file
- `tests/transpiler_tests.rs` - 17 new tests
- `tests/error_handling_tests.rs` - New test file

## Actual Compatibility Results

### Baseline (commit 465fdb9)
- **Overall:** 57.13%
- timestamp.sql: 125/177 passed (70.6%)
- time.sql: 28/44 passed (63.6%)
- strings.sql: 288/550 passed (52.4%)
- interval.sql: 138/449 passed (30.7%)
- uuid.sql: 16/63 passed (25.4%)

### After Phase 4 Changes
- **Overall:** 55.99% (-1.14%)
- timestamp.sql: 38/177 passed (21.5%) **-49.1% regression**
- time.sql: 9/44 passed (20.5%) **-43.1% regression**
- strings.sql: 301/550 passed (54.7%) **+2.3% improvement**
- interval.sql: 138/449 passed (30.7%) **no change**
- uuid.sql: 16/63 passed (25.4%) **no change**

## Root Cause Analysis

### Critical Bug Found (Fixed)
**Issue:** Divide-by-zero panic in `lpad()`/`rpad()` functions when fill string is empty  
**Location:** `src/handler/mod.rs:1230`  
**Error:** `attempt to calculate the remainder with a divisor of zero`  
**Impact:** Proxy crashes during compatibility suite execution  
**Status:** FIXED - Added empty fill string check

### Remaining Issues

The timestamp.sql and time.sql files show massive regressions that are NOT explained by the panic bug. We need to investigate:

1. **Connection Instability** - "connection already closed" errors appearing during test runs
2. **Behavior Changes** - Something in our changes altered how timestamp/time queries are processed
3. **Test File Specific Issues** - Need to isolate which specific statements are failing

## What We Know

### From Manual Testing
- Individual queries work fine: `SELECT now()`, `SHOW search_path`, `SELECT lpad('hi', 5)`
- Timestamp creation works: `CREATE TABLE test_ts (d1 timestamp)`, `INSERT INTO test_ts VALUES ('2024-01-15')`
- No crashes when running simple test scripts

### From Error Logs
- Missing functions: `crc32c`, `encode`, `decode`, `get_bit`, `set_bit`, `initcap`
- These are NOT related to our changes - they're pre-existing gaps
- The "connection already closed" errors suggest proxy instability

### From Comparison
- strings.sql improved slightly (288→301 passed statements)
- interval.sql and uuid.sql unchanged
- timestamp.sql and time.sql massively regressed

## Investigation Plan

### Step 1: Isolate the Regression
**Goal:** Determine exactly which commit introduced the regression

```bash
# Test each commit in the feature branch
git checkout 6acfba9  # INTERVAL support only
cargo build --release
cd postgres-compatibility-suite && python3 runner_with_stats.py

# If timestamp.sql passes >70%, test next commit
git checkout 4451725  # String functions
cargo build --release
python3 runner_with_stats.py

# Continue until regression is found
```

### Step 2: Bisect timestamp.sql Failures
**Goal:** Find specific failing statements

```bash
# Run only timestamp.sql with verbose output
cd postgres-compatibility-suite
python3 -c "
import psycopg2, subprocess, time
proc = subprocess.Popen(['../target/release/pgqt', '--port', '5435', '--database', 'test.db'])
time.sleep(2)
conn = psycopg2.connect(host='127.0.0.1', port=5435, ...)
# Read timestamp.sql and execute each statement
# Log which ones fail
"
```

### Step 3: Check for Behavioral Changes
**Goal:** Identify if our code changes altered query processing

Suspect areas to investigate:
1. **operators.rs:213-250** - JSONB key removal changes
2. **operators.rs:250-280** - Datetime + Interval operations
3. **mod.rs:241-270** - SHOW ALL implementation

The `-` operator handling was modified to check for datetime-interval operations BEFORE checking for JSONB key removal. This could affect queries that use `-` with timestamps.

### Step 4: Verify Connection Stability
**Goal:** Determine if proxy crashes during test runs

```bash
# Run compatibility suite with error log monitoring
tail -f test_db.db.error.log &
python3 runner_with_stats.py
# Check if any panics occur
```

### Step 5: Fix and Verify
**Goal:** Implement fixes and verify compatibility improvement

Once the root cause is identified:
1. Fix the issue
2. Re-run full compatibility suite
3. Verify timestamp.sql passes >70%
4. Verify overall compatibility >57.13%

## Hypotheses

### H1: Connection Pool Exhaustion
The compatibility suite may be exhausting connection pools due to our new functions or changes.

### H2: Operator Precedence Change
Our datetime-interval detection may be incorrectly matching timestamp queries.

### H3: Session State Issues
The SHOW command implementation may have altered session handling.

### H4: Build/Environment Issue
The test may be using wrong binary or there may be stale state.

## Immediate Actions Needed

1. **Confirm the regression is real** by running the full suite 3 times
2. **Bisect commits** to find which one introduced the regression
3. **Test timestamp.sql in isolation** to see specific failures
4. **Check if baseline still passes** at 57.13%

## Success Criteria

- timestamp.sql passes ≥70% (back to baseline)
- time.sql passes ≥60% (back to baseline)
- Overall compatibility ≥57.13% (back to baseline)
- Ideally: Overall compatibility ≥60% (showing improvement)

---

## Notes for Next Session

1. Start by confirming baseline still works at 57.13%
2. The feature branch is merged to main, commits are:
   - 6acfba9 feat: add INTERVAL type literal support
   - 4451725 feat: add string functions
   - 43f3f6f feat: add UUID functions
   - 7c58327 feat: add EXPLAIN query tests
   - b921ff1 feat: add SHOW command support
   - 62c8d92 feat: add timestamp functions
   - 627982e docs: document error handling gaps
   - 18fc200 feat: add validation functions
   - 26b7386 fix: add search_path to current_setting
   - e6f6cfc fix: add empty fill string protection

3. Focus on timestamp.sql and time.sql specifically
4. Check if the issue is in transpiler or handler code
5. Consider reverting specific commits if needed
