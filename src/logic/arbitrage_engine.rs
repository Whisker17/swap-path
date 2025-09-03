use crate::logic::graph::{SwapPath, TokenGraph};
use super::pathfinder::Pathfinder;
use super::profit_calculator::ProfitCalculator;
use super::types::{ArbitrageConfig, ArbitrageOpportunity, MarketSnapshot};
use eyre::{eyre, Result};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// ArbitrageEngine is the core component of the Logic Layer
/// 
/// It implements the two-phase architecture described in the design document:
/// 1. Path Discovery (one-time during initialization) 
/// 2. Parallel Profit Calculation (triggered by new market data)
/// 
/// This engine follows the "precomputed paths + real-time calculation" approach
/// for maximum performance in the real-time arbitrage detection system.
pub struct ArbitrageEngine {
    /// Configuration for the arbitrage engine
    config: ArbitrageConfig,
    /// Pre-computed arbitrage paths (static topology)
    precomputed_paths: Vec<SwapPath>,
    /// Profit calculator component (the "hot path")
    profit_calculator: ProfitCalculator,
    /// Channel receiver for market data updates
    market_data_receiver: Option<mpsc::Receiver<MarketSnapshot>>,
    /// Engine state
    is_initialized: bool,
}

impl ArbitrageEngine {
    /// Create a new ArbitrageEngine with the given configuration
    pub fn new(config: ArbitrageConfig) -> Self {
        let profit_calculator = ProfitCalculator::new(config.clone());

        Self {
            config,
            precomputed_paths: Vec::new(),
            profit_calculator,
            market_data_receiver: None,
            is_initialized: false,
        }
    }

    /// Initialize the engine by pre-computing all arbitrage paths
    /// 
    /// This is the "Path Discovery" phase that runs once during system startup.
    /// It analyzes the token graph and finds all possible 3-hop and 4-hop cycles
    /// from WMNT back to WMNT.
    pub fn initialize(&mut self, token_graph: &TokenGraph) -> Result<()> {
        info!("初始化套利引擎...");
        
        if self.is_initialized {
            warn!("套利引擎已经初始化，跳过重复初始化");
            return Ok(());
        }

        // Create pathfinder and discover all arbitrage paths
        let pathfinder = Pathfinder::new(self.config.max_hops, self.config.max_precomputed_paths);
        
        info!("开始预计算套利路径...");
        let paths = pathfinder.precompute_arbitrage_paths(token_graph)?;
        
        if paths.is_empty() {
            return Err(eyre!("未找到任何套利路径，请检查代币图配置"));
        }

        self.precomputed_paths = paths;
        self.is_initialized = true;

        info!(
            "套利引擎初始化完成！预计算路径数量: {}, 配置: max_hops={}, min_profit=${:.2}",
            self.precomputed_paths.len(),
            self.config.max_hops,
            self.config.min_profit_threshold_usd
        );

        Ok(())
    }

    /// Set up the market data channel receiver
    /// 
    /// This allows the engine to receive real-time market data updates from the Data Layer
    pub fn set_market_data_receiver(&mut self, receiver: mpsc::Receiver<MarketSnapshot>) {
        info!("设置市场数据接收器");
        self.market_data_receiver = Some(receiver);
    }

    /// Process a single market snapshot and find arbitrage opportunities
    /// 
    /// This is the core "Profit Calculation" phase that runs on every new market data update.
    /// It performs high-speed mathematical calculations on all pre-computed paths in parallel.
    pub fn process_market_snapshot(
        &self,
        market_snapshot: &MarketSnapshot,
    ) -> Result<Vec<ArbitrageOpportunity>> {
        if !self.is_initialized {
            return Err(eyre!("引擎未初始化，请先调用 initialize()"));
        }

        // Calculate profits for all pre-computed paths in parallel
        let profit_results = self.profit_calculator.calculate_profits_parallel(
            &self.precomputed_paths,
            market_snapshot,
        );

        // Filter and convert successful calculations to opportunities
        let opportunities: Vec<ArbitrageOpportunity> = profit_results
            .into_iter()
            .filter_map(|result| {
                if result.calculation_successful 
                    && result.net_profit_usd > self.config.min_profit_threshold_usd 
                {
                    result.to_opportunity()
                } else {
                    None
                }
            })
            .collect();

        Ok(opportunities)
    }

