use crate::constants::{NATIVE, WETH};
use crate::graph::path_builder::find_all_paths;
use crate::pool_id::PoolId;
use crate::swap_path_set::SwapPathSet;
use crate::{PoolWrapper, SwapPath, Token};
use ahash::RandomState;
use alloy_primitives::Address;
use eyre::eyre;
use petgraph::graph::{EdgeIndex, NodeIndex, UnGraph};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;

pub type FastHasher = RandomState;
/// FastHashMap using ahash
pub type FastHashMap<K, V> = HashMap<K, V, FastHasher>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenGraph {
    // We not use stable right now because we never delete nodes or edges
    // The graph consists of nodes (tokens) and edges (pools). The edges are a hashmap of pool id's and the pool.
    pub graph: UnGraph<TokenNode, HashMap<PoolId, PoolEdge>, usize>,
    // pool_address -> pool
    pub pools: HashMap<PoolId, PoolWrapper>,
    // token_address -> token (Keep reference for fast access of token details)
    pub tokens: HashMap<Address, Arc<Token>>,
    // token -> node index
    pub token_index: FastHashMap<Address, NodeIndex<usize>>,
    // pool -> edge index (in an edge is a hashmap of pools where the pool is part of)
    pub pool_index: FastHashMap<PoolId, EdgeIndex<usize>>,
}

impl TokenGraph {
    pub fn new() -> Self {
        Self {
            graph: UnGraph::default(),
            pools: HashMap::new(),
            tokens: HashMap::default(),
            token_index: FastHashMap::default(),
            pool_index: FastHashMap::default(),
        }
    }

    pub fn set_pool_active(&mut self, pool_id: PoolId, is_active: bool) -> eyre::Result<()> {
        if let Some(edge_index) = self.pool_index.get(&pool_id) {
            let Some(edge) = self.graph.edge_weight_mut(*edge_index) else {
                return Err(eyre!("Edge not found in graph: {:?}", pool_id));
            };
            let Some(pool) = edge.get_mut(&pool_id) else {
                return Err(eyre!("Pool not found in edge: {:?}", pool_id));
            };
            pool.is_active = is_active;
        } else {
            return Err(eyre!("Pool not found in graph: {:?}", pool_id));
        }
        Ok(())
    }

    pub fn add_or_get_token_idx_by_token(&mut self, arc_token: Arc<Token>) -> NodeIndex<usize> {
        *self.token_index.entry(arc_token.get_address()).or_insert_with(|| {
            let node = TokenNode::new(arc_token.clone());
            let idx = self.graph.add_node(node);
            self.tokens.insert(arc_token.get_address(), arc_token);
            idx
        })
    }

    pub(crate) fn add_or_get_token_idx_by_address(&mut self, address: Address) -> NodeIndex<usize> {
        if let Some(&idx) = self.token_index.get(&address) {
            return idx;
        }
        let arc_token = Arc::new(Token::new(address));
        let node = TokenNode::new(arc_token.clone());
        let idx = self.graph.add_node(node);
        self.token_index.insert(address, idx);
        self.tokens.insert(address, arc_token);
        idx
    }

    // Add a new pool as an edge to the graph.
    pub fn add_pool<T: Into<PoolWrapper>>(&mut self, pool: T) -> eyre::Result<()> {
        let pool_wrapper = pool.into();
        let pool_edge = PoolEdge::new(pool_wrapper.clone());

        let swap_directions = pool_wrapper.get_swap_directions();

        for (from_token, to_token) in swap_directions {
            let node_from = self.token_index.get(&from_token).ok_or_else(|| eyre!("Token not found in graph: {:?}", from_token))?;
            let node_to = self.token_index.get(&to_token).ok_or_else(|| eyre!("Token not found in graph: {:?}", to_token))?;

            if let Some(edge_index) = self.graph.find_edge(*node_from, *node_to) {
                let pools = self.graph.edge_weight_mut(edge_index).unwrap();
                if pools.contains_key(&pool_wrapper.get_pool_id()) {
                    continue;
                }
                pools.insert(pool_wrapper.get_pool_id(), pool_edge.clone());
                self.pool_index.insert(pool_wrapper.get_pool_id(), edge_index);
            } else {
                let mut pools = HashMap::new();
                pools.insert(pool_wrapper.get_pool_id(), pool_edge.clone());
                let edge_index = self.graph.add_edge(*node_from, *node_to, pools);
                self.pool_index.insert(pool_wrapper.get_pool_id(), edge_index);
            }
        }

        self.pools.insert(pool_wrapper.get_pool_id(), pool_wrapper);

        Ok(())
    }

