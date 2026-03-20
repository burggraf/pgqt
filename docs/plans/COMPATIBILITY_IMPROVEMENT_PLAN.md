# PGQT Compatibility Improvement Plan

> **How to Use This Plan:**
> - **For overview and prioritization**: Read this main plan
> - **For implementation**: See the detailed phase plans in `plans/` directory:
>   - `plans/PHASE_1_JSON_JSONB.md`
>   - `plans/PHASE_2_INTERVAL.md`
>   - `plans/PHASE_3_AGGREGATES.md`
>   - `plans/PHASE_4_INSERT.md`
>   - `plans/PHASE_5_CTE.md`
>   - `plans/PHASE_6_FLOAT.md`
>   - `plans/PHASE_7_ERROR_HANDLING.md`
> - **For implementation strategy**: See "Implementation Strategy" section below

## Executive Summary

This plan outlines 7 phases of work to improve PGQT's PostgreSQL compatibility from **66.68%** to approximately **85%**. Each phase focuses on high-impact features that bring the most value to users.

**Current Status:** 66.68% (6,813/10,217 statements passing)
**Target Status:** ~85% (8,600+ statements passing)
**Estimated Score Gain:** +18-20%

---

## Plan Structure

Each phase is divided into sub-phases. Every sub-phase MUST:
1. ✅ Run `cargo build --release` and ensure the build succeeds
2. ✅ Fix any build warnings encountered
3. ✅ Run `./run_tests.sh` and ensure every test passes
4. ✅ Create/update relevant documentation
5. ✅ Update CHANGELOG.md if necessary

**NO SUB-PHASE IS COMPLETE UNTIL ALL 5 ITEMS ARE CHECKED OFF.**

---

## Phase 1: JSON/JSONB Functions & Operators (Highest Impact)

**Estimated Score Gain:** +7-10%  
**Files Affected:** `json.sql` (38.5% → 80%), `jsonb.sql` (58.5% → 85%)

### Phase 1.1: JSON Constructor Functions

**Goal:** Implement JSON construction functions that convert various inputs to JSON.

**Functions to Implement:**
- `to_json(anyelement)` - Convert any value to JSON
- `to_jsonb(anyelement)` - Convert any value to JSONB
- `row_to_json(record)` - Convert a row to JSON object
- `array_to_json(anyarray)` - Convert array to JSON array
- `json_build_object(VARIADIC "any")` - Build JSON object from variadic args
- `jsonb_build_object(VARIADIC "any")` - Build JSONB object from variadic args
- `json_build_array(VARIADIC "any")` - Build JSON array from variadic args
- `jsonb_build_array(VARIADIC "any")` - Build JSONB array from variadic args

**Implementation Details:**
- Add functions to `src/jsonb.rs` or create new `src/json.rs` module
- Register functions in `src/handler/mod.rs` in `register_custom_functions()`
- Handle special cases: NULL values, numeric formatting, timestamp formatting
- For variadic functions, use SQLite's support for variable arguments

**Transpiler Changes:**
- Update `src/transpiler/func.rs` to recognize these functions
- Map `row_to_json` to appropriate SQLite JSON construction

