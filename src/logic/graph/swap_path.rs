use super::super::pools::pool_id::PoolId;
use super::swap_path_hash::SwapPathHash;
use crate::{PoolWrapper, Token};
use eyre::Result;

use sha2::digest::Update;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Debug, Default, Eq)]
pub struct SwapPath {
    // hash of the path. We use this to compare paths in the database
    pub swap_path_hash: SwapPathHash,
    // internal lookup for faster contains_pool
    pub pools_map: HashSet<PoolId>,
    // The tokens of the path e.g. token0 -> token1 -> token0
    pub tokens: Vec<Arc<Token>>,
    // The pools of the path e.g. pool0 -> pool1
    pub pools: Vec<PoolWrapper>,
}

impl Display for SwapPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SwapPath(pools={:?}, tokens={:?})",
            self.pools.iter().map(|p| format!("{:#}", p.get_address())).collect::<Vec<String>>(),
            self.tokens.iter().map(|t| format!("{:#}", t.get_address())).collect::<Vec<String>>()
        )
    }
}

impl SwapPath {
    /// Create a new swap path for a list of tokens and pools
    pub fn new<T: Into<Arc<Token>>, P: Into<PoolWrapper>>(tokens: Vec<T>, pools: Vec<P>) -> Self {
        let mut pools_vec = vec![];
        let mut pools_map = HashSet::new();
        for pool in pools {
            let pool: PoolWrapper = pool.into();
            pools_map.insert(pool.get_pool_id());
            pools_vec.push(pool);
        }
        let tokens: Vec<Arc<Token>> = tokens.into_iter().map(|i| i.into()).collect();
        let swap_path_hash = generate_swap_path_hash(&tokens, &pools_vec);

        SwapPath { swap_path_hash, tokens, pools: pools_vec, pools_map }
    }

    /// Create a new swap path with only one hop
    pub fn new_first(token_from: Arc<Token>, token_to: Arc<Token>, pool: PoolWrapper) -> Self {
        let pool_id = pool.get_pool_id();
        let tokens = vec![token_from, token_to];
        let pools = vec![pool];
        let swap_path_hash = generate_swap_path_hash(&tokens, &pools);

        SwapPath { swap_path_hash, tokens, pools, pools_map: HashSet::from([pool_id]) }
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.pools.is_empty()
    }

    pub fn tokens_count(&self) -> usize {
        self.tokens.len()
    }

    /// Invert the swap path
    pub fn invert(&self) -> Self {
        let mut tokens = self.tokens.clone();
        tokens.reverse();
        let mut pools = self.pools.clone();
        pools.reverse();
        let swap_path_hash = generate_swap_path_hash(&tokens, &pools);

        SwapPath { swap_path_hash, tokens, pools, pools_map: self.pools_map.clone() }
    }

    /// Push a new pool hop to the swap path. The caller is responsible for checking that the pool is connected
    pub fn push_swap_hop(&mut self, token_to: Arc<Token>, pool: PoolWrapper) -> Result<&mut Self> {
        if self.is_empty() {
            return Err(eyre::eyre!("Swap path is empty"));
        }

        self.pools_map.insert(pool.get_pool_id());
        self.tokens.push(token_to);
        self.pools.push(pool);

        self.swap_path_hash = generate_swap_path_hash(&self.tokens, &self.pools);

        Ok(self)
    }

    /// Check if the swap path contains a pool
    pub fn contains_pool(&self, pool: &PoolWrapper) -> bool {
        self.pools_map.contains(&pool.get_pool_id())
    }

    /// The hop count of the swap path
    pub fn len(&self) -> usize {
        self.pools.len()
    }
}

impl Hash for SwapPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tokens.hash(state);
        self.pools.hash(state);
    }
}

impl PartialEq for SwapPath {
    fn eq(&self, other: &Self) -> bool {
        self.tokens == other.tokens && self.pools == other.pools
    }
}

