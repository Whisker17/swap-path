use super::graph::SwapPath;
use super::types::{ArbitrageConfig, MarketSnapshot, ProfitCalculationResult};
use super::pools::CalculationError;
use alloy_primitives::{Address, U256};
use eyre::Result;
use rayon::prelude::*;
use tracing::debug;

/// ProfitCalculator is the "hot path" component responsible for high-speed profit evaluation
/// 
/// This component takes pre-computed paths and performs pure mathematical calculations
/// to evaluate profit potential using the latest market data. It's designed for
/// maximum performance with parallel processing capabilities.
pub struct ProfitCalculator {
    config: ArbitrageConfig,
}

impl ProfitCalculator {
    pub fn new(config: ArbitrageConfig) -> Self {
        Self { config }
    }

    /// Calculate profits for all paths in parallel
    /// 
    /// This is the main entry point for profit calculation. It takes a list of 
    /// pre-computed paths and the latest market snapshot, then calculates profits
    /// for all paths in parallel using Rayon.
    pub fn calculate_profits_parallel(
        &self,
        paths: &[SwapPath],
        market_snapshot: &MarketSnapshot,
    ) -> Vec<ProfitCalculationResult> {
        if !self.config.enable_parallel_calculation {
            return self.calculate_profits_sequential(paths, market_snapshot);
        }

        debug!("开始并行利润计算，路径数量: {}", paths.len());

        let results: Vec<ProfitCalculationResult> = paths
            .par_iter() // <-- Rayon's parallel iterator
            .map(|path| {
                self.calculate_path_profit(path, market_snapshot)
            })
            .collect();

        let profitable_count = results.iter()
            .filter(|r| r.calculation_successful && r.net_profit_mnt_wei > self.config.min_profit_threshold_mnt_wei)
            .count();

        debug!("并行利润计算完成，有利可图的路径: {}/{}", profitable_count, results.len());

        results
    }

    /// Sequential profit calculation (fallback for debugging)
    fn calculate_profits_sequential(
        &self,
        paths: &[SwapPath],
        market_snapshot: &MarketSnapshot,
    ) -> Vec<ProfitCalculationResult> {
        debug!("开始顺序利润计算，路径数量: {}", paths.len());

        let results: Vec<ProfitCalculationResult> = paths
            .iter()
            .map(|path| {
                self.calculate_path_profit(path, market_snapshot)
            })
            .collect();

        debug!("顺序利润计算完成");
        results
    }

    /// Calculate profit for a single path
    /// 
    /// This is the core profit calculation logic. For a given path, it:
    /// 1. Finds the optimal input amount using numerical optimization
    /// 2. Calculates expected output using chain of getAmountOut calls
    /// 3. Estimates gas costs
    /// 4. Returns detailed profit calculation result
    fn calculate_path_profit(
        &self,
        path: &SwapPath,
        market_snapshot: &MarketSnapshot,
    ) -> ProfitCalculationResult {
        // Verify we have reserve data for all pools in this path
        for pool in &path.pools {
            let pool_id = pool.get_pool_id();
            if market_snapshot.get_pool_reserves(&pool_id).is_none() {
                return ProfitCalculationResult::failure(
                    path.clone(),
                    format!("Missing reserve data for pool {:?}", pool_id),
                );
            }
        }

        // Find optimal input amount using ternary search
        match self.find_optimal_input_amount(path, market_snapshot) {
            Ok((optimal_input, expected_output)) => {
                // Calculate gas cost in MNT Wei
                let gas_cost_mnt_wei = self.calculate_gas_cost_mnt_wei(path);
                
                // Calculate gross profit in MNT Wei (expected output - input)
                let gross_profit_mnt_wei = if expected_output > optimal_input {
                    expected_output - optimal_input
                } else {
                    U256::ZERO
                };

                ProfitCalculationResult::success(
                    path.clone(),
                    optimal_input,
                    expected_output,
                    gross_profit_mnt_wei,
                    gas_cost_mnt_wei,
                )
            }
            Err(e) => ProfitCalculationResult::failure(
                path.clone(),
                format!("Failed to calculate optimal input: {}", e),
            ),
        }
    }