**Testing:**
- Add unit tests in `src/jsonb.rs` (following existing patterns)
- Add integration test file `tests/json_function_tests.rs`
- Test edge cases: NULL inputs, nested structures, special characters

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/JSON.md` (create if doesn't exist)
- [ ] CHANGELOG.md updated with new functions

---

### Phase 1.2: JSON Processing Functions

**Goal:** Implement functions that extract and process JSON data.

**Functions to Implement:**
- `json_each(json)` - Expand JSON object/array to row set
- `jsonb_each(jsonb)` - Expand JSONB object to row set
- `json_each_text(json)` - Like json_each but returns text values
- `jsonb_each_text(jsonb)` - Like jsonb_each but returns text values
- `json_array_elements(json)` - Expand JSON array to row set
- `jsonb_array_elements(jsonb)` - Expand JSONB array to row set
- `json_array_elements_text(json)` - Like json_array_elements but text
- `jsonb_array_elements_text(jsonb)` - Like jsonb_array_elements but text
- `json_object_keys(json)` - Return keys of JSON object
- `jsonb_object_keys(jsonb)` - Return keys of JSONB object

**Implementation Details:**
- These are table-valued functions (return multiple rows)
- Use SQLite's JSON1 extension as foundation
- Implement as custom table-valued functions using `create_module` or scalar functions that return JSON arrays

**Transpiler Changes:**
- Handle `LATERAL` joins with these functions (may already work from Phase 1.3 JOIN improvements)
- Ensure proper column expansion in SELECT

**Testing:**
- Test with `LATERAL` joins
- Test nested JSON structures
- Test empty objects/arrays

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

### Phase 1.3: JSON Aggregation Functions

**Goal:** Implement aggregate functions for JSON.

**Functions to Implement:**
- `json_agg(anyelement)` - Aggregate values into JSON array
- `jsonb_agg(anyelement)` - Aggregate values into JSONB array
- `json_object_agg(key, value)` - Aggregate key-value pairs into JSON object
- `jsonb_object_agg(key, value)` - Aggregate key-value pairs into JSONB object
- `json_agg_strict(anyelement)` - Like json_agg but skips NULLs
- `jsonb_agg_strict(anyelement)` - Like jsonb_agg but skips NULLs

**Implementation Details:**
- These are aggregate functions, not scalar functions
- Implement using SQLite's aggregate function API
- Store intermediate state as JSON array string
- Finalize by returning the JSON array/object

**Integration with Existing Code:**
- Look at `src/array_agg.rs` for aggregate function implementation patterns
- Register in `src/handler/mod.rs` alongside other aggregates

**Testing:**
- Test with GROUP BY
- Test with ORDER BY within aggregate (if supported)
- Test NULL handling

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

### Phase 1.4: JSON Operators

**Goal:** Implement PostgreSQL JSON operators in the transpiler.

**Operators to Implement:**
- `->` (Get JSON object field / array element)
- `->>` (Get JSON object field / array element as text)
- `#>` (Get JSON object at specified path)
- `#>>` (Get JSON object at specified path as text)
- `@>` (JSON contains)
- `<@` (JSON is contained by)
- `?` (Does key exist?)
- `?|` (Does any key exist?)
- `?&` (Do all keys exist?)
- `||` (Concatenate JSON)
- `-` (Delete key/array element)
- `#-` (Delete at path)

**Implementation Details:**
- Most operators already have function implementations in `src/jsonb.rs`
- Update `src/transpiler/expr.rs` to map operators to functions
- `@>`, `<@` → `jsonb_contains()`, `jsonb_contained()`
- `?` → `jsonb_exists()`
- `?|` → `jsonb_exists_any()`
- `?&` → `jsonb_exists_all()`
- `->`, `->>`, `#>`, `#>>` → Use SQLite's `json_extract()` with path syntax

**Transpiler Changes:**
- In `reconstruct_a_expr()` or similar, detect JSON operators
- Map to appropriate SQLite JSON1 functions
- Handle path syntax conversion (PostgreSQL uses `{a,b,c}`, SQLite uses `$.a.b.c`)

**Testing:**
- Test each operator with various JSON types
- Test nested path access
- Test array vs object semantics

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

### Phase 1.5: JSON Type Casting & Validation

**Goal:** Support JSON type casting and validation functions.

**Functions to Implement:**
- `json_typeof(json)` - Return type of JSON value
- `jsonb_typeof(jsonb)` - Return type of JSONB value
- `json_strip_nulls(json)` - Remove object fields with null values
- `jsonb_strip_nulls(jsonb)` - Remove object fields with null values
- `json_pretty(json)` - Pretty-print JSON
- `jsonb_pretty(jsonb)` - Pretty-print JSONB
- `jsonb_set(target, path, new_value)` - Update value at path
- `jsonb_insert(target, path, new_value)` - Insert value at path

