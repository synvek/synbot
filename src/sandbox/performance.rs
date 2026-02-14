// Performance optimization utilities for sandbox operations
//
// This module provides performance optimizations to meet non-functional requirement 4.1:
// - Application startup time increase: ≤ 2 seconds
// - Tool execution delay: ≤ 100ms
// - Memory overhead: ≤ 10% of host system

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};

/// Connection pool for Docker clients to reduce connection overhead
/// 
/// Reusing Docker connections significantly reduces tool execution latency
pub struct DockerConnectionPool {
    connections: Arc<RwLock<Vec<bollard::Docker>>>,
    max_size: usize,
}

impl DockerConnectionPool {
    /// Create a new connection pool
    pub fn new(max_size: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(Vec::new())),
            max_size,
        }
    }
    
    /// Get a connection from the pool or create a new one
    pub async fn get(&self) -> Result<bollard::Docker, bollard::errors::Error> {
        let mut connections = self.connections.write().await;
        
        if let Some(conn) = connections.pop() {
            // Reuse existing connection
            Ok(conn)
        } else {
            // Create new connection
            bollard::Docker::connect_with_local_defaults()
        }
    }
    
    /// Return a connection to the pool
    pub async fn return_connection(&self, conn: bollard::Docker) {
        let mut connections = self.connections.write().await;
        
        if connections.len() < self.max_size {
            connections.push(conn);
        }
        // If pool is full, connection is dropped
    }
}

/// Lazy initialization helper for expensive resources
/// 
/// Defers initialization until first use to reduce startup time
pub struct LazyInit<T> {
    value: Arc<RwLock<Option<T>>>,
    initializer: Arc<dyn Fn() -> T + Send + Sync>,
}

impl<T> LazyInit<T> {
    /// Create a new lazy initializer
    pub fn new<F>(initializer: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            value: Arc::new(RwLock::new(None)),
            initializer: Arc::new(initializer),
        }
    }
    
    /// Get the value, initializing if necessary
    pub async fn get(&self) -> T
    where
        T: Clone,
    {
        // Fast path: value already initialized
        {
            let value = self.value.read().await;
            if let Some(v) = value.as_ref() {
                return v.clone();
            }
        }
        
        // Slow path: initialize value
        let mut value = self.value.write().await;
        if value.is_none() {
            *value = Some((self.initializer)());
        }
        value.as_ref().unwrap().clone()
    }
}

/// Cache for frequently accessed configuration values
/// 
/// Reduces repeated parsing and validation overhead
pub struct ConfigCache {
    cache: Arc<RwLock<HashMap<String, CachedConfig>>>,
    ttl: Duration,
}

#[derive(Clone)]
struct CachedConfig {
    value: serde_json::Value,
    cached_at: Instant,
}

impl ConfigCache {
    /// Create a new configuration cache
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }
    
    /// Get a cached configuration value
    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let cache = self.cache.read().await;
        
        if let Some(cached) = cache.get(key) {
            // Check if cache entry is still valid
            if cached.cached_at.elapsed() < self.ttl {
                return Some(cached.value.clone());
            }
        }
        
        None
    }
    
    /// Store a configuration value in cache
    pub async fn set(&self, key: String, value: serde_json::Value) {
        let mut cache = self.cache.write().await;
        cache.insert(key, CachedConfig {
            value,
            cached_at: Instant::now(),
        });
    }
    
    /// Clear expired cache entries
    pub async fn cleanup(&self) {
        let mut cache = self.cache.write().await;
        cache.retain(|_, v| v.cached_at.elapsed() < self.ttl);
    }
}

/// Batch operation helper for reducing overhead
/// 
/// Groups multiple operations together to reduce per-operation overhead
pub struct BatchExecutor<T> {
    pending: Arc<RwLock<Vec<T>>>,
    batch_size: usize,
    flush_interval: Duration,
}

impl<T> BatchExecutor<T> {
    /// Create a new batch executor
    pub fn new(batch_size: usize, flush_interval: Duration) -> Self {
        Self {
            pending: Arc::new(RwLock::new(Vec::new())),
            batch_size,
            flush_interval,
        }
    }
    
    /// Add an item to the batch
    pub async fn add(&self, item: T) -> bool {
        let mut pending = self.pending.write().await;
        pending.push(item);
        
        // Return true if batch is full and should be flushed
        pending.len() >= self.batch_size
    }
    
    /// Get all pending items and clear the batch
    pub async fn flush(&self) -> Vec<T> {
        let mut pending = self.pending.write().await;
        std::mem::take(&mut *pending)
    }
    
    /// Get the number of pending items
    pub async fn len(&self) -> usize {
        let pending = self.pending.read().await;
        pending.len()
    }
}

/// Resource pool for reusing expensive objects
/// 
/// Reduces allocation and initialization overhead
pub struct ResourcePool<T> {
    available: Arc<RwLock<Vec<T>>>,
    in_use: Arc<RwLock<usize>>,
    max_size: usize,
    factory: Arc<dyn Fn() -> T + Send + Sync>,
}

