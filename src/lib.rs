// Three-Layer Architecture
pub mod data_sync;   // Data Layer: Market data polling, pool management
pub mod logic;      // Logic Layer: Arbitrage algorithms, path finding
pub mod execution;  // Execution Layer: Transaction execution, pool protocols

// Common utilities and types
pub mod utils;

#[cfg(test)]
pub mod benchmarks;

// Re-export key components from each layer
pub use data_sync::{Market, MarketWithoutLock, MarketConfigSection};
pub use logic::{
    ArbitrageEngine, ArbitrageEngineBuilder, Pathfinder, ProfitCalculator, 
    ArbitrageOpportunity, MarketSnapshot, ProfitCalculationResult,
    SwapPath, SwapPathHash, SwapPathsContainer, add_swap_path, remove_pool,
    find_all_paths_spfa, SPFAPathBuilder, TokenGraph,
    AbiSwapEncoder, CalculationError, Pool, PoolClass, PoolProtocol, PoolWrapper, 
    PreswapRequirement, get_protocol_by_factory, PoolId, MockPool
};
pub use execution::{TransactionExecutor};
pub use utils::{Token, TokenWrapper, StateCache, CachedStateProvider, CacheStats, CacheManager};