**Implementation Details:**
- Use serde_json for parsing and manipulation
- For `jsonb_set`/`jsonb_insert`, implement path navigation and modification

**Testing:**
- Test type detection for all JSON types
- Test pretty-printing output format
- Test path-based modifications

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/JSON.md`
- [ ] CHANGELOG.md updated

---

### Phase 1.6: Integration & Compatibility Suite Run

**Goal:** Run the full compatibility suite and verify JSON improvements.

**Tasks:**
1. Run `python3 postgres-compatibility-suite/runner_with_stats.py`
2. Compare results with baseline (json.sql: 38.5%, jsonb.sql: 58.5%)
3. Document improvements
4. Fix any remaining high-priority JSON failures

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in json.sql and jsonb.sql scores
- [ ] CHANGELOG.md updated with phase summary

---

## Phase 2: Interval Type & Functions (High Impact)

**Estimated Score Gain:** +3-4%  
**File Affected:** `interval.sql` (30.1% → 70%)

### Phase 2.1: Interval Input Parsing

**Goal:** Support PostgreSQL interval input formats.

**Input Formats to Support:**
- `'1.5 weeks'::interval`
- `'@ 1 minute'::interval` (at-style)
- `'1 day 2 hours 3 minutes 4 seconds'::interval`
- `'6 years'::interval`
- `'5 months'::interval`
- `'infinity'::interval` and `'-infinity'::interval`
- ISO 8601 format: `P1Y2M3DT4H5M6S`

**Implementation Details:**
- Create `src/interval.rs` module for interval handling
- Implement interval parsing function
- Store intervals in a normalized format (microseconds + months + days)
- Handle special values: infinity, -infinity

**Transpiler Changes:**
- Detect `::interval` casts
- Parse the string literal using interval parser
- Store as structured data (could use SQLite JSON or custom format)

**Testing:**
- Test all input format variations
- Test edge cases: empty strings, invalid formats
- Test round-trip (parse → format → parse)

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/INTERVAL.md` (create)
- [ ] CHANGELOG.md updated

---

### Phase 2.2: Interval Arithmetic Operators

**Goal:** Support interval arithmetic.

**Operators to Implement:**
- `interval + interval` → interval
- `interval - interval` → interval
- `interval * number` → interval
- `number * interval` → interval
- `interval / number` → interval
- `+ interval` → interval (unary plus)
- `- interval` → interval (unary minus)

**Implementation Details:**
- Implement as SQLite custom functions
- Handle component-wise arithmetic (months, days, microseconds)
- Handle overflow/underflow

**Functions to Create:**
- `interval_add(i1, i2)`
- `interval_sub(i1, i2)`
- `interval_mul(i, n)`
- `interval_div(i, n)`
- `interval_neg(i)`

**Transpiler Changes:**
- Map interval operators to these functions
- Detect interval types from context or casts

**Testing:**
- Test all operator combinations
- Test with different interval components
- Test edge cases: zero, infinity

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/INTERVAL.md`
- [ ] CHANGELOG.md updated

---

### Phase 2.3: Interval Comparison Operators

**Goal:** Support interval comparisons.

**Operators to Implement:**
- `interval = interval` → boolean
- `interval <> interval` → boolean
- `interval != interval` → boolean
- `interval < interval` → boolean
- `interval <= interval` → boolean
- `interval > interval` → boolean
- `interval >= interval` → boolean

**Implementation Details:**
- Normalize intervals before comparison
- Convert to common base (e.g., microseconds) for comparison
- Handle infinity values specially

**Functions to Create:**
- `interval_eq(i1, i2)`
- `interval_lt(i1, i2)`
- `interval_le(i1, i2)`
- `interval_gt(i1, i2)`
- `interval_ge(i1, i2)`

**Testing:**
- Test all comparison operators
- Test with different units (months vs days)
- Test boundary conditions

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/INTERVAL.md`
- [ ] CHANGELOG.md updated

