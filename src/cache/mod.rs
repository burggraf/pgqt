use std::sync::Mutex;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};
use crate::transpiler::TranspileResult;

#[cfg(feature = "metrics")]
use crate::metrics::get_metrics;

/// Default cache size (number of unique queries to cache)
const DEFAULT_CACHE_SIZE: usize = 256;

/// Cache entry with optional expiration time
struct CacheEntry<T> {
    value: T,
    expires_at: Option<Instant>,
}

impl<T> CacheEntry<T> {
    fn new(value: T, ttl: Option<Duration>) -> Self {
        Self {
            value,
            expires_at: ttl.map(|d| Instant::now() + d),
        }
    }

    fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |exp| Instant::now() > exp)
    }
}

/// Cache for transpiled query results with optional TTL support.
///
/// Uses an LRU (Least Recently Used) eviction policy to bound memory usage.
/// Thread-safe via Mutex wrapping.
pub struct TranspileCache {
    cache: Mutex<LruCache<String, CacheEntry<TranspileResult>>>,
    ttl: Option<Duration>,
}

impl TranspileCache {
    /// Create a new transpile cache with the default size and no TTL.
    pub fn new() -> Self {
        Self::with_size_and_ttl(DEFAULT_CACHE_SIZE, None)
    }

    /// Create a new transpile cache with a specific size.
    #[cfg(test)]
    pub fn with_size(size: usize) -> Self {
        Self::with_size_and_ttl(size, None)
    }

    /// Create a new transpile cache with a specific size and TTL.
    ///
    /// # Arguments
    /// * `size` - Maximum number of entries in the cache
    /// * `ttl` - Optional time-to-live duration for cache entries
    pub fn with_size_and_ttl(size: usize, ttl: Option<Duration>) -> Self {
        let cap = NonZeroUsize::new(size).unwrap_or(NonZeroUsize::new(64).unwrap());
        Self {
            cache: Mutex::new(LruCache::new(cap)),
            ttl,
        }
    }

    /// Create a new transpile cache from configuration.
    ///
    /// # Arguments
    /// * `size` - Maximum number of entries in the cache
    /// * `ttl_secs` - TTL in seconds (0 means no TTL)
    #[cfg(test)]
    pub fn from_config(size: usize, ttl_secs: u64) -> Self {
        let ttl = if ttl_secs > 0 {
            Some(Duration::from_secs(ttl_secs))
        } else {
            None
        };
        Self::with_size_and_ttl(size, ttl)
    }

    /// Get a cached transpile result if available and not expired.
    pub fn get(&self, sql: &str) -> Option<TranspileResult> {
        let mut cache = self.cache.lock().unwrap();
        let entry = cache.get(sql)?;

        if entry.is_expired() {
            // Remove expired entry
            cache.pop(sql);
            return None;
        }

        Some(entry.value.clone())
    }

    /// Put a transpile result into the cache.
    pub fn put(&self, sql: String, result: TranspileResult) {
        let mut cache = self.cache.lock().unwrap();
        let entry = CacheEntry::new(result, self.ttl);
        cache.put(sql, entry);
    }

    /// Get or compute a transpile result.
    ///
    /// Returns the cached result if available, otherwise computes it using
    /// the provided closure and caches the result.
    pub fn get_or_compute<F>(&self, sql: &str, compute: F) -> TranspileResult
    where
        F: FnOnce() -> TranspileResult,
    {
        if let Some(cached) = self.get(sql) {
            #[cfg(feature = "metrics")]
            if let Some(metrics) = get_metrics() {
                metrics.transpile_cache_hits.inc();
            }
            return cached;
        }

        #[cfg(feature = "metrics")]
        if let Some(metrics) = get_metrics() {
            metrics.transpile_cache_misses.inc();
        }

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

    /// Get the configured TTL.
    #[allow(dead_code)]
    pub fn ttl(&self) -> Option<Duration> {
        self.ttl
    }
}

impl Default for TranspileCache {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export query result cache
mod query_result;
// QueryResultCache is available for future use when query result caching is implemented
#[cfg(test)]
pub use query_result::QueryResultCache;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transpiler::OperationType;
    use std::thread::sleep;

    fn make_result(sql: &str) -> TranspileResult {
        TranspileResult {
            sql: sql.to_lowercase(),
            create_table_metadata: None,
            copy_metadata: None,
            referenced_tables: Vec::new(),
            operation_type: OperationType::SELECT,
            errors: Vec::new(),
            warnings: Vec::new(),
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

        let result1 = cache.get_or_compute("SELECT 1", || {
            call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            make_result("SELECT 1")
        });

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
        cache.put("c".to_string(), make_result("c"));

        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn test_cache_ttl() {
        let cache = TranspileCache::with_size_and_ttl(10, Some(Duration::from_millis(50)));

        cache.put("SELECT 1".to_string(), make_result("SELECT 1"));

        // Should be available immediately
        assert!(cache.get("SELECT 1").is_some());

        // Wait for TTL to expire
        sleep(Duration::from_millis(100));

        // Should be expired now
        assert!(cache.get("SELECT 1").is_none());
    }

    #[test]
    fn test_cache_from_config() {
        let cache = TranspileCache::from_config(100, 60);
        assert_eq!(cache.len(), 0);
        assert!(cache.ttl().is_some());

        let cache_no_ttl = TranspileCache::from_config(100, 0);
        assert!(cache_no_ttl.ttl().is_none());
    }
}