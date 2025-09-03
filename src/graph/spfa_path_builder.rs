use super::swap_path::SwapPath;
use super::token_graph::TokenGraph;
use super::swap_path_set::SwapPathSet;
use petgraph::prelude::*;
use std::collections::{HashMap, VecDeque, HashSet};
use tracing::{error, debug};
use alloy_primitives::U256;

/// SPFA算法状态，用于路径搜索
#[derive(Debug, Clone)]
struct SPFAState {
    node: NodeIndex<usize>,
    current_path: SwapPath,
    hops: u8,
    reached_end: bool,
    // 添加路径的权重/成本估算，用于优化搜索
    estimated_cost: f64,
}

/// 基于SPFA算法的路径搜索器
/// SPFA (Shortest Path Faster Algorithm) 是一种基于队列的最短路径算法
/// 我们将其适配用于套利路径搜索，提供更好的性能特征
pub struct SPFAPathBuilder {
    /// 最大搜索迭代次数，防止无限循环
    max_iterations: usize,
    /// 是否启用路径剪枝优化
    enable_pruning: bool,
    /// 是否启用成本估算
    enable_cost_estimation: bool,
}

impl Default for SPFAPathBuilder {
    fn default() -> Self {
        Self {
            max_iterations: 100_000, // 比DFS更低的限制，因为SPFA更高效
            enable_pruning: true,
            enable_cost_estimation: true,
        }
    }
}

