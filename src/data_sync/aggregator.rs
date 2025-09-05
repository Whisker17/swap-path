use crate::logic::types::MarketSnapshot;
use crate::logic::pools::PoolId;
use crate::data_sync::multicall::MulticallManager;
use crate::data_sync::websocket::BlockHeader;
use crate::data_sync::markets::Market;
use alloy_primitives::U256;
use eyre::Result;
use std::collections::HashMap;
use std::time::Instant;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Data aggregator that combines pool data into market snapshots
/// Reuses existing Market infrastructure for better code reuse
pub struct DataAggregator {
    multicall_manager: MulticallManager,
    max_pools_per_batch: usize,
    // Track previous reserves to detect changes
    previous_reserves: HashMap<PoolId, (U256, U256)>,
    // Reference to market for pool/token information (for enhanced logging)
    market: Option<Arc<RwLock<Market>>>,
}

impl DataAggregator {
    /// Create a new data aggregator
    pub fn new(
        multicall_manager: MulticallManager,
        max_pools_per_batch: usize,
    ) -> Self {
        Self {
            multicall_manager,
            max_pools_per_batch,
            previous_reserves: HashMap::new(),
            market: None,
        }
    }
    
    /// Set market reference for enhanced logging
    pub fn set_market(&mut self, market: Arc<RwLock<Market>>) {
        self.market = Some(market);
    }
    

    
    /// Aggregate pool data for a new block into a market snapshot
    /// Includes detailed logging for performance monitoring and change detection
    pub async fn aggregate_market_data(&mut self, block_header: &BlockHeader, monitored_pools: Vec<PoolId>, total_pools_count: Option<usize>) -> Result<MarketSnapshot> {
        let start_time = Instant::now();
        
        let block_number = block_header.block_number()?;
        debug!("Aggregating market data for block {}", block_number);
        
        if monitored_pools.is_empty() {
            warn!("No pools to monitor");
            return Ok(MarketSnapshot::new(block_number));
        }
        
        // Create new market snapshot with pool context
        let mut snapshot = MarketSnapshot::new(block_number);
        
        // Set enabled pools and total count for optimization (avoid repeated MarketWithoutLock queries)
        let enabled_pools_set = monitored_pools.iter().cloned().collect();
        snapshot.set_enabled_pools(enabled_pools_set);
        if let Some(count) = total_pools_count {
            snapshot.set_total_pools_count(count);
        }
        
        // Log the start of reserves fetching
        let fetch_start = Instant::now();
        info!("Block {}: Starting reserves fetch for {} pools", block_number, monitored_pools.len());
        
        // Batch query pools
        let pool_batches = self.split_pools_into_batches(&monitored_pools);
        let mut all_pool_data = HashMap::new();
        let mut total_successful = 0;
        let mut total_failed = 0;
        
        for (batch_idx, batch) in pool_batches.iter().enumerate() {
            let batch_start = Instant::now();
            
            match self.multicall_manager.batch_get_reserves(batch, Some(block_number)).await {
                Ok(batch_results) => {
                    let batch_elapsed = batch_start.elapsed();
                    debug!("Block {}: Batch {} ({} pools) completed in {:?}", 
                           block_number, batch_idx + 1, batch.len(), batch_elapsed);
                    
                    for (pool_id, reserves_opt) in batch_results {
                        if reserves_opt.is_some() {
                            total_successful += 1;
                        } else {
                            total_failed += 1;
                        }
                        all_pool_data.insert(pool_id, reserves_opt);
                    }
                }
                Err(e) => {
                    let batch_elapsed = batch_start.elapsed();
                    error!("Block {}: Batch {} failed after {:?}: {}", 
                           block_number, batch_idx + 1, batch_elapsed, e);
                    
                    // Mark all pools in failed batch as failed
                    for pool_id in batch {
                        all_pool_data.insert(*pool_id, None);
                        total_failed += 1;
                    }
                }
            }
        }
        
        let fetch_elapsed = fetch_start.elapsed();
        
        // Process results and detect changes
        let mut changed_pools = Vec::new();
        
        for (pool_id, reserves_opt) in all_pool_data {
            match reserves_opt {
                Some((reserve0, reserve1)) => {
                    snapshot.set_pool_reserves(pool_id, reserve0, reserve1);
                    
                    // Check for reserve changes
                    if let Some((prev_reserve0, prev_reserve1)) = self.previous_reserves.get(&pool_id) {
                        if *prev_reserve0 != reserve0 || *prev_reserve1 != reserve1 {
                            changed_pools.push((pool_id, (*prev_reserve0, *prev_reserve1), (reserve0, reserve1)));
                        }
                    } else {
                        // New pool, consider it as changed
                        changed_pools.push((pool_id, (U256::ZERO, U256::ZERO), (reserve0, reserve1)));
                    }
                    
                    // Update previous reserves
                    self.previous_reserves.insert(pool_id, (reserve0, reserve1));
                }
                None => {
                    warn!("Block {}: Failed to get reserves for pool {:?}", block_number, pool_id);
                }
            }
        }
        
        let total_elapsed = start_time.elapsed();
        
        // Log detailed timing information
        info!(
            "Block {}: Reserve fetch completed in {:?} (total: {:?}) - {} successful, {} failed pools",
            block_number, fetch_elapsed, total_elapsed, total_successful, total_failed
        );
        
        // Log reserve changes with enhanced formatting
        if !changed_pools.is_empty() {
            info!("🔄 Block {}: {} pools have reserve changes:", block_number, changed_pools.len());
            for (pool_id, (prev_r0, prev_r1), (new_r0, new_r1)) in &changed_pools {
                self.log_reserve_change_detailed(pool_id, (prev_r0, prev_r1), (new_r0, new_r1)).await;
            }
        } else {
            debug!("✅ Block {}: No reserve changes detected", block_number);
        }
        
        // Update timestamp to reflect actual aggregation time
        snapshot.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        Ok(snapshot)
    }
    
