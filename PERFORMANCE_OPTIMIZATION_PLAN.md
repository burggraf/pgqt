# PGQT Performance Optimization Plan

## Overview

This plan addresses performance bottlenecks identified through benchmarking and profiling. The goal is to improve throughput by 2-5x for repeated queries while maintaining all existing functionality.

## Executive Summary

| Priority | Optimization | Effort | Impact | Risk |
|----------|-------------|--------|--------|------|
| 1 | Transpilation LRU Cache | Medium | High (2-3x) | Low |
| 2 | SQLite Prepared Statement Caching | Low | Medium (20-30%) | Low |
| 3 | Reduce robust_split() Calls | Medium | Medium (10-20%) | Low |
| 4 | BETWEEN String Optimization | Very Low | Low (~5%) | Very Low |

**Estimated Total Improvement**: 2.5-4x for OLTP workloads with repeated queries.

---

## Verification Process (Required After Each Step)

After completing each implementation step in this plan, you **MUST** perform the following verification:

### Step Verification Checklist

1. **Compile Check**
   ```bash
   cargo check
   ```
   - Ensure the code compiles without errors
   - Note any warnings for the next step

2. **Fix Warnings**
   ```bash
   cargo check 2>&1 | grep -i warning
   ```
   - Fix all compiler warnings before proceeding
   - Common warnings to address:
     - Unused imports
     - Unused variables (prefix with `_` if intentional)
     - Dead code
     - Missing documentation

3. **Run Full Test Suite**
   ```bash
   ./run_tests.sh
   ```
   - All unit tests must pass
   - All integration tests must pass
   - All E2E tests must pass
   - If any tests fail, fix before proceeding

4. **Update Documentation**
   - Update `README.md` if new CLI flags are added
   - Update `AGENTS.md` if architecture changes
   - Update inline code documentation as needed
   - Add entries to CHANGELOG if one exists

### Quick Verification Command

Run this after each step to verify everything is working:

```bash
echo "=== Running cargo check ===" && \
cargo check 2>&1 | tail -20 && \
echo "" && \
echo "=== Checking for warnings ===" && \
cargo check 2>&1 | grep -c "warning:" && \
echo "" && \
echo "=== Running tests ===" && \
./run_tests.sh
```

---

## Phase 1: Transpilation LRU Cache

### Problem

Every query goes through the full transpilation pipeline:
```
Query → pg_query::parse() → transpile() → SQLite prepare() → execute()
```

For repeated queries (common in OLTP), we parse and transpile the same SQL pattern repeatedly.

### Solution

Add an LRU cache for transpiled query results keyed by the original SQL string.

### Implementation

#### 1.1 Add LRU Dependency

**File**: `Cargo.toml`

```toml
[dependencies]
# Add:
lru = "0.12"
```

---

**✓ VERIFICATION 1.1**:
```bash
cargo check
```
Expected: Compiles successfully (dependency downloaded)

---

#### 1.2 Create Cache Module

**File**: `src/cache/mod.rs` (new file)

