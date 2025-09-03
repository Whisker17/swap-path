/// Data Synchronization Layer
/// 
/// This layer implements the high-performance data synchronization architecture
/// as described in the design document. It provides:
/// 
/// - WebSocket-based blockchain event subscription (newHeads)
/// - Multicall-based batch pool data querying
/// - Real-time market data aggregation
/// - Atomic data delivery to the logic layer
/// 
/// Architecture follows the proactive aggregated polling approach (方案A) for
/// optimal performance in high-frequency arbitrage scenarios.

// Core data sync components
pub mod config;
pub mod websocket;
pub mod multicall;
pub mod aggregator;
pub mod service;

// Legacy market components (to be refactored)
pub mod markets;

// Tests
#[cfg(test)]
mod tests;

// Re-export main components for easy usage
pub use config::DataSyncConfig;
pub use service::{DataSyncService, DataSyncServiceBuilder};
pub use websocket::{WebSocketManager, BlockHeader};
pub use multicall::MulticallManager;
pub use aggregator::{DataAggregator, AggregatorStats};

// Legacy re-exports (maintain compatibility)
pub use markets::{Market, MarketWithoutLock, MarketConfigSection};