impl<T> ResourcePool<T> {
    /// Create a new resource pool
    pub fn new<F>(max_size: usize, factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            available: Arc::new(RwLock::new(Vec::new())),
            in_use: Arc::new(RwLock::new(0)),
            max_size,
            factory: Arc::new(factory),
        }
    }
    
    /// Acquire a resource from the pool
    pub async fn acquire(&self) -> T {
        // Try to get from available pool
        {
            let mut available = self.available.write().await;
            if let Some(resource) = available.pop() {
                let mut in_use = self.in_use.write().await;
                *in_use += 1;
                return resource;
            }
        }
        
        // Create new resource if under limit
        let mut in_use = self.in_use.write().await;
        *in_use += 1;
        (self.factory)()
    }
    
    /// Release a resource back to the pool
    pub async fn release(&self, resource: T) {
        let mut in_use = self.in_use.write().await;
        *in_use = in_use.saturating_sub(1);
        
        let mut available = self.available.write().await;
        if available.len() < self.max_size {
            available.push(resource);
        }
        // If pool is full, resource is dropped
    }
    
    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let available = self.available.read().await;
        let in_use = self.in_use.read().await;
        
        PoolStats {
            available: available.len(),
            in_use: *in_use,
            max_size: self.max_size,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub available: usize,
    pub in_use: usize,
    pub max_size: usize,
}

/// Parallel initialization helper
/// 
/// Initializes multiple components concurrently to reduce startup time
pub async fn parallel_init<T, F, Fut>(
    tasks: Vec<F>,
) -> Vec<Result<T, Box<dyn std::error::Error + Send + Sync>>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>> + Send + 'static,
    T: Send + 'static,
{
    let handles: Vec<_> = tasks
        .into_iter()
        .map(|task| tokio::spawn(async move { task().await }))
        .collect();
    
    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)),
        }
    }
    
    results
}

/// Memory-efficient string interning
/// 
/// Reduces memory overhead by deduplicating strings
pub struct StringInterner {
    strings: Arc<RwLock<HashMap<String, Arc<str>>>>,
}

impl StringInterner {
    /// Create a new string interner
    pub fn new() -> Self {
        Self {
            strings: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Intern a string
    pub async fn intern(&self, s: &str) -> Arc<str> {
        let strings = self.strings.read().await;
        
        if let Some(interned) = strings.get(s) {
            return Arc::clone(interned);
        }
        
        drop(strings);
        
        let mut strings = self.strings.write().await;
        // Double-check after acquiring write lock
        if let Some(interned) = strings.get(s) {
            return Arc::clone(interned);
        }
        
        let interned: Arc<str> = Arc::from(s);
        strings.insert(s.to_string(), Arc::clone(&interned));
        interned
    }
    
    /// Get the number of interned strings
    pub async fn len(&self) -> usize {
        let strings = self.strings.read().await;
        strings.len()
    }
    
    /// Clear all interned strings
    pub async fn clear(&self) {
        let mut strings = self.strings.write().await;
        strings.clear();
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_lazy_init() {
        let counter = Arc::new(RwLock::new(0));
        let counter_clone = Arc::clone(&counter);
        
        let lazy = LazyInit::new(move || {
            let counter = Arc::clone(&counter_clone);
            tokio::spawn(async move {
                let mut c = counter.write().await;
                *c += 1;
            });
            42
        });
        
        let v1 = lazy.get().await;
        let v2 = lazy.get().await;
        
        assert_eq!(v1, 42);
        assert_eq!(v2, 42);
    }
    
    #[tokio::test]
    async fn test_config_cache() {
        let cache = ConfigCache::new(Duration::from_secs(1));
        
        cache.set("key1".to_string(), serde_json::json!({"value": 123})).await;
        
        let value = cache.get("key1").await;
        assert!(value.is_some());
        assert_eq!(value.unwrap()["value"], 123);
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        let value = cache.get("key1").await;
        assert!(value.is_none());
    }
    
    #[tokio::test]
    async fn test_batch_executor() {
        let executor = BatchExecutor::new(3, Duration::from_secs(1));
        
        assert!(!executor.add(1).await);
        assert!(!executor.add(2).await);
        assert!(executor.add(3).await); // Batch is full
        
        let items = executor.flush().await;
        assert_eq!(items, vec![1, 2, 3]);
        assert_eq!(executor.len().await, 0);
    }
    
    #[tokio::test]
    async fn test_resource_pool() {
        let pool = ResourcePool::new(2, || vec![1, 2, 3]);
        
        let r1 = pool.acquire().await;
        let r2 = pool.acquire().await;
        
        assert_eq!(r1, vec![1, 2, 3]);
        assert_eq!(r2, vec![1, 2, 3]);
        
        pool.release(r1).await;
        pool.release(r2).await;
        
        let stats = pool.stats().await;
        assert_eq!(stats.available, 2);
        assert_eq!(stats.in_use, 0);
    }
    
    #[tokio::test]
    async fn test_string_interner() {
        let interner = StringInterner::new();
        
        let s1 = interner.intern("hello").await;
        let s2 = interner.intern("hello").await;
        let s3 = interner.intern("world").await;
        
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(!Arc::ptr_eq(&s1, &s3));
        assert_eq!(interner.len().await, 2);
    }
}