    pub fn build_swap_paths(&self, pool: &PoolWrapper, max_hops: u8) -> eyre::Result<Vec<SwapPath>> {
        let mut total_swap_paths = SwapPathSet::new();
        for (from_token_address, to_token_address) in pool.get_swap_directions() {
            let Some(from_node_index) = self.token_index.get(&from_token_address) else {
                return Err(eyre!("Token not found in graph: {:?}", from_token_address));
            };
            let Some(to_node_index) = self.token_index.get(&to_token_address) else {
                return Err(eyre!("Token not found in graph: {:?}", to_token_address));
            };

            let Some(from_token) = self.tokens.get(&from_token_address) else {
                return Err(eyre!("Token not found in graph: {:?}", from_token_address));
            };
            let Some(to_token) = self.tokens.get(&to_token_address) else {
                return Err(eyre!("Token not found in graph: {:?}", to_token_address));
            };

            // We do not want to search for paths with WETH in between
            if to_token.is_wrapped() {
                continue;
            }

            // CASE A: We search from weth token and back to the origin
            if from_token.is_wrapped() {
                let initial_swap_path = SwapPath::new_first(from_token.clone(), to_token.clone(), pool.clone());
                let swap_paths = find_all_paths(self, initial_swap_path, *to_node_index, *from_node_index, max_hops, false)?;
                total_swap_paths.extend(swap_paths.vec());

                // In this case the origin is the native token
                let native_node_index_opt = self.token_index.get(&NATIVE);

                if let Some(native_node_index) = native_node_index_opt {
                    let initial_swap_path = SwapPath::new_first(from_token.clone(), to_token.clone(), pool.clone());
                    let swap_paths = find_all_paths(self, initial_swap_path, *to_node_index, *native_node_index, max_hops, false)?;
                    total_swap_paths.extend(swap_paths.vec());
                }
            }
            // CASE A+: We search from native token and back to native and WETH (uniswap V4)
            else if from_token.is_native() {
                // Search back to native
                let native_node_index_opt = self.token_index.get(&NATIVE);

                if let Some(native_node_index) = native_node_index_opt {
                    let initial_swap_path = SwapPath::new_first(from_token.clone(), to_token.clone(), pool.clone());
                    let swap_paths = find_all_paths(self, initial_swap_path, *to_node_index, *native_node_index, max_hops, false)?;
                    total_swap_paths.extend(swap_paths.vec());
                }

                // Search back to WETH
                let wrapped_node_index_opt = self.token_index.get(&WETH);
                if let Some(wrapped_node_index) = wrapped_node_index_opt {
                    let initial_swap_path = SwapPath::new_first(from_token.clone(), to_token.clone(), pool.clone());
                    let swap_paths = find_all_paths(self, initial_swap_path, *to_node_index, *wrapped_node_index, max_hops, false)?;
                    total_swap_paths.extend(swap_paths.vec());
                }
            }
        }
        // We save the time to travel the graph by inverting the swap paths
        let mut swap_paths_with_inverted = vec![];
        for swap_path in total_swap_paths.vec() {
            swap_paths_with_inverted.push(swap_path.invert());
            swap_paths_with_inverted.push(swap_path);
        }

        Ok(swap_paths_with_inverted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TokenNode {
    pub token: Arc<Token>,
}

impl Display for TokenNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.token.get_address())
    }
}

impl TokenNode {
    pub fn new(token: Arc<Token>) -> Self {
        Self { token }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PoolEdge {
    pub is_active: bool,
    pub inner: PoolWrapper,
}

impl Display for PoolEdge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.inner.get_address())
    }
}

impl PoolEdge {
    pub fn new(pool_wrapper: PoolWrapper) -> Self {
        Self { is_active: true, inner: pool_wrapper }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MockPool, Token};
    use std::sync::Arc;

