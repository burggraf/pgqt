# PGQT PostgreSQL Compatibility Implementation Plan

**Objective:** Improve PostgreSQL compatibility from 40.89% to ~65%+
**Timeline:** 8-10 weeks
**Success Criteria:** Each phase must pass all validation steps before proceeding

---

## Pre-Implementation Setup

### 1. Environment Preparation
```bash
# Ensure clean working state
git status
git pull origin main

# Verify PostgreSQL is running for compatibility tests
pg_isready -h localhost -p 5432

# Build release binary (used by compatibility suite)
cargo build --release

# Run baseline compatibility test
./run_compatibility_suite.sh --summary
# Record baseline: _____% (target: 40.89%)
```

### 2. Create Work Branch
```bash
git checkout -b feature/postgres-compatibility-improvements
```

---

## Phase 1: Critical Foundations (Weeks 1-3)
**Target:** 40.89% → 50% (+9%)
**Focus:** JOIN operations, String functions, DML improvements

---

### Phase 1.1: JOIN Operations Improvement
**Goal:** 33.0% → 80% pass rate (+428 statements)
**File:** `src/transpiler/dml.rs`

#### Implementation Steps:

1. **Research Current JOIN Implementation**
   - Read `src/transpiler/dml.rs` completely
   - Identify how JOIN nodes are handled in AST
   - Look for `JoinExpr` handling
   - Check existing JOIN tests in `tests/integration_test.rs`

2. **Fix USING Clause Support**
   - Find where `JoinExpr` with `usingClause` is handled
   - Implement proper column expansion for USING (e.g., `JOIN ... USING (id)`)
   - Handle implicit column naming from USING

3. **Fix Multiple JOIN Handling**
   - Test queries with 3+ JOINs
   - Ensure proper parentheses and precedence
   - Fix alias handling across multiple joins

4. **Fix OUTER JOIN Edge Cases**
   - RIGHT OUTER JOIN (may need to swap table order for SQLite)
   - FULL OUTER JOIN (requires UNION workaround)
   - CROSS JOIN handling

#### Validation Checklist (MUST COMPLETE BEFORE PROCEEDING):

```bash
# Step 1: Build Check
cargo check
# If errors, fix before proceeding

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings before proceeding

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass (unit + integration + e2e)
# If failures, debug and fix before proceeding

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/join.sql"
# Verify pass rate improved (target: 80%+)

# Step 5: Documentation Update
# Update CHANGELOG.md with JOIN improvements
# Add/modify docs/joins.md if needed
# Update src/transpiler/dml.rs module docs if significant changes
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with JOIN fixes
- [ ] Add examples of newly supported JOIN patterns to relevant docs
- [ ] Document any limitations (e.g., FULL OUTER JOIN workarounds)

---

### Phase 1.2: String Operations Enhancement
**Goal:** 27.1% → 75% pass rate (+264 statements)
**Files:** `src/transpiler/expr.rs`, `src/transpiler/func.rs`

#### Implementation Steps:

1. **Audit Current String Functions**
   - List all currently supported string functions
   - Identify missing functions from PostgreSQL compatibility suite failures

2. **Implement Missing String Functions**
   - `substring(string FROM pattern)` - regex substring
   - `trim([BOTH|LEADING|TRAILING] [characters] FROM string)`
   - `position(substring IN string)`
   - `overlay(string PLACING replacement FROM start [FOR length])`

3. **Fix LIKE/ILIKE Handling**
   - Ensure `LIKE` case sensitivity matches PostgreSQL
   - Implement `ILIKE` as case-insensitive LIKE
   - Handle escape characters properly

4. **Fix String Concatenation**
   - Verify `||` operator works for text concatenation
   - Handle NULL behavior (NULL || 'text' = NULL in PostgreSQL)

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/strings.sql"
# Verify pass rate improved (target: 75%+)

# Step 5: Documentation Update
# Update CHANGELOG.md
# Update any string function documentation
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with string function additions
- [ ] Document string function compatibility in relevant feature docs

---

### Phase 1.3: INSERT/UPDATE with RETURNING
**Goal:** Avg 47% → 80% pass rate (+230 statements)
**Files:** `src/transpiler/ddl.rs`, `src/transpiler/dml.rs`

#### Implementation Steps:

1. **Implement RETURNING Clause**
   - Add RETURNING support for INSERT statements
   - Add RETURNING support for UPDATE statements
   - Add RETURNING support for DELETE statements
   - Handle RETURNING * vs RETURNING specific columns

2. **Fix INSERT with Multiple VALUES**
   - Support `INSERT INTO t VALUES (1), (2), (3)` syntax
   - Handle DEFAULT values in multi-row inserts

3. **Fix ON CONFLICT (Upsert)**
   - Implement `ON CONFLICT DO NOTHING`
   - Implement `ON CONFLICT DO UPDATE SET ...`
   - Handle conflict_target specification

4. **Fix UPDATE with JOIN**
   - Support UPDATE with FROM clause
   - Handle table aliases in UPDATE

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep -E "(insert|update).sql"
# Verify pass rates improved

# Step 5: Documentation Update
# Update CHANGELOG.md
# Update RETURNING clause documentation
# Document ON CONFLICT support
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with DML improvements
- [ ] Document RETURNING clause support
- [ ] Document ON CONFLICT/upsert patterns

---

### Phase 1 Checkpoint
**Before proceeding to Phase 2, verify:**

```bash
# Full validation
./run_compatibility_suite.sh --summary
# Record Phase 1 results: _____% (target: 50%+)
```

---

## Phase 2: Core Data Types (Weeks 4-6)
**Target:** 50% → 61.5% (+11.5%)
**Focus:** JSONB, NUMERIC, TIMESTAMPTZ

---

### Phase 2.1: JSONB Support Enhancement
**Goal:** 25.4% → 70% pass rate (+494 statements)
**Files:** `src/transpiler/expr.rs`, new JSONB-specific files if needed

#### Implementation Steps:

1. **Audit Current JSONB Implementation**
   - Check existing JSONB support in transpiler
   - Identify which operators work vs fail

2. **Implement JSONB Operators**
   - `->` (Get JSON object field)
   - `->>` (Get JSON object field as text)
   - `#>` (Get JSON object at specified path)
   - `#>>` (Get JSON object at path as text)
   - `@>` (JSON contains)
   - `<@` (JSON is contained by)
   - `?` (Does key exist?)
   - `?|` (Do any keys exist?)
   - `?&` (Do all keys exist?)

