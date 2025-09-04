use eyre::{Result, eyre};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{timeout, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn, error, debug};
use url::Url;

/// Result of WebSocket connection attempt
#[derive(Debug)]
enum ConnectionResult {
    /// Connection ended normally due to shutdown signal
    NormalShutdown,
    /// Connection dropped by server or network error - should retry
    ConnectionLost(String),
}

/// Block header information from newHeads subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    #[serde(rename = "number")]
    pub number: String,
    #[serde(rename = "hash")]
    pub hash: String,
    #[serde(rename = "parentHash")]
    pub parent_hash: String,
    #[serde(rename = "timestamp")]
    pub timestamp: String,
}

impl BlockHeader {
    /// Parse block number from hex string
    pub fn block_number(&self) -> Result<u64> {
        let num_str = self.number.trim_start_matches("0x");
        u64::from_str_radix(num_str, 16)
            .map_err(|e| eyre!("Invalid block number format: {}", e))
    }
    
    /// Parse timestamp from hex string
    pub fn timestamp_secs(&self) -> Result<u64> {
        let ts_str = self.timestamp.trim_start_matches("0x");
        u64::from_str_radix(ts_str, 16)
            .map_err(|e| eyre!("Invalid timestamp format: {}", e))
    }
}

/// WebSocket manager for subscribing to blockchain events
pub struct WebSocketManager {
    rpc_url: String,
    connection_timeout: Duration,
    max_reconnect_attempts: u32,
    reconnect_delay: Duration,
}

impl WebSocketManager {
    pub fn new(
        rpc_url: String,
        connection_timeout: Duration,
        max_reconnect_attempts: u32,
        reconnect_delay: Duration,
    ) -> Self {
        Self {
            rpc_url,
            connection_timeout,
            max_reconnect_attempts,
            reconnect_delay,
        }
    }
    
    /// Start subscribing to newHeads events
    /// Returns a receiver for block headers and a shutdown sender
    pub async fn subscribe_new_heads(&self) -> Result<(mpsc::Receiver<BlockHeader>, mpsc::Sender<()>)> {
        let (block_tx, block_rx) = mpsc::channel(100);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        
        let rpc_url = self.rpc_url.clone();
        let connection_timeout = self.connection_timeout;
        let max_reconnect_attempts = self.max_reconnect_attempts;
        let reconnect_delay = self.reconnect_delay;
        
        // Spawn WebSocket management task
        tokio::spawn(async move {
            let mut reconnect_count = 0;
            let mut first_connection = true;
            
            loop {
                // If this is not the first connection attempt, it's a reconnection
                if !first_connection {
                    reconnect_count += 1;
                    
                    if reconnect_count > max_reconnect_attempts {
                        error!("Max reconnection attempts ({}) reached, giving up", max_reconnect_attempts);
                        break;
                    }
                    
                    info!("Attempting reconnection #{}/{} in {:?}", 
                          reconnect_count, max_reconnect_attempts, reconnect_delay);
                    sleep(reconnect_delay).await;
                }
                first_connection = false;
                
                match Self::connect_and_subscribe(
                    &rpc_url,
                    connection_timeout,
                    &block_tx,
                    &mut shutdown_rx,
                ).await {
                    ConnectionResult::NormalShutdown => {
                        info!("WebSocket subscription ended normally");
                        break;
                    }
                    ConnectionResult::ConnectionLost(reason) => {
                        warn!("WebSocket connection lost: {}", reason);
                        // Continue to next iteration for reconnection attempt
                    }
                }
                
                // If we reach here and the connection was successful for some time,
                // reset the reconnect count to avoid accumulating historical failures
                if reconnect_count > 0 {
                    info!("Connection was active, resetting reconnect count");
                    reconnect_count = 0;
                }
            }
        });
        
        Ok((block_rx, shutdown_tx))
    }
    