---

### Phase 2.4: Interval Extraction Functions

**Goal:** Support EXTRACT from intervals and related functions.

**Functions to Implement:**
- `extract(EPOCH FROM interval)` → double precision
- `extract(CENTURY FROM interval)` → double precision
- `extract(DECADE FROM interval)` → double precision
- `extract(YEAR FROM interval)` → double precision
- `extract(MONTH FROM interval)` → double precision
- `extract(DAY FROM interval)` → double precision
- `extract(HOUR FROM interval)` → double precision
- `extract(MINUTE FROM interval)` → double precision
- `extract(SECOND FROM interval)` → double precision
- `extract(MILLISECOND FROM interval)` → double precision
- `extract(MICROSECOND FROM interval)` → double precision

**Implementation Details:**
- Parse interval structure
- Extract requested component
- Handle fractional values appropriately

**Transpiler Changes:**
- Update EXTRACT handling to support interval sources
- Map to appropriate extraction functions

**Testing:**
- Test extraction of all components
- Test with mixed-component intervals

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/INTERVAL.md`
- [ ] CHANGELOG.md updated

---

### Phase 2.5: Integration & Compatibility Suite Run

**Goal:** Run the full compatibility suite and verify interval improvements.

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in interval.sql score
- [ ] CHANGELOG.md updated with phase summary

---

## Phase 3: Boolean & Bitwise Aggregate Functions (High Impact)

**Estimated Score Gain:** +4-5%  
**Files Affected:** `aggregates.sql`, `float4.sql`, `float8.sql`

### Phase 3.1: Boolean Aggregate Functions

**Goal:** Implement boolean aggregate functions.

**Functions to Implement:**
- `bool_and(boolean)` - AND of all non-null values
- `bool_or(boolean)` - OR of all non-null values
- `every(boolean)` - Equivalent to bool_and (SQL standard)

**State Functions (for custom aggregates):**
- `booland_statefunc(boolean, boolean)`
- `boolor_statefunc(boolean, boolean)`

**Implementation Details:**
- Implement as aggregate functions using SQLite's aggregate API
- Initial state: NULL (or true for AND, false for OR)
- State transition: apply boolean operation
- Finalize: return accumulated result

**Code Location:**
- Add to `src/functions.rs` or create `src/aggregates.rs`
- Register in `src/handler/mod.rs`

**Testing:**
- Test with all-true, all-false, mixed values
- Test with NULL values
- Test with empty result set

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/FUNCTIONS.md` or `docs/AGGREGATES.md`
- [ ] CHANGELOG.md updated

---

### Phase 3.2: Bitwise Aggregate Functions

**Goal:** Implement bitwise aggregate functions.

**Functions to Implement:**
- `bit_and(integer)` - bitwise AND of all non-null values
- `bit_or(integer)` - bitwise OR of all non-null values
- `bit_xor(integer)` - bitwise XOR of all non-null values

**Implementation Details:**
- Implement as aggregate functions
- Support different integer sizes (int2, int4, int8)
- Handle NULL values appropriately

**Testing:**
- Test with various bit patterns
- Test with NULL values
- Test with single value, multiple values

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 3.3: Statistical Aggregate Functions

**Goal:** Implement internal aggregate functions used by PostgreSQL.

**Functions to Implement:**
- `float8_accum(real[], real)` - accumulate for statistical aggregates
- `float8_regr_accum(real[], real, real)` - accumulate for regression
- `float8_combine(real[], real[])` - combine accumulators
- `float8_regr_combine(real[], real[])` - combine regression accumulators

**Implementation Details:**
- These are internal functions used by statistical aggregates
- They operate on arrays representing running statistics
- Implement the algorithms from PostgreSQL source

**Testing:**
- Test with sample data
- Verify results match PostgreSQL

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 3.4: Integration & Compatibility Suite Run

**Goal:** Run the full compatibility suite and verify aggregate improvements.

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in aggregates.sql score
- [ ] CHANGELOG.md updated with phase summary