3. **Implement JSONB Construction Functions**
   - `jsonb_build_object(...)`
   - `jsonb_build_array(...)`
   - `to_jsonb(...)`
   - `jsonb_agg(...)` (aggregate)

4. **Fix JSONB Path Queries**
   - Handle nested path access
   - Array indexing in JSONB

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/jsonb.sql"
# Verify pass rate improved (target: 70%+)

# Step 5: Documentation Update
# Update CHANGELOG.md
# Create/update docs/jsonb.md
# Update README.md JSONB section
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with JSONB improvements
- [ ] Document all supported JSONB operators
- [ ] Add JSONB examples to documentation

---

### Phase 2.2: NUMERIC Type Precision
**Goal:** 14.3% → 60% pass rate (+483 statements)
**Files:** `src/transpiler/utils.rs`, `src/transpiler/expr.rs`

#### Implementation Steps:

1. **Audit Current NUMERIC Handling**
   - Review how NUMERIC/DECIMAL types are stored
   - Check precision/scale handling
   - Identify SQLite REAL limitations

2. **Implement NUMERIC Storage**
   - Store NUMERIC as TEXT in SQLite to preserve precision
   - Add type tag metadata for NUMERIC columns
   - Implement NUMERIC arithmetic with proper precision

3. **Fix NUMERIC Functions**
   - `round(numeric, scale)`
   - `trunc(numeric, scale)`
   - `numeric + numeric`, `numeric - numeric`, etc.
   - Casting to/from NUMERIC

4. **Fix NUMERIC Comparisons**
   - Proper decimal comparison
   - Handle scale differences

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/numeric.sql"
# Verify pass rate improved (target: 60%+)

# Step 5: Documentation Update
# Update CHANGELOG.md
# Document NUMERIC handling strategy
# Update type compatibility docs
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with NUMERIC precision improvements
- [ ] Document NUMERIC type implementation strategy
- [ ] Note any precision limitations

---

### Phase 2.3: Timestamp with Time Zone
**Goal:** 14.9% → 65% pass rate (+202 statements)
**Files:** `src/transpiler/expr.rs`, `src/transpiler/utils.rs`

#### Implementation Steps:

1. **Audit Current Timestamp Handling**
   - Check how TIMESTAMPTZ is currently stored
   - Review timezone conversion logic
   - Identify failing test patterns

2. **Implement Timezone Storage**
   - Store all timestamps in UTC internally
   - Track timezone metadata in catalog
   - Convert to local time on retrieval

3. **Fix AT TIME ZONE Operator**
   - `timestamp AT TIME ZONE zone`
   - `timestamptz AT TIME ZONE zone`
   - Handle named timezones vs offsets

