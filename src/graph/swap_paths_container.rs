use crate::pools::pool_id::PoolId;
use super::swap_path_hash::SwapPathHash;
use crate::MarketWithoutLock;
use super::swap_path::SwapPath;
use ahash::HashMap;
use dashmap::{DashMap, DashSet};

use std::sync::Arc;
/*
   This container allows to add any swap path and makes sure that the path is only added once.
   It also allows to get all swap paths for a specific pool.
   All swap paths are immutable and should not be altered after adding them to the container.
*/
#[derive(Clone, Debug, Default)]
pub struct SwapPathsContainer {
    // Used to check before inserting a new path if it already exists. It saves 1 lookup
    // (clippy would complain about mutable_key_type if the SwapPath is used directly)
    pub swap_path_hashes: DashSet<SwapPathHash>,
    // All swap paths for a pool. (Again clippy would complain if we use a HashSet for the SwapPath)
    pub pool_paths: DashMap<PoolId, HashMap<SwapPathHash, SwapPath>>,
}

impl SwapPathsContainer {
    /// Create a new empty [SwapPathsContainer]
    pub fn new() -> SwapPathsContainer {
        SwapPathsContainer { swap_path_hashes: DashSet::new(), pool_paths: DashMap::new() }
    }

    /// Create a new [SwapPathsContainer] from a list of [SwapPath]
    pub fn from(paths: Vec<SwapPath>) -> Self {
        let mut ret = Self::default();
        for p in paths {
            ret.add(p);
        }
        ret
    }

    /// The number of paths
    pub fn len(&self) -> usize {
        self.swap_path_hashes.len()
    }

    /// Returns true if the paths is empty
    pub fn is_empty(&self) -> bool {
        self.swap_path_hashes.is_empty()
    }

    /// Add a new swap path to the container
    pub fn add<T: Into<SwapPath> + Clone>(&mut self, path: T) {
        let swap_path: SwapPath = path.into();

        if !self.swap_path_hashes.contains(&swap_path.swap_path_hash) {
            self.swap_path_hashes.insert(swap_path.swap_path_hash.clone());
            for pool in swap_path.pools.iter() {
                let mut pool_paths = self.pool_paths.entry(pool.get_pool_id()).or_default();
                pool_paths.insert(swap_path.swap_path_hash.clone(), swap_path.clone());
            }
        }
    }

    /// Remove a pool from the container
    pub fn remove_pool(&mut self, pool_id: &PoolId) {
        if self.pool_paths.get(pool_id).is_none() {
            return;
        }
        let swap_paths_for_deletion = self.pool_paths.get(pool_id).unwrap().clone();
        // iterate all swap paths that contain the input pool
        for (swap_path_hash, swap_path) in swap_paths_for_deletion {
            self.swap_path_hashes.remove(&swap_path_hash);
            // Update all pools swap path that contain the input pool
            for pool in swap_path.pools.iter() {
                if let Some(mut pool_paths) = self.pool_paths.get_mut(&pool.get_pool_id()) {
                    pool_paths.remove(&swap_path_hash);
                }
            }
        }
        self.pool_paths.remove(pool_id);
    }

    /// Get all swap paths for a specific pool
    pub fn get_pool_paths_vec(&self, pool_id: &PoolId) -> Vec<SwapPath> {
        if let Some(swap_paths) = self.pool_paths.get(pool_id) {
            return swap_paths.value().values().cloned().collect();
        }
        vec![]
    }

    pub fn get_pool_paths_len(&self, pool_id: &PoolId) -> u64 {
        if let Some(swap_paths) = self.pool_paths.get(pool_id) {
            return swap_paths.value().len() as u64;
        }
        0
    }
}

/// Add a new swap path to the container
pub fn add_swap_path<T: Into<SwapPath> + Clone>(market_without_lock: Arc<MarketWithoutLock>, path: T) {
    let swap_path: SwapPath = path.into();

    if !market_without_lock.swap_paths.swap_path_hashes.contains(&swap_path.swap_path_hash) {
        market_without_lock.swap_paths.swap_path_hashes.insert(swap_path.swap_path_hash.clone());
        // add the swap path to all pools that are part of the swap path
        for pool in swap_path.pools.iter() {
            let mut pool_paths = market_without_lock.swap_paths.pool_paths.entry(pool.get_pool_id()).or_default();
            pool_paths.insert(swap_path.swap_path_hash.clone(), swap_path.clone());
        }
    }
}

/// Remove a pool from the container
pub fn remove_pool(market_without_lock: Arc<MarketWithoutLock>, pool_id: &PoolId) {
    if market_without_lock.swap_paths.pool_paths.get(pool_id).is_none() {
        return;
    }
    let swap_paths_for_deletion = market_without_lock.swap_paths.pool_paths.get(pool_id).unwrap().clone();
    // iterate all swap paths that contain the input pool
    for (swap_path_hash, swap_path) in swap_paths_for_deletion {
        market_without_lock.swap_paths.swap_path_hashes.remove(&swap_path_hash);
        // Update all pools swap path that contain the input pool
        for pool in swap_path.pools.iter() {
            if let Some(mut pool_paths) = market_without_lock.swap_paths.pool_paths.get_mut(&pool.get_pool_id()) {
                pool_paths.remove(&swap_path_hash);
            }
        }
    }
    market_without_lock.swap_paths.pool_paths.remove(pool_id);
}

#[cfg(test)]
mod test {
    use crate::graph::swap_path::*;
    use crate::graph::swap_paths_container::SwapPathsContainer;
    use crate::{MockPool, PoolWrapper, Token};
    use alloy_primitives::Address;
    use std::sync::Arc;

