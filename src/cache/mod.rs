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
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Check if the cache is empty.
    #[allow(dead_code)]
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