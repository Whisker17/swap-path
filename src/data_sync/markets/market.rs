use crate::logic::graph::{SwapPath, TokenGraph, SwapPathSet, SwapPathsContainer};
use crate::utils::constants::WMNT;
use super::market_config::MarketConfigSection;
use crate::logic::pools::pool_id::PoolId;
use crate::{PoolWrapper, Token};
use alloy_primitives::Address;
use dashmap::{DashMap, DashSet};
use eyre::Result;

use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct MarketWithoutLock {
    // pool_address -> is_disabled (For faster lookup, graph needs two lookups)
    pools_disabled: DashSet<PoolId>,
    // pools in the market For faster lookup
    pools_exists: DashSet<PoolId>,
    // Token -> Token -> Pool
    pub token_to_pool: DashMap<Address, DashMap<Address, DashSet<SwapPath>>>,
    // All swap paths with deduplication and fast lookup
    pub swap_paths: SwapPathsContainer,
}

/// Helper struct to check if a pool is disabled or exists in the market.
impl MarketWithoutLock {
    pub fn contains(&self, pool_id: &PoolId) -> bool {
        self.pools_exists.contains(pool_id)
    }

    pub fn is_pool_disabled(&self, pool_id: &PoolId) -> bool {
        self.pools_disabled.contains(pool_id)
    }

    pub fn pool_exists(&self, pool_id: &PoolId) -> bool {
        self.pools_exists.contains(pool_id)
    }

    pub fn pool_exists_and_enabled(&self, pool_id: &PoolId) -> bool {
        if self.pools_disabled.contains(pool_id) {
            return false;
        }
        self.pools_exists.contains(pool_id)
    }

    pub fn pools_len(&self) -> u64 {
        self.pools_exists.len() as u64
    }
    pub fn disabled_pools_len(&self) -> u64 {
        self.pools_disabled.len() as u64
    }

    /// Get all swap paths from the market by the pool id.
    pub fn get_pool_paths(&self, pool_id: &PoolId) -> Vec<SwapPath> {
        if self.is_pool_disabled(pool_id) {
            return vec![];
        }
        self.swap_paths.get_pool_paths_vec(pool_id)
    }

    pub fn pool_paths_len(&self, pool_id: &PoolId) -> u64 {
        self.swap_paths.get_pool_paths_len(pool_id)
    }
}

/// The market struct contains all the pools and tokens.
/// It keeps track if a pool is disabled or not and the swap paths.
#[derive(Default)]
pub struct Market {
    pub market_config: MarketConfigSection,
    // Fast lookup tables outside the market RwLock.
    // This is used for checks if pools are existing or disabled and do not lock the market.
    pub market_without_lock: Arc<MarketWithoutLock>,

    // Graph with all tokens and pools to build swap paths
    pub token_graph: TokenGraph,
}