---

## Phase 4: INSERT Statement Improvements (Medium Impact)

**Estimated Score Gain:** +1-2%  
**File Affected:** `insert.sql` (57.8% → 75%)

### Phase 4.1: RETURNING Clause Enhancements

**Goal:** Fix remaining RETURNING clause issues.

**Issues to Address:**
- RETURNING with complex expressions
- RETURNING with aggregate functions
- RETURNING with subqueries
- RETURNING with column aliases

**Implementation Details:**
- Review current RETURNING implementation in `src/transpiler/dml.rs`
- Ensure proper handling of all expression types
- Fix any transpilation issues

**Testing:**
- Test complex RETURNING expressions
- Test with triggers that modify NEW rows

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 4.2: ON CONFLICT Enhancements

**Goal:** Fix remaining ON CONFLICT (upsert) issues.

**Issues to Address:**
- ON CONFLICT with multiple conflict targets
- ON CONFLICT with complex WHERE clauses
- ON CONFLICT DO UPDATE with subqueries
- ON CONFLICT with RETURNING

**Implementation Details:**
- Review current ON CONFLICT implementation
- Map to SQLite's ON CONFLICT/UPSERT correctly
- Handle PostgreSQL-specific syntax variations

**Testing:**
- Test various ON CONFLICT scenarios
- Test interaction with triggers

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 4.3: Multi-Row INSERT Improvements

**Goal:** Fix multi-row INSERT edge cases.

**Issues to Address:**
- Multi-row INSERT with different column orders
- Multi-row INSERT with DEFAULT values
- Multi-row INSERT with complex expressions

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 4.4: Integration & Compatibility Suite Run

**Goal:** Run the full compatibility suite and verify INSERT improvements.

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in insert.sql score
- [ ] CHANGELOG.md updated with phase summary

---

## Phase 5: CTE (WITH Clause) Enhancements (Medium Impact)

**Estimated Score Gain:** +1-2%  
**File Affected:** `with.sql` (52.5% → 75%)

### Phase 5.1: Recursive CTE Support

**Goal:** Support recursive CTEs (`WITH RECURSIVE`).

**Features to Support:**
- Basic recursive CTE: `WITH RECURSIVE t AS (base_query UNION ALL recursive_query)`
- Multiple recursive CTEs in one query
- Recursive CTEs with cycle detection

**Implementation Details:**
- SQLite supports recursive CTEs natively
- Ensure transpiler correctly passes through WITH RECURSIVE
- Handle PostgreSQL-specific syntax differences

**Transpiler Changes:**
- Update `src/transpiler/dml.rs` to handle RECURSIVE keyword
- Ensure proper handling of UNION/UNION ALL in CTEs

**Testing:**
- Test tree traversal queries
- Test graph traversal queries
- Test cycle detection

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated in `docs/CTE.md` (create)
- [ ] CHANGELOG.md updated

---

### Phase 5.2: Multiple CTEs Enhancement

**Goal:** Fix issues with multiple CTEs.

**Issues to Address:**
- Multiple CTEs referencing each other
- CTEs with column name lists
- CTEs in subqueries

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 5.3: Data-Modifying CTEs

**Goal:** Support data-modifying statements in CTEs.

**Features to Support:**
- `WITH t AS (INSERT ... RETURNING ...) SELECT ...`
- `WITH t AS (UPDATE ... RETURNING ...) SELECT ...`
- `WITH t AS (DELETE ... RETURNING ...) SELECT ...`

**Implementation Details:**
- SQLite supports this via `RETURNING` in CTEs (SQLite 3.35.0+)
- Ensure transpiler correctly structures these queries

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 5.4: Integration & Compatibility Suite Run

**Goal:** Run the full compatibility suite and verify CTE improvements.

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in with.sql score
- [ ] CHANGELOG.md updated with phase summary

---

## Phase 6: Float/Real Edge Cases (Lower Priority)