/// Hash all the addresses of the tokens and pools in the path to a sha256 hash.
/// To have a stable reproducible hash and to make it easy to use in other languages.
pub fn generate_swap_path_hash(tokens: &[Arc<Token>], pools: &[PoolWrapper]) -> SwapPathHash {
    let mut hasher = Sha256::new();

    for token in tokens.iter() {
        Update::update(&mut hasher, token.get_address().as_slice());
    }
    for pool in pools.iter() {
        Update::update(&mut hasher, pool.get_address().as_slice());
    }

    let hash_slice: [u8; 32] = hasher.finalize().into();
    SwapPathHash(hash_slice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MockPool, Pool, PoolWrapper, Token};
    use alloy_primitives::Address;
    use std::sync::Arc;

    #[test]
    fn test_serialize_swap_path_hash() {
        let swap_path_hash = SwapPathHash([1; 32]);

        let serialized = serde_json::to_string(&swap_path_hash).unwrap();
        let deserialized: SwapPathHash = serde_json::from_str(&serialized).unwrap();

        assert_eq!(swap_path_hash, deserialized);
    }

    #[test]
    fn test_serialize_swap_path() {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let token3 = Arc::new(Token::random());

        let pool_1_2 = PoolWrapper::new(Arc::new(MockPool::new(token1.get_address(), token2.get_address(), Address::random())));
        let pool_2_3 = PoolWrapper::new(Arc::new(MockPool::new(token2.get_address(), token3.get_address(), Address::random())));
        let pool_3_1 = PoolWrapper::new(Arc::new(MockPool::new(token3.get_address(), token1.get_address(), Address::random())));

        let swap_path =
            SwapPath::new(vec![token1.clone(), token2.clone(), token3.clone()], vec![pool_1_2.clone(), pool_2_3.clone(), pool_3_1.clone()]);

        // 序列化测试已移除，因为SwapPath不再支持序列化
        assert!(swap_path.contains_pool(&pool_1_2));
    }

    #[test]
    fn test_new_swap_path() {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let token3 = Arc::new(Token::random());

        let pool_1_2 = MockPool::new(token1.get_address(), token2.get_address(), Address::random());
        let pool_2_3 = MockPool::new(token2.get_address(), token3.get_address(), Address::random());
        let pool_3_1 = MockPool::new(token3.get_address(), token1.get_address(), Address::random());

        let swap_path =
            SwapPath::new(vec![token1.clone(), token2.clone(), token3.clone()], vec![pool_1_2.clone(), pool_2_3.clone(), pool_3_1.clone()]);

        assert!(!swap_path.is_empty());
        assert_eq!(swap_path.tokens_count(), 3);
        assert_eq!(swap_path.len(), 3);
        assert_eq!(swap_path.swap_path_hash, generate_swap_path_hash(&swap_path.tokens, &swap_path.pools));

        assert_eq!(swap_path.tokens.first().unwrap().get_address(), token1.get_address());
        assert_eq!(swap_path.tokens.get(1).unwrap().get_address(), token2.get_address());
        assert_eq!(swap_path.tokens.get(2).unwrap().get_address(), token3.get_address());

        assert!(swap_path.contains_pool(&PoolWrapper::from(pool_1_2.clone())));
        assert!(swap_path.contains_pool(&PoolWrapper::from(pool_2_3.clone())));
        assert!(swap_path.contains_pool(&PoolWrapper::from(pool_3_1.clone())));

        assert_eq!(swap_path.pools.first().unwrap().pool.get_address(), pool_1_2.get_address());
        assert_eq!(swap_path.pools.get(1).unwrap().pool.get_address(), pool_2_3.get_address());
        assert_eq!(swap_path.pools.get(2).unwrap().pool.get_address(), pool_3_1.get_address());
    }

    #[test]
    fn test_new_swap_path_first() {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());

        let pool_1_2 = MockPool::new(token1.get_address(), token2.get_address(), Address::random());

        let swap_path = SwapPath::new_first(token1.clone(), token2.clone(), pool_1_2.clone().into());

        assert!(!swap_path.is_empty());
        assert_eq!(swap_path.tokens_count(), 2);
        assert_eq!(swap_path.len(), 1);
        assert_eq!(swap_path.swap_path_hash, generate_swap_path_hash(&swap_path.tokens, &swap_path.pools));

        assert_eq!(swap_path.tokens.first().unwrap().get_address(), token1.get_address());
        assert_eq!(swap_path.tokens.get(1).unwrap().get_address(), token2.get_address());

        assert!(swap_path.contains_pool(&PoolWrapper::from(pool_1_2.clone())));

        assert_eq!(swap_path.pools.first().unwrap().pool.get_address(), pool_1_2.get_address());
    }

    #[test]
    fn test_invert_swap_path() {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let token3 = Arc::new(Token::random());

        let pool_1_2 = MockPool::new(token1.get_address(), token2.get_address(), Address::random());
        let pool_2_3 = MockPool::new(token2.get_address(), token3.get_address(), Address::random());
        let pool_3_1 = MockPool::new(token3.get_address(), token1.get_address(), Address::random());

        let swap_path =
            SwapPath::new(vec![token1.clone(), token2.clone(), token3.clone()], vec![pool_1_2.clone(), pool_2_3.clone(), pool_3_1.clone()]);

        let inverted_swap_path = swap_path.invert();

        assert!(!inverted_swap_path.is_empty());
        assert_eq!(inverted_swap_path.tokens_count(), 3);
        assert_eq!(inverted_swap_path.len(), 3);
        assert_eq!(inverted_swap_path.swap_path_hash, generate_swap_path_hash(&inverted_swap_path.tokens, &inverted_swap_path.pools));

        assert_eq!(inverted_swap_path.tokens.first().unwrap().get_address(), token3.get_address());
        assert_eq!(inverted_swap_path.tokens.get(1).unwrap().get_address(), token2.get_address());
        assert_eq!(inverted_swap_path.tokens.get(2).unwrap().get_address(), token1.get_address());

        assert!(inverted_swap_path.contains_pool(&PoolWrapper::from(pool_3_1.clone())));
        assert!(inverted_swap_path.contains_pool(&PoolWrapper::from(pool_2_3.clone())));
        assert!(inverted_swap_path.contains_pool(&PoolWrapper::from(pool_1_2.clone())));

        assert_eq!(inverted_swap_path.pools.first().unwrap().pool.get_address(), pool_3_1.get_address());
        assert_eq!(inverted_swap_path.pools.get(1).unwrap().pool.get_address(), pool_2_3.get_address());
        assert_eq!(inverted_swap_path.pools.get(2).unwrap().pool.get_address(), pool_1_2.get_address());
    }

    #[test]
    fn test_push_swap_hop() {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let token3 = Arc::new(Token::random());

        let pool_1_2 = MockPool::new(token1.get_address(), token2.get_address(), Address::random());
        let pool_2_3 = MockPool::new(token2.get_address(), token3.get_address(), Address::random());
        let pool_3_1 = MockPool::new(token3.get_address(), token1.get_address(), Address::random());

        let mut swap_path = SwapPath::new(vec![token1.clone(), token2.clone()], vec![pool_1_2.clone(), pool_2_3.clone()]);

        let result = swap_path.push_swap_hop(token3.clone(), pool_3_1.clone().into());
        assert!(result.is_ok());

        assert!(!swap_path.is_empty());
        assert_eq!(swap_path.tokens_count(), 3);
        assert_eq!(swap_path.len(), 3);
        assert_eq!(swap_path.swap_path_hash, generate_swap_path_hash(&swap_path.tokens, &swap_path.pools));

        assert_eq!(swap_path.tokens.first().unwrap().get_address(), token1.get_address());
        assert_eq!(swap_path.tokens.get(1).unwrap().get_address(), token2.get_address());
        assert_eq!(swap_path.tokens.get(2).unwrap().get_address(), token3.get_address());

        assert!(swap_path.contains_pool(&PoolWrapper::from(pool_1_2.clone())));
        assert!(swap_path.contains_pool(&PoolWrapper::from(pool_2_3.clone())));
        assert!(swap_path.contains_pool(&PoolWrapper::from(pool_3_1.clone())));

        assert_eq!(swap_path.pools.first().unwrap().pool.get_address(), pool_1_2.get_address());
        assert_eq!(swap_path.pools.get(1).unwrap().pool.get_address(), pool_2_3.get_address());
        assert_eq!(swap_path.pools.get(2).unwrap().pool.get_address(), pool_3_1.get_address());
    }

    #[test]
    fn test_push_swap_hop_empty() {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let pool_1_2 = MockPool::new(token1.get_address(), token2.get_address(), Address::random());

        let mut swap_path = SwapPath { swap_path_hash: SwapPathHash([0; 32]), tokens: vec![], pools: vec![], pools_map: HashSet::new() };

        let result = swap_path.push_swap_hop(token2.clone(), pool_1_2.clone().into());
        assert!(result.is_err());
    }

    #[test]
    fn test_swap_path_hash() {
        let token1 = Arc::new(Token::repeat_byte(1));
        let token2 = Arc::new(Token::repeat_byte(2));
        let token3 = Arc::new(Token::repeat_byte(3));

        let pool_1_2 = MockPool::new(token1.get_address(), token2.get_address(), Address::repeat_byte(4));
        let pool_2_3 = MockPool::new(token2.get_address(), token3.get_address(), Address::repeat_byte(5));
        let pool_3_1 = MockPool::new(token3.get_address(), token1.get_address(), Address::repeat_byte(6));

        let swap_path =
            SwapPath::new(vec![token1.clone(), token2.clone(), token3.clone()], vec![pool_1_2.clone(), pool_2_3.clone(), pool_3_1.clone()]);

        let swap_path_hash = generate_swap_path_hash(&swap_path.tokens, &swap_path.pools);

        assert_eq!(swap_path.swap_path_hash, swap_path_hash);
        assert_eq!(swap_path_hash.to_string(), "0xc628ae21db2d836c87150c0ebf85ace60fef81298d7f490797f4298205fa9bfd");
    }
}