    /// Split monitored pools into batches for multicall
    fn split_pools_into_batches(&self, pools: &[PoolId]) -> Vec<Vec<PoolId>> {
        let mut batches = Vec::new();
        
        for chunk in pools.chunks(self.max_pools_per_batch) {
            batches.push(chunk.to_vec());
        }
        
        if batches.is_empty() {
            // Ensure we always have at least one batch, even if empty
            batches.push(Vec::new());
        }
        
        batches
    }
    
    /// Validate market snapshot
    pub fn validate_snapshot(&self, snapshot: &MarketSnapshot, monitored_count: usize) -> Result<()> {
        if snapshot.block_number == 0 {
            return Err(eyre::eyre!("Invalid block number: 0"));
        }
        
        if snapshot.timestamp == 0 {
            return Err(eyre::eyre!("Invalid timestamp: 0"));
        }
        
        
        // Check that we have data for most of our monitored pools
        let received_count = snapshot.pool_reserves.len();
        
        if monitored_count > 0 {
            let success_rate = (received_count as f64) / (monitored_count as f64);
            if success_rate < 0.5 {
                warn!(
                    "Low success rate for pool data: {}/{} ({:.1}%)",
                    received_count, monitored_count, success_rate * 100.0
                );
            }
        }
        
        Ok(())
    }
    
    /// Get statistics about the aggregator
    pub fn get_stats(&self, monitored_pools_count: usize) -> AggregatorStats {
        AggregatorStats {
            monitored_pools_count,
            max_pools_per_batch: self.max_pools_per_batch,
        }
    }
    