```rust
//! Query transpilation caching for improved performance.
//!
//! Provides an LRU cache for transpiled queries to avoid repeated
//! parsing and transpilation of identical SQL statements.

use std::sync::Mutex;
use lru::LruCache;
use std::num::NonZeroUsize;
use crate::transpiler::TranspileResult;

/// Default cache size (number of unique queries to cache)
const DEFAULT_CACHE_SIZE: usize = 256;

/// Cache for transpiled query results.
///
/// Uses an LRU (Least Recently Used) eviction policy to bound memory usage.
/// Thread-safe via Mutex wrapping.
pub struct TranspileCache {
    cache: Mutex<LruCache<String, TranspileResult>>,
}

impl TranspileCache {
    /// Create a new transpile cache with the default size.
    pub fn new() -> Self {
        Self::with_size(DEFAULT_CACHE_SIZE)
    }

    /// Create a new transpile cache with a specific size.
    pub fn with_size(size: usize) -> Self {
        let cap = NonZeroUsize::new(size).unwrap_or(NonZeroUsize::new(64).unwrap());
        Self {
            cache: Mutex::new(LruCache::new(cap)),
        }
    }

    /// Get a cached transpile result if available.
    pub fn get(&self, sql: &str) -> Option<TranspileResult> {
        let mut cache = self.cache.lock().unwrap();
        cache.get(sql).cloned()
    }

    /// Put a transpile result into the cache.
    pub fn put(&self, sql: String, result: TranspileResult) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(sql, result);
    }

    /// Get or compute a transpile result.
    ///
    /// Returns the cached result if available, otherwise computes it using
    /// the provided closure and caches the result.
    pub fn get_or_compute<F>(&self, sql: &str, compute: F) -> TranspileResult
    where
        F: FnOnce() -> TranspileResult,
    {
        // Check cache first
        if let Some(cached) = self.get(sql) {
            return cached;
        }

        // Compute and cache
        let result = compute();
        self.put(sql.to_string(), result.clone());
        result
    }

    /// Clear the cache.
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Get the current number of cached entries.
    pub fn len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for TranspileCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transpiler::OperationType;

    fn make_result(sql: &str) -> TranspileResult {
        TranspileResult {
            sql: sql.to_lowercase(),
            create_table_metadata: None,
            copy_metadata: None,
            referenced_tables: Vec::new(),
            operation_type: OperationType::SELECT,
            errors: Vec::new(),
            column_aliases: Vec::new(),
            column_types: Vec::new(),
        }
    }

    #[test]
    fn test_cache_put_get() {
        let cache = TranspileCache::new();
        let result = make_result("SELECT 1");
        
        cache.put("SELECT 1".to_string(), result.clone());
        
        let cached = cache.get("SELECT 1");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().sql, "select 1");
    }

    #[test]
    fn test_cache_miss() {
        let cache = TranspileCache::new();
        assert!(cache.get("SELECT 1").is_none());
    }

    #[test]
    fn test_get_or_compute() {
        let cache = TranspileCache::new();
        let call_count = std::sync::atomic::AtomicUsize::new(0);
        
        // First call should compute
        let result1 = cache.get_or_compute("SELECT 1", || {
            call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            make_result("SELECT 1")
        });
        
        // Second call should use cache
        let result2 = cache.get_or_compute("SELECT 1", || {
            call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            make_result("SELECT 1")
        });
        
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(result1.sql, result2.sql);
    }

    #[test]
    fn test_cache_eviction() {
        let cache = TranspileCache::with_size(2);
        
        cache.put("a".to_string(), make_result("a"));
        cache.put("b".to_string(), make_result("b"));
        cache.put("c".to_string(), make_result("c")); // Should evict "a"
        
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }
}
```

**File**: `src/lib.rs` (add module declaration)

```rust
// Add near other module declarations:
pub mod cache;
```

---

**✓ VERIFICATION 1.2**:
```bash
# Compile check
cargo check

# Check for warnings
cargo check 2>&1 | grep -i warning

# Run unit tests for the cache module
cargo test cache::

# Run full test suite
./run_tests.sh
```
Expected: All tests pass, no warnings

---

#### 1.3 Integrate Cache into SqliteHandler

**File**: `src/handler/mod.rs`

```rust
// Add import at top
use crate::cache::TranspileCache;

// Add field to SqliteHandler struct (around line 188)
pub struct SqliteHandler {
    // ... existing fields ...
    
    /// LRU cache for transpiled queries
    pub transpile_cache: Arc<TranspileCache>,
}

// Initialize in SqliteHandler::new() (around line 207)
impl SqliteHandler {
    pub fn new(db_path: &str) -> Result<Self> {
        // ... existing initialization ...
        
        let transpile_cache = Arc::new(TranspileCache::new());
        
        Ok(Self {
            // ... existing fields ...
            transpile_cache,
        })
    }
}
```

**File**: `src/handler/utils.rs`

```rust
// Add accessor to HandlerUtils trait
pub trait HandlerUtils: Clone {
    // ... existing methods ...
    
    /// Get the transpilation cache
    fn transpile_cache(&self) -> &Arc<TranspileCache>;
}
```

**File**: `src/handler/mod.rs` (implement the trait method)

```rust
impl HandlerUtils for SqliteHandler {
    // ... existing implementations ...
    
    fn transpile_cache(&self) -> &Arc<TranspileCache> {
        &self.transpile_cache
    }
}
```

---

**✓ VERIFICATION 1.3**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 1.4 Use Cache in Query Execution

**File**: `src/handler/query.rs`

```rust
// In execute_single_query_params, replace the transpile call:

// OLD:
let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
ctx.set_metadata_provider(self.as_metadata_provider());
let transpile_result = crate::transpiler::transpile_with_context(sql, &mut ctx);

// NEW:
let transpile_result = self.transpile_cache().get_or_compute(sql, || {
    let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
    ctx.set_metadata_provider(self.as_metadata_provider());
    crate::transpiler::transpile_with_context(sql, &mut ctx)
});
```

---

**✓ VERIFICATION 1.4**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 1.5 Handle Cache Invalidation