    #[test]
    fn test_serialize_token_graph() -> eyre::Result<()> {
        let token_weth_arc = Arc::new(Token::new_with_data(WETH, Some("WETH".to_string()), None, Some(18)));
        let token1_arc = Arc::new(Token::new_with_data(Address::repeat_byte(1), Some("token1".to_string()), None, Some(18)));

        let token_weth = token_weth_arc.get_address();
        let token1 = token1_arc.get_address();

        let mock_pool = MockPool { address: Address::repeat_byte(2), token0: token_weth, token1 };
        let wrapped_pool = PoolWrapper::new(Arc::new(mock_pool));

        let mut token_graph = TokenGraph::new();
        token_graph.add_or_get_token_idx_by_token(token_weth_arc.clone());
        token_graph.add_or_get_token_idx_by_token(token1_arc.clone());
        token_graph.add_pool(wrapped_pool.clone())?;

        let serialized = serde_json::to_string(&token_graph)?;
        let token_graph_deserialized: TokenGraph = serde_json::from_str(&serialized)?;

        assert_eq!(token_graph.graph.node_count(), 2);
        assert_eq!(token_graph.graph.edge_count(), 1);
        assert_eq!(token_graph_deserialized.graph.node_count(), 2);
        assert_eq!(token_graph_deserialized.graph.edge_count(), 1);

        Ok(())
    }

    #[test]
    fn test_swap_path_two_hops() -> eyre::Result<()> {
        let token_weth_arc = Arc::new(Token::new_with_data(WETH, Some("WETH".to_string()), None, Some(18)));
        let token1_arc = Arc::new(Token::new_with_data(Address::repeat_byte(1), Some("token1".to_string()), None, Some(18)));

        let token_weth = token_weth_arc.get_address();
        let token1 = token1_arc.get_address();

        let mock_pool = MockPool { address: Address::repeat_byte(2), token0: token_weth, token1 };
        let wrapped_pool = PoolWrapper::new(Arc::new(mock_pool));
        let mock_pool2 = MockPool { address: Address::repeat_byte(3), token0: token_weth, token1 };
        let wrapped_pool2 = PoolWrapper::new(Arc::new(mock_pool2));

        let mut all_pair_graph = TokenGraph::default();
        all_pair_graph.add_or_get_token_idx_by_token(token_weth_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token1_arc.clone());
        all_pair_graph.add_pool(wrapped_pool.clone())?;
        all_pair_graph.add_pool(wrapped_pool2)?;

        let swap_paths = all_pair_graph.build_swap_paths(&wrapped_pool, 3)?;

        assert_eq!(swap_paths.len(), 2);

        Ok(())
    }