4. **Fix Timezone Functions**
   - `now()` with timezone
   - `timezone(zone, timestamp)`
   - `CURRENT_TIMESTAMP`, `LOCALTIMESTAMP`

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/timestamptz.sql"
# Verify pass rate improved (target: 65%+)

# Step 5: Documentation Update
# Update CHANGELOG.md
# Update timestamp type documentation
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with TIMESTAMPTZ support
- [ ] Document timezone handling approach
- [ ] List supported timezone functions

---

### Phase 2 Checkpoint
**Before proceeding to Phase 3, verify:**

```bash
# Full validation
./run_compatibility_suite.sh --summary
# Record Phase 2 results: _____% (target: 61.5%+)
```

---

## Phase 3: Advanced Features (Weeks 7-9)
**Target:** 61.5% → 65% (+3.5%)
**Focus:** Window functions, CTEs, Arrays

---

### Phase 3.1: Window Functions Enhancement
**Goal:** 44.3% → 75% pass rate (+132 statements)
**Files:** `src/transpiler/window.rs`

#### Implementation Steps:

1. **Audit Current Window Functions**
   - Check which window functions are implemented
   - Identify failing window function tests

2. **Implement Missing Window Functions**
   - `RANK()`
   - `DENSE_RANK()`
   - `NTILE(n)`
   - `PERCENT_RANK()`
   - `CUME_DIST()`
   - `FIRST_VALUE()`, `LAST_VALUE()`
   - `NTH_VALUE()`

3. **Fix Window Frame Clauses**
   - `ROWS BETWEEN ... AND ...`
   - `RANGE BETWEEN ... AND ...`
   - `UNBOUNDED PRECEDING/FOLLOWING`
   - `CURRENT ROW`

4. **Fix PARTITION BY Issues**
   - Multiple PARTITION BY columns
   - Complex expressions in PARTITION BY

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/window.sql"
# Verify pass rate improved (target: 75%+)

# Step 5: Documentation Update
# Update CHANGELOG.md
# Update window function documentation
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with window function additions
- [ ] Document supported window functions
- [ ] Add window function examples

---

### Phase 3.2: Common Table Expressions (CTEs)
**Goal:** 29.6% → 70% pass rate (+127 statements)
**Files:** `src/transpiler/dml.rs` (WITH clause handling)

#### Implementation Steps:

1. **Audit Current CTE Support**
   - Check existing WITH clause handling
   - Identify recursive CTE support gaps

2. **Fix CTE Scoping**
   - Ensure CTEs are visible in main query
   - Handle CTEs with same name as table
   - Fix column reference resolution

3. **Implement Recursive CTEs**
   - `WITH RECURSIVE ...`
   - Handle UNION vs UNION ALL in recursive
   - Proper cycle detection

4. **Fix Multiple CTEs**
   - `WITH a AS (...), b AS (...) SELECT ...`
   - CTEs referencing other CTEs

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/with.sql"
# Verify pass rate improved (target: 70%+)

# Step 5: Documentation Update
# Update CHANGELOG.md
# Document CTE support
# Add recursive CTE examples
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with CTE improvements
- [ ] Document CTE support including recursive

---

### Phase 3.3: Array Operations
**Goal:** 60.9% → 85% pass rate (+129 statements)
**Files:** `src/array.rs`, `src/transpiler/expr.rs`

#### Implementation Steps:

1. **Audit Current Array Support**
   - Check existing array implementation
   - Identify missing array functions

2. **Fix Array Constructor**
   - `ARRAY[1, 2, 3]` syntax
   - Multidimensional arrays
   - Array of NULLs

3. **Implement Array Slicing**
   - `arr[1:3]` (slice from 1 to 3)
   - `arr[2:]` (slice from 2 to end)
   - `arr[:3]` (slice from start to 3)

4. **Add Array Functions**
   - `array_length(arr, dim)`
   - `array_append(arr, elem)`
   - `array_prepend(elem, arr)`
   - `array_cat(arr1, arr2)`
   - `array_remove(arr, elem)`
   - `array_replace(arr, old, new)`

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary | grep "pg_regress/arrays.sql"
# Verify pass rate improved (target: 85%+)

