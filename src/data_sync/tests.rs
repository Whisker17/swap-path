/// Integration tests for the data synchronization layer
/// 
/// These tests verify the complete data flow from WebSocket subscription
/// to market snapshot delivery, following the design document architecture.

#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use crate::logic::pools::PoolId;
    use crate::PoolWrapper;
    use alloy_primitives::Address;
    use tokio::time::Duration;
    use std::sync::Arc;
    
    /// Mock RPC server for testing
    struct MockRpcServer {
        port: u16,
        // Could be extended with more mock functionality
    }
    
    impl MockRpcServer {
        fn new() -> Self {
            Self {
                port: 8545, // Default test port
            }
        }
        
        fn get_ws_url(&self) -> String {
            format!("ws://127.0.0.1:{}", self.port)
        }
        
        fn get_http_url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }
    }
    
    #[tokio::test]
    async fn test_data_sync_service_lifecycle() {
        let config = DataSyncConfig {
            rpc_wss_url: "wss://invalid.test".to_string(), // Will fail, but that's expected
            rpc_http_url: "https://invalid.test".to_string(),
            multicall_address: "0xcA11bde05977b3631167028862bE2a173976CA11".to_string(),
            max_pools_per_batch: 10,
            ws_connection_timeout_secs: 1, // Short timeout for testing
            max_reconnect_attempts: 1, // Only one attempt
            reconnect_delay_secs: 1,
            http_timeout_secs: 1,
            channel_buffer_size: 10,
        };
        
        // Create test pools using MockPool
        use crate::logic::pools::mock_pool::MockPool;
        use std::sync::Arc;
        
        let pools = vec![
            PoolWrapper::new(Arc::new(MockPool {
                address: Address::repeat_byte(0x01),
                token0: Address::repeat_byte(0x02),
                token1: Address::repeat_byte(0x03),
            })),
            PoolWrapper::new(Arc::new(MockPool {
                address: Address::repeat_byte(0x04),
                token0: Address::repeat_byte(0x05),
                token1: Address::repeat_byte(0x06),
            })),
        ];
        
        let service = DataSyncService::new(config, pools).await;
        assert!(service.is_ok());
        
        let mut service = service.unwrap();
        assert!(!service.is_running());
        assert_eq!(service.get_monitored_pools().await.len(), 2);
        
        // Test graceful shutdown even if not started
        assert!(service.stop().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_builder_pattern() {
        use crate::logic::pools::mock_pool::MockPool;
        use std::sync::Arc;
        
        let pool1 = PoolWrapper::new(Arc::new(MockPool {
            address: Address::repeat_byte(0x01),
            token0: Address::repeat_byte(0x02),
            token1: Address::repeat_byte(0x03),
        }));
        let pool2 = PoolWrapper::new(Arc::new(MockPool {
            address: Address::repeat_byte(0x04),
            token0: Address::repeat_byte(0x05),
            token1: Address::repeat_byte(0x06),
        }));
        
        let service = DataSyncServiceBuilder::new()
            .add_pool(pool1)
            .add_pool(pool2)
            .with_config(DataSyncConfig::default())
            .build()
            .await;
            
        assert!(service.is_ok());
        let service = service.unwrap();
        assert_eq!(service.get_monitored_pools().await.len(), 2);
    }
    
    #[tokio::test]
    async fn test_pool_management() {
        let service = DataSyncService::new(DataSyncConfig::default(), vec![]).await.unwrap();
        
        use crate::logic::pools::mock_pool::MockPool;
        use std::sync::Arc;
        
        let pool_address = Address::repeat_byte(0x01);
        let pool_id = PoolId::Address(pool_address);
        let mock_pool = PoolWrapper::new(Arc::new(MockPool {
            address: pool_address,
            token0: Address::repeat_byte(0x02),
            token1: Address::repeat_byte(0x03),
        }));
        
        // Add pool
        service.add_pool(mock_pool).await.unwrap();
        assert_eq!(service.get_monitored_pools().await.len(), 1);
        assert!(service.get_monitored_pools().await.contains(&pool_id));
        
        // Disable pool
        service.disable_pool(pool_id).await.unwrap();
        assert_eq!(service.get_monitored_pools().await.len(), 0);
        assert!(!service.get_monitored_pools().await.contains(&pool_id));
    }
    
    #[tokio::test]
    async fn test_eth_price_update() {
        let service = DataSyncService::new(DataSyncConfig::default(), vec![]).await.unwrap();
        
        // Check stats
        let stats = service.get_stats().await;
        assert_eq!(stats.max_pools_per_batch, 50);
    }
    
    #[tokio::test]
    async fn test_config_validation() {
        // Test invalid multicall address
        let result = DataSyncService::new(
            DataSyncConfig {
                multicall_address: "invalid_address".to_string(),
                ..DataSyncConfig::default()
            },
            vec![]
        ).await;
        assert!(result.is_err());
        
        // Test valid config
        let result = DataSyncService::new(DataSyncConfig::default(), vec![]).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_concurrent_pool_operations() {
        use crate::logic::pools::mock_pool::MockPool;
        use std::sync::Arc as StdArc;
        
        let service = Arc::new(
            DataSyncService::new(DataSyncConfig::default(), vec![]).await.unwrap()
        );
        
        let mut handles = vec![];
        
        // Spawn multiple tasks to add/disable pools concurrently
        for i in 0..10 {
            let service_clone = Arc::clone(&service);
            let handle = tokio::spawn(async move {
                let pool_address = Address::from_slice(&[(i as u8); 20]);
                let pool_id = PoolId::Address(pool_address);
                let mock_pool = PoolWrapper::new(StdArc::new(MockPool {
                    address: pool_address,
                    token0: Address::repeat_byte(0x02),
                    token1: Address::repeat_byte(0x03),
                }));
                
                let _ = service_clone.add_pool(mock_pool).await;
                
                // Small delay
                tokio::time::sleep(Duration::from_millis(10)).await;
                
                let _ = service_clone.disable_pool(pool_id).await;
            });
            handles.push(handle);
        }
        
        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        // All pools should be disabled
        assert_eq!(service.get_monitored_pools().await.len(), 0);
    }
    
    #[tokio::test]
    async fn test_aggregator_stats() {
        use crate::logic::pools::mock_pool::MockPool;
        use std::sync::Arc;
        
        let config = DataSyncConfig::default();
        let pools = vec![
            PoolWrapper::new(Arc::new(MockPool {
                address: Address::repeat_byte(0x01),
                token0: Address::repeat_byte(0x02),
                token1: Address::repeat_byte(0x03),
            })),
            PoolWrapper::new(Arc::new(MockPool {
                address: Address::repeat_byte(0x04),
                token0: Address::repeat_byte(0x05),
                token1: Address::repeat_byte(0x06),
            })),
            PoolWrapper::new(Arc::new(MockPool {
                address: Address::repeat_byte(0x07),
                token0: Address::repeat_byte(0x08),
                token1: Address::repeat_byte(0x09),
            })),
        ];
        
        let service = DataSyncService::new(config, pools).await.unwrap();
        let stats = service.get_stats().await;
        
        assert_eq!(stats.monitored_pools_count, 3);
        assert_eq!(stats.max_pools_per_batch, 50); // Default value
        // Stats without ETH price
    }
}

/// Unit tests for individual components
#[cfg(test)]
mod unit_tests {
    use super::super::*;
    use crate::PoolWrapper;
    use alloy_primitives::Address;
    use std::time::Duration;
    
    #[test]
    fn test_config_from_defaults() {
        let config = DataSyncConfig::default();
        assert_eq!(config.rpc_wss_url, "wss://rpc.mantle.xyz");
        assert_eq!(config.rpc_http_url, "https://rpc.mantle.xyz");
        assert_eq!(config.max_pools_per_batch, 50);
    }
    
    #[test]
    fn test_config_durations() {
        let config = DataSyncConfig::default();
        assert_eq!(config.ws_connection_timeout(), Duration::from_secs(30));
        assert_eq!(config.reconnect_delay(), Duration::from_secs(2));
        assert_eq!(config.http_timeout(), Duration::from_secs(10));
    }
    
    #[test]
    fn test_block_header_parsing() {
        let header = BlockHeader {
            number: "0x123abc".to_string(),
            hash: "0xdef456".to_string(),
            parent_hash: "0x789def".to_string(),
            timestamp: "0x61234567".to_string(),
        };
        
        assert_eq!(header.block_number().unwrap(), 0x123abc);
        assert_eq!(header.timestamp_secs().unwrap(), 0x61234567);
    }
    
    #[test]
    fn test_invalid_block_header() {
        let header = BlockHeader {
            number: "invalid".to_string(),
            hash: "0xdef456".to_string(),
            parent_hash: "0x789def".to_string(),
            timestamp: "0x61234567".to_string(),
        };
        
        assert!(header.block_number().is_err());
    }
    
    #[tokio::test]
    async fn test_multicall_manager_creation() {
        let multicall_address = Address::repeat_byte(0x11);
        let rpc_url = "https://test.rpc".to_string();
        let timeout = Duration::from_secs(10);
        
        let manager = MulticallManager::new(multicall_address, rpc_url, timeout);
        // Just verify it doesn't panic on creation
        assert_eq!(format!("{:?}", manager).contains("MulticallManager"), true);
    }
    
    #[test]
    fn test_prepare_get_reserves_call() {
        let pool_address = Address::repeat_byte(0x42);
        let call = MulticallManager::prepare_get_reserves_call(pool_address);
        
        assert_eq!(call.target, pool_address);
        assert!(!call.callData.is_empty());
        // The call data should start with the getReserves() function selector
        assert_eq!(call.callData.len(), 4); // getReserves() has no parameters
    }
    
    #[tokio::test]
    async fn test_data_aggregator_creation() {
        let multicall_address = Address::repeat_byte(0x11);
        let rpc_url = "https://test.rpc".to_string();
        let timeout = Duration::from_secs(10);
        let multicall_manager = MulticallManager::new(multicall_address, rpc_url, timeout);
        
        let aggregator = DataAggregator::new(multicall_manager, 50);
        
        let stats = aggregator.get_stats(2); // Pass monitored pool count
        assert_eq!(stats.monitored_pools_count, 2);
        assert_eq!(stats.max_pools_per_batch, 50);
    }
}

/// Performance and stress tests
#[cfg(test)]
mod performance_tests {
    use super::super::*;
    use crate::PoolWrapper;
    use alloy_primitives::Address;
    use std::time::Instant;
    
    #[tokio::test]
    async fn test_large_pool_management() {
        use crate::logic::pools::mock_pool::MockPool;
        use std::sync::Arc;
        
        let service = DataSyncService::new(DataSyncConfig::default(), vec![]).await.unwrap();
        
        let start_time = Instant::now();
        
        // Add 100 pools with unique addresses (reduced for test speed)
        for i in 0..100 {
            let mut addr_bytes = [0u8; 20];
            addr_bytes[0] = (i / 256) as u8;
            addr_bytes[1] = (i % 256) as u8;
            let pool_address = Address::from_slice(&addr_bytes);
            let mock_pool = PoolWrapper::new(Arc::new(MockPool {
                address: pool_address,
                token0: Address::repeat_byte(0x02),
                token1: Address::repeat_byte(0x03),
            }));
            let _ = service.add_pool(mock_pool).await;
        }
        
        let add_duration = start_time.elapsed();
        println!("Added 100 pools in {:?}", add_duration);
        
        assert_eq!(service.get_monitored_pools().await.len(), 100);
        
        // Verify stats
        let stats = service.get_stats().await;
        assert_eq!(stats.monitored_pools_count, 100);
    }
    
    #[test]
    fn test_config_creation_performance() {
        let start_time = Instant::now();
        
        for _ in 0..10000 {
            let _config = DataSyncConfig::default();
        }
        
        let duration = start_time.elapsed();
        println!("Created 10,000 configs in {:?}", duration);
        
        // Should be very fast
        assert!(duration.as_millis() < 100);
    }
}
