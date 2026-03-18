//! Query Result Cache
//!
//! Provides caching for query execution results to improve performance
//! for frequently executed read-only queries.

#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use lru::LruCache;
#[cfg(test)]
use std::num::NonZeroUsize;
#[cfg(test)]
use std::time::{Duration, Instant};

/// Default cache size for query results
#[cfg(test)]
const DEFAULT_RESULT_CACHE_SIZE: usize = 64;

/// A cached query result entry
#[derive(Clone, Debug)]
#[cfg(test)]
pub struct QueryResultCacheEntry {
    /// The cached rows as serialized strings
    pub rows: Vec<Vec<Option<String>>>,
    /// Column names
    pub columns: Vec<String>,
    /// Column types (OID)
    pub column_types: Vec<i32>,
    /// When this entry was cached
    pub cached_at: Instant,
    /// Time-to-live duration
    pub ttl: Duration,
}

#[cfg(test)]
impl QueryResultCacheEntry {
    /// Create a new cache entry
    pub fn new(
        rows: Vec<Vec<Option<String>>>,
        columns: Vec<String>,
        column_types: Vec<i32>,
        ttl: Duration,
    ) -> Self {
        Self {
            rows,
            columns,
            column_types,
            cached_at: Instant::now(),
            ttl,
        }
    }

    /// Check if this cache entry has expired
    pub fn is_expired(&self) -> bool {
        Instant::now().duration_since(self.cached_at) > self.ttl
    }
}

/// Cache for query execution results.
///
/// Uses an LRU eviction policy with optional TTL support.
/// Thread-safe via Mutex wrapping.
#[cfg(test)]
pub struct QueryResultCache {
    cache: Mutex<LruCache<String, QueryResultCacheEntry>>,
    ttl: Duration,
    enabled: bool,
}

#[cfg(test)]
impl QueryResultCache {
    /// Create a new query result cache with default settings (disabled).
    pub fn new() -> Self {
        Self::with_config(false, DEFAULT_RESULT_CACHE_SIZE, Duration::from_secs(60))
    }

    /// Create a new query result cache with specific configuration.
    ///
    /// # Arguments
    /// * `enabled` - Whether the cache is enabled
    /// * `size` - Maximum number of entries in the cache
    /// * `ttl` - Time-to-live duration for cache entries
    pub fn with_config(enabled: bool, size: usize, ttl: Duration) -> Self {
        let cap = NonZeroUsize::new(size).unwrap_or(NonZeroUsize::new(64).unwrap());
        Self {
            cache: Mutex::new(LruCache::new(cap)),
            ttl,
            enabled,
        }
    }

    /// Create a new query result cache from configuration values.
    ///
    /// # Arguments
    /// * `enabled` - Whether the cache is enabled
    /// * `size` - Maximum number of entries
    /// * `ttl_secs` - TTL in seconds
    pub fn from_config(enabled: bool, size: usize, ttl_secs: u64) -> Self {
        let ttl = Duration::from_secs(ttl_secs);
        Self::with_config(enabled, size, ttl)
    }

    /// Check if the cache is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable the cache.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    /// Get a cached query result if available and not expired.
    pub fn get(&self, sql: &str) -> Option<QueryResultCacheEntry> {
        if !self.enabled {
            return None;
        }

        let mut cache = self.cache.lock().unwrap();
        let entry = cache.get(sql)?;

        if entry.is_expired() {
            // Remove expired entry
            cache.pop(sql);
            return None;
        }

        Some(entry.clone())
    }

    /// Put a query result into the cache.
    pub fn put(&self, sql: String, rows: Vec<Vec<Option<String>>>, columns: Vec<String>, column_types: Vec<i32>) {
        if !self.enabled {
            return;
        }

        let mut cache = self.cache.lock().unwrap();
        let entry = QueryResultCacheEntry::new(rows, columns, column_types, self.ttl);
        cache.put(sql, entry);
    }

    /// Put a pre-built cache entry.
    pub fn put_entry(&self, sql: String, entry: QueryResultCacheEntry) {
        if !self.enabled {
            return;
        }

        let mut cache = self.cache.lock().unwrap();
        cache.put(sql, entry);
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

    /// Get the configured TTL.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Get the maximum cache size.
    pub fn capacity(&self) -> usize {
        self.cache.lock().unwrap().cap().into()
    }
}

#[cfg(test)]
impl Default for QueryResultCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_cache_disabled_by_default() {
        let cache = QueryResultCache::new();
        assert!(!cache.is_enabled());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_enable_disable() {
        let mut cache = QueryResultCache::new();

        cache.set_enabled(true);
        assert!(cache.is_enabled());

        cache.put(
            "SELECT 1".to_string(),
            vec![vec![Some("1".to_string())]],
            vec!["?column?".to_string()],
            vec![23], // INT4
        );
        assert_eq!(cache.len(), 1);

        cache.set_enabled(false);
        assert!(!cache.is_enabled());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_put_get() {
        let cache = QueryResultCache::with_config(true, 10, Duration::from_secs(60));

        cache.put(
            "SELECT 1".to_string(),
            vec![vec![Some("1".to_string())]],
            vec!["?column?".to_string()],
            vec![23],
        );

        let entry = cache.get("SELECT 1");
        assert!(entry.is_some());

        let entry = entry.unwrap();
        assert_eq!(entry.rows.len(), 1);
        assert_eq!(entry.columns.len(), 1);
    }

    #[test]
    fn test_cache_miss_when_disabled() {
        let cache = QueryResultCache::with_config(false, 10, Duration::from_secs(60));

        cache.put(
            "SELECT 1".to_string(),
            vec![vec![Some("1".to_string())]],
            vec!["?column?".to_string()],
            vec![23],
        );

        assert!(cache.get("SELECT 1").is_none());
    }

    #[test]
    fn test_cache_ttl() {
        let cache = QueryResultCache::with_config(true, 10, Duration::from_millis(50));

        cache.put(
            "SELECT 1".to_string(),
            vec![vec![Some("1".to_string())]],
            vec!["?column?".to_string()],
            vec![23],
        );

        // Should be available immediately
        assert!(cache.get("SELECT 1").is_some());

        // Wait for TTL to expire
        sleep(Duration::from_millis(100));

        // Should be expired now
        assert!(cache.get("SELECT 1").is_none());
    }

    #[test]
    fn test_cache_eviction() {
        let cache = QueryResultCache::with_config(true, 2, Duration::from_secs(60));

        cache.put("a".to_string(), vec![], vec![], vec![]);
        cache.put("b".to_string(), vec![], vec![], vec![]);
        cache.put("c".to_string(), vec![], vec![], vec![]);

        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn test_cache_from_config() {
        let cache = QueryResultCache::from_config(true, 100, 60);
        assert!(cache.is_enabled());
        assert_eq!(cache.capacity(), 100);
        assert_eq!(cache.ttl(), Duration::from_secs(60));
    }
}
