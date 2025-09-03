/// Logic Layer - Arbitrage Engine
/// 
/// This layer is responsible for:
/// - Core arbitrage logic and path finding algorithms
/// - Profit calculation and optimization
/// - Graph-based token and pool relationships
/// - Real-time opportunity detection
/// 
/// Implements the core arbitrage logic based on pre-computed paths and parallel profit calculation
/// as described in the design document (方案B).

pub mod arbitrage_engine;
pub mod pathfinder;
pub mod profit_calculator;
pub mod types;
pub mod graph;
pub mod pools;

// Re-export key components from the logic layer
pub use arbitrage_engine::{ArbitrageEngine, ArbitrageEngineBuilder};
pub use pathfinder::Pathfinder;
pub use profit_calculator::ProfitCalculator;
pub use types::{ArbitrageOpportunity, MarketSnapshot, ProfitCalculationResult};
pub use graph::{
    SwapPath, SwapPathHash, SwapPathsContainer, add_swap_path, remove_pool,
    find_all_paths_spfa, SPFAPathBuilder, TokenGraph
};
pub use pools::{
    AbiSwapEncoder, CalculationError, Pool, PoolClass, PoolProtocol, PoolWrapper, 
    PreswapRequirement, get_protocol_by_factory, PoolId, MockPool
};
