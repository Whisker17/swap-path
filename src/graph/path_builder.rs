use crate::SwapPath;
use crate::graph::TokenGraph;
use crate::swap_path_set::SwapPathSet;
use petgraph::prelude::*;
use std::collections::VecDeque;
use tracing::error;

/// State of the search for all paths between two nodes in the graph.
#[derive(Debug)]
struct PathState {
    node: NodeIndex<usize>,
    current_path: SwapPath,
    hops: u8,
    reached_end: bool,
}

/// Find all paths between two nodes in the graph with a maximum number of hops.
/// The search is performed in a depth-first manner.
/// The `start_node` must be the end of the initial swap path. We do not need to search for this path.
/// The `end_node` must be the index of the token we want to reach e.g. the start of our swap path.
/// The search will not return paths that contain the same pool twice, and will not return paths
/// that contain the same pool as the first pool unless `allow_duplicate_first` is set to true.
/// We do allow this because for non-basic token paths, we want to allow the path to exit where it started.
/// e.g. DAI -> token -> DAI
pub fn find_all_paths(
    token_graph: &TokenGraph,
    initial_swap_path: SwapPath,
    start_node: NodeIndex<usize>,
    end_node: NodeIndex<usize>,
    max_hops: u8,
    allow_duplicate_first: bool,
) -> eyre::Result<SwapPathSet> {
    if let Some(last_token) = initial_swap_path.tokens.last() {
        if last_token.is_wrapped() {
            error!(
                "Initial swap path must not end with a wrapped token start_node={}, end_node={}, initial_swap_path={:?}, allowing duplicate first={}",
                token_graph.graph.node_weight(start_node).map(|node| node.token.get_address()).unwrap_or_default(),
                token_graph.graph.node_weight(end_node).map(|node| node.token.get_address()).unwrap_or_default(),
                initial_swap_path,
                allow_duplicate_first
            );
            return Err(eyre::eyre!("Initial swap path must not end with a wrapped token"));
        }
    } else {
        error!(
            "Initial swap path last token must be not empty start_node={}, end_node={}, initial_swap_path={:?}, allowing duplicate first={}",
            token_graph.graph.node_weight(start_node).map(|node| node.token.get_address()).unwrap_or_default(),
            token_graph.graph.node_weight(end_node).map(|node| node.token.get_address()).unwrap_or_default(),
            initial_swap_path,
            allow_duplicate_first
        );
        return Err(eyre::eyre!("Initial swap path last token must be not empty"));
    }

    let mut all_swap_paths = SwapPathSet::new();
    let mut stack = VecDeque::new();

    // Initialize the search
    stack.push_back(PathState { node: start_node, current_path: initial_swap_path.clone(), hops: 1, reached_end: false });

    let mut searched_path_counter = 0;

    while let Some(PathState { node, current_path, hops, reached_end }) = stack.pop_back() {
        // This is the upper limit to prevent infinite loops in case of a bug and limit the search space
        if searched_path_counter > 500_000 {
            error!(
                "Find all path too many iterations sanity check failed for start_node={}, end_node={}, initial_swap_path={:?}, allowing duplicate first={}",
                token_graph.graph.node_weight(start_node).map(|node| node.token.get_address()).unwrap_or_default(),
                token_graph.graph.node_weight(end_node).map(|node| node.token.get_address()).unwrap_or_default(),
                initial_swap_path,
                allow_duplicate_first
            );
            break;
        }
        searched_path_counter += 1;

        // If we reached the target node, add the path to results e.g. token=WETH
        if node == end_node {
            if current_path.len() > 1 {
                all_swap_paths.insert(current_path);
            }
            continue;
        }

        // If we've used all allowed hops, skip expansion
        if hops >= max_hops {
            continue;
        }

        // We do not like to travel further after WETH token
        if reached_end {
            continue;
        }

        // Explore neighbors
        for edge in token_graph.graph.edges(node) {
            let to_token = token_graph.graph.node_weight(edge.target()).unwrap().token.clone();

            // Process each pool for this edge
            for pool in edge.weight().values() {
                if !pool.is_active {
                    continue;
                }

                if current_path.contains_pool(&pool.inner) {
                    if !allow_duplicate_first {
                        continue;
                    } else if current_path.pools.first().map(|p| p.get_pool_id() != pool.inner.get_pool_id()).unwrap_or(false) {
                        // We like for none basic token path allow to exit where we started
                        continue;
                    }
                }

                // Create new path only when we know it's valid
                let mut new_path = current_path.clone();
                if new_path.push_swap_hop(to_token.clone(), pool.inner.clone()).is_ok() {
                    if to_token.is_wrapped() {
                        // After WETH token, we can not go further
                        stack.push_back(PathState { node: edge.target(), current_path: new_path, hops: hops + 1, reached_end: true });
                    } else {
                        stack.push_back(PathState { node: edge.target(), current_path: new_path, hops: hops + 1, reached_end: false });
                    }
                }
            }
        }
    }

    Ok(all_swap_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::TokenGraph;
    use crate::{MockPool, PoolWrapper, Token};
    use alloy_primitives::Address;
    use std::sync::Arc;

    #[test]
    fn test_simple_path_with_start_edge() -> eyre::Result<()> {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let token3 = Arc::new(Token::random());

        let mut token_graph = TokenGraph::new();
        token_graph.add_or_get_token_idx_by_token(token1.clone());
        token_graph.add_or_get_token_idx_by_token(token2.clone());
        token_graph.add_or_get_token_idx_by_token(token3.clone());

        let pool_1_2 = PoolWrapper::from(MockPool::new(token1.get_address(), token2.get_address(), Address::random()));
        let pool_2_3 = PoolWrapper::from(MockPool::new(token2.get_address(), token3.get_address(), Address::random()));
        let pool_3_1 = PoolWrapper::from(MockPool::new(token3.get_address(), token1.get_address(), Address::random()));

        token_graph.add_pool(pool_1_2.clone())?;
        token_graph.add_pool(pool_2_3.clone())?;
        token_graph.add_pool(pool_3_1.clone())?;

        let start_node_index = token_graph.token_index.get(&token2.get_address()).unwrap();
        let end_node_index = token_graph.token_index.get(&token1.get_address()).unwrap();

        let initial_swap_path = SwapPath::new_first(token1.clone(), token2.clone(), pool_1_2.clone());

        let swap_paths = find_all_paths(&token_graph, initial_swap_path, *start_node_index, *end_node_index, 3, false)?;

        // we search only in one direction and expect only one path
        assert_eq!(swap_paths.len(), 1);

        Ok(())
    }

    #[test]
    fn test_not_connected_path() -> eyre::Result<()> {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let token3 = Arc::new(Token::random());

        let mut token_graph = TokenGraph::new();
        token_graph.add_or_get_token_idx_by_token(token1.clone());
        token_graph.add_or_get_token_idx_by_token(token2.clone());
        token_graph.add_or_get_token_idx_by_token(token3.clone());

        let pool_1_2 = PoolWrapper::from(MockPool::new(token1.get_address(), token2.get_address(), Address::random()));
        let pool_3_1 = PoolWrapper::from(MockPool::new(token3.get_address(), token1.get_address(), Address::random()));

        token_graph.add_pool(pool_1_2.clone())?;
        // leave the gap
        token_graph.add_pool(pool_3_1.clone())?;

        let start_node_index = token_graph.token_index.get(&token2.get_address()).unwrap();
        let end_node_index = token_graph.token_index.get(&token1.get_address()).unwrap();

        let initial_swap_path = SwapPath::new_first(token1.clone(), token2.clone(), pool_1_2.clone());

        let swap_paths = find_all_paths(&token_graph, initial_swap_path, *start_node_index, *end_node_index, 3, false)?;

        // we search only in one direction and expect no path
        assert_eq!(swap_paths.len(), 0);

        Ok(())
    }
}