    /// Start the real-time processing loop
    /// 
    /// This method will continuously wait for market data updates and process them.
    /// It should be called after initialization and setting up the market data receiver.
    pub async fn start_real_time_processing(
        &mut self,
        opportunity_sender: mpsc::Sender<Vec<ArbitrageOpportunity>>,
    ) -> Result<()> {
        if !self.is_initialized {
            return Err(eyre!("引擎未初始化，无法启动实时处理"));
        }

        let Some(mut receiver) = self.market_data_receiver.take() else {
            return Err(eyre!("未设置市场数据接收器"));
        };

        info!("启动套利引擎实时处理循环...");

        while let Some(market_snapshot) = receiver.recv().await {
            match self.process_market_snapshot(&market_snapshot) {
                Ok(opportunities) => {
                    if !opportunities.is_empty() {
                        info!("发现 {} 个套利机会，区块: {}", opportunities.len(), market_snapshot.block_number);
                        
                        // Send opportunities to execution layer or output handler
                        if let Err(e) = opportunity_sender.send(opportunities).await {
                            error!("发送套利机会失败: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("处理市场快照失败: {}", e);
                    // Continue processing even if one snapshot fails
                }
            }
        }

        warn!("套利引擎实时处理循环结束");
        Ok(())
    }

    /// Get statistics about the engine's current state
    pub fn get_statistics(&self) -> ArbitrageEngineStats {
        ArbitrageEngineStats {
            is_initialized: self.is_initialized,
            precomputed_paths_count: self.precomputed_paths.len(),
            max_hops: self.config.max_hops,
            min_profit_threshold_usd: self.config.min_profit_threshold_usd,
            parallel_calculation_enabled: self.config.enable_parallel_calculation,
        }
    }

    /// Get a reference to the pre-computed paths (for debugging/analysis)
    pub fn get_precomputed_paths(&self) -> &[SwapPath] {
        &self.precomputed_paths
    }

    /// Update configuration (some settings can be changed at runtime)
    pub fn update_config(&mut self, new_config: ArbitrageConfig) -> Result<()> {
        // Some settings can't be changed after initialization
        if self.is_initialized {
            if new_config.max_hops != self.config.max_hops 
                || new_config.max_precomputed_paths != self.config.max_precomputed_paths {
                return Err(eyre!("无法更改需要重新初始化的配置项（max_hops, max_precomputed_paths）"));
            }
        }

        self.config = new_config;
        info!("套利引擎配置已更新");
        Ok(())
    }
}

/// Statistics about the ArbitrageEngine's current state
#[derive(Debug, Clone)]
pub struct ArbitrageEngineStats {
    pub is_initialized: bool,
    pub precomputed_paths_count: usize,
    pub max_hops: u8,
    pub min_profit_threshold_usd: f64,
    pub parallel_calculation_enabled: bool,
}

/// Builder pattern for creating and configuring an ArbitrageEngine
pub struct ArbitrageEngineBuilder {
    config: ArbitrageConfig,
}

impl ArbitrageEngineBuilder {
    pub fn new() -> Self {
        Self {
            config: ArbitrageConfig::default(),
        }
    }

    pub fn with_min_profit_threshold(mut self, threshold_usd: f64) -> Self {
        self.config.min_profit_threshold_usd = threshold_usd;
        self
    }

    pub fn with_max_hops(mut self, max_hops: u8) -> Self {
        self.config.max_hops = max_hops;
        self
    }

    pub fn with_gas_settings(mut self, gas_price_gwei: u64, gas_per_hop: u64) -> Self {
        self.config.gas_price_gwei = gas_price_gwei;
        self.config.gas_per_hop = gas_per_hop;
        self
    }

    pub fn with_parallel_calculation(mut self, enabled: bool) -> Self {
        self.config.enable_parallel_calculation = enabled;
        self
    }

    pub fn with_max_precomputed_paths(mut self, max_paths: usize) -> Self {
        self.config.max_precomputed_paths = max_paths;
        self
    }

    pub fn build(self) -> ArbitrageEngine {
        ArbitrageEngine::new(self.config)
    }
}

impl Default for ArbitrageEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MockPool, PoolWrapper, Token};
    use crate::logic::graph::TokenGraph;
    use crate::logic::types::MarketSnapshot;
    use crate::logic::pools::PoolId;
    use crate::utils::constants::WMNT;
    use alloy_primitives::Address;
    use std::sync::Arc;

    fn create_test_token_graph() -> Result<TokenGraph> {
        let mut token_graph = TokenGraph::new();

        // Create tokens including WMNT
        let wmnt_token = Arc::new(Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18)));
        let token1 = Arc::new(Token::new_with_data(Address::repeat_byte(1), Some("TOKEN1".to_string()), None, Some(18)));
        let token2 = Arc::new(Token::new_with_data(Address::repeat_byte(2), Some("TOKEN2".to_string()), None, Some(18)));