# Step 5: Documentation Update
# Update CHANGELOG.md
# Update array documentation
```

#### Documentation Requirements:
- [ ] Update `CHANGELOG.md` with array improvements
- [ ] Document array functions and operators

---

### Phase 3 Checkpoint
**Before proceeding to Phase 4, verify:**

```bash
# Full validation
./run_compatibility_suite.sh --summary
# Record Phase 3 results: _____% (target: 65%+)
```

---

## Phase 4: Polish & Edge Cases (Week 10+)
**Target:** 65% → 70%+ (stretch goal)
**Focus:** Quick wins, remaining edge cases

---

### Phase 4.1: Quick Wins
**Goal:** Address low-hanging fruit

#### Implementation Steps:

1. **Fix VARCHAR Type** (13.6% → 80%+)
   - Only 22 test statements - easy fix
   - Likely type casting issue

2. **Fix Error Handling Gaps** (269 failures)
   - Many are simple validation issues
   - Check error code matching

3. **Implement Missing Simple Functions** (101 failures)
   - Pick low-hanging fruit from function list
   - Focus on commonly used missing functions

#### Validation Checklist:

```bash
# Step 1: Build Check
cargo check
# Fix any errors

# Step 2: Warning Cleanup
cargo clippy -- -D warnings
# Fix ALL warnings

# Step 3: Unit/Integration Tests
./run_tests.sh
# ALL tests must pass

# Step 4: Compatibility Suite Verification
./run_compatibility_suite.sh --summary
# Verify overall improvement

# Step 5: Documentation Update
# Update CHANGELOG.md
```

---

## Final Validation & Release

### Pre-Release Checklist

```bash
# 1. Full Test Suite
./run_tests.sh
# All tests must pass

# 2. Compatibility Suite
./run_compatibility_suite.sh --summary
# Record final compatibility: _____%

# 3. Build Check
cargo build --release
cargo check
cargo clippy -- -D warnings

# 4. Documentation Review
# Review all updated documentation
# Ensure CHANGELOG.md is complete
# Update README.md if needed

# 5. Git Status
git status
git diff --stat
# Ensure no uncommitted changes except docs/plans
```

### Release Documentation

Create final summary document:

```bash
# Create compatibility improvement report
cat > docs/COMPATIBILITY_IMPROVEMENTS.md << 'EOF'
# PostgreSQL Compatibility Improvements

## Summary
- Starting compatibility: 40.89%
- Final compatibility: ___%
- Total improvement: ___%

## Changes by Phase

### Phase 1: Critical Foundations
- JOIN operations: 33.0% → ___%
- String operations: 27.1% → ___%
- INSERT/UPDATE: 47% → ___%

### Phase 2: Core Data Types
- JSONB: 25.4% → ___%
- NUMERIC: 14.3% → ___%
- TIMESTAMPTZ: 14.9% → ___%

### Phase 3: Advanced Features
- Window functions: 44.3% → ___%
- CTEs: 29.6% → ___%
- Arrays: 60.9% → ___%

## New Features
[List new features implemented]

## Breaking Changes
[List any breaking changes]
EOF
```

---

## Appendix: Daily Workflow

### For Each Implementation Day:

1. **Start of Day:**
   ```bash
   git pull origin feature/postgres-compatibility-improvements
   cargo check
   ./run_tests.sh --unit-only  # Quick check
   ```

2. **During Development:**
   - Make focused changes
   - Run `cargo check` frequently
   - Write tests for new functionality

3. **End of Each Task:**
   ```bash
   # Required validation sequence
   cargo check
   cargo clippy -- -D warnings
   ./run_tests.sh
   ./run_compatibility_suite.sh --summary | grep "<relevant feature>"
   
   # Update documentation
   # Update CHANGELOG.md
   
   git add -A
   git commit -m "feat: <description>"
   ```

4. **End of Phase:**
   ```bash
   ./run_compatibility_suite.sh --summary
   # Record results
   git tag "compat-phase-X"
   git push origin feature/postgres-compatibility-improvements --tags
   ```

---

## Risk Mitigation

### If a Phase Falls Behind:
1. Reduce scope - drop lower-priority items
2. Focus on highest-impact items within the phase
3. Move incomplete items to next phase

### If Build/Test Issues Arise:
1. Do NOT proceed until resolved
2. Document the issue
3. Seek help if blocked for >1 day
4. Consider alternative implementation approach

### If Compatibility Suite is Unavailable:
1. Use existing test suite: `./run_tests.sh`
2. Create targeted unit tests for new features
3. Run compatibility suite when PostgreSQL is available

---

## Success Metrics

| Phase | Target | Minimum Acceptable | Stretch |
|-------|--------|-------------------|---------|
| Phase 1 | 50% | 48% | 52% |
| Phase 2 | 61.5% | 59% | 64% |
| Phase 3 | 65% | 63% | 68% |
| Phase 4 | 70% | 67% | 75% |

---

*Plan created: 2026-03-16*
*Last updated: 2026-03-16*
