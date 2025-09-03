use super::graph::{SwapPath, TokenGraph};
use crate::utils::constants::WMNT;
use eyre::{eyre, Result};
use petgraph::prelude::*;
use std::collections::HashSet;
use tracing::{debug, info, warn};

/// Pathfinder component responsible for pre-computing all possible arbitrage paths
/// 
/// This implementation uses Depth-First Search (DFS) instead of SPFA for better performance
/// in the specific use case of finding cycles from WMNT back to WMNT with limited hops.
pub struct Pathfinder {
    /// Maximum search depth (number of hops)
    max_hops: u8,
    /// Maximum number of paths to find before stopping (to prevent memory issues)
    max_paths_limit: usize,
}

impl Pathfinder {
    pub fn new(max_hops: u8, max_paths_limit: usize) -> Self {
        Self {
            max_hops,
            max_paths_limit,
        }
    }

    /// Pre-compute all arbitrage paths from WMNT back to WMNT
    /// 
    /// This method implements the pathfinding strategy described in the design document:
    /// - Use DFS with depth limitation
    /// - Find all cycles from WMNT back to WMNT
    /// - Focus on 3-hop and 4-hop paths
    /// - Return the static topology structures for later profit calculation
    pub fn precompute_arbitrage_paths(&self, token_graph: &TokenGraph) -> Result<Vec<SwapPath>> {
        info!("开始预计算套利路径，最大跳数: {}, 路径限制: {}", self.max_hops, self.max_paths_limit);
        
        // Find WMNT node index
        let wmnt_node_index = token_graph.token_index.get(&WMNT)
            .ok_or_else(|| eyre!("WMNT token not found in graph"))?;
        
        let wmnt_token = token_graph.tokens.get(&WMNT)
            .ok_or_else(|| eyre!("WMNT token not found in tokens map"))?;

        // Run DFS to find all cycles
        let mut all_paths: Vec<SwapPath> = Vec::new();
        let mut visited_global = HashSet::new();

        // Explore from WMNT to all its neighbors
        for edge in token_graph.graph.edges(*wmnt_node_index) {
            let neighbor_node = edge.target();
            let neighbor_token = &token_graph.graph.node_weight(neighbor_node).unwrap().token;

            // Skip if neighbor is also WMNT (self-loop) or wrapped token
            if neighbor_token.is_wrapped() {
                continue;
            }

            // Try each pool connecting WMNT to this neighbor
            for pool_edge in edge.weight().values() {
                if !pool_edge.is_active {
                    continue;
                }

                // Create initial path: WMNT -> neighbor
                let initial_path = SwapPath::new_first(
                    wmnt_token.clone(),
                    neighbor_token.clone(),
                    pool_edge.inner.clone(),
                );

                // Run DFS from this initial path
                let paths = self.dfs_find_cycles(
                    token_graph,
                    initial_path,
                    neighbor_node,
                    *wmnt_node_index,
                    1, // We've already taken one hop
                    &mut visited_global,
                )?;

                all_paths.extend(paths);

                // Check if we've hit the limit
                if all_paths.len() >= self.max_paths_limit {
                    warn!("达到路径数量限制 {}, 停止搜索", self.max_paths_limit);
                    break;
                }
            }

            if all_paths.len() >= self.max_paths_limit {
                break;
            }
        }

        info!("预计算完成，找到 {} 条套利路径", all_paths.len());
        debug!("路径长度分布:");
        
        let mut hop_counts = std::collections::HashMap::new();
        for path in &all_paths {
            *hop_counts.entry(path.len()).or_insert(0) += 1;
        }
        
        for (hops, count) in hop_counts {
            debug!("  {}-hop 路径: {} 条", hops, count);
        }

        Ok(all_paths)
    }

    /// Internal DFS implementation to find cycles back to WMNT
    fn dfs_find_cycles(
        &self,
        token_graph: &TokenGraph,
        current_path: SwapPath,
        current_node: NodeIndex<usize>,
        target_node: NodeIndex<usize>, // WMNT node
        current_hops: u8,
        visited_global: &mut HashSet<String>,
    ) -> Result<Vec<SwapPath>> {
        let mut cycles = Vec::new();

        // Generate a signature for this path to avoid duplicates
        let path_signature = self.generate_path_signature(&current_path);
        if visited_global.contains(&path_signature) {
            return Ok(cycles);
        }

        // If we've reached the maximum hops, stop searching
        if current_hops >= self.max_hops {
            return Ok(cycles);
        }

        // Explore all neighbors of current node
        for edge in token_graph.graph.edges(current_node) {
            let neighbor_node = edge.target();
            let neighbor_token = &token_graph.graph.node_weight(neighbor_node).unwrap().token;

            // Check if we've found a cycle back to WMNT
            if neighbor_node == target_node && current_hops >= 2 {
                // We found a valid cycle! Try each pool that connects back to WMNT
                for pool_edge in edge.weight().values() {
                    if !pool_edge.is_active {
                        continue;
                    }

                    // Don't reuse the same pool in a path
                    if current_path.contains_pool(&pool_edge.inner) {
                        continue;
                    }

                    // Create the final path back to WMNT
                    let mut final_path = current_path.clone();
                    if final_path.push_swap_hop(neighbor_token.clone(), pool_edge.inner.clone()).is_ok() {
                        visited_global.insert(self.generate_path_signature(&final_path));
                        cycles.push(final_path);
                    }
                }
                continue; // Don't continue exploring from target node
            }

            // Skip if neighbor is wrapped (we only want to end at WMNT)
            if neighbor_token.is_wrapped() && neighbor_node != target_node {
                continue;
            }

            // Try each pool connecting to this neighbor
            for pool_edge in edge.weight().values() {
                if !pool_edge.is_active {
                    continue;
                }

                // Don't reuse pools in the same path
                if current_path.contains_pool(&pool_edge.inner) {
                    continue;
                }

                // Don't revisit the same token (except for the target WMNT)
                let neighbor_address = neighbor_token.get_address();
                if neighbor_node != target_node && 
                   current_path.tokens.iter().any(|token| token.get_address() == neighbor_address) {
                    continue;
                }

                // Create new path with this hop
                let mut new_path = current_path.clone();
                if new_path.push_swap_hop(neighbor_token.clone(), pool_edge.inner.clone()).is_ok() {
                    // Recursively search from this new position
                    let deeper_cycles = self.dfs_find_cycles(
                        token_graph,
                        new_path,
                        neighbor_node,
                        target_node,
                        current_hops + 1,
                        visited_global,
                    )?;
                    cycles.extend(deeper_cycles);
                }
            }
        }

        Ok(cycles)
    }