Some events should invalidate the cache:
- DDL operations (CREATE, ALTER, DROP)
- Schema changes
- Function creation/dropping

**File**: `src/handler/query.rs`

```rust
// In execute_transpiled_stmt_params, after successful DDL:
fn execute_transpiled_stmt_params(...) -> Result<Vec<Response>> {
    // ... existing code ...
    
    // Check if this was a DDL operation that might affect future transpilations
    if transpile_result.operation_type == OperationType::DDL {
        self.transpile_cache().clear();
    }
    
    // ... rest of function ...
}
```

---

**✓ VERIFICATION 1.5**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 1.6 Add Configuration Option

**File**: `src/main.rs`

```rust
// Add CLI argument for cache size
#[derive(Parser)]
struct Args {
    // ... existing args ...
    
    /// Size of the transpilation cache (0 to disable caching)
    #[arg(long, default_value = "256")]
    transpile_cache_size: usize,
}

// In SqliteHandler, add constructor that accepts cache size:
impl SqliteHandler {
    pub fn with_cache_size(db_path: &str, cache_size: usize) -> Result<Self> {
        // ... existing initialization ...
        
        let transpile_cache = Arc::new(
            if cache_size > 0 {
                TranspileCache::with_size(cache_size)
            } else {
                TranspileCache::with_size(1) // Minimal cache even when "disabled"
            }
        );
        
        // ...
    }
}

// In main(), use the new constructor:
let handler = SqliteHandler::with_cache_size(&args.database, args.transpile_cache_size)?;
```

---

**✓ VERIFICATION 1.6**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh

# Test the new CLI flag
cargo run -- --help | grep transpile-cache-size
```
Expected: Compiles, no warnings, all tests pass, CLI flag appears in help

---

#### 1.7 Update Documentation

**File**: `README.md`

Add documentation for the new CLI flag:

```markdown
### Performance Options

- `--transpile-cache-size <SIZE>`: Size of the transpilation LRU cache (default: 256).
  Set to 0 to disable caching. Larger caches improve performance for workloads
  with many repeated queries but use more memory.
```

**File**: `AGENTS.md`

Update if needed to document the cache architecture.

---

**✓ VERIFICATION 1.7**:
```bash
# Verify documentation was updated
grep -n "transpile-cache-size" README.md

# Final verification for Phase 1
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Documentation updated, all checks pass

---

### Phase 1 Complete Checklist

- [ ] 1.1 LRU dependency added to Cargo.toml
- [ ] 1.2 Cache module created with tests
- [ ] 1.3 Cache integrated into SqliteHandler
- [ ] 1.4 Cache used in query execution
- [ ] 1.5 Cache invalidation on DDL implemented
- [ ] 1.6 CLI configuration option added
- [ ] 1.7 Documentation updated
- [ ] All tests pass (`./run_tests.sh`)
- [ ] No compiler warnings (`cargo check`)
- [ ] Manual smoke test: connect with psql and run queries

---

## Phase 2: SQLite Prepared Statement Caching

### Problem

Every query does `conn.prepare(&sql)` which causes SQLite to:
1. Parse the SQL
2. Generate bytecode
3. Optimize the query plan

For parameterized queries, we re-prepare the same statement structure repeatedly.

### Solution

Use `rusqlite`'s built-in prepared statement caching via `prepare_cached()`.

### Implementation

#### 2.1 Find All prepare() Calls

First, identify all locations that need updating:

```bash
rg "conn\.prepare\(" src/ --type rust -n
```

Document all occurrences for systematic replacement.

---

**✓ VERIFICATION 2.1**:
```bash
# Count occurrences (save for comparison)
rg "conn\.prepare\(" src/ --type rust -c
```
Expected: List of files with counts

---

#### 2.2 Replace prepare() with prepare_cached()

For each occurrence found in step 2.1, replace:

```rust
// OLD:
let stmt = conn.prepare(&sql)?;

// NEW:
let stmt = conn.prepare_cached(&sql)?;
```

**Files to modify** (expected):
- `src/handler/query.rs`
- `src/handler/utils.rs`
- `src/catalog/mod.rs`
- `src/catalog/table.rs`
- `src/catalog/function.rs`
- `src/catalog/rls.rs`
- Other catalog files as needed

---

**✓ VERIFICATION 2.2**:
After replacing each file:
```bash
cargo check
cargo check 2>&1 | grep -i warning
```
Expected: Compiles without errors after each file

After all replacements:
```bash
# Verify no more direct prepare() calls (should only find prepare_cached)
rg "conn\.prepare\(" src/ --type rust
```
Expected: Only `prepare_cached` calls remain

