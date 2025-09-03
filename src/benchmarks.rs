use crate::logic::graph::{TokenGraph, SwapPath, SPFAPathBuilder};
use crate::{PoolWrapper, Token, MockPool, CacheManager};
use alloy_primitives::Address;
use std::sync::Arc;
use std::time::Instant;

/// 性能测试模块，测试SPFA算法和缓存系统的性能
pub struct PerformanceBenchmark {
    token_graph: TokenGraph,
    tokens: Vec<Arc<Token>>,
    pools: Vec<PoolWrapper>,
    cache_manager: CacheManager,
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub spfa_duration: std::time::Duration,
    pub spfa_paths_found: usize,
    pub cache_hit_rate: f64,
    pub average_iteration_count: f64,
}

impl PerformanceBenchmark {
    /// 创建新的性能测试实例
    pub fn new(token_count: usize, connectivity_ratio: f64) -> eyre::Result<Self> {
        let mut benchmark = Self {
            token_graph: TokenGraph::new(),
            tokens: Vec::new(),
            pools: Vec::new(),
            cache_manager: CacheManager::new(),
        };

        benchmark.setup_test_graph(token_count, connectivity_ratio)?;
        Ok(benchmark)
    }

    /// 设置测试图
    fn setup_test_graph(&mut self, token_count: usize, connectivity_ratio: f64) -> eyre::Result<()> {
        // 创建代币
        self.tokens = (0..token_count)
            .map(|_| Arc::new(Token::random()))
            .collect();

        // 添加代币到图中
        for token in &self.tokens {
            self.token_graph.add_or_get_token_idx_by_token(token.clone());
        }

        // 创建池连接（根据连接比例）
        let total_possible_connections = token_count * (token_count - 1) / 2;
        let connections_to_create = (total_possible_connections as f64 * connectivity_ratio) as usize;

        let mut created_connections = 0;
        for i in 0..self.tokens.len() {
            for j in (i + 1)..self.tokens.len() {
                if created_connections >= connections_to_create {
                    break;
                }

                let pool = PoolWrapper::from(MockPool::new(
                    self.tokens[i].get_address(),
                    self.tokens[j].get_address(),
                    Address::random(),
                ));

                self.token_graph.add_pool(pool.clone())?;
                self.pools.push(pool);
                created_connections += 1;
            }
            if created_connections >= connections_to_create {
                break;
            }
        }

        println!(
            "创建了 {} 个代币和 {} 个池的测试图",
            token_count, created_connections
        );

        Ok(())
    }

    /// 运行SPFA算法性能基准测试
    pub fn run_spfa_benchmark(&self, max_hops: u8) -> eyre::Result<BenchmarkResult> {
        if self.tokens.len() < 3 {
            return Err(eyre::eyre!("测试图至少需要3个代币"));
        }

        // 选择起始和结束代币
        let start_token = self.tokens[0].clone();
        let intermediate_token = self.tokens[1].clone();

        // 查找节点索引
        let start_node = *self.token_graph.token_index.get(&intermediate_token.get_address())
            .ok_or_else(|| eyre::eyre!("找不到起始节点"))?;
        let end_node = *self.token_graph.token_index.get(&start_token.get_address())
            .ok_or_else(|| eyre::eyre!("找不到结束节点"))?;

        // 创建初始交换路径
        let initial_pool = self.pools.iter()
            .find(|pool| {
                let tokens = pool.get_tokens();
                tokens.contains(&start_token.get_address()) && tokens.contains(&intermediate_token.get_address())
            })
            .ok_or_else(|| eyre::eyre!("找不到初始池"))?;

        let initial_swap_path = SwapPath::new_first(
            start_token.clone(),
            intermediate_token.clone(),
            initial_pool.clone(),
        );

        println!("开始SPFA算法性能基准测试...");

        // 测试SPFA算法
        let spfa_start = Instant::now();
        let spfa_builder = SPFAPathBuilder::new()
            .with_max_iterations(100_000)
            .with_pruning(true)
            .with_cost_estimation(true);
        
        let spfa_paths = spfa_builder.find_all_paths(
            &self.token_graph,
            initial_swap_path,
            start_node,
            end_node,
            max_hops,
            false,
        )?;
        let spfa_duration = spfa_start.elapsed();
        let spfa_paths_count = spfa_paths.len();

        println!("SPFA完成：找到 {} 条路径，耗时 {:?}", spfa_paths_count, spfa_duration);

        // 获取缓存统计
        let cache_hit_rate = self.cache_manager.get_stats().hit_rate();

        Ok(BenchmarkResult {
            spfa_duration,
            spfa_paths_found: spfa_paths_count,
            cache_hit_rate,
            average_iteration_count: 0.0, // 可以后续从SPFA算法中获取
        })
    }