    /// Connect to WebSocket and handle subscription
    async fn connect_and_subscribe(
        rpc_url: &str,
        connection_timeout: Duration,
        block_tx: &mpsc::Sender<BlockHeader>,
        shutdown_rx: &mut mpsc::Receiver<()>,
    ) -> ConnectionResult {
        // Parse and connect to WebSocket URL
        let url = match Url::parse(rpc_url) {
            Ok(url) => url,
            Err(e) => return ConnectionResult::ConnectionLost(format!("Invalid URL: {}", e)),
        };
        info!("Connecting to WebSocket: {}", url);
        
        let (ws_stream, _) = match timeout(connection_timeout, connect_async(url.as_str())).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => return ConnectionResult::ConnectionLost(format!("WebSocket connection failed: {}", e)),
            Err(_) => return ConnectionResult::ConnectionLost("WebSocket connection timeout".to_string()),
        };
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        // Subscribe to newHeads
        let subscribe_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_subscribe",
            "params": ["newHeads"]
        });
        
        if let Err(e) = ws_sender.send(Message::Text(subscribe_request.to_string().into())).await {
            return ConnectionResult::ConnectionLost(format!("Failed to send subscription request: {}", e));
        }
        info!("Sent newHeads subscription request");
        
        // Wait for subscription confirmation
        let subscription_id = match ws_receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<Value>(text.as_str()) {
                    Ok(response) => {
                        if let Some(result) = response.get("result") {
                            info!("Subscription confirmed with ID: {}", result);
                            match result.as_str() {
                                Some(id) => id.to_string(),
                                None => return ConnectionResult::ConnectionLost("Invalid subscription ID format".to_string()),
                            }
                        } else if let Some(error) = response.get("error") {
                            return ConnectionResult::ConnectionLost(format!("Subscription error: {}", error));
                        } else {
                            return ConnectionResult::ConnectionLost(format!("Unexpected subscription response: {}", text));
                        }
                    }
                    Err(e) => return ConnectionResult::ConnectionLost(format!("Failed to parse subscription response: {}", e)),
                }
            }
            Some(Ok(msg)) => {
                return ConnectionResult::ConnectionLost(format!("Unexpected message type during subscription: {:?}", msg));
            }
            Some(Err(e)) => {
                return ConnectionResult::ConnectionLost(format!("WebSocket error during subscription: {}", e));
            }
            None => {
                return ConnectionResult::ConnectionLost("WebSocket closed during subscription".to_string());
            }
        };
        
        info!("Successfully subscribed to newHeads with ID: {}", subscription_id);
        
        // Connection established successfully - we will handle reconnection counting in the main loop
        
        // Main event loop
        loop {
            tokio::select! {
                // Handle incoming WebSocket messages
                ws_msg = ws_receiver.next() => {
                    match ws_msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = Self::handle_message(text.as_str(), block_tx).await {
                                warn!("Failed to handle WebSocket message: {}", e);
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("WebSocket closed by server");
                            return ConnectionResult::ConnectionLost("Server closed the connection".to_string());
                        }
                        Some(Ok(Message::Ping(data))) => {
                            // Respond to ping with pong
                            if let Err(e) = ws_sender.send(Message::Pong(data)).await {
                                error!("Failed to send pong: {}", e);
                                return ConnectionResult::ConnectionLost(format!("Failed to send pong: {}", e));
                            }
                        }
                        Some(Ok(_)) => {
                            // Ignore other message types
                        }
                        Some(Err(e)) => {
                            error!("WebSocket error: {}", e);
                            return ConnectionResult::ConnectionLost(format!("WebSocket error: {}", e));
                        }
                        None => {
                            info!("WebSocket stream ended");
                            return ConnectionResult::ConnectionLost("WebSocket stream ended unexpectedly".to_string());
                        }
                    }
                }
                
                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown signal");
                    return ConnectionResult::NormalShutdown;
                }
            }
        }
    }
    
    /// Handle incoming WebSocket message
    async fn handle_message(text: &str, block_tx: &mpsc::Sender<BlockHeader>) -> Result<()> {
        let message: Value = serde_json::from_str(text)?;
        
        // Check if this is a subscription notification
        if let Some(params) = message.get("params") {
            if let Some(result) = params.get("result") {
                // Parse block header
                let block_header: BlockHeader = serde_json::from_value(result.clone())?;
                
                debug!("Received new block: {}", block_header.number);
                
                // Send to channel (non-blocking)
                if let Err(e) = block_tx.try_send(block_header) {
                    match e {
                        mpsc::error::TrySendError::Full(_) => {
                            warn!("Block header channel is full, dropping block");
                        }
                        mpsc::error::TrySendError::Closed(_) => {
                            return Err(eyre!("Block header channel is closed"));
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_block_header_parsing() {
        let header = BlockHeader {
            number: "0x1a2b3c".to_string(),
            hash: "0xabcd".to_string(),
            parent_hash: "0x1234".to_string(),
            timestamp: "0x61234567".to_string(),
        };
        
        assert_eq!(header.block_number().unwrap(), 0x1a2b3c);
        assert_eq!(header.timestamp_secs().unwrap(), 0x61234567);
    }
    
    #[test]
    fn test_websocket_manager_creation() {
        let manager = WebSocketManager::new(
            "wss://rpc.mantle.xyz".to_string(),
            Duration::from_secs(30),
            5,
            Duration::from_secs(2),
        );
        
        assert_eq!(manager.rpc_url, "wss://rpc.mantle.xyz");
        assert_eq!(manager.connection_timeout, Duration::from_secs(30));
        assert_eq!(manager.max_reconnect_attempts, 5);
        assert_eq!(manager.reconnect_delay, Duration::from_secs(2));
    }
}