    #[test]
    fn test_swap_path_three_hops() -> eyre::Result<()> {
        let token_weth_arc = Arc::new(Token::new_with_data(WETH, Some("WETH".to_string()), None, Some(18)));
        let token1_arc = Arc::new(Token::new_with_data(Address::repeat_byte(1), Some("token1".to_string()), None, Some(18)));
        let token2_arc = Arc::new(Token::new_with_data(Address::repeat_byte(2), Some("token2".to_string()), None, Some(18)));

        let token_weth = token_weth_arc.get_address();
        let token1 = token1_arc.get_address();
        let token2 = token2_arc.get_address();

        let mock_pool = MockPool { address: Address::repeat_byte(3), token0: token_weth, token1 };
        let wrapped_pool = PoolWrapper::new(Arc::new(mock_pool));
        let mock_pool2 = MockPool { address: Address::repeat_byte(4), token0: token1, token1: token2 };
        let wrapped_pool2 = PoolWrapper::new(Arc::new(mock_pool2));
        let mock_pool3 = MockPool { address: Address::repeat_byte(5), token0: token2, token1: token_weth };
        let wrapped_pool3 = PoolWrapper::new(Arc::new(mock_pool3));

        let mut all_pair_graph = TokenGraph::default();
        all_pair_graph.add_or_get_token_idx_by_token(token_weth_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token1_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token2_arc.clone());
        all_pair_graph.add_pool(wrapped_pool.clone())?;
        all_pair_graph.add_pool(wrapped_pool2.clone())?;
        all_pair_graph.add_pool(wrapped_pool3.clone())?;

        let swap_paths = all_pair_graph.build_swap_paths(&wrapped_pool3, 3).unwrap();

        assert_eq!(swap_paths.len(), 2);

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_swap_path_four_hops() -> eyre::Result<()> {
        let token_weth_arc = Arc::new(Token::new_with_data(WETH, Some("WETH".to_string()), None, Some(18)));
        let token1_arc = Arc::new(Token::new_with_data(Address::random(), Some("token1".to_string()), None, Some(18)));
        let token2_arc = Arc::new(Token::new_with_data(Address::random(), Some("token2".to_string()), None, Some(18)));
        let token3_arc = Arc::new(Token::new_with_data(Address::random(), Some("token3".to_string()), None, Some(18)));

        let token_weth = token_weth_arc.get_address();
        let token1 = token1_arc.get_address();
        let token2 = token2_arc.get_address();
        let token3 = token3_arc.get_address();

        let mock_pool = MockPool { address: Address::random(), token0: token_weth, token1 };
        let mock_pool2 = MockPool { address: Address::random(), token0: token1, token1: token2 };
        let mock_pool3 = MockPool { address: Address::random(), token0: token2, token1: token3 };
        let mock_pool4 = MockPool { address: Address::random(), token0: token3, token1: token_weth };
        let wrapped_pool4 = PoolWrapper::new(Arc::new(mock_pool4));

        let mut all_pair_graph = TokenGraph::default();
        all_pair_graph.add_or_get_token_idx_by_token(token_weth_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token1_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token2_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token3_arc.clone());
        all_pair_graph.add_pool(mock_pool)?;
        all_pair_graph.add_pool(mock_pool2)?;
        all_pair_graph.add_pool(mock_pool3)?;
        all_pair_graph.add_pool(wrapped_pool4.clone())?;

        let swap_paths = all_pair_graph.build_swap_paths(&wrapped_pool4, 3).unwrap();

        assert_eq!(swap_paths.len(), 2);

        Ok(())
    }

    #[test]
    fn test_swap_path_four_and_two_hops() -> eyre::Result<()> {
        let mut tokens = HashMap::new();
        let token_weth_arc = Arc::new(Token::new_with_data(WETH, Some("WETH".to_string()), None, Some(18)));
        tokens.insert(token_weth_arc.get_address(), token_weth_arc.clone());
        let token1_arc = Arc::new(Token::new_with_data(Address::random(), Some("token1".to_string()), None, Some(18)));
        tokens.insert(token1_arc.get_address(), token1_arc.clone());
        let token2_arc = Arc::new(Token::new_with_data(Address::random(), Some("token2".to_string()), None, Some(18)));
        tokens.insert(token2_arc.get_address(), token2_arc.clone());
        let token3_arc = Arc::new(Token::new_with_data(Address::random(), Some("token3".to_string()), None, Some(18)));
        tokens.insert(token3_arc.get_address(), token3_arc.clone());

        let token_weth = token_weth_arc.get_address();
        let token1 = token1_arc.get_address();
        let token2 = token2_arc.get_address();
        let token3 = token3_arc.get_address();

        let mock_pool = MockPool { address: Address::random(), token0: token_weth, token1 };
        let mock_pool2 = MockPool { address: Address::random(), token0: token1, token1: token2 };
        let mock_pool3 = MockPool { address: Address::random(), token0: token2, token1: token3 };
        let mock_pool4 = MockPool { address: Address::random(), token0: token3, token1: token_weth };
        let wrapped_pool4 = PoolWrapper::new(Arc::new(mock_pool4));
        let mock_pool5 = MockPool { address: Address::random(), token0: token1, token1: token_weth };

        let mut all_pair_graph = TokenGraph::default();
        all_pair_graph.add_or_get_token_idx_by_token(token_weth_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token1_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token2_arc.clone());
        all_pair_graph.add_or_get_token_idx_by_token(token3_arc.clone());
        all_pair_graph.add_pool(mock_pool)?;
        all_pair_graph.add_pool(mock_pool2)?;
        all_pair_graph.add_pool(mock_pool3)?;
        all_pair_graph.add_pool(wrapped_pool4.clone())?;
        all_pair_graph.add_pool(mock_pool5)?;

        let swap_paths = all_pair_graph.build_swap_paths(&wrapped_pool4, 4)?;

        // 4 hops + 2 hops
        assert_eq!(swap_paths.len(), 4);

        Ok(())
    }
}