impl Market {
    pub fn new(market_config: MarketConfigSection) -> Self {
        let mut market =
            Market { market_config, market_without_lock: Arc::new(MarketWithoutLock::default()), token_graph: TokenGraph::new() };
        market.add_token(Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18)));

        market
    }

    /// Add a [`Token`] reference to the market. If the token already exists nothing will happen.
    pub fn add_token<T: Into<Arc<Token>>>(&mut self, token: T) {
        let arc_token: Arc<Token> = token.into();
        self.token_graph.add_or_get_token_idx_by_token(arc_token);
    }

    /// Check if the given address is the WMNT address.
    pub fn is_wmnt(address: &Address) -> bool {
        address.eq(&WMNT)
    }

    /// Add a new pool to the market
    pub fn add_pool<T: Into<PoolWrapper>>(&mut self, pool: T) {
        let pool_contract = pool.into();
        let pool_id = pool_contract.get_pool_id();

        if self.token_graph.pools.contains_key(&pool_id) {
            return;
        }

        // add to graph
        for token_address in pool_contract.get_tokens() {
            self.token_graph.add_or_get_token_idx_by_address(token_address);
        }
        self.token_graph.add_pool(pool_contract.clone()).expect("Tokens are missing from graph. This should never happen");

        self.market_without_lock.pools_exists.insert(pool_id);

        for (from_token, to_token) in pool_contract.pool.get_swap_directions() {
            let token0 = self.token_graph.tokens.get(&from_token).unwrap();
            let token1 = self.token_graph.tokens.get(&to_token).unwrap();
            let swap_path = SwapPath::new_first(token0.clone(), token1.clone(), pool_contract.clone());
            self.market_without_lock.token_to_pool.entry(from_token).or_default().entry(to_token).or_default().insert(swap_path.clone());
        }
    }

    /// Get a pool reference to the pool by the address.
    pub fn get_pool(&self, pool_id: &PoolId) -> Option<&PoolWrapper> {
        self.token_graph.pools.get(pool_id)
    }

    /// Get a pool reference to the pool by the address if it is enabled.
    pub fn get_pool_if_enabled(&self, pool_id: &PoolId) -> Option<&PoolWrapper> {
        if self.market_without_lock.is_pool_disabled(pool_id) {
            return None;
        }
        self.token_graph.pools.get(pool_id)
    }

    /// Check if the pool exists and is enabled.
    pub fn pool_exists_and_enabled(&self, pool_id: &PoolId) -> bool {
        if self.market_without_lock.pools_disabled.contains(pool_id) {
            return false;
        }
        self.token_graph.pools.contains_key(pool_id)
    }

    /// Get a reference to the pools map in the market. Used for web interface
    pub fn pools(&self) -> &HashMap<PoolId, PoolWrapper> {
        &self.token_graph.pools
    }

    /// Get a reference to the pools map in the market.
    pub fn enabled_pools(&self) -> Vec<PoolWrapper> {
        self.token_graph.pools.values().filter(|pool| !self.market_without_lock.is_pool_disabled(&pool.get_pool_id())).cloned().collect()
    }

    pub fn total_enabled_pools(&self) -> u64 {
        (self.token_graph.pools.len() - self.market_without_lock.pools_disabled.len()) as u64
    }

    /// Set the pool status to disable. Returns an error if the pool does not exist in the graph.
    pub fn disable_pool(&mut self, pool_id: PoolId) -> Result<()> {
        // Fast lookup table
        self.market_without_lock.pools_disabled.insert(pool_id);

        // disable in the graph
        self.token_graph.set_pool_active(pool_id, false)?;

        Ok(())
    }

    /// Set the pool status to enable. Returns an error if the pool does not exist in the graph.
    pub fn enable_pool(&mut self, pool_id: PoolId) -> Result<()> {
        // Fast lookup table
        self.market_without_lock.pools_disabled.remove(&pool_id);

        // enable in the graph
        self.token_graph.set_pool_active(pool_id, true)?;

        Ok(())
    }

    /// Get a [`Token`] reference from the market by the address of the token.
    pub fn get_token(&self, address: &Address) -> Option<Arc<Token>> {
        self.token_graph.tokens.get(address).cloned()
    }

    /// Build a list of swap paths from the given undeployed pools.
    pub fn build_undeployed_pools_paths(&self, undeployed_pools: &[PoolWrapper]) -> Result<Vec<SwapPath>> {
        let mut swap_paths_set = SwapPathSet::new();
        for pool in undeployed_pools.iter() {
            // verify again that the pool was not added in the meantime
            if self.market_without_lock.pool_exists(&pool.get_pool_id()) {
                continue;
            }
            let swap_paths = self.token_graph.build_swap_paths(pool, self.market_config.max_hops)?;
            for swap_path in swap_paths.iter() {
                swap_paths_set.insert(swap_path.clone());
            }
        }
        Ok(swap_paths_set.vec())
    }

    /// Update the swap paths with the given pool.
    pub fn update_paths(&mut self, pool: PoolWrapper) -> Result<Vec<SwapPath>> {
        if self.market_without_lock.is_pool_disabled(&pool.get_pool_id()) {
            return Ok(vec![]);
        }
        self.add_pool(pool.clone());
        let swap_paths = self.token_graph.build_swap_paths(&pool, self.market_config.max_hops)?;

        Ok(swap_paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::pools::mock_pool::MockPool;
    use alloy_primitives::Address;
    use eyre::Result;

    #[test]
    fn test_serialize_market() {
        let mut market = Market::default();
        let pool_address = Address::random();
        let token0 = Address::random();
        let token1 = Address::random();
        let mock_pool = MockPool { address: pool_address, token0, token1 };
        market.add_pool(mock_pool);

        // 序列化测试已移除，因为Market不再支持序列化
        assert_eq!(market.pools().len(), 1);
        assert_eq!(market.enabled_pools().len(), 1);
    }

    #[test]
    fn test_add_pool() {
        let mut market = Market::default();
        let pool_address = Address::random();
        let token0 = Address::random();
        let token1 = Address::random();
        let mock_pool = MockPool { address: pool_address, token0, token1 };

        market.add_pool(mock_pool);

        assert_eq!(market.get_pool(&PoolId::Address(pool_address)).unwrap().pool.get_address(), pool_address);
    }

    #[test]
    fn test_add_token() {
        let mut market = Market::default();
        let token_address = Address::random();

        market.add_token(Arc::new(Token::new(token_address)));

        assert_eq!(market.get_token(&token_address).unwrap().get_address(), token_address);
    }

    #[test]
    fn test_get_pool() {
        let mut market = Market::default();
        let pool_address = Address::random();
        let mock_pool = MockPool { address: pool_address, token0: Address::ZERO, token1: Address::ZERO };
        market.add_pool(mock_pool.clone());

        let pool = market.get_pool(&PoolId::Address(pool_address));

        assert_eq!(pool.unwrap().get_address(), pool_address);
    }

    #[test]
    fn test_is_pool() {
        let mut market = Market::default();
        let pool_address = Address::random();
        let mock_pool = MockPool { address: pool_address, token0: Address::ZERO, token1: Address::ZERO };
        market.add_pool(mock_pool.clone());

        let is_pool = market.pool_exists_and_enabled(&PoolId::Address(pool_address));

        assert!(is_pool);
    }

    #[test]
    fn test_is_pool_not_found() {
        let market = Market::default();
        let pool_address = Address::random();

        let is_pool = market.pool_exists_and_enabled(&PoolId::Address(pool_address));

        assert!(!is_pool);
    }

    #[test]
    fn test_set_pool_ok() -> Result<()> {
        let mut market = Market::default();
        let pool_address = Address::random();
        let pool_id = PoolId::Address(pool_address);
        let token0 = Address::random();
        let token1 = Address::random();
        let mock_pool = MockPool { address: pool_address, token0, token1 };
        market.add_pool(mock_pool.clone());

        assert!(!market.market_without_lock.is_pool_disabled(&pool_id));

        // toggle not ok
        market.disable_pool(pool_id)?;
        assert!(market.market_without_lock.is_pool_disabled(&pool_id));

        // toggle back
        market.enable_pool(pool_id)?;
        assert!(!market.market_without_lock.is_pool_disabled(&pool_id));

        Ok(())
    }

    #[test]
    fn test_build_swap_path_vec_two_hops() -> Result<()> {
        let mut market = Market::default();

        // Add basic token for start/end
        let wmnt_token = Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18));
        market.add_token(wmnt_token);

        // Swap pool: token wmnt -> token1
        let pool_address1 = Address::random();
        let token1 = Address::random();
        market.add_token(Token::new(token1));
        let mock_pool1 = PoolWrapper::new(Arc::new(MockPool { address: pool_address1, token0: WMNT, token1 }));
        market.add_pool(mock_pool1.clone());

        // Swap pool: token wmnt -> token1
        let pool_address2 = Address::random();
        let mock_pool2 = PoolWrapper::new(Arc::new(MockPool { address: pool_address2, token0: WMNT, token1 }));
        market.add_pool(mock_pool2.clone());

        // Add test swap paths
        let swap_paths = market.update_paths(mock_pool2)?;
        for swap_path in swap_paths.iter() {
            println!("Swap path: {}", swap_path);
        }
        // verify that we have two paths, with 2 pools and 3 tokens
        assert_eq!(swap_paths.len(), 2);
        assert_eq!(swap_paths.first().unwrap().len(), 2);
        assert_eq!(swap_paths.first().unwrap().tokens_count(), 3);
        assert_eq!(swap_paths.get(1).unwrap().len(), 2);
        assert_eq!(swap_paths.get(1).unwrap().tokens_count(), 3);

        // the order of the swap paths is not deterministic
        let (first_path, second_path) = if swap_paths.first().unwrap().pools.first().unwrap().get_address() == pool_address1 {
            (swap_paths.first().unwrap(), swap_paths.get(1).unwrap())
        } else {
            (swap_paths.get(1).unwrap(), swap_paths.first().unwrap())
        };

        // first path wmnt -> token1 -> wmnt
        let tokens = first_path.tokens.iter().map(|token| token.get_address()).collect::<Vec<Address>>();
        assert_eq!(tokens.first(), Some(&WMNT));
        assert_eq!(tokens.get(1), Some(&token1));
        assert_eq!(tokens.get(2), Some(&WMNT));

        let pools = first_path.pools.iter().map(|pool| pool.get_address()).collect::<Vec<Address>>();
        assert_eq!(pools.first(), Some(&pool_address1));
        assert_eq!(pools.get(1), Some(&pool_address2));

        // other way around
        let tokens = second_path.tokens.iter().map(|token| token.get_address()).collect::<Vec<Address>>();
        assert_eq!(tokens.first(), Some(&WMNT));
        assert_eq!(tokens.get(1), Some(&token1));
        assert_eq!(tokens.get(2), Some(&WMNT));

        let pools = second_path.pools.iter().map(|pool| pool.get_address()).collect::<Vec<Address>>();
        assert_eq!(pools.first(), Some(&pool_address2));
        assert_eq!(pools.get(1), Some(&pool_address1));

        Ok(())
    }

    #[test]
    fn test_build_swap_path_vec_three_hops() -> Result<()> {
        let mut market = Market::default();

        // Add basic token for start/end
        let wmnt_token = Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18));
        market.add_token(wmnt_token);

        // tokens
        let token1 = Address::random();
        market.add_token(Token::new(token1));
        let token2 = Address::random();
        market.add_token(Token::new(token2));

        // Swap pool: wmnt -> token1
        let pool_address1 = Address::random();
        let mock_pool = PoolWrapper::new(Arc::new(MockPool { address: pool_address1, token0: token1, token1: WMNT }));
        market.add_pool(mock_pool);

        // Swap pool: token1 -> token2
        let pool_address2 = Address::random();
        let mock_pool2 = PoolWrapper::new(Arc::new(MockPool { address: pool_address2, token0: token1, token1: token2 }));
        market.add_pool(mock_pool2);

        // Swap pool: token2 -> wmnt
        let pool_address3 = Address::random();
        let mock_pool3 = PoolWrapper::new(Arc::new(MockPool { address: pool_address3, token0: token2, token1: WMNT }));
        market.add_pool(mock_pool3.clone());

        // under test
        let swap_paths = market.update_paths(mock_pool3)?;

        for swap_path in swap_paths.iter() {
            println!("Swap path: {}", swap_path);
        }

        // verify that we have two paths, with 3 pools and 4 tokens
        assert_eq!(swap_paths.len(), 2);
        assert_eq!(swap_paths.first().unwrap().len(), 3);
        assert_eq!(swap_paths.first().unwrap().tokens_count(), 4);
        assert_eq!(swap_paths.get(1).unwrap().len(), 3);
        assert_eq!(swap_paths.get(1).unwrap().tokens_count(), 4);

        // the order of the swap paths is not deterministic
        let (first_path, second_path) = if swap_paths.first().unwrap().tokens.get(1).unwrap().get_address() == token1 {
            (swap_paths.first().unwrap(), swap_paths.get(1).unwrap())
        } else {
            (swap_paths.get(1).unwrap(), swap_paths.first().unwrap())
        };

        // first path wmnt -> token1 -> token2 -> wmnt
        let tokens = first_path.tokens.iter().map(|token| token.get_address()).collect::<Vec<Address>>();
        assert_eq!(tokens.first(), Some(&WMNT));
        assert_eq!(tokens.get(1), Some(&token1));
        assert_eq!(tokens.get(2), Some(&token2));
        assert_eq!(tokens.get(3), Some(&WMNT));

        let pools = first_path.pools.iter().map(|pool| pool.get_address()).collect::<Vec<Address>>();
        assert_eq!(pools.first(), Some(&pool_address1));
        assert_eq!(pools.get(1), Some(&pool_address2));
        assert_eq!(pools.get(2), Some(&pool_address3));

        // other way around
        let tokens = second_path.tokens.iter().map(|token| token.get_address()).collect::<Vec<Address>>();
        assert_eq!(tokens.first(), Some(&WMNT));
        assert_eq!(tokens.get(1), Some(&token2));
        assert_eq!(tokens.get(2), Some(&token1));
        assert_eq!(tokens.get(3), Some(&WMNT));

        let pools = second_path.pools.iter().map(|pool| pool.get_address()).collect::<Vec<Address>>();
        assert_eq!(pools.first(), Some(&pool_address3));
        assert_eq!(pools.get(1), Some(&pool_address2));
        assert_eq!(pools.get(2), Some(&pool_address1));

        Ok(())
    }

    #[test]
    fn test_build_swap_path_vec_four_hops() -> Result<()> {
        let mut market = Market::new(MarketConfigSection::default().with_max_hops(4));

        // Add basic token for start/end
        let wmnt_token = Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18));
        market.add_token(wmnt_token);

        // tokens
        let token1 = Address::random();
        market.add_token(Token::new(token1));
        let token2 = Address::random();
        market.add_token(Token::new(token2));
        let token3 = Address::random();
        market.add_token(Token::new(token3));

        // Swap pool: wmnt -> token1
        let pool_address1 = Address::random();
        let mock_pool1 = PoolWrapper::new(Arc::new(MockPool { address: pool_address1, token0: WMNT, token1 }));
        market.add_pool(mock_pool1);

        // Swap pool: token1 -> token2
        let pool_address2 = Address::random();
        let mock_pool2 = PoolWrapper::new(Arc::new(MockPool { address: pool_address2, token0: token1, token1: token2 }));
        market.add_pool(mock_pool2);

        // Swap pool: token2 -> token3
        let pool_address3 = Address::random();
        let mock_pool3 = PoolWrapper::new(Arc::new(MockPool { address: pool_address3, token0: token2, token1: token3 }));
        market.add_pool(mock_pool3);

        // Swap pool: token3 -> wmnt
        let pool_address4 = Address::random();
        let mock_pool4 = PoolWrapper::new(Arc::new(MockPool { address: pool_address4, token0: token3, token1: WMNT }));
        market.add_pool(mock_pool4.clone());

        // under test
        let swap_paths = market.update_paths(mock_pool4)?;

        // verify that we have two paths, with 4 pools and 5 tokens
        assert_eq!(swap_paths.len(), 2);
        assert_eq!(swap_paths.first().unwrap().len(), 4);
        assert_eq!(swap_paths.first().unwrap().tokens_count(), 5);
        assert_eq!(swap_paths.get(1).unwrap().len(), 4);
        assert_eq!(swap_paths.get(1).unwrap().tokens_count(), 5);

        // the order of the swap paths is not deterministic
        let (first_path, second_path) = if swap_paths.first().unwrap().tokens.get(1).unwrap().get_address() == token1 {
            (swap_paths.first().unwrap(), swap_paths.get(1).unwrap())
        } else {
            (swap_paths.get(1).unwrap(), swap_paths.first().unwrap())
        };

        // first path wmnt -> token1 -> token2 -> token3 -> wmnt
        let tokens = first_path.tokens.iter().map(|token| token.get_address()).collect::<Vec<Address>>();
        assert_eq!(tokens.first(), Some(&WMNT));
        assert_eq!(tokens.get(1), Some(&token1));
        assert_eq!(tokens.get(2), Some(&token2));
        assert_eq!(tokens.get(3), Some(&token3));
        assert_eq!(tokens.get(4), Some(&WMNT));

        let pools = first_path.pools.iter().map(|pool| pool.get_address()).collect::<Vec<Address>>();
        assert_eq!(pools.first(), Some(&pool_address1));
        assert_eq!(pools.get(1), Some(&pool_address2));
        assert_eq!(pools.get(2), Some(&pool_address3));
        assert_eq!(pools.get(3), Some(&pool_address4));

        // other way around
        let tokens = second_path.tokens.iter().map(|token| token.get_address()).collect::<Vec<Address>>();
        assert_eq!(tokens.first(), Some(&WMNT));
        assert_eq!(tokens.get(1), Some(&token3));
        assert_eq!(tokens.get(2), Some(&token2));
        assert_eq!(tokens.get(3), Some(&token1));
        assert_eq!(tokens.get(4), Some(&WMNT));

        let pools = second_path.pools.iter().map(|pool| pool.get_address()).collect::<Vec<Address>>();
        assert_eq!(pools.first(), Some(&pool_address4));
        assert_eq!(pools.get(1), Some(&pool_address3));
        assert_eq!(pools.get(2), Some(&pool_address2));
        assert_eq!(pools.get(3), Some(&pool_address1));

        Ok(())
    }
}