---

#### 2.3 Configure Cache Size

**File**: `src/handler/mod.rs`

```rust
impl SqliteHandler {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        
        // Configure SQLite prepared statement cache (default is 16, increase to 64)
        conn.set_prepared_statement_cache_capacity(64);
        
        // ... rest of initialization ...
    }
}
```

---

**✓ VERIFICATION 2.3**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 2.4 Handle Schema Changes

When schema changes, cached statements may become invalid. Add cache clearing after DDL:

**File**: `src/handler/query.rs`

```rust
// After DDL operations, clear the SQLite statement cache
fn execute_transpiled_stmt_params(...) -> Result<Vec<Response>> {
    // ... existing DDL detection ...
    
    if is_ddl_operation {
        // Clear transpilation cache (from Phase 1)
        self.transpile_cache().clear();
        
        // Clear SQLite prepared statement cache
        let conn = self.conn.lock().unwrap();
        conn.clear_prepared_statement_cache();
    }
    
    // ... rest of function ...
}
```

---

**✓ VERIFICATION 2.4**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 2.5 Update Documentation

**File**: `README.md`

```markdown
### Performance Tuning

PGQT uses two levels of caching for optimal performance:

1. **Transpilation Cache**: Caches the result of PostgreSQL to SQLite transpilation.
   Configure with `--transpile-cache-size` (default: 256 entries).

2. **SQLite Prepared Statement Cache**: Caches compiled SQLite statements.
   Currently fixed at 64 entries (will be configurable in a future release).

Both caches are automatically cleared when DDL operations (CREATE, ALTER, DROP) are executed.
```

---

**✓ VERIFICATION 2.5**:
```bash
grep -n "Prepared Statement Cache" README.md
```
Expected: Documentation updated

---

### Phase 2 Complete Checklist

- [ ] 2.1 All `prepare()` calls identified
- [ ] 2.2 All `prepare()` calls replaced with `prepare_cached()`
- [ ] 2.3 SQLite cache size configured (64)
- [ ] 2.4 Cache cleared on DDL operations
- [ ] 2.5 Documentation updated
- [ ] All tests pass (`./run_tests.sh`)
- [ ] No compiler warnings (`cargo check`)
- [ ] Manual smoke test: run batch queries and verify performance improvement

---

## Phase 3: Reduce robust_split() Calls

### Problem

`robust_split()` is called multiple times per query:
1. In `execute_query_params()` → splits original SQL
2. In `execute_single_query_params()` → splits again on error path
3. After transpilation → splits transpiled SQL
4. In error handling → splits again

Each call does `sql.to_uppercase()` and potentially `pg_query::split_with_scanner()`.

### Solution

1. Add fast-path check for single statements
2. Cache split results
3. Avoid unnecessary `to_uppercase()` calls

### Implementation

#### 3.1 Add Fast-Path Single Statement Check

**File**: `src/handler/query.rs`

Add before the `robust_split` function:

```rust
/// Fast check if SQL is likely a single statement.
/// Returns true if there's no semicolon or only trailing semicolons.
fn is_likely_single_statement(sql: &str) -> bool {
    let trimmed = sql.trim();
    if !trimmed.contains(';') {
        return true;
    }
    
    // Check if semicolon is only at the end (possibly multiple trailing semicolons)
    let without_trailing = trimmed.trim_end_matches(';').trim();
    !without_trailing.contains(';')
}

/// Result of splitting SQL statements.
#[derive(Clone)]
struct SplitResult {
    statements: Vec<String>,
    is_single: bool,
}

/// Split SQL into statements, with fast path for single statements.
fn split_sql(sql: &str) -> SplitResult {
    if is_likely_single_statement(sql) {
        // Fast path: single statement, no need for expensive splitting
        return SplitResult {
            statements: vec![sql.trim().trim_end_matches(';').to_string()],
            is_single: true,
        };
    }
    
    // Slow path: multiple statements, use robust_split
    let statements = robust_split(sql);
    SplitResult {
        statements,
        is_single: false,
    }
}
```

---

