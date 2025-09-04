use crate::data_sync::{
    config::DataSyncConfig,
    websocket::{WebSocketManager, BlockHeader},
    multicall::MulticallManager,
    aggregator::{DataAggregator, AggregatorStats},
    markets::{Market, MarketConfigSection},
};
use crate::logic::types::MarketSnapshot;
use crate::logic::pools::PoolId;
use crate::PoolWrapper;
use alloy_primitives::Address;
use eyre::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{info, warn, error, debug};

/// Main data synchronization service
/// 
/// This service implements the Data Layer architecture as described in the design document.
/// It uses WebSocket to subscribe to newHeads events and Multicall to batch query pool reserves.
/// Now reuses existing Market infrastructure for better code organization.
pub struct DataSyncService {
    config: DataSyncConfig,
    websocket_manager: Arc<WebSocketManager>,
    market: Arc<RwLock<Market>>,
    aggregator: Arc<RwLock<DataAggregator>>,
    
    // Channels for communication
    market_data_tx: mpsc::Sender<MarketSnapshot>,
    market_data_rx: Option<mpsc::Receiver<MarketSnapshot>>,
    
    // Task handles
    websocket_task: Option<JoinHandle<()>>,
    aggregation_task: Option<JoinHandle<()>>,
    
    // Shutdown coordination
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl DataSyncService {
    /// Create a new data synchronization service
    pub async fn new(config: DataSyncConfig, initial_pools: Vec<PoolWrapper>) -> Result<Self> {
        info!("Initializing DataSyncService with {} initial pools", initial_pools.len());
        
        // Create WebSocket manager
        let websocket_manager = Arc::new(WebSocketManager::new(
            config.rpc_wss_url.clone(),
            config.ws_connection_timeout(),
            config.max_reconnect_attempts,
            config.reconnect_delay(),
        ));
        
        // Create Multicall manager
        let multicall_address = config.multicall_address.parse::<Address>()
            .map_err(|e| eyre::eyre!("Invalid multicall address: {}", e))?;
        
        let multicall_manager = MulticallManager::new(
            multicall_address,
            config.rpc_http_url.clone(),
            config.http_timeout(),
        );
        
        // Create market and add initial pools
        let mut market = Market::new(MarketConfigSection::default());
        for pool in initial_pools {
            market.add_pool(pool);
        }
        let market = Arc::new(RwLock::new(market));
        
        // Create data aggregator
        let aggregator = Arc::new(RwLock::new(DataAggregator::new(
            multicall_manager,
            config.max_pools_per_batch,
        )));
        
        // Create market data channel
        let (market_data_tx, market_data_rx) = mpsc::channel(config.channel_buffer_size);
        
        Ok(Self {
            config,
            websocket_manager,
            market,
            aggregator,
            market_data_tx,
            market_data_rx: Some(market_data_rx),
            websocket_task: None,
            aggregation_task: None,
            shutdown_tx: None,
        })
    }
    
    /// Start the data synchronization service
    pub async fn start(&mut self) -> Result<mpsc::Receiver<MarketSnapshot>> {
        info!("Starting DataSyncService");
        
        // Take the receiver to return to caller
        let market_data_rx = self.market_data_rx.take()
            .ok_or_else(|| eyre::eyre!("DataSyncService already started"))?;
        
        // Start WebSocket subscription
        let (block_rx, shutdown_tx) = self.websocket_manager.subscribe_new_heads().await?;
        self.shutdown_tx = Some(shutdown_tx);
        
        // Start aggregation task
        let aggregation_task = self.start_aggregation_task(block_rx).await?;
        self.aggregation_task = Some(aggregation_task);
        
        info!("DataSyncService started successfully");
        Ok(market_data_rx)
    }
    
    /// Stop the data synchronization service
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping DataSyncService");
        
        // Send shutdown signal
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(()).await;
        }
        
        // Wait for tasks to complete
        if let Some(aggregation_task) = self.aggregation_task.take() {
            if let Err(e) = aggregation_task.await {
                warn!("Aggregation task error during shutdown: {}", e);
            }
        }
        
        if let Some(websocket_task) = self.websocket_task.take() {
            if let Err(e) = websocket_task.await {
                warn!("WebSocket task error during shutdown: {}", e);
            }
        }
        