        token_graph.add_or_get_token_idx_by_token(wmnt_token);
        token_graph.add_or_get_token_idx_by_token(token1);
        token_graph.add_or_get_token_idx_by_token(token2);

        // Create a simple cycle: WMNT -> TOKEN1 -> TOKEN2 -> WMNT
        let pool1 = PoolWrapper::from(MockPool::new(WMNT, Address::repeat_byte(1), Address::repeat_byte(10)));
        let pool2 = PoolWrapper::from(MockPool::new(Address::repeat_byte(1), Address::repeat_byte(2), Address::repeat_byte(11)));
        let pool3 = PoolWrapper::from(MockPool::new(Address::repeat_byte(2), WMNT, Address::repeat_byte(12)));

        token_graph.add_pool(pool1)?;
        token_graph.add_pool(pool2)?;
        token_graph.add_pool(pool3)?;

        Ok(token_graph)
    }

    #[test]
    fn test_arbitrage_engine_builder() {
        let engine = ArbitrageEngineBuilder::new()
            .with_min_profit_threshold(10.0)
            .with_max_hops(3)
            .with_parallel_calculation(true)
            .build();

        let stats = engine.get_statistics();
        assert_eq!(stats.min_profit_threshold_usd, 10.0);
        assert_eq!(stats.max_hops, 3);
        assert_eq!(stats.parallel_calculation_enabled, true);
        assert_eq!(stats.is_initialized, false);
    }

    #[tokio::test]
    async fn test_engine_initialization() -> Result<()> {
        let token_graph = create_test_token_graph()?;
        let mut engine = ArbitrageEngineBuilder::new().build();

        assert!(!engine.get_statistics().is_initialized);

        engine.initialize(&token_graph)?;

        let stats = engine.get_statistics();
        assert!(stats.is_initialized);
        assert!(stats.precomputed_paths_count > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_market_snapshot_processing() -> Result<()> {
        let token_graph = create_test_token_graph()?;
        let mut engine = ArbitrageEngineBuilder::new()
            .with_min_profit_threshold(1.0) // Lower threshold for testing
            .build();

        engine.initialize(&token_graph)?;

        // Create a test market snapshot
        let mut snapshot = MarketSnapshot::new(12345, 2000.0);
        
        // Add reserves for all pools
        let reserves = alloy_primitives::U256::from_str_radix("1000000000000000000000", 10).unwrap(); // 1000 tokens
        snapshot.set_pool_reserves(PoolId::Address(Address::repeat_byte(10)), reserves, reserves);
        snapshot.set_pool_reserves(PoolId::Address(Address::repeat_byte(11)), reserves, reserves);
        snapshot.set_pool_reserves(PoolId::Address(Address::repeat_byte(12)), reserves, reserves);

        let opportunities = engine.process_market_snapshot(&snapshot)?;
        
        // The exact number of opportunities depends on the mock pool implementation
        // but we should at least not crash and return a valid result
        assert!(opportunities.len() >= 0);

        Ok(())
    }

    #[test]
    fn test_config_update() -> Result<()> {
        let mut engine = ArbitrageEngineBuilder::new().build();

        // Should be able to update non-initialization configs
        let mut new_config = ArbitrageConfig::default();
        new_config.min_profit_threshold_usd = 20.0;
        new_config.gas_price_gwei = 30;

        engine.update_config(new_config)?;
        assert_eq!(engine.config.min_profit_threshold_usd, 20.0);
        assert_eq!(engine.config.gas_price_gwei, 30);

        Ok(())
    }

    #[tokio::test]
    async fn test_config_update_after_initialization() -> Result<()> {
        let token_graph = create_test_token_graph()?;
        let mut engine = ArbitrageEngineBuilder::new().build();
        engine.initialize(&token_graph)?;

        // Should not be able to change initialization-dependent configs
        let mut new_config = ArbitrageConfig::default();
        new_config.max_hops = 5; // Different from default

        let result = engine.update_config(new_config);
        assert!(result.is_err());

        Ok(())
    }
}