**Estimated Score Gain:** +2-3%  
**Files Affected:** `float4.sql` (34% → 60%), `float8.sql` (52.7% → 75%)

### Phase 6.1: Special Float Value Handling

**Goal:** Handle special float values correctly.

**Values to Support:**
- `'NaN'::float4` / `'NaN'::float8`
- `'infinity'::float4` / `'infinity'::float8`
- `'-infinity'::float4` / `'-infinity'::float8`

**Implementation Details:**
- SQLite uses IEEE 754 floats which support these values
- Ensure proper parsing and formatting
- Handle arithmetic with special values

**Testing:**
- Test arithmetic with special values
- Test comparisons with special values
- Test aggregation with special values

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 6.2: Float Input Validation

**Goal:** Match PostgreSQL's float input validation.

**Issues to Address:**
- `'xyz'::float4` should error
- `'5.0.0'::float4` should error
- `'     - 3.0'::float4` should error
- Empty string handling

**Implementation Details:**
- Add validation in type casting
- Match PostgreSQL error messages where possible

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 6.3: Integration & Compatibility Suite Run

**Goal:** Run the full compatibility suite and verify float improvements.

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Compatibility suite shows improvement in float4.sql and float8.sql scores
- [ ] CHANGELOG.md updated with phase summary

---

## Phase 7: Error Handling Alignment (Lower Priority)

**Estimated Score Gain:** +3-5% (but low user impact)

### Phase 7.1: Input Validation Improvements

**Goal:** Align input validation with PostgreSQL semantics.

**Areas to Address:**
- Invalid interval string rejection
- Invalid JSON rejection
- Out-of-range numeric rejection
- Type casting validation

**Implementation Details:**
- Add validation functions
- Return appropriate PostgreSQL error codes
- Match PostgreSQL error messages where practical

**Note:** This phase is lower priority because PGQT being more permissive doesn't break working queries.

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

---

### Phase 7.2: Final Compatibility Suite Run & Summary

**Goal:** Run the full compatibility suite and document final results.

**Tasks:**
1. Run `python3 postgres-compatibility-suite/runner_with_stats.py`
2. Compare with baseline (66.68%)
3. Document all improvements
4. Create summary report

**Verification Checklist:**
- [ ] `cargo build --release` succeeds with no errors
- [ ] `cargo clippy` shows no warnings
- [ ] `./run_tests.sh` passes all tests
- [ ] Final compatibility score documented
- [ ] CHANGELOG.md updated with final summary
- [ ] README.md updated with new compatibility percentage

---

## Implementation Strategy

### How to Use These Plans with Subagents

#### Option A: Serial Implementation (Recommended for First Phase)

**Best for:** Learning the patterns, minimizing merge conflicts, establishing foundations

1. **Start with Phase 1** (JSON/JSONB) as a pilot
2. **Use worktree isolation:**
   ```bash
   git worktree add .worktrees/phase1-json -b feature/phase1-json
   cd .worktrees/phase1-json
   ```
3. **Assign one subagent** to `plans/PHASE_1_JSON_JSONB.md`
4. **Review and merge** before starting Phase 2
5. **Repeat** for subsequent phases

**Pros:**
- Minimal merge conflicts
- Each phase builds on verified code
- Easier to review and debug
- Establishes patterns for later phases

**Cons:**
- Longer total timeline
- Phases 4-7 could potentially run in parallel after Phase 1-3 establish patterns

#### Option B: Grouped Parallel Implementation (Recommended After Phase 1)

**Best for:** Speed while managing conflict risk

**Group 1 (Independent, High Impact):**
- Phase 1: JSON/JSONB (do first, establishes patterns)

**Group 2 (Mostly Independent):**
- Phase 2: Interval (new module, minimal overlap)
- Phase 3: Aggregates (adds to existing aggregate infrastructure)

**Group 3 (Independent, Lower Risk):**
- Phase 4: INSERT (transpiler changes only)
- Phase 5: CTE (transpiler changes only)