**✓ VERIFICATION 3.1**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
cargo test is_likely_single_statement
```
Expected: Compiles, no warnings, unit test passes (add test if needed)

---

#### 3.2 Optimize robust_split()

**File**: `src/handler/query.rs`

Update `robust_split()` to use the fast path:

```rust
fn robust_split(sql: &str) -> Vec<String> {
    // OPTIMIZATION: Fast path for single statements
    if is_likely_single_statement(sql) {
        return vec![sql.trim().trim_end_matches(';').to_string()];
    }

    // ... rest of existing implementation for multi-statement SQL ...
}
```

---

**✓ VERIFICATION 3.2**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 3.3 Refactor Call Sites to Use split_sql()

**File**: `src/handler/query.rs`

Replace `robust_split()` calls with `split_sql()` where the single-statement fast path helps:

```rust
// In execute_query_params:
fn execute_query_params(&self, client_id: u32, sql: &str, params: &[Option<String>]) -> Result<Vec<Response>> {
    let split = split_sql(sql);
    
    if split.is_single {
        // Fast path: single statement, skip iteration overhead
        return self.execute_single_query_params(client_id, &split.statements[0], params);
    }
    
    // Multiple statements
    let mut all_responses = Vec::new();
    for stmt in split.statements {
        let responses = self.execute_single_query_params(client_id, &stmt, params)?;
        all_responses.extend(responses);
    }
    Ok(all_responses)
}
```

Do similar updates for other call sites where appropriate.

---

**✓ VERIFICATION 3.3**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 3.4 Add Unit Tests for Edge Cases

**File**: `src/handler/query.rs`

Add tests at the bottom of the file:

```rust
#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn test_single_statement_no_semicolon() {
        assert!(is_likely_single_statement("SELECT 1"));
        assert!(is_likely_single_statement("  SELECT 1  "));
    }

    #[test]
    fn test_single_statement_trailing_semicolon() {
        assert!(is_likely_single_statement("SELECT 1;"));
        assert!(is_likely_single_statement("SELECT 1;;"));
        assert!(is_likely_single_statement("  SELECT 1;  "));
    }

    #[test]
    fn test_multiple_statements() {
        assert!(!is_likely_single_statement("SELECT 1; SELECT 2"));
        assert!(!is_likely_single_statement("SELECT 1; SELECT 2;"));
    }

    #[test]
    fn test_semicolon_in_string() {
        // Semicolon inside string literal - should be detected as potentially multi
        // (robust_split will handle this correctly)
        assert!(!is_likely_single_statement("SELECT 'a;b'"));
    }

    #[test]
    fn test_split_sql_single() {
        let result = split_sql("SELECT 1");
        assert!(result.is_single);
        assert_eq!(result.statements, vec!["SELECT 1"]);
    }

    #[test]
    fn test_split_sql_multiple() {
        let result = split_sql("SELECT 1; SELECT 2");
        assert!(!result.is_single);
        assert_eq!(result.statements.len(), 2);
    }
}
```

---

**✓ VERIFICATION 3.4**:
```bash
cargo test split_tests
./run_tests.sh
```
Expected: New tests pass, all existing tests pass

---

#### 3.5 Update Documentation

**File**: `AGENTS.md`

Add note about the optimization:

```markdown
### Query Splitting Optimization

The `split_sql()` function provides a fast path for single-statement queries,
avoiding the overhead of `pg_query::split_with_scanner()` for the common case.
The `is_likely_single_statement()` check uses a simple semicolon detection that
handles most cases correctly; complex cases fall back to `robust_split()`.
```

---

**✓ VERIFICATION 3.5**:
```bash
grep -n "Query Splitting" AGENTS.md
```
Expected: Documentation updated

---

### Phase 3 Complete Checklist

- [ ] 3.1 Fast-path check added
- [ ] 3.2 `robust_split()` optimized
- [ ] 3.3 Call sites refactored
- [ ] 3.4 Unit tests added
- [ ] 3.5 Documentation updated
- [ ] All tests pass (`./run_tests.sh`)
- [ ] No compiler warnings (`cargo check`)

---

## Phase 4: BETWEEN String Optimization

### Problem

BETWEEN transpilation does inefficient string operations:

```rust
let bounds = rexpr_sql.replace(", ", " AND ").replace(",", " AND ");
```

Two `replace()` calls and a `format!()` per BETWEEN clause.

### Solution

Simplify to a single replace operation.

### Implementation

#### 4.1 Optimize BETWEEN Handling

**File**: `src/transpiler/expr/operators.rs`

Locate the BETWEEN handling code (around line 134) and update:

```rust
pg_query::protobuf::AExprKind::AexprBetween => {
    // PostgreSQL allows BETWEEN x, y syntax (with comma)
    // SQLite requires BETWEEN x AND y
    // Single replace handles both ", " and "," cases
    let bounds = rexpr_sql.replace(',', " AND ");
    return format!("{} BETWEEN {}", lexpr_sql, bounds);
}

pg_query::protobuf::AExprKind::AexprNotBetween => {
    let bounds = rexpr_sql.replace(',', " AND ");
    return format!("{} NOT BETWEEN {}", lexpr_sql, bounds);
}