    /// 运行缓存性能测试
    pub fn run_cache_benchmark(&mut self) -> eyre::Result<()> {
        println!("开始缓存性能测试...");

        let cache = self.cache_manager.state_cache();
        
        // 模拟缓存操作
        let test_token = Address::random();
        let test_balance = alloy_primitives::U256::from(1000000);

        // 测试缓存写入性能
        let write_start = Instant::now();
        for i in 0..10000 {
            let addr = Address::with_last_byte((i % 256) as u8);
            cache.set_balance(addr, test_token, test_balance);
        }
        let write_duration = write_start.elapsed();

        // 测试缓存读取性能
        let read_start = Instant::now();
        let mut hit_count = 0;
        for i in 0..10000 {
            let addr = Address::with_last_byte((i % 256) as u8);
            if cache.get_balance(addr, test_token).is_some() {
                hit_count += 1;
            }
        }
        let read_duration = read_start.elapsed();

        println!(
            "缓存性能测试结果：\n  写入10000项耗时: {:?}\n  读取10000项耗时: {:?}\n  命中率: {:.2}%\n  缓存大小: {:?}",
            write_duration,
            read_duration,
            (hit_count as f64 / 10000.0) * 100.0,
            cache.cache_sizes()
        );

        Ok(())
    }

    /// 运行不同规模的基准测试
    pub fn run_scalability_test(&self) -> eyre::Result<()> {
        println!("开始可扩展性测试...");
        
        let hop_counts = vec![2, 3, 4, 5];
        
        for &max_hops in &hop_counts {
            println!("\n测试最大跳数: {}", max_hops);
            
            let result = self.run_spfa_benchmark(max_hops)?;
            println!(
                "  找到路径: {} 条\n  耗时: {:?}\n  缓存命中率: {:.2}%",
                result.spfa_paths_found,
                result.spfa_duration,
                result.cache_hit_rate * 100.0
            );
        }
        
        Ok(())
    }

    /// 获取缓存管理器
    pub fn cache_manager(&self) -> &CacheManager {
        &self.cache_manager
    }
}

impl BenchmarkResult {
    /// 打印详细的基准测试结果
    pub fn print_detailed_results(&self) {
        println!("\n========== SPFA算法性能测试结果 ==========");
        println!("SPFA算法:");
        println!("  耗时: {:?}", self.spfa_duration);
        println!("  找到路径数: {}", self.spfa_paths_found);
        println!("  缓存命中率: {:.2}%", self.cache_hit_rate * 100.0);
        
        // 计算每秒处理的路径数
        if self.spfa_duration.as_secs_f64() > 0.0 {
            let paths_per_second = self.spfa_paths_found as f64 / self.spfa_duration.as_secs_f64();
            println!("  处理速度: {:.0} 路径/秒", paths_per_second);
        }
        
        println!("==========================================\n");
    }

    /// 检查结果是否表明性能良好
    pub fn is_good_performance(&self) -> bool {
        // 简单的性能评估标准
        self.spfa_duration.as_millis() < 5000 && // 5秒内完成
        self.spfa_paths_found > 0 // 找到至少一条路径
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_graph_spfa_benchmark() -> eyre::Result<()> {
        let benchmark = PerformanceBenchmark::new(5, 0.8)?;
        let result = benchmark.run_spfa_benchmark(3)?;
        
        result.print_detailed_results();
        
        // 基本检查
        assert!(result.spfa_duration.as_nanos() > 0);
        assert!(result.is_good_performance());
        
        Ok(())
    }

    #[test]
    fn test_medium_graph_spfa_benchmark() -> eyre::Result<()> {
        let benchmark = PerformanceBenchmark::new(10, 0.6)?;
        let result = benchmark.run_spfa_benchmark(4)?;
        
        result.print_detailed_results();
        
        // 在中等规模图上，SPFA应该表现良好
        assert!(result.spfa_paths_found > 0);
        assert!(result.is_good_performance());
        
        Ok(())
    }

    #[test]
    fn test_cache_performance() -> eyre::Result<()> {
        let mut benchmark = PerformanceBenchmark::new(5, 0.5)?;
        benchmark.run_cache_benchmark()?;
        Ok(())
    }

    #[test]
    fn test_scalability() -> eyre::Result<()> {
        let benchmark = PerformanceBenchmark::new(8, 0.7)?;
        benchmark.run_scalability_test()?;
        Ok(())
    }
}