**Group 4 (Polish):**
- Phase 6: Float (edge cases)
- Phase 7: Error Handling (validation)

**Execution:**
```bash
# After Phase 1 is merged to main:

# Group 2 (can run in parallel)
git worktree add .worktrees/phase2-interval -b feature/phase2-interval
git worktree add .worktrees/phase3-aggregates -b feature/phase3-aggregates

# Group 3 (can run in parallel after Group 2)
git worktree add .worktrees/phase4-insert -b feature/phase4-insert
git worktree add .worktrees/phase5-cte -b feature/phase5-cte
```

**Merge Order:**
1. Phase 1 → main
2. Phase 2 → main (rebase on main first)
3. Phase 3 → main (rebase on main first)
4. Phase 4 → main
5. Phase 5 → main
6. Phase 6 → main
7. Phase 7 → main

**Pros:**
- Faster overall completion
- Phases in different groups won't conflict
- Parallel work after patterns established

**Cons:**
- Need to manage rebase/merge order
- Some waiting between groups

#### Option C: Full Parallel (Advanced)

**Best for:** Maximum speed with experienced team

**Risk Assessment for Parallel Execution:**

| Phase | Conflict Risk | Overlapping Files | Can Parallelize With |
|-------|---------------|-------------------|---------------------|
| 1 | High | handler/mod.rs, transpiler/func.rs, transpiler/expr.rs | None (do first) |
| 2 | Low | New files only | 3, 4, 5, 6, 7 |
| 3 | Medium | handler/mod.rs, new aggregate files | 2, 4, 5, 6, 7 |
| 4 | Medium | transpiler/dml.rs | 2, 3, 5, 6, 7 |
| 5 | Medium | transpiler/dml.rs | 2, 3, 4, 6, 7 |
| 6 | Low | transpiler/expr.rs | 2, 3, 4, 5, 7 |
| 7 | Low | Various validation points | 2, 3, 4, 5, 6 |

**Full Parallel Execution:**
```bash
# Create all worktrees at once
git worktree add .worktrees/phase1-json -b feature/phase1-json
git worktree add .worktrees/phase2-interval -b feature/phase2-interval
git worktree add .worktrees/phase3-aggregates -b feature/phase3-aggregates
git worktree add .worktrees/phase4-insert -b feature/phase4-insert
git worktree add .worktrees/phase5-cte -b feature/phase5-cte
git worktree add .worktrees/phase6-float -b feature/phase6-float
git worktree add .worktrees/phase7-error -b feature/phase7-error

# Assign each to a subagent
# Each subagent works from their phase plan in plans/PHASE_X_*.md
```

**Merge Strategy:**
- Phase 1 merges first (foundational)
- All others rebase on main after Phase 1 merges
- Then merge in any order, rebasing as needed

**Pros:**
- Fastest completion
- Maximum parallelization

**Cons:**
- Highest conflict risk
- Requires careful merge management
- May need multiple rebase cycles

### Recommended Approach for This Project

**Start with Option A (Serial) for Phase 1, then Option B (Grouped Parallel):**

1. **Phase 1: JSON/JSONB** - Do serially (pilot phase)
   - Establishes patterns for function registration
   - Sets up transpiler conventions
   - Creates test patterns

2. **Groups 2-4** - Run in parallel groups
   - Group 2: Phases 2-3 (new modules)
   - Group 3: Phases 4-5 (transpiler DML)
   - Group 4: Phases 6-7 (edge cases)

### Subagent Assignment Template

When assigning to a subagent, provide:

```
Task: Implement Phase X: [Name]

Read these files in order:
1. plans/PHASE_X_[NAME].md - Detailed implementation plan
2. src/handler/mod.rs - See register_custom_functions() for examples
3. [Other relevant existing files]

Work in: .worktrees/phaseX-name/

Deliverables:
- [ ] All sub-phases complete with verification checklists
- [ ] All tests passing
- [ ] Documentation updated
- [ ] CHANGELOG.md updated

Merge requirements:
- Rebase on main before final review
- All CI checks pass
```