    #[test]
    fn test_serialize() {
        let token_1 = Token::new(Address::repeat_byte(0x11));
        let token_2 = Token::new(Address::repeat_byte(0x22));
        let pool_1_2 = PoolWrapper::new(Arc::new(MockPool::new(token_1.get_address(), token_2.get_address(), Address::repeat_byte(0x12))));
        let pool_2_1 = PoolWrapper::new(Arc::new(MockPool::new(token_2.get_address(), token_1.get_address(), Address::repeat_byte(0x13))));

        let swap_path = SwapPath::new(vec![token_1.clone(), token_2.clone(), token_1.clone()], vec![pool_1_2.clone(), pool_2_1.clone()]);
        let mut swap_path_container = SwapPathsContainer::new();
        swap_path_container.add(swap_path.clone());

        // 序列化测试已移除，因为SwapPathsContainer不再支持序列化
        assert!(swap_path_container.swap_path_hashes.contains(&swap_path.swap_path_hash));
    }

    #[test]
    fn test_add_path() {
        let token_1 = Token::new(Address::repeat_byte(0x11));
        let token_2 = Token::new(Address::repeat_byte(0x22));
        let pool_1_2 = PoolWrapper::new(Arc::new(MockPool::new(token_1.get_address(), token_2.get_address(), Address::repeat_byte(0x12))));
        let pool_2_1 = PoolWrapper::new(Arc::new(MockPool::new(token_2.get_address(), token_1.get_address(), Address::repeat_byte(0x13))));

        let swap_path = SwapPath::new(vec![token_1.clone(), token_2.clone(), token_1.clone()], vec![pool_1_2.clone(), pool_2_1.clone()]);
        let mut swap_paths = SwapPathsContainer::new();

        assert_eq!(swap_paths.len(), 0);
        assert!(swap_paths.is_empty());
        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_1_2.get_pool_id()), &vec![]);

        swap_paths.add(swap_path.clone());
        assert_eq!(swap_paths.len(), 1);
        assert!(!swap_paths.is_empty());
        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_2_1.get_pool_id()), &vec![swap_path.clone()]);
    }

    #[test]
    fn test_remove_pool_from_swap_paths() {
        let token1 = Token::random();
        let token2 = Token::random();
        let token3 = Token::random();

        // (TOKEN1 -> TOKEN2) -> (TOKEN2 -> TOKEN1)
        let pool_1_to_2 = PoolWrapper::new(Arc::new(MockPool::new(token1.get_address(), token2.get_address(), Address::random())));
        let pool_2_to_1 = PoolWrapper::new(Arc::new(MockPool::new(token2.get_address(), token1.get_address(), Address::random())));
        let swap_path1 =
            SwapPath::new(vec![token1.clone(), token2.clone(), token1.clone()], vec![pool_1_to_2.clone(), pool_2_to_1.clone()]);

        // (TOKEN1 -> TOKEN3) -> (TOKEN3 -> TOKEN1)
        let pool_1_to_3 = PoolWrapper::new(Arc::new(MockPool::new(token1.get_address(), token3.get_address(), Address::random())));
        let pool_3_to_1 = PoolWrapper::new(Arc::new(MockPool::new(token3.get_address(), token1.get_address(), Address::random())));
        let swap_path2 =
            SwapPath::new(vec![token1.clone(), token3.clone(), token1.clone()], vec![pool_1_to_3.clone(), pool_3_to_1.clone()]);

        // (TOKEN1 -> TOKEN2) -> (TOKEN2 -> TOKEN3) -> (TOKEN3 -> TOKEN1)
        let pool_2_to_3 = PoolWrapper::new(Arc::new(MockPool::new(token2.get_address(), token3.get_address(), Address::random())));
        let swap_path3 = SwapPath::new(
            vec![token1.clone(), token2.clone(), token3.clone(), token1.clone()],
            vec![pool_1_to_2.clone(), pool_2_to_3.clone(), pool_3_to_1.clone()],
        );

        let mut swap_paths = SwapPathsContainer::new();
        swap_paths.add(swap_path1.clone());
        swap_paths.add(swap_path2.clone());
        swap_paths.add(swap_path3.clone());

        // The order is not deterministic, so we need to check if the vector contains the swap path
        assert_eq!(swap_paths.get_pool_paths_vec(&pool_3_to_1.get_pool_id()).len(), 2);
        assert!(swap_paths.get_pool_paths_vec(&pool_1_to_2.get_pool_id()).contains(&swap_path1));
        assert!(swap_paths.get_pool_paths_vec(&pool_1_to_2.get_pool_id()).contains(&swap_path3));

        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_2_to_1.get_pool_id()), &vec![swap_path1.clone()]);
        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_1_to_3.get_pool_id()), &vec![swap_path2.clone()]);

        // The order is not deterministic, so we need to check if the vector contains the swap path
        assert_eq!(swap_paths.get_pool_paths_vec(&pool_3_to_1.get_pool_id()).len(), 2);
        assert!(swap_paths.get_pool_paths_vec(&pool_3_to_1.get_pool_id()).contains(&swap_path2));
        assert!(swap_paths.get_pool_paths_vec(&pool_3_to_1.get_pool_id()).contains(&swap_path3));

        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_2_to_3.get_pool_id()), &vec![swap_path3.clone()]);

        // remove pool_1_to_2 which is in swap_path1 and swap_path3
        swap_paths.remove_pool(&pool_1_to_2.get_pool_id());

        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_1_to_2.get_pool_id()), &vec![]);
        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_2_to_1.get_pool_id()), &vec![]);
        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_1_to_3.get_pool_id()), &vec![swap_path2.clone()]);
        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_3_to_1.get_pool_id()), &vec![swap_path2.clone()]);
        assert_eq!(&swap_paths.get_pool_paths_vec(&pool_2_to_3.get_pool_id()), &vec![]);
    }
}