pg_query::protobuf::AExprKind::AexprBetweenSym => {
    // Symmetric BETWEEN - treat as regular BETWEEN for now
    let bounds = rexpr_sql.replace(',', " AND ");
    return format!("{} BETWEEN {}", lexpr_sql, bounds);
}

pg_query::protobuf::AExprKind::AexprNotBetweenSym => {
    let bounds = rexpr_sql.replace(',', " AND ");
    return format!("{} NOT BETWEEN {}", lexpr_sql, bounds);
}
```

---

**✓ VERIFICATION 4.1**:
```bash
cargo check
cargo check 2>&1 | grep -i warning

# Run BETWEEN-related tests
cargo test between
./run_tests.sh
```
Expected: Compiles, no warnings, all BETWEEN tests pass

---

#### 4.2 Add Unit Tests for BETWEEN Edge Cases

**File**: `src/transpiler/expr/operators.rs` (or appropriate test file)

```rust
#[cfg(test)]
mod between_tests {
    use super::*;

    #[test]
    fn test_between_with_space() {
        // BETWEEN 1, 2 -> BETWEEN 1 AND 2
        let result = "1, 2".replace(',', " AND ");
        assert_eq!(result, "1 AND  2"); // Note: extra space is harmless
    }

    #[test]
    fn test_between_without_space() {
        // BETWEEN 1,2 -> BETWEEN 1 AND 2
        let result = "1,2".replace(',', " AND ");
        assert_eq!(result, "1 AND 2");
    }

    #[test]
    fn test_between_mixed() {
        // Mixed formatting
        let result = "1, 2,3".replace(',', " AND ");
        assert!(result.contains("AND"));
    }
}
```

---

**✓ VERIFICATION 4.2**:
```bash
cargo test between_tests
./run_tests.sh
```
Expected: New tests pass, all tests pass

---

#### 4.3 Update Documentation

**File**: `src/transpiler/expr/operators.rs`

Add/update comment explaining the optimization:

```rust
// BETWEEN operator transpilation:
// PostgreSQL allows: expr BETWEEN low, high
// SQLite requires: expr BETWEEN low AND high
// A single replace(',', " AND ") handles both:
//   "1, 2"  -> "1 AND  2" (extra space harmless)
//   "1,2"   -> "1 AND 2"
// This is more efficient than the previous double replace approach.
```

---

**✓ VERIFICATION 4.3**:
```bash
cargo check
./run_tests.sh
```
Expected: All tests pass

---

### Phase 4 Complete Checklist

- [ ] 4.1 BETWEEN handling optimized
- [ ] 4.2 Unit tests added
- [ ] 4.3 Documentation updated
- [ ] All tests pass (`./run_tests.sh`)
- [ ] No compiler warnings (`cargo check`)

---

## Phase 5: Add Metrics and Observability

### Goal

Add instrumentation to measure cache effectiveness and identify future optimization opportunities.

### Implementation

#### 5.1 Add Metrics Module

**File**: `src/metrics.rs` (new file)

```rust
//! Performance metrics for PGQT.
//!
//! Provides thread-safe counters for monitoring cache effectiveness
//! and identifying performance bottlenecks.

use std::sync::atomic::{AtomicU64, Ordering};

/// Performance metrics for the handler.
pub struct Metrics {
    /// Total queries processed
    pub queries_total: AtomicU64,
    
    /// Transpilation cache hits
    pub transpile_cache_hits: AtomicU64,
    
    /// Transpilation cache misses
    pub transpile_cache_misses: AtomicU64,
    
    /// SQLite prepared statement cache hits
    pub stmt_cache_hits: AtomicU64,
    
    /// SQLite prepared statement cache misses
    pub stmt_cache_misses: AtomicU64,
    
    /// Queries that required robust_split
    pub multi_statement_queries: AtomicU64,
    