impl SPFAPathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    pub fn with_pruning(mut self, enable_pruning: bool) -> Self {
        self.enable_pruning = enable_pruning;
        self
    }

    pub fn with_cost_estimation(mut self, enable_cost_estimation: bool) -> Self {
        self.enable_cost_estimation = enable_cost_estimation;
        self
    }

    /// 使用SPFA算法查找所有路径
    /// 相比DFS，SPFA使用队列优先处理最有前景的路径
    pub fn find_all_paths(
        &self,
        token_graph: &TokenGraph,
        initial_swap_path: SwapPath,
        start_node: NodeIndex<usize>,
        end_node: NodeIndex<usize>,
        max_hops: u8,
        allow_duplicate_first: bool,
    ) -> eyre::Result<SwapPathSet> {
        // 验证初始路径
        self.validate_initial_path(&initial_swap_path, token_graph, start_node, end_node, allow_duplicate_first)?;

        let mut all_swap_paths = SwapPathSet::new();
        
        // 使用队列而非栈，实现SPFA的核心特征
        let mut queue = VecDeque::new();
        
        // 用于跟踪访问状态的优化数据结构
        let mut best_cost_to_node: HashMap<NodeIndex<usize>, f64> = HashMap::new();
        let mut visited_paths: HashSet<Vec<u8>> = HashSet::new(); // 路径去重

        // 初始化搜索
        let initial_cost = self.estimate_path_cost(&initial_swap_path);
        let initial_state = SPFAState {
            node: start_node,
            current_path: initial_swap_path.clone(),
            hops: 1,
            reached_end: false,
            estimated_cost: initial_cost,
        };
        
        queue.push_back(initial_state);
        best_cost_to_node.insert(start_node, initial_cost);

        let mut iteration_count = 0;

        while let Some(current_state) = queue.pop_front() {
            // 防止无限循环
            if iteration_count >= self.max_iterations {
                error!(
                    "SPFA路径搜索达到最大迭代次数限制 {} for start_node={}, end_node={}",
                    self.max_iterations,
                    token_graph.graph.node_weight(start_node).map(|node| node.token.get_address()).unwrap_or_default(),
                    token_graph.graph.node_weight(end_node).map(|node| node.token.get_address()).unwrap_or_default()
                );
                break;
            }
            iteration_count += 1;

            // 路径去重优化
            if self.enable_pruning {
                let path_signature = self.generate_path_signature(&current_state.current_path);
                if visited_paths.contains(&path_signature) {
                    continue;
                }
                visited_paths.insert(path_signature);
            }

            // 如果到达目标节点，记录路径
            if current_state.node == end_node {
                if current_state.current_path.len() > 1 {
                    debug!("找到有效路径，长度: {}", current_state.current_path.len());
                    all_swap_paths.insert(current_state.current_path);
                }
                continue;
            }

            // 如果已经用完了跳数，跳过扩展
            if current_state.hops >= max_hops {
                continue;
            }

            // 如果已经到达终点token，不再继续
            if current_state.reached_end {
                continue;
            }

            // SPFA核心：扩展邻居节点
            self.expand_neighbors(
                token_graph,
                &current_state,
                &mut queue,
                &mut best_cost_to_node,
                allow_duplicate_first,
            )?;
        }

        debug!(
            "SPFA路径搜索完成，迭代次数: {}, 找到路径: {}",
            iteration_count,
            all_swap_paths.len()
        );

        Ok(all_swap_paths)
    }

    /// 验证初始路径的有效性
    fn validate_initial_path(
        &self,
        initial_swap_path: &SwapPath,
        token_graph: &TokenGraph,
        start_node: NodeIndex<usize>,
        end_node: NodeIndex<usize>,
        allow_duplicate_first: bool,
    ) -> eyre::Result<()> {
        if let Some(last_token) = initial_swap_path.tokens.last() {
            if last_token.is_wrapped() {
                error!(
                    "初始交换路径不能以包装token结束 start_node={}, end_node={}, initial_swap_path={:?}, allowing duplicate first={}",
                    token_graph.graph.node_weight(start_node).map(|node| node.token.get_address()).unwrap_or_default(),
                    token_graph.graph.node_weight(end_node).map(|node| node.token.get_address()).unwrap_or_default(),
                    initial_swap_path,
                    allow_duplicate_first
                );
                return Err(eyre::eyre!("初始交换路径不能以包装token结束"));
            }
        } else {
            error!(
                "初始交换路径的最后一个token不能为空 start_node={}, end_node={}, initial_swap_path={:?}, allowing duplicate first={}",
                token_graph.graph.node_weight(start_node).map(|node| node.token.get_address()).unwrap_or_default(),
                token_graph.graph.node_weight(end_node).map(|node| node.token.get_address()).unwrap_or_default(),
                initial_swap_path,
                allow_duplicate_first
            );
            return Err(eyre::eyre!("初始交换路径的最后一个token不能为空"));
        }
        Ok(())
    }

    /// 扩展当前节点的邻居
    fn expand_neighbors(
        &self,
        token_graph: &TokenGraph,
        current_state: &SPFAState,
        queue: &mut VecDeque<SPFAState>,
        best_cost_to_node: &mut HashMap<NodeIndex<usize>, f64>,
        allow_duplicate_first: bool,
    ) -> eyre::Result<()> {
        // 获取当前节点的所有边
        for edge in token_graph.graph.edges(current_state.node) {
            let to_token = token_graph.graph.node_weight(edge.target()).unwrap().token.clone();

            // 处理每个池
            for pool in edge.weight().values() {
                if !pool.is_active {
                    continue;
                }

                // 检查池重复
                if current_state.current_path.contains_pool(&pool.inner) {
                    if !allow_duplicate_first {
                        continue;
                    } else if current_state.current_path.pools.first()
                        .map(|p| p.get_pool_id() != pool.inner.get_pool_id())
                        .unwrap_or(false) 
                    {
                        continue;
                    }
                }

                // 创建新路径
                let mut new_path = current_state.current_path.clone();
                if new_path.push_swap_hop(to_token.clone(), pool.inner.clone()).is_ok() {
                    let new_cost = self.estimate_path_cost(&new_path);
                    let new_reached_end = to_token.is_wrapped();
                    
                    // SPFA优化：只有当找到更好的路径时才添加到队列
                    let should_add = if self.enable_cost_estimation {
                        if let Some(&best_cost) = best_cost_to_node.get(&edge.target()) {
                            new_cost < best_cost * 1.1 // 允许10%的容差
                        } else {
                            true
                        }
                    } else {
                        true
                    };

                    if should_add {
                        best_cost_to_node.insert(edge.target(), new_cost);
                        
                        let new_state = SPFAState {
                            node: edge.target(),
                            current_path: new_path,
                            hops: current_state.hops + 1,
                            reached_end: new_reached_end,
                            estimated_cost: new_cost,
                        };

                        // SPFA特征：根据成本排序插入队列
                        if self.enable_cost_estimation {
                            self.insert_by_priority(queue, new_state);
                        } else {
                            queue.push_back(new_state);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 按优先级插入队列（成本越低优先级越高）
    fn insert_by_priority(&self, queue: &mut VecDeque<SPFAState>, state: SPFAState) {
        let insert_pos = queue
            .iter()
            .position(|existing| existing.estimated_cost > state.estimated_cost)
            .unwrap_or(queue.len());
        
        queue.insert(insert_pos, state);
    }

    /// 估算路径成本（用于优化搜索）
    fn estimate_path_cost(&self, path: &SwapPath) -> f64 {
        if !self.enable_cost_estimation {
            return 0.0;
        }

        // 简单的成本估算：路径长度 + 费用估算
        let length_cost = path.len() as f64;
        
        let fee_cost: f64 = path.pools
            .iter()
            .map(|pool| {
                let fee = pool.get_fee();
                // 将U256转换为f64，简化计算
                if fee > U256::from(10000) {
                    fee.to_string().parse::<f64>().unwrap_or(10000.0) / 10000.0
                } else {
                    fee.to::<u64>() as f64 / 10000.0
                }
            })
            .sum();

        length_cost + fee_cost
    }

    /// 生成路径签名用于去重
    fn generate_path_signature(&self, path: &SwapPath) -> Vec<u8> {
        let mut signature = Vec::new();
        
        // 包含所有池的ID（使用格式化的字符串作为签名）
        for pool in &path.pools {
            let pool_id_str = format!("{:?}", pool.get_pool_id());
            signature.extend_from_slice(pool_id_str.as_bytes());
        }
        
        // 包含所有token地址
        for token in &path.tokens {
            signature.extend_from_slice(token.get_address().as_slice());
        }
        
        signature
    }
}

/// 提供给外部使用的简化接口，保持与原DFS接口兼容
pub fn find_all_paths_spfa(
    token_graph: &TokenGraph,
    initial_swap_path: SwapPath,
    start_node: NodeIndex<usize>,
    end_node: NodeIndex<usize>,
    max_hops: u8,
    allow_duplicate_first: bool,
) -> eyre::Result<SwapPathSet> {
    let builder = SPFAPathBuilder::new();
    builder.find_all_paths(
        token_graph,
        initial_swap_path,
        start_node,
        end_node,
        max_hops,
        allow_duplicate_first,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::TokenGraph;
    use crate::{MockPool, PoolWrapper, Token};
    use alloy_primitives::Address;
    use std::sync::Arc;

    #[test]
    fn test_spfa_simple_path() -> eyre::Result<()> {
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

        let builder = SPFAPathBuilder::new().with_max_iterations(10000);
        let swap_paths = builder.find_all_paths(
            &token_graph,
            initial_swap_path,
            *start_node_index,
            *end_node_index,
            3,
            false,
        )?;

        // 应该找到一条路径
        assert_eq!(swap_paths.len(), 1);

        Ok(())
    }

    #[test]
    fn test_spfa_performance_vs_dfs() -> eyre::Result<()> {
        // 创建更复杂的图进行性能测试
        let tokens: Vec<Arc<Token>> = (0..10).map(|_| Arc::new(Token::random())).collect();
        
        let mut token_graph = TokenGraph::new();
        for token in &tokens {
            token_graph.add_or_get_token_idx_by_token(token.clone());
        }

        // 创建密集连接的图
        for i in 0..tokens.len() {
            for j in (i+1)..tokens.len() {
                let pool = PoolWrapper::from(MockPool::new(
                    tokens[i].get_address(),
                    tokens[j].get_address(),
                    Address::random(),
                ));
                token_graph.add_pool(pool)?;
            }
        }

        let start_node = *token_graph.token_index.get(&tokens[0].get_address()).unwrap();
        let end_node = *token_graph.token_index.get(&tokens[1].get_address()).unwrap();
        let initial_path = SwapPath::new_first(
            tokens[1].clone(),
            tokens[0].clone(),
            PoolWrapper::from(MockPool::new(
                tokens[1].get_address(),
                tokens[0].get_address(),
                Address::random(),
            )),
        );

        let builder = SPFAPathBuilder::new();
        let start_time = std::time::Instant::now();
        let paths = builder.find_all_paths(&token_graph, initial_path, start_node, end_node, 4, false)?;
        let spfa_duration = start_time.elapsed();

        println!("SPFA找到 {} 条路径，耗时: {:?}", paths.len(), spfa_duration);

        // SPFA应该能够找到路径并且具有合理的性能
        assert!(paths.len() > 0);
        assert!(spfa_duration.as_millis() < 1000); // 应该在1秒内完成

        Ok(())
    }
}