        info!("DataSyncService stopped");
        Ok(())
    }
    
    /// Add a pool to monitoring (adds to market)
    pub async fn add_pool(&self, pool: PoolWrapper) -> Result<()> {
        let mut market = self.market.write().await;
        market.add_pool(pool);
        Ok(())
    }
    
    /// Enable a pool for monitoring
    pub async fn enable_pool(&self, pool_id: PoolId) -> Result<()> {
        let mut market = self.market.write().await;
        market.enable_pool(pool_id)
    }
    
    /// Disable a pool from monitoring
    pub async fn disable_pool(&self, pool_id: PoolId) -> Result<()> {
        let mut market = self.market.write().await;
        market.disable_pool(pool_id)
    }
    
    /// Get current monitored pools
    pub async fn get_monitored_pools(&self) -> Vec<PoolId> {
        let market = self.market.read().await;
        market.enabled_pools()
            .into_iter()
            .map(|pool| pool.get_pool_id())
            .collect()
    }
    
    
    /// Get aggregator statistics
    pub async fn get_stats(&self) -> AggregatorStats {
        let aggregator = self.aggregator.read().await;
        let monitored_count = self.get_monitored_pools().await.len();
        aggregator.get_stats(monitored_count)
    }
    
    /// Start the aggregation task that processes new blocks
    async fn start_aggregation_task(
        &self,
        mut block_rx: mpsc::Receiver<BlockHeader>,
    ) -> Result<JoinHandle<()>> {
        let aggregator = Arc::clone(&self.aggregator);
        let market = Arc::clone(&self.market);
        let market_data_tx = self.market_data_tx.clone();
        
        let task = tokio::spawn(async move {
            info!("Aggregation task started");
            
            while let Some(block_header) = block_rx.recv().await {
                debug!("Processing new block: {}", block_header.number);
                
                // Get current monitored pools and market stats from market
                let (monitored_pools, total_pools_count) = {
                    let market_guard = market.read().await;
                    let pools = market_guard.enabled_pools()
                        .into_iter()
                        .map(|pool| pool.get_pool_id())
                        .collect::<Vec<_>>();
                    let total_count = market_guard.pools().len();
                    (pools, total_count)
                };
                
                let monitored_count = monitored_pools.len();
                
                // Aggregate market data for this block
                let mut aggregator_guard = aggregator.write().await;
                match aggregator_guard.aggregate_market_data(&block_header, monitored_pools, Some(total_pools_count)).await {
                    Ok(snapshot) => {
                        // Validate snapshot before sending
                        if let Err(e) = aggregator_guard.validate_snapshot(&snapshot, monitored_count) {
                            warn!("Invalid market snapshot: {}", e);
                            continue;
                        }
                        
                        // Send snapshot to logic layer
                        if let Err(e) = market_data_tx.try_send(snapshot) {
                            match e {
                                mpsc::error::TrySendError::Full(_) => {
                                    warn!("Market data channel is full, dropping snapshot");
                                }
                                mpsc::error::TrySendError::Closed(_) => {
                                    error!("Market data channel is closed, stopping aggregation");
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to aggregate market data for block {}: {}", 
                               block_header.number, e);
                    }
                }
                
                drop(aggregator_guard);
            }
            
            info!("Aggregation task ended");
        });
        
        Ok(task)
    }
    
    /// Check if the service is running
    pub fn is_running(&self) -> bool {
        self.aggregation_task.is_some() && 
        !self.aggregation_task.as_ref().unwrap().is_finished()
    }
    
    /// Get service configuration
    pub fn get_config(&self) -> &DataSyncConfig {
        &self.config
    }
}

impl Drop for DataSyncService {
    fn drop(&mut self) {
        // Attempt graceful shutdown on drop
        if self.is_running() {
            warn!("DataSyncService dropped while running, tasks may be orphaned");
        }
    }
}

/// Builder for DataSyncService to make creation more ergonomic
pub struct DataSyncServiceBuilder {
    config: Option<DataSyncConfig>,
    pools: Vec<PoolWrapper>,
}

impl DataSyncServiceBuilder {
    pub fn new() -> Self {
        Self {
            config: None,
            pools: Vec::new(),
        }
    }
    
    pub fn with_config(mut self, config: DataSyncConfig) -> Self {
        self.config = Some(config);
        self
    }
    
    pub fn with_pools(mut self, pools: Vec<PoolWrapper>) -> Self {
        self.pools = pools;
        self
    }
    
    pub fn add_pool(mut self, pool: PoolWrapper) -> Self {
        self.pools.push(pool);
        self
    }
    
    pub async fn build(self) -> Result<DataSyncService> {
        let config = self.config.unwrap_or_else(|| {
            DataSyncConfig::from_env().unwrap_or_default()
        });
        
        DataSyncService::new(config, self.pools).await
    }
}

impl Default for DataSyncServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_service_creation() {
        let config = DataSyncConfig::default();
        let pools = vec![];
        
        let service = DataSyncService::new(config, pools).await;
        assert!(service.is_ok());
        
        let service = service.unwrap();
        assert!(!service.is_running());
        assert_eq!(service.get_monitored_pools().await.len(), 0);
    }
    
    #[tokio::test]
    async fn test_builder_pattern() {
        let service = DataSyncServiceBuilder::new()
            .build()
            .await;
            
        assert!(service.is_ok());
        let service = service.unwrap();
        assert_eq!(service.get_monitored_pools().await.len(), 0);
    }
    
    #[tokio::test]
    async fn test_pool_management() {
        use crate::logic::pools::mock_pool::MockPool;
        use std::sync::Arc;
        
        let service = DataSyncService::new(DataSyncConfig::default(), vec![]).await.unwrap();
        
        let pool_address = Address::repeat_byte(0x01);
        let pool_id = PoolId::Address(pool_address);
        let mock_pool = PoolWrapper::new(Arc::new(MockPool { 
            address: pool_address, 
            token0: Address::repeat_byte(0x02), 
            token1: Address::repeat_byte(0x03) 
        }));
        
        // Add pool
        service.add_pool(mock_pool).await.unwrap();
        assert_eq!(service.get_monitored_pools().await.len(), 1);
        
        // Disable pool
        service.disable_pool(pool_id).await.unwrap();
        assert_eq!(service.get_monitored_pools().await.len(), 0);
        
        // Enable pool
        service.enable_pool(pool_id).await.unwrap();
        assert_eq!(service.get_monitored_pools().await.len(), 1);
    }
    
}
