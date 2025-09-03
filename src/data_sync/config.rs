use serde::{Deserialize, Serialize};
use url::Url;
use std::time::Duration;

/// Configuration for the data synchronization layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSyncConfig {
    /// WebSocket RPC URL for blockchain connection
    pub rpc_wss_url: String,
    /// HTTP RPC URL for fallback and Multicall requests
    pub rpc_http_url: String,
    /// Multicall contract address on Mantle
    pub multicall_address: String,
    /// Maximum number of pools to query in a single Multicall batch
    pub max_pools_per_batch: usize,
    /// WebSocket connection timeout in seconds
    pub ws_connection_timeout_secs: u64,
    /// Maximum number of reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Delay between reconnection attempts in seconds
    pub reconnect_delay_secs: u64,
    /// Timeout for HTTP requests in seconds
    pub http_timeout_secs: u64,
    /// Buffer size for the data channel to logic layer
    pub channel_buffer_size: usize,
}

impl Default for DataSyncConfig {
    fn default() -> Self {
        Self {
            rpc_wss_url: "wss://rpc.mantle.xyz".to_string(),
            rpc_http_url: "https://rpc.mantle.xyz".to_string(),
            // Standard Multicall3 address (deployed on most chains)
            multicall_address: "0xcA11bde05977b3631167028862bE2a173976CA11".to_string(),
            max_pools_per_batch: 50,
            ws_connection_timeout_secs: 30,
            max_reconnect_attempts: 5,
            reconnect_delay_secs: 2,
            http_timeout_secs: 10,
            channel_buffer_size: 100,
        }
    }
}

impl DataSyncConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> eyre::Result<Self> {
        let mut config = Self::default();
        
        if let Ok(rpc_wss_url) = std::env::var("RPC_WSS_URL") {
            // Validate WebSocket URL
            let _url = Url::parse(&rpc_wss_url)
                .map_err(|e| eyre::eyre!("Invalid RPC_WSS_URL: {}", e))?;
            config.rpc_wss_url = rpc_wss_url;
        }
        
        if let Ok(rpc_http_url) = std::env::var("RPC_HTTP_URL") {
            // Validate HTTP URL
            let _url = Url::parse(&rpc_http_url)
                .map_err(|e| eyre::eyre!("Invalid RPC_HTTP_URL: {}", e))?;
            config.rpc_http_url = rpc_http_url;
        }
        
        if let Ok(multicall_address) = std::env::var("MULTICALL_ADDRESS") {
            config.multicall_address = multicall_address;
        }
        
        if let Ok(max_pools_str) = std::env::var("MAX_POOLS_PER_BATCH") {
            config.max_pools_per_batch = max_pools_str.parse()
                .map_err(|e| eyre::eyre!("Invalid MAX_POOLS_PER_BATCH: {}", e))?;
        }
        
        if let Ok(timeout_str) = std::env::var("WS_CONNECTION_TIMEOUT_SECS") {
            config.ws_connection_timeout_secs = timeout_str.parse()
                .map_err(|e| eyre::eyre!("Invalid WS_CONNECTION_TIMEOUT_SECS: {}", e))?;
        }
        
        if let Ok(max_attempts_str) = std::env::var("MAX_RECONNECT_ATTEMPTS") {
            config.max_reconnect_attempts = max_attempts_str.parse()
                .map_err(|e| eyre::eyre!("Invalid MAX_RECONNECT_ATTEMPTS: {}", e))?;
        }
        
        if let Ok(delay_str) = std::env::var("RECONNECT_DELAY_SECS") {
            config.reconnect_delay_secs = delay_str.parse()
                .map_err(|e| eyre::eyre!("Invalid RECONNECT_DELAY_SECS: {}", e))?;
        }
        
        if let Ok(timeout_str) = std::env::var("HTTP_TIMEOUT_SECS") {
            config.http_timeout_secs = timeout_str.parse()
                .map_err(|e| eyre::eyre!("Invalid HTTP_TIMEOUT_SECS: {}", e))?;
        }
        
        if let Ok(buffer_size_str) = std::env::var("CHANNEL_BUFFER_SIZE") {
            config.channel_buffer_size = buffer_size_str.parse()
                .map_err(|e| eyre::eyre!("Invalid CHANNEL_BUFFER_SIZE: {}", e))?;
        }
        
        Ok(config)
    }
    
    pub fn ws_connection_timeout(&self) -> Duration {
        Duration::from_secs(self.ws_connection_timeout_secs)
    }
    
    pub fn reconnect_delay(&self) -> Duration {
        Duration::from_secs(self.reconnect_delay_secs)
    }
    
    pub fn http_timeout(&self) -> Duration {
        Duration::from_secs(self.http_timeout_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = DataSyncConfig::default();
        assert_eq!(config.rpc_wss_url, "wss://rpc.mantle.xyz");
        assert_eq!(config.rpc_http_url, "https://rpc.mantle.xyz");
        assert_eq!(config.max_pools_per_batch, 50);
        assert_eq!(config.ws_connection_timeout_secs, 30);
    }
    
    #[test]
    fn test_durations() {
        let config = DataSyncConfig::default();
        assert_eq!(config.ws_connection_timeout(), Duration::from_secs(30));
        assert_eq!(config.reconnect_delay(), Duration::from_secs(2));
        assert_eq!(config.http_timeout(), Duration::from_secs(10));
    }
}
