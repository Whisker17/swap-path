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
    /// Expected gross profit (before gas costs) in MNT Wei
    pub gross_profit_mnt_wei: U256,
    /// Estimated gas cost in MNT Wei
    pub gas_cost_mnt_wei: U256,
    /// Net profit (gross profit - gas cost) in MNT Wei
    pub net_profit_mnt_wei: U256,
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
        gross_profit_mnt_wei: U256,
        gas_cost_mnt_wei: U256,
    ) -> Self {
        let net_profit_mnt_wei = if gross_profit_mnt_wei > gas_cost_mnt_wei {
            gross_profit_mnt_wei - gas_cost_mnt_wei
        } else {
            U256::ZERO
        };
        
        let profit_margin_percent = if !gross_profit_mnt_wei.is_zero() {
            let gross_profit_f64 = gross_profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0);
            let net_profit_f64 = net_profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0);
            (net_profit_f64 / gross_profit_f64) * 100.0
        } else {
            0.0
        };

        Self {
            path,
            optimal_input_amount,
            gross_profit_mnt_wei,
            gas_cost_mnt_wei,
            net_profit_mnt_wei,
            profit_margin_percent,
            discovered_at: Instant::now(),
            expected_output_amount,
        }
    }

    pub fn is_profitable(&self, min_profit_threshold_mnt_wei: U256) -> bool {
        self.net_profit_mnt_wei > min_profit_threshold_mnt_wei
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
    /// Set of enabled pools (optimization to avoid repeated MarketWithoutLock queries)
    pub enabled_pools: std::collections::HashSet<PoolId>,
    /// Total number of pools in the market (for statistics)
    pub total_pools_count: usize,
}

impl MarketSnapshot {
    pub fn new(block_number: u64) -> Self {
        Self {
            pool_reserves: HashMap::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            block_number,
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
    pub gross_profit_mnt_wei: U256,
    pub gas_cost_mnt_wei: U256,
    pub net_profit_mnt_wei: U256,
    pub calculation_successful: bool,
    pub error_message: Option<String>,
}

impl ProfitCalculationResult {
    pub fn success(
        path: SwapPath,
        optimal_input_amount: U256,
        expected_output_amount: U256,
        gross_profit_mnt_wei: U256,
        gas_cost_mnt_wei: U256,
    ) -> Self {
        let net_profit_mnt_wei = if gross_profit_mnt_wei > gas_cost_mnt_wei {
            gross_profit_mnt_wei - gas_cost_mnt_wei
        } else {
            U256::ZERO
        };
        
        Self {
            path,
            optimal_input_amount,
            expected_output_amount,
            gross_profit_mnt_wei,
            gas_cost_mnt_wei,
            net_profit_mnt_wei,
            calculation_successful: true,
            error_message: None,
        }
    }

    pub fn failure(path: SwapPath, error_message: String) -> Self {
        Self {
            path,
            optimal_input_amount: U256::ZERO,
            expected_output_amount: U256::ZERO,
            gross_profit_mnt_wei: U256::ZERO,
            gas_cost_mnt_wei: U256::ZERO,
            net_profit_mnt_wei: U256::ZERO,
            calculation_successful: false,
            error_message: Some(error_message),
        }
    }

    pub fn to_opportunity(&self) -> Option<ArbitrageOpportunity> {
        if self.calculation_successful && !self.net_profit_mnt_wei.is_zero() {
            Some(ArbitrageOpportunity::new(
                self.path.clone(),
                self.optimal_input_amount,
                self.expected_output_amount,
                self.gross_profit_mnt_wei,
                self.gas_cost_mnt_wei,
            ))
        } else {
            None
        }
    }
}

/// Configuration for the arbitrage engine
#[derive(Debug, Clone)]
pub struct ArbitrageConfig {
    /// Minimum profit threshold in MNT Wei to consider an opportunity
    pub min_profit_threshold_mnt_wei: U256,
    /// Maximum number of hops for arbitrage paths (3-4 as per design)
    pub max_hops: u8,
    /// Gas price in Gwei for cost calculation
    pub gas_price_gwei: f64,
    /// Estimated gas usage per transaction (covers multiple hops)
    pub gas_per_transaction: u64,
    /// Maximum number of paths to pre-compute
    pub max_precomputed_paths: usize,
    /// Enable parallel profit calculation
    pub enable_parallel_calculation: bool,
}

impl Default for ArbitrageConfig {
    fn default() -> Self {
        Self {
            // Default to 0.01 MNT minimum profit (0.01 * 10^18 wei)
            min_profit_threshold_mnt_wei: U256::from_str_radix("10000000000000000", 10).unwrap(),
            max_hops: 4, // As per design document
            gas_price_gwei: 0.02, // 0.02 gwei as mentioned by user
            gas_per_transaction: 700_000_000, // 700M gas as mentioned by user
            max_precomputed_paths: 10_000,
            enable_parallel_calculation: true,
        }
    }
}