    /// Total transpilation time (microseconds)
    pub transpile_time_us: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            queries_total: AtomicU64::new(0),
            transpile_cache_hits: AtomicU64::new(0),
            transpile_cache_misses: AtomicU64::new(0),
            stmt_cache_hits: AtomicU64::new(0),
            stmt_cache_misses: AtomicU64::new(0),
            multi_statement_queries: AtomicU64::new(0),
            transpile_time_us: AtomicU64::new(0),
        }
    }
    
    pub fn record_query(&self) {
        self.queries_total.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_transpile_cache_hit(&self) {
        self.transpile_cache_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_transpile_cache_miss(&self) {
        self.transpile_cache_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_multi_statement(&self) {
        self.multi_statement_queries.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_transpile_time(&self, us: u64) {
        self.transpile_time_us.fetch_add(us, Ordering::Relaxed);
    }
    
    /// Calculate cache hit rate as a percentage
    pub fn transpile_cache_hit_rate(&self) -> f64 {
        let hits = self.transpile_cache_hits.load(Ordering::Relaxed);
        let misses = self.transpile_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            (hits as f64 / total as f64) * 100.0
        }
    }
    
    /// Calculate average transpile time in microseconds
    pub fn avg_transpile_time_us(&self) -> f64 {
        let total = self.transpile_time_us.load(Ordering::Relaxed);
        let misses = self.transpile_cache_misses.load(Ordering::Relaxed);
        if misses == 0 {
            0.0
        } else {
            total as f64 / misses as f64
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
```

**File**: `src/lib.rs`

```rust
// Add module declaration:
pub mod metrics;
```

---

**✓ VERIFICATION 5.1**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
cargo test metrics
```
Expected: Compiles, no warnings

---

#### 5.2 Integrate Metrics into Handler

**File**: `src/handler/mod.rs`

```rust
use crate::metrics::Metrics;

pub struct SqliteHandler {
    // ... existing fields ...
    
    /// Performance metrics
    pub metrics: Arc<Metrics>,
}

impl SqliteHandler {
    pub fn new(db_path: &str) -> Result<Self> {
        // ... existing initialization ...
        
        let metrics = Arc::new(Metrics::new());
        
        Ok(Self {
            // ... existing fields ...
            metrics,
        })
    }
}
```

---

**✓ VERIFICATION 5.2**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 5.3 Add Metrics Recording in Cache

**File**: `src/cache/mod.rs`

```rust
use crate::metrics::Metrics;

pub struct TranspileCache {
    cache: Mutex<LruCache<String, TranspileResult>>,
    metrics: Option<Arc<Metrics>>,
}

impl TranspileCache {
    pub fn new() -> Self {
        Self::with_metrics(None)
    }
    
    pub fn with_metrics(metrics: Option<Arc<Metrics>>) -> Self {
        let cap = NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap();
        Self {
            cache: Mutex::new(LruCache::new(cap)),
            metrics,
        }
    }
    
    pub fn get(&self, sql: &str) -> Option<TranspileResult> {
        let mut cache = self.cache.lock().unwrap();
        let result = cache.get(sql).cloned();
        
        // Record hit/miss
        if let Some(ref metrics) = self.metrics {
            if result.is_some() {
                metrics.record_transpile_cache_hit();
            } else {
                metrics.record_transpile_cache_miss();
            }
        }
        
        result
    }
    
    // ... rest of implementation ...
}
```

---

**✓ VERIFICATION 5.3**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 5.4 Add Metrics Display Method

**File**: `src/handler/mod.rs`

```rust
impl SqliteHandler {
    /// Format metrics for logging/display
    pub fn format_metrics(&self) -> String {
        let m = &self.metrics;
        format!(
            "Queries: {}, Transpile cache: {:.1}% hit ({}/{}), Avg transpile: {:.2}us, Multi-stmt: {}",
            m.queries_total.load(Ordering::Relaxed),
            m.transpile_cache_hit_rate(),
            m.transpile_cache_hits.load(Ordering::Relaxed),
            m.transpile_cache_hits.load(Ordering::Relaxed) + m.transpile_cache_misses.load(Ordering::Relaxed),
            m.avg_transpile_time_us(),
            m.multi_statement_queries.load(Ordering::Relaxed)
        )
    }
}
```

---

**✓ VERIFICATION 5.4**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh
```
Expected: Compiles, no warnings, all tests pass

---

#### 5.5 Add CLI Flag for Metrics Logging

**File**: `src/main.rs`

```rust
#[derive(Parser)]
struct Args {
    // ... existing args ...
    
    /// Interval in seconds for logging metrics (0 to disable)
    #[arg(long, default_value = "0")]
    metrics_interval_secs: u64,
}

// In main(), add periodic metrics logging:
if args.metrics_interval_secs > 0 {
    let handler_clone = handler.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(args.metrics_interval_secs)
        );
        loop {
            interval.tick().await;
            log::info!("{}", handler_clone.format_metrics());
        }
    });
}
```

---

**✓ VERIFICATION 5.5**:
```bash
cargo check
cargo check 2>&1 | grep -i warning
./run_tests.sh

# Test the CLI flag
cargo run -- --help | grep metrics-interval
```
Expected: Compiles, no warnings, all tests pass, CLI flag appears

---

#### 5.6 Update Documentation

**File**: `README.md`

```markdown
### Monitoring

PGQT can log performance metrics at regular intervals:

```bash
pgqt --metrics-interval-secs 60  # Log metrics every 60 seconds
```

Metrics include:
- Total queries processed
- Transpilation cache hit rate
- Average transpilation time
- Multi-statement query count
```

**File**: `AGENTS.md`

```markdown
### Performance Metrics

The `Metrics` struct in `src/metrics.rs` tracks:
- Query counts
- Cache hit rates
- Transpilation times

Access via `handler.format_metrics()` or enable periodic logging with
`--metrics-interval-secs`.
```

---

**✓ VERIFICATION 5.6**:
```bash
grep -n "metrics-interval" README.md
grep -n "Performance Metrics" AGENTS.md
```
Expected: Documentation updated

---

### Phase 5 Complete Checklist

- [ ] 5.1 Metrics module created
- [ ] 5.2 Metrics integrated into handler
- [ ] 5.3 Cache records metrics
- [ ] 5.4 Display method added
- [ ] 5.5 CLI flag added
- [ ] 5.6 Documentation updated
- [ ] All tests pass (`./run_tests.sh`)
- [ ] No compiler warnings (`cargo check`)

---

## Final Verification

After completing all phases, run the comprehensive verification:

### Complete Verification Checklist

```bash
# 1. Clean build
cargo clean
cargo build --release

# 2. Check for any warnings
cargo check 2>&1 | grep -i warning
# Expected: No warnings

# 3. Run full test suite
./run_tests.sh
# Expected: All tests pass

# 4. Run benchmarks to verify performance improvement
python3 simple_benchmark.py --host 127.0.0.1 --port 5436 --iterations 1000

# 5. Test new CLI flags
./target/release/pgqt --help | grep -E "(transpile-cache|metrics)"

# 6. Manual smoke test
./target/release/pgqt --port 5436 --database /tmp/test.db --transpile-cache-size 512 --metrics-interval-secs 30
# Connect with psql and run some queries
psql -h 127.0.0.1 -p 5436 -U postgres
```

### Success Criteria

| Metric | Target | How to Verify |
|--------|--------|---------------|
| All tests pass | 100% | `./run_tests.sh` |
| Compiler warnings | 0 | `cargo check 2>&1 | grep -c warning` |
| Cache hit rate (repeated queries) | >80% | `--metrics-interval-secs` logging |
| Ops/sec improvement | 2-5x | Compare benchmark before/after |
| No regressions | 0 new bugs | Manual testing + test suite |

---

## Rollback Plan

Each phase should be implemented on a separate git branch:

```bash
git checkout -b perf/phase-1-transpile-cache
# Implement Phase 1
# Verify all tests pass
git commit -m "Phase 1: Add transpilation LRU cache"
git merge main

git checkout -b perf/phase-2-sqlite-cache
# Implement Phase 2
# Verify all tests pass
git commit -m "Phase 2: Use SQLite prepared statement caching"
git merge main

# ... and so on for each phase
```

If any phase causes issues:

```bash
git revert HEAD  # Revert the problematic phase
git checkout main
```

---

## Estimated Timeline

| Phase | Effort | Duration | Cumulative |
|-------|--------|----------|------------|
| Phase 1 | Medium | 1-2 days | 1-2 days |
| Phase 2 | Low | 2-4 hours | 1.5-2.5 days |
| Phase 3 | Medium | 4-8 hours | 2-3 days |
| Phase 4 | Very Low | 30 min | 2-3 days |
| Phase 5 | Low | 2-4 hours | 2.5-3.5 days |

**Total**: 2.5-3.5 days including testing and documentation

---

## Future Considerations

After implementing these optimizations:

1. **Query plan caching**: Cache SQLite query plans for parameterized queries
2. **Connection pooling improvements**: Optimize connection reuse
3. **Async transpilation**: Move transpilation to async tasks for better concurrency
4. **WAL mode optimization**: Tune SQLite WAL settings for better write performance
5. **Batch operation optimization**: Improve executemany() performance
6. **Configurable cache sizes**: Make all cache sizes configurable via CLI
7. **Metrics export**: Add Prometheus/OpenMetrics endpoint for monitoring

---

## References

- [rusqlite::CachedStatement documentation](https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.prepare_cached)
- [LRU crate documentation](https://docs.rs/lru/latest/lru/)
- [SQLite performance tuning](https://www.sqlite.org/optoverview.html)
- [pg_query documentation](https://docs.rs/pg_query/latest/pg_query/)
- [Rust atomic ordering](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html)