    /// Log detailed reserve changes with token information and human-readable format
    async fn log_reserve_change_detailed(&self, pool_id: &PoolId, prev_reserves: (&U256, &U256), new_reserves: (&U256, &U256)) {
        if let Some(market) = &self.market {
            let market_guard = market.read().await;
            if let Some(pool) = market_guard.get_pool(pool_id) {
                let tokens = pool.get_tokens();
                if tokens.len() >= 2 {
                    let token0_addr = tokens[0];
                    let token1_addr = tokens[1];
                    
                    // Get token information from market
                    let token0_info = market_guard.token_graph.tokens.get(&token0_addr);
                    let token1_info = market_guard.token_graph.tokens.get(&token1_addr);
                    
                    let token0_symbol = token0_info.map(|t| t.get_symbol()).unwrap_or_else(|| format!("{:#x}", token0_addr));
                    let token1_symbol = token1_info.map(|t| t.get_symbol()).unwrap_or_else(|| format!("{:#x}", token1_addr));
                    
                    // Format reserves with decimals
                    let (prev_r0_formatted, new_r0_formatted) = if let Some(token0) = token0_info {
                        let prev_f = token0.to_float(*prev_reserves.0);
                        let new_f = token0.to_float(*new_reserves.0);
                        (format!("{:.3}", prev_f), format!("{:.3}", new_f))
                    } else {
                        (prev_reserves.0.to_string(), new_reserves.0.to_string())
                    };
                    
                    let (prev_r1_formatted, new_r1_formatted) = if let Some(token1) = token1_info {
                        let prev_f = token1.to_float(*prev_reserves.1);
                        let new_f = token1.to_float(*new_reserves.1);
                        (format!("{:.3}", prev_f), format!("{:.3}", new_f))
                    } else {
                        (prev_reserves.1.to_string(), new_reserves.1.to_string())
                    };
                    
                    // Determine change direction emojis
                    let r0_emoji = if new_reserves.0 > prev_reserves.0 { "📈" } else if new_reserves.0 < prev_reserves.0 { "📉" } else { "➡️" };
                    let r1_emoji = if new_reserves.1 > prev_reserves.1 { "📈" } else if new_reserves.1 < prev_reserves.1 { "📉" } else { "➡️" };
                    
                    info!("  💧 Pool {}: {} {} -> {} {} | {} {} -> {} {}",
                        pool_id,
                        token0_symbol, 
                        prev_r0_formatted,
                        r0_emoji,
                        new_r0_formatted,
                        token1_symbol,
                        prev_r1_formatted,
                        r1_emoji,
                        new_r1_formatted
                    );
                    
                    // Also log raw values for debugging if needed
                    debug!("    Raw: {} -> {} | {} -> {}", 
                           prev_reserves.0, new_reserves.0, prev_reserves.1, new_reserves.1);
                    
                    return;
                }
            }
        }
        
        // Fallback to basic logging if market info unavailable
        info!("  Pool {:?}: reserves {} -> {} | {} -> {}", 
              pool_id, prev_reserves.0, new_reserves.0, prev_reserves.1, new_reserves.1);
    }
}

/// Statistics about the data aggregator
#[derive(Debug, Clone)]
pub struct AggregatorStats {
    pub monitored_pools_count: usize,
    pub max_pools_per_batch: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_sync::multicall::MulticallManager;
    use alloy_primitives::Address;
    use std::time::Duration;
    
    fn create_test_aggregator() -> DataAggregator {
        let multicall_address = Address::repeat_byte(0x11);
        let rpc_url = "https://test.rpc".to_string();
        let timeout = Duration::from_secs(10);
        let multicall_manager = MulticallManager::new(multicall_address, rpc_url, timeout);
        
        DataAggregator::new(multicall_manager, 50)
    }
    
    #[test]
    fn test_aggregator_creation() {
        let aggregator = create_test_aggregator();
        assert_eq!(aggregator.max_pools_per_batch, 50);
    }
    
    #[test]
    fn test_split_pools_into_batches() {
        let aggregator = create_test_aggregator();
        
        let pools = vec![
            PoolId::Address(Address::repeat_byte(0x01)),
            PoolId::Address(Address::repeat_byte(0x02)),
            PoolId::Address(Address::repeat_byte(0x03)),
        ];
        
        let batches = aggregator.split_pools_into_batches(&pools);
        assert_eq!(batches.len(), 1); // All pools fit in one batch with default settings
        assert_eq!(batches[0].len(), 3);
    }
    
    #[test]
    fn test_validate_snapshot() {
        let aggregator = create_test_aggregator();
        let mut snapshot = MarketSnapshot::new(123);
        
        // Valid snapshot should pass
        assert!(aggregator.validate_snapshot(&snapshot, 2).is_ok());
        
        // Invalid block number
        snapshot.block_number = 0;
        assert!(aggregator.validate_snapshot(&snapshot, 2).is_err());
    }
    
    
    #[test]
    fn test_get_stats() {
        let aggregator = create_test_aggregator();
        let stats = aggregator.get_stats(2);
        
        assert_eq!(stats.monitored_pools_count, 2);
        assert_eq!(stats.max_pools_per_batch, 50);
    }
}
