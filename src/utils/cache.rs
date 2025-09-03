use alloy_primitives::{Address, U256};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 缓存项，包含数据和时间戳
#[derive(Clone, Debug)]
pub struct CacheItem<T> {
    pub data: T,
    pub timestamp: Instant,
    pub ttl: Duration,
}

impl<T> CacheItem<T> {
    pub fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            timestamp: Instant::now(),
            ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }
}

/// 高性能缓存系统，替代区块链状态数据库访问
#[derive(Debug)]
pub struct StateCache {
    // 存储余额缓存 (address, token) -> balance
    balances: DashMap<(Address, Address), CacheItem<U256>>,
    // 存储存储缓存 (address, slot) -> value  
    storage: DashMap<(Address, U256), CacheItem<U256>>,
    // 缓存统计
    pub stats: CacheStats,
    // 默认TTL
    default_ttl: Duration,
}

#[derive(Debug, Default)]
pub struct CacheStats {
    pub hits: std::sync::atomic::AtomicU64,
    pub misses: std::sync::atomic::AtomicU64,
    pub evictions: std::sync::atomic::AtomicU64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed);
        let misses = self.misses.load(std::sync::atomic::Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

impl StateCache {
    /// 创建新的缓存实例
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            balances: DashMap::new(),
            storage: DashMap::new(),
            stats: CacheStats::default(),
            default_ttl,
        }
    }

    /// 创建默认缓存实例（5分钟TTL）
    pub fn new_default() -> Self {
        Self::new(Duration::from_secs(300))
    }

    /// 获取余额
    pub fn get_balance(&self, address: Address, token: Address) -> Option<U256> {
        let key = (address, token);
        if let Some(item) = self.balances.get(&key) {
            if !item.is_expired() {
                self.stats.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Some(item.data);
            } else {
                // 清理过期项
                self.balances.remove(&key);
                self.stats.evictions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.stats.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }

    /// 设置余额
    pub fn set_balance(&self, address: Address, token: Address, balance: U256) {
        let key = (address, token);
        let item = CacheItem::new(balance, self.default_ttl);
        self.balances.insert(key, item);
    }

    /// 获取存储
    pub fn get_storage(&self, address: Address, slot: U256) -> Option<U256> {
        let key = (address, slot);
        if let Some(item) = self.storage.get(&key) {
            if !item.is_expired() {
                self.stats.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Some(item.data);
            } else {
                self.storage.remove(&key);
                self.stats.evictions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.stats.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }

    /// 设置存储
    pub fn set_storage(&self, address: Address, slot: U256, value: U256) {
        let key = (address, slot);
        let item = CacheItem::new(value, self.default_ttl);
        self.storage.insert(key, item);
    }

    /// 清理所有过期项
    pub fn cleanup_expired(&self) {
        let now = Instant::now();
        
        // 清理余额缓存
        self.balances.retain(|_, item| {
            let expired = now.duration_since(item.timestamp) > item.ttl;
            if expired {
                self.stats.evictions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            !expired
        });

        // 清理存储缓存
        self.storage.retain(|_, item| {
            let expired = now.duration_since(item.timestamp) > item.ttl;
            if expired {
                self.stats.evictions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            !expired
        });
    }

    /// 获取缓存大小信息
    pub fn cache_sizes(&self) -> CacheSizes {
        CacheSizes {
            balances: self.balances.len(),
            storage: self.storage.len(),
        }
    }

    /// 清空所有缓存
    pub fn clear_all(&self) {
        self.balances.clear();
        self.storage.clear();
    }
}

#[derive(Debug)]
pub struct CacheSizes {
    pub balances: usize,
    pub storage: usize,
}

/// 简化的缓存状态提供者
pub struct CachedStateProvider {
    cache: Arc<StateCache>,
}

impl CachedStateProvider {
    pub fn new(cache: Arc<StateCache>) -> Self {
        Self {
            cache,
        }
    }

    pub fn get_cache(&self) -> &StateCache {
        &self.cache
    }
}

/// 缓存管理器，用于协调不同类型的缓存
pub struct CacheManager {
    state_cache: Arc<StateCache>,
}

impl CacheManager {
    pub fn new() -> Self {
        let state_cache = Arc::new(StateCache::new_default());
        
        Self {
            state_cache,
        }
    }

    pub fn state_cache(&self) -> &Arc<StateCache> {
        &self.state_cache
    }

    /// 创建缓存状态提供者
    pub fn create_provider(&self) -> CachedStateProvider {
        CachedStateProvider::new(self.state_cache.clone())
    }

    /// 清理过期缓存
    pub fn cleanup(&self) {
        self.state_cache.cleanup_expired();
    }

    /// 获取缓存统计信息
    pub fn get_stats(&self) -> &CacheStats {
        &self.state_cache.stats
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn test_cache_basic_operations() {
        let cache = StateCache::new_default();
        let addr = address!("0x1234567890123456789012345678901234567890");
        let token = address!("0x0987654321098765432109876543210987654321");
        let balance = U256::from(1000);

        // 测试缓存未命中
        assert!(cache.get_balance(addr, token).is_none());

        // 设置并获取
        cache.set_balance(addr, token, balance);
        assert_eq!(cache.get_balance(addr, token), Some(balance));

        // 测试统计
        assert!(cache.stats.hits.load(std::sync::atomic::Ordering::Relaxed) > 0);
        assert!(cache.stats.misses.load(std::sync::atomic::Ordering::Relaxed) > 0);
    }
}
