use super::graph::SwapPath;
use super::pools::PoolId;
use alloy_primitives::U256;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Represents a profitable arbitrage opportunity discovered by the engine
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    /// The swap path that creates the arbitrage opportunity
    pub path: SwapPath,
    /// Optimal input amount (in Wei) that maximizes profit
    pub optimal_input_amount: U256,
    /// Expected gross profit (before gas costs) in USD
    pub gross_profit_usd: f64,
    /// Estimated gas cost in USD
    pub gas_cost_usd: f64,
    /// Net profit (gross profit - gas cost) in USD
    pub net_profit_usd: f64,
    /// Profit margin as percentage
    pub profit_margin_percent: f64,
    /// When this opportunity was discovered
    pub discovered_at: Instant,
    /// Expected return in the output token (usually WMNT)
    pub expected_output_amount: U256,
}

impl ArbitrageOpportunity {
    pub fn new(
        path: SwapPath,
        optimal_input_amount: U256,
        expected_output_amount: U256,
        gross_profit_usd: f64,
        gas_cost_usd: f64,
    ) -> Self {
        let net_profit_usd = gross_profit_usd - gas_cost_usd;
        let profit_margin_percent = if gross_profit_usd > 0.0 {
            (net_profit_usd / gross_profit_usd) * 100.0
        } else {
            0.0
        };

        Self {
            path,
            optimal_input_amount,
            gross_profit_usd,
            gas_cost_usd,
            net_profit_usd,
            profit_margin_percent,
            discovered_at: Instant::now(),
            expected_output_amount,
        }
    }

    pub fn is_profitable(&self, min_profit_threshold_usd: f64) -> bool {
        self.net_profit_usd > min_profit_threshold_usd
    }
}

/// Market data snapshot containing pool reserves and other market information
#[derive(Debug, Clone, Default)]
pub struct MarketSnapshot {
    /// Pool reserves: pool_id -> (reserve0, reserve1)
    pub pool_reserves: HashMap<PoolId, (U256, U256)>,
    /// Timestamp when this snapshot was taken
    pub timestamp: u64,
    /// Block number from which this data comes
    pub block_number: u64,
    /// ETH price in USD (for gas cost calculation)
    pub eth_price_usd: f64,
    /// Set of enabled pools (optimization to avoid repeated MarketWithoutLock queries)
    pub enabled_pools: std::collections::HashSet<PoolId>,
    /// Total number of pools in the market (for statistics)
    pub total_pools_count: usize,
}

impl MarketSnapshot {
    pub fn new(block_number: u64, eth_price_usd: f64) -> Self {
        Self {
            pool_reserves: HashMap::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            block_number,
            eth_price_usd,
            enabled_pools: HashSet::new(),
            total_pools_count: 0,
        }
    }

    pub fn set_pool_reserves(&mut self, pool_id: PoolId, reserve0: U256, reserve1: U256) {
        self.pool_reserves.insert(pool_id, (reserve0, reserve1));
    }

    pub fn get_pool_reserves(&self, pool_id: &PoolId) -> Option<(U256, U256)> {
        self.pool_reserves.get(pool_id).copied()
    }

    /// Set the enabled pools for this snapshot (optimization method)
    pub fn set_enabled_pools(&mut self, enabled_pools: HashSet<PoolId>) {
        self.enabled_pools = enabled_pools;
    }

    /// Set the total pools count for this snapshot
    pub fn set_total_pools_count(&mut self, count: usize) {
        self.total_pools_count = count;
    }

    /// Get pools that are enabled and have sufficient liquidity
    pub fn get_liquid_enabled_pools(&self, min_liquidity: U256) -> Vec<PoolId> {
        self.pool_reserves
            .iter()
            .filter(|(pool_id, (reserve0, reserve1))| {
                self.enabled_pools.contains(pool_id) && 
                *reserve0 > min_liquidity && 
                *reserve1 > min_liquidity
            })
            .map(|(pool_id, _)| *pool_id)
            .collect()
    }

    /// Check if a pool is enabled in this snapshot
    pub fn is_pool_enabled(&self, pool_id: &PoolId) -> bool {
        self.enabled_pools.contains(pool_id)
    }

    /// Get the number of enabled pools with reserves data
    pub fn enabled_pools_with_data_count(&self) -> usize {
        self.pool_reserves
            .keys()
            .filter(|pool_id| self.enabled_pools.contains(pool_id))
            .count()
    }
}

/// Result of profit calculation for a single path
#[derive(Debug, Clone)]
pub struct ProfitCalculationResult {
    pub path: SwapPath,
    pub optimal_input_amount: U256,
    pub expected_output_amount: U256,
    pub gross_profit_wei: U256,
    pub gross_profit_usd: f64,
    pub gas_cost_usd: f64,
    pub net_profit_usd: f64,
    pub calculation_successful: bool,
    pub error_message: Option<String>,
}

impl ProfitCalculationResult {
    pub fn success(
        path: SwapPath,
        optimal_input_amount: U256,
        expected_output_amount: U256,
        gross_profit_wei: U256,
        gross_profit_usd: f64,
        gas_cost_usd: f64,
    ) -> Self {
        Self {
            path,
            optimal_input_amount,
            expected_output_amount,
            gross_profit_wei,
            gross_profit_usd,
            gas_cost_usd,
            net_profit_usd: gross_profit_usd - gas_cost_usd,
            calculation_successful: true,
            error_message: None,
        }
    }

    pub fn failure(path: SwapPath, error_message: String) -> Self {
        Self {
            path,
            optimal_input_amount: U256::ZERO,
            expected_output_amount: U256::ZERO,
            gross_profit_wei: U256::ZERO,
            gross_profit_usd: 0.0,
            gas_cost_usd: 0.0,
            net_profit_usd: 0.0,
            calculation_successful: false,
            error_message: Some(error_message),
        }
    }

    pub fn to_opportunity(&self) -> Option<ArbitrageOpportunity> {
        if self.calculation_successful && self.net_profit_usd > 0.0 {
            Some(ArbitrageOpportunity::new(
                self.path.clone(),
                self.optimal_input_amount,
                self.expected_output_amount,
                self.gross_profit_usd,
                self.gas_cost_usd,
            ))
        } else {
            None
        }
    }
}

/// Configuration for the arbitrage engine
#[derive(Debug, Clone)]
pub struct ArbitrageConfig {
    /// Minimum profit threshold in USD to consider an opportunity
    pub min_profit_threshold_usd: f64,
    /// Maximum number of hops for arbitrage paths (3-4 as per design)
    pub max_hops: u8,
    /// Gas price in Gwei for cost calculation
    pub gas_price_gwei: u64,
    /// Estimated gas usage per hop
    pub gas_per_hop: u64,
    /// Maximum number of paths to pre-compute
    pub max_precomputed_paths: usize,
    /// Enable parallel profit calculation
    pub enable_parallel_calculation: bool,
}

impl Default for ArbitrageConfig {
    fn default() -> Self {
        Self {
            min_profit_threshold_usd: 5.0, // $5 minimum profit
            max_hops: 4, // As per design document
            gas_price_gwei: 20,
            gas_per_hop: 150_000, // Rough estimate
            max_precomputed_paths: 10_000,
            enable_parallel_calculation: true,
        }
    }
}