    /// Generate a unique signature for a path to avoid duplicates
    fn generate_path_signature(&self, path: &SwapPath) -> String {
        let mut signature = String::new();
        
        // Include token sequence
        for token in &path.tokens {
            signature.push_str(&format!("{:?}-", token.get_address()));
        }
        
        signature.push('|');
        
        // Include pool sequence
        for pool in &path.pools {
            signature.push_str(&format!("{:?}-", pool.get_pool_id()));
        }
        
        signature
    }
}

/// Helper function to create pathfinder with default settings for 3-4 hop arbitrage
pub fn create_arbitrage_pathfinder() -> Pathfinder {
    Pathfinder::new(4, 50_000) // Max 4 hops, up to 50k paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MockPool, PoolWrapper, Token};
    use alloy_primitives::Address;
    use std::sync::Arc;

    fn create_test_graph_with_wmnt_cycle() -> Result<TokenGraph> {
        let mut token_graph = TokenGraph::new();

        // Create tokens
        let wmnt_token = Arc::new(Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18)));
        let token1 = Arc::new(Token::new_with_data(Address::repeat_byte(1), Some("TOKEN1".to_string()), None, Some(18)));
        let token2 = Arc::new(Token::new_with_data(Address::repeat_byte(2), Some("TOKEN2".to_string()), None, Some(18)));

        // Add tokens to graph
        token_graph.add_or_get_token_idx_by_token(wmnt_token.clone());
        token_graph.add_or_get_token_idx_by_token(token1.clone());
        token_graph.add_or_get_token_idx_by_token(token2.clone());

        // Create pools: WMNT <-> TOKEN1 <-> TOKEN2 <-> WMNT
        let pool1 = PoolWrapper::from(MockPool::new(
            WMNT,
            Address::repeat_byte(1),
            Address::repeat_byte(10),
        ));
        let pool2 = PoolWrapper::from(MockPool::new(
            Address::repeat_byte(1),
            Address::repeat_byte(2),
            Address::repeat_byte(11),
        ));
        let pool3 = PoolWrapper::from(MockPool::new(
            Address::repeat_byte(2),
            WMNT,
            Address::repeat_byte(12),
        ));

        token_graph.add_pool(pool1)?;
        token_graph.add_pool(pool2)?;
        token_graph.add_pool(pool3)?;

        Ok(token_graph)
    }

    #[test]
    fn test_pathfinder_finds_simple_cycle() -> Result<()> {
        let token_graph = create_test_graph_with_wmnt_cycle()?;
        let pathfinder = Pathfinder::new(4, 1000);

        let paths = pathfinder.precompute_arbitrage_paths(&token_graph)?;

        // Should find at least one cycle: WMNT -> TOKEN1 -> TOKEN2 -> WMNT
        assert!(paths.len() > 0);
        
        // Check that we have a valid 3-hop path
        let has_3_hop = paths.iter().any(|path| path.len() == 3);
        assert!(has_3_hop, "Should find at least one 3-hop cycle");

        // Verify the cycle starts and ends with WMNT
        for path in &paths {
            assert_eq!(path.tokens.first().unwrap().get_address(), WMNT);
            assert_eq!(path.tokens.last().unwrap().get_address(), WMNT);
        }

        Ok(())
    }

    #[test]
    fn test_pathfinder_respects_hop_limit() -> Result<()> {
        let token_graph = create_test_graph_with_wmnt_cycle()?;
        let pathfinder = Pathfinder::new(2, 1000); // Very restrictive hop limit

        let paths = pathfinder.precompute_arbitrage_paths(&token_graph)?;

        // With max 2 hops, we shouldn't find the 3-hop cycle
        for path in &paths {
            assert!(path.len() <= 2);
        }

        Ok(())
    }

    #[test]
    fn test_pathfinder_avoids_duplicate_paths() -> Result<()> {
        let token_graph = create_test_graph_with_wmnt_cycle()?;
        let pathfinder = Pathfinder::new(4, 1000);

        let paths = pathfinder.precompute_arbitrage_paths(&token_graph)?;

        // Check for duplicates by comparing path signatures
        let mut signatures = HashSet::new();
        let mut duplicate_found = false;

        for path in &paths {
            let signature = pathfinder.generate_path_signature(path);
            if signatures.contains(&signature) {
                duplicate_found = true;
                break;
            }
            signatures.insert(signature);
        }

        assert!(!duplicate_found, "Found duplicate paths");

        Ok(())
    }
}