---

## Implementation Notes

### Code Organization

**New Files to Create:**
- `src/json.rs` - Additional JSON functions (if not adding to jsonb.rs)
- `src/interval.rs` - Interval type handling
- `src/aggregates.rs` - Aggregate functions (or add to existing)
- `docs/JSON.md` - JSON function documentation
- `docs/INTERVAL.md` - Interval type documentation
- `docs/CTE.md` - CTE documentation
- `tests/json_function_tests.rs` - JSON function integration tests
- `tests/interval_tests.rs` - Interval integration tests

**Files to Modify:**
- `src/handler/mod.rs` - Register new functions
- `src/transpiler/func.rs` - Function call handling
- `src/transpiler/expr.rs` - Operator handling
- `src/transpiler/dml.rs` - DML statement handling
- `src/lib.rs` - Export new modules (if needed)
- `CHANGELOG.md` - Document all changes

### Testing Strategy

1. **Unit Tests:** Add to source files using `#[cfg(test)]` modules
2. **Integration Tests:** Create new test files in `tests/`
3. **E2E Tests:** Add to existing or create new Python e2e tests
4. **Compatibility Suite:** Run after each phase

### Documentation Standards

1. **Code Documentation:** Use rustdoc comments (`///`)
2. **User Documentation:** Create/update markdown files in `docs/`
3. **CHANGELOG.md:** Follow existing format with sections: Added, Fixed, Performance
4. **README.md:** Update compatibility percentage and feature list

### Build Requirements

Every sub-phase MUST:
```bash
# 1. Build succeeds
cargo build --release

# 2. No warnings
cargo clippy --release

# 3. All tests pass
./run_tests.sh

# 4. Documentation complete
# (manual check)

# 5. CHANGELOG updated
# (manual check)
```

---

## Success Metrics

| Phase | Target Score Gain | Target File Improvements |
|-------|-------------------|-------------------------|
| Phase 1: JSON | +7-10% | json.sql: 38.5% → 80%, jsonb.sql: 58.5% → 85% |
| Phase 2: Interval | +3-4% | interval.sql: 30.1% → 70% |
| Phase 3: Aggregates | +4-5% | aggregates.sql: 79.9% → 90% |
| Phase 4: INSERT | +1-2% | insert.sql: 57.8% → 75% |
| Phase 5: CTE | +1-2% | with.sql: 52.5% → 75% |
| Phase 6: Float | +2-3% | float4.sql: 34% → 60%, float8.sql: 52.7% → 75% |
| Phase 7: Error Handling | +3-5% | Overall improvement |
| **TOTAL** | **+21-31%** | **66.68% → 85-95%** |

**Conservative Target:** 85%  
**Optimistic Target:** 90%

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| JSON operators complex to transpile | Medium | Implement as functions first, then map operators |
| Interval format parsing edge cases | Low | Use well-tested parsing libraries, extensive tests |
| Aggregate functions performance | Low | Profile and optimize if needed |
| CTE recursive queries | Medium | Leverage SQLite's native support |
| Breaking existing functionality | High | Comprehensive test suite, incremental changes |

---

## Timeline Estimate

| Phase | Estimated Duration | Cumulative |
|-------|-------------------|------------|
| Phase 1: JSON | 2-3 weeks | 2-3 weeks |
| Phase 2: Interval | 1-2 weeks | 3-5 weeks |
| Phase 3: Aggregates | 1 week | 4-6 weeks |
| Phase 4: INSERT | 3-4 days | 5-7 weeks |
| Phase 5: CTE | 3-4 days | 5-7 weeks |
| Phase 6: Float | 2-3 days | 6-8 weeks |
| Phase 7: Error Handling | 3-4 days | 7-9 weeks |

**Total Estimated Duration:** 7-9 weeks

---

*This plan was generated on 2026-03-18 and is based on compatibility test results from the same date.*
