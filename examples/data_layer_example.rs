/// Data Layer Usage Example
/// 
/// This example demonstrates how to use the data synchronization layer
/// according to the design document's architecture (方案A).

use swap_path::data_sync::{DataSyncConfig, DataSyncServiceBuilder};
// use swap_path::logic::pools::PoolId; // Removed unused import
// use alloy_primitives::Address;
use eyre::Result;
use tracing::{info, warn, error};
use tokio::time::{timeout, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Data Layer Example");
    
    // Load configuration from environment or use defaults
    let config = DataSyncConfig::from_env().unwrap_or_else(|_| {
        warn!("Failed to load config from environment, using defaults");
        DataSyncConfig::default()
    });
    
    info!("Configuration loaded: WebSocket={}, HTTP={}", 
          config.rpc_wss_url, config.rpc_http_url);
    
    // Create data sync service using builder pattern
    let mut data_service = DataSyncServiceBuilder::new()
        .with_config(config)
        .build()
        .await?;
    
    info!("Data service created successfully");
    
    // Start the service and get market data receiver
    let mut market_data_rx = data_service.start().await?;
    
    info!("Data service started, waiting for market data...");
    
    // Process market data for a limited time
    let process_duration = Duration::from_secs(60); // Run for 1 minute
    let result = timeout(process_duration, async {
        let mut snapshot_count = 0;
        
        while let Some(market_snapshot) = market_data_rx.recv().await {
            snapshot_count += 1;
            
            info!("Received market snapshot #{}: block={}, pools={}, timestamp={}", 
                  snapshot_count,
                  market_snapshot.block_number,
                  market_snapshot.pool_reserves.len(),
                  market_snapshot.timestamp);
            
            // Example: Process the market snapshot
            process_market_snapshot(&market_snapshot).await;
            
            // Print service statistics every 10 snapshots
            if snapshot_count % 10 == 0 {
                let stats = data_service.get_stats().await;
                info!("Service stats: monitored_pools={}", 
                      stats.monitored_pools_count);
            }
        }
        
        info!("Market data stream ended");
    }).await;
    
    match result {
        Ok(_) => info!("Data processing completed normally"),
        Err(_) => info!("Data processing timed out after {:?}", process_duration),
    }
    
    // Stop the service
    if let Err(e) = data_service.stop().await {
        error!("Error stopping data service: {}", e);
    } else {
        info!("Data service stopped successfully");
    }
    
    info!("Data Layer Example completed");
    Ok(())
}

/// Example function to process market snapshots
async fn process_market_snapshot(snapshot: &swap_path::logic::types::MarketSnapshot) {
    // Example processing logic that would be implemented by the Logic Layer
    
    let mut total_liquidity = 0.0;
    let mut pool_count = 0;
    
    for (pool_id, (reserve0, reserve1)) in &snapshot.pool_reserves {
        pool_count += 1;
        
        // Simple liquidity calculation (assuming both tokens have similar value)
        // Convert U256 to f64 for display purposes
        let reserve0_f64 = reserve0.to::<u128>() as f64;
        let reserve1_f64 = reserve1.to::<u128>() as f64;
        let pool_liquidity = reserve0_f64 + reserve1_f64;
        total_liquidity += pool_liquidity;
        
        // Log detailed info for first few pools
        if pool_count <= 3 {
            info!("Pool {:?}: reserve0={}, reserve1={}", 
                  pool_id, reserve0, reserve1);
        }
    }
    
    if pool_count > 0 {
        let avg_liquidity = total_liquidity / pool_count as f64;
        info!("Block {} analysis: {} pools, avg liquidity: {:.2e}", 
              snapshot.block_number, pool_count, avg_liquidity);
    }
    
    // Here you would typically:
    // 1. Update your internal market state
    // 2. Calculate arbitrage opportunities
    // 3. Execute trades if profitable
    // 4. Update risk management parameters
}

/// Example of dynamic pool management
#[allow(dead_code)]
async fn demonstrate_pool_management(service: &swap_path::data_sync::DataSyncService) -> Result<()> {
    use swap_path::logic::pools::mock_pool::MockPool;
    use swap_path::PoolWrapper;
    use std::sync::Arc;
    
    // Add a new pool to monitoring
    let pool_address = "0x4567890123456789012345678901234567890123".parse()?;
    let mock_pool = PoolWrapper::new(Arc::new(MockPool {
        address: pool_address,
        token0: "0x1111111111111111111111111111111111111111".parse()?,
        token1: "0x2222222222222222222222222222222222222222".parse()?,
    }));
    service.add_pool(mock_pool).await?;
    info!("Added new pool to monitoring: {:?}", pool_address);
    
    // Service is running and monitoring pool data
    info!("Service is monitoring {} pools", service.get_monitored_pools().await.len());
    
    // Get current stats
    let stats = service.get_stats().await;
    info!("Current stats: {} pools monitored", stats.monitored_pools_count);
    
    Ok(())
}

/// Example configuration for different environments
#[allow(dead_code)]
fn create_configs_for_different_environments() -> Result<()> {
    // Development configuration
    let dev_config = DataSyncConfig {
        rpc_wss_url: "wss://rpc.mantle.xyz".to_string(),
        rpc_http_url: "https://rpc.mantle.xyz".to_string(),
        multicall_address: "0xcA11bde05977b3631167028862bE2a173976CA11".to_string(),
        max_pools_per_batch: 20, // Smaller batches for dev
        ws_connection_timeout_secs: 30,
        max_reconnect_attempts: 3,
        reconnect_delay_secs: 5,
        http_timeout_secs: 15,
        channel_buffer_size: 50,
    };
    
    // Production configuration
    let prod_config = DataSyncConfig {
        rpc_wss_url: "wss://prod-rpc.mantle.xyz".to_string(),
        rpc_http_url: "https://prod-rpc.mantle.xyz".to_string(),
        multicall_address: "0xcA11bde05977b3631167028862bE2a173976CA11".to_string(),
        max_pools_per_batch: 50, // Larger batches for efficiency
        ws_connection_timeout_secs: 10, // Faster timeout
        max_reconnect_attempts: 10, // More attempts
        reconnect_delay_secs: 2, // Faster reconnection
        http_timeout_secs: 5, // Tighter timeout
        channel_buffer_size: 200, // Larger buffer
    };
    
    info!("Dev config max batch size: {}", dev_config.max_pools_per_batch);
    info!("Prod config max batch size: {}", prod_config.max_pools_per_batch);
    
    Ok(())
}