    /// Find the optimal input amount using ternary search
    /// 
    /// The profit function is generally unimodal (single peak) due to slippage,
    /// so ternary search is an efficient way to find the maximum.
    fn find_optimal_input_amount(
        &self,
        path: &SwapPath,
        market_snapshot: &MarketSnapshot,
    ) -> Result<(U256, U256)> {
        // Define search range (0.01 ETH to 100 ETH in Wei)
        let min_input = U256::from(10_000_000_000_000_000u64); // 0.01 ETH
        let max_input = U256::from_str_radix("100000000000000000000", 10).unwrap(); // 100 ETH

        self.ternary_search_optimal_input(path, market_snapshot, min_input, max_input)
    }

    /// Ternary search implementation for finding optimal input amount
    fn ternary_search_optimal_input(
        &self,
        path: &SwapPath,
        market_snapshot: &MarketSnapshot,
        mut left: U256,
        mut right: U256,
    ) -> Result<(U256, U256)> {
        let precision = U256::from(1_000_000_000_000_000u64); // 0.001 ETH precision
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 50;

        let mut best_input = left;
        let mut best_output = U256::ZERO;
        let mut best_profit = 0.0f64;

        while right - left > precision && iterations < MAX_ITERATIONS {
            let one_third = (right - left) / U256::from(3);
            let mid1 = left + one_third;
            let mid2 = right - one_third;

            let profit1 = self.calculate_profit_for_input(path, market_snapshot, mid1);
            let profit2 = self.calculate_profit_for_input(path, market_snapshot, mid2);

            match (profit1, profit2) {
                (Ok((profit1_val, output1)), Ok((profit2_val, output2))) => {
                    if profit1_val > profit2_val {
                        right = mid2;
                        if profit1_val > best_profit {
                            best_input = mid1;
                            best_output = output1;
                            best_profit = profit1_val;
                        }
                    } else {
                        left = mid1;
                        if profit2_val > best_profit {
                            best_input = mid2;
                            best_output = output2;
                            best_profit = profit2_val;
                        }
                    }
                }
                (Ok((profit1_val, output1)), Err(_)) => {
                    right = mid2;
                    if profit1_val > best_profit {
                        best_input = mid1;
                        best_output = output1;
                        best_profit = profit1_val;
                    }
                }
                (Err(_), Ok((profit2_val, output2))) => {
                    left = mid1;
                    if profit2_val > best_profit {
                        best_input = mid2;
                        best_output = output2;
                        best_profit = profit2_val;
                    }
                }
                (Err(_), Err(_)) => {
                    // Both failed, narrow the search randomly
                    left = (left + right) / U256::from(2);
                    right = left + precision;
                }
            }

            iterations += 1;
        }

        if best_profit > 0.0 {
            Ok((best_input, best_output))
        } else {
            // Fallback: try a few fixed amounts
            self.try_fixed_input_amounts(path, market_snapshot)
        }
    }

    /// Try a few common input amounts as fallback
    fn try_fixed_input_amounts(
        &self,
        path: &SwapPath,
        market_snapshot: &MarketSnapshot,
    ) -> Result<(U256, U256)> {
        let test_amounts = [
            U256::from(100_000_000_000_000_000u64), // 0.1 ETH
            U256::from(500_000_000_000_000_000u64), // 0.5 ETH
            U256::from_str_radix("1000000000000000000", 10).unwrap(), // 1 ETH
            U256::from_str_radix("5000000000000000000", 10).unwrap(), // 5 ETH
        ];

        let mut best_input = test_amounts[0];
        let mut best_output = U256::ZERO;
        let mut best_profit = 0.0f64;

        for &amount in &test_amounts {
            if let Ok((profit, output)) = self.calculate_profit_for_input(path, market_snapshot, amount) {
                if profit > best_profit {
                    best_input = amount;
                    best_output = output;
                    best_profit = profit;
                }
            }
        }

        Ok((best_input, best_output))
    }

    /// Calculate profit for a specific input amount
    fn calculate_profit_for_input(
        &self,
        path: &SwapPath,
        market_snapshot: &MarketSnapshot,
        input_amount: U256,
    ) -> Result<(f64, U256), CalculationError> {
        let output_amount = self.simulate_swap_path(path, market_snapshot, input_amount)?;
        
        if output_amount <= input_amount {
            return Err(CalculationError::NotImplemented);
        }

        let profit_mnt_wei = output_amount - input_amount;
        
        Ok((profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0), output_amount))
    }

    /// Simulate executing a swap path with given input amount
    /// 
    /// This chains getAmountOut calls across all pools in the path
    fn simulate_swap_path(
        &self,
        path: &SwapPath,
        market_snapshot: &MarketSnapshot,
        mut amount: U256,
    ) -> Result<U256, CalculationError> {
        for (i, pool) in path.pools.iter().enumerate() {
            let pool_id = pool.get_pool_id();
            let (reserve0, reserve1) = market_snapshot.get_pool_reserves(&pool_id)
                .ok_or(CalculationError::NotImplemented)?;

            // Determine which token we're swapping from and to
            let token_in = path.tokens.get(i).unwrap();
            let token_out = path.tokens.get(i + 1).unwrap();

            // For now, use a simplified calculation (constant product formula)
            // This should be replaced with proper pool-specific calculations
            amount = self.simple_constant_product_formula(
                amount,
                token_in.get_address(),
                token_out.get_address(),
                reserve0,
                reserve1,
                pool.get_fee(),
            )?;
        }

        Ok(amount)
    }

    /// Calculate gas cost in MNT Wei for executing a path
    pub fn calculate_gas_cost_mnt_wei(&self, _path: &SwapPath) -> U256 {
        // Use the total gas per transaction as configured
        let total_gas = self.config.gas_per_transaction;
        
        // Convert gas price from Gwei to Wei
        // gas_price_gwei is f64, so we need to handle fractional gwei
        let gwei_to_wei = 1_000_000_000u64; // 1 Gwei = 10^9 Wei
        let gas_price_wei_f64 = self.config.gas_price_gwei * (gwei_to_wei as f64);
        
        // Calculate total gas cost in Wei
        let gas_cost_wei_f64 = (total_gas as f64) * gas_price_wei_f64;
        
        // Convert to U256 (rounding down)
        U256::from(gas_cost_wei_f64 as u64)
    }


    /// Simplified constant product formula for AMM calculations
    /// This is a temporary implementation until proper pool-specific calculations are implemented
    fn simple_constant_product_formula(
        &self,
        amount_in: U256,
        token_in: Address,
        token_out: Address,
        reserve0: U256,
        reserve1: U256,
        fee: U256,
    ) -> Result<U256, CalculationError> {
        if amount_in.is_zero() {
            return Ok(U256::ZERO);
        }

        // Determine which reserve is for which token
        // This is simplified and assumes token ordering - in real implementation,
        // we'd need to check the actual token addresses against the pool's token0/token1
        let (reserve_in, reserve_out) = if token_in < token_out {
            (reserve0, reserve1)
        } else {
            (reserve1, reserve0)
        };

        if reserve_in.is_zero() || reserve_out.is_zero() {
            return Err(CalculationError::NotImplemented);
        }

        // Apply fee (assuming fee is in basis points, e.g., 30 for 0.3%)
        let fee_multiplier = U256::from(10000) - fee;
        let amount_in_with_fee = amount_in * fee_multiplier / U256::from(10000);

        // Constant product formula: x * y = k
        // amount_out = (amount_in_with_fee * reserve_out) / (reserve_in + amount_in_with_fee)
        let numerator = amount_in_with_fee * reserve_out;
        let denominator = reserve_in + amount_in_with_fee;

        if denominator.is_zero() {
            return Err(CalculationError::NotImplemented);
        }

        Ok(numerator / denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::graph::SwapPath;
    use crate::logic::types::MarketSnapshot;
    use crate::{MockPool, PoolWrapper, Token};
    use crate::logic::pools::PoolId;
    use crate::utils::constants::WMNT;
    use alloy_primitives::Address;
    use std::sync::Arc;

    fn create_test_path() -> SwapPath {
        let wmnt_token = Arc::new(Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18)));
        let token1 = Arc::new(Token::new_with_data(Address::repeat_byte(1), Some("TOKEN1".to_string()), None, Some(18)));

        let pool = PoolWrapper::from(MockPool::new(WMNT, Address::repeat_byte(1), Address::repeat_byte(10)));

        SwapPath::new_first(wmnt_token, token1, pool)
    }

    fn create_test_market_snapshot() -> MarketSnapshot {
        let mut snapshot = MarketSnapshot::new(12345);
        
        // Add mock reserves for the test pool
        let pool_id = PoolId::Address(Address::repeat_byte(10));
        snapshot.set_pool_reserves(
            pool_id,
            U256::from_str_radix("1000000000000000000000", 10).unwrap(), // 1000 tokens
            U256::from_str_radix("1000000000000000000000", 10).unwrap(), // 1000 tokens
        );

        snapshot
    }

    #[test]
    fn test_profit_calculator_creation() {
        let config = ArbitrageConfig::default();
        let calculator = ProfitCalculator::new(config);
        
        assert!(calculator.config.enable_parallel_calculation);
        assert_eq!(calculator.config.max_hops, 4);
    }

    #[test]
    fn test_gas_cost_calculation_mnt_wei() {
        let config = ArbitrageConfig::default();
        let calculator = ProfitCalculator::new(config);
        let path = create_test_path();
        
        // Test gas cost calculation returns MNT Wei
        let gas_cost_wei = calculator.calculate_gas_cost_mnt_wei(&path);
        
        // Should be positive (700M gas * 0.02 gwei = 14,000,000 gwei = 14,000,000,000,000,000 wei)
        assert!(!gas_cost_wei.is_zero());
        
        // Expected: 700,000,000 * 0.02 * 10^9 = 14,000,000,000,000,000 wei = 0.014 MNT
        let expected_wei = U256::from(14_000_000_000_000_000u64);
        assert_eq!(gas_cost_wei, expected_wei);
    }


    #[test]
    fn test_profit_calculation_with_missing_reserves() {
        let config = ArbitrageConfig::default();
        let calculator = ProfitCalculator::new(config);
        let path = create_test_path();
        let empty_snapshot = MarketSnapshot::new(12345); // No reserves

        let result = calculator.calculate_path_profit(&path, &empty_snapshot);
        
        assert!(!result.calculation_successful);
        assert!(result.error_message.is_some());
    }

    #[test]
    fn test_parallel_vs_sequential_calculation() {
        let mut config = ArbitrageConfig::default();
        let path = create_test_path();
        let paths = vec![path; 100]; // 100 identical paths
        let snapshot = create_test_market_snapshot();

        // Test parallel calculation
        config.enable_parallel_calculation = true;
        let parallel_calculator = ProfitCalculator::new(config.clone());
        let parallel_results = parallel_calculator.calculate_profits_parallel(&paths, &snapshot);

        // Test sequential calculation  
        config.enable_parallel_calculation = false;
        let sequential_calculator = ProfitCalculator::new(config);
        let sequential_results = sequential_calculator.calculate_profits_parallel(&paths, &snapshot);

        // Results should be the same length
        assert_eq!(parallel_results.len(), sequential_results.len());
        assert_eq!(parallel_results.len(), 100);
    }
}
