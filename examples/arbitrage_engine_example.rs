use swap_path::{
    ArbitrageEngine, ArbitrageEngineBuilder, ArbitrageOpportunity, Market, MarketSnapshot, Token, 
    MockPool, PoolWrapper, PoolId
};
use swap_path::utils::constants::WMNT;
use alloy_primitives::{Address, U256};
use std::sync::Arc;
use tokio::sync::mpsc;
use eyre::Result;

/// Example demonstrating the new Logic Layer architecture
/// 
/// This example shows how to:
/// 1. Set up a token graph with pools
/// 2. Initialize the ArbitrageEngine with path precomputation
/// 3. Process market data snapshots to find arbitrage opportunities
/// 4. Use the real-time processing loop
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();
    
    println!("🚀 套利引擎示例 - 基于方案B架构");
    println!("================================");
    
    // Step 1: Create a sample market with pools
    println!("\n📊 步骤1: 创建示例市场");
    let mut market = create_sample_market()?;
    
    // Step 2: Initialize the ArbitrageEngine
    println!("\n🔧 步骤2: 初始化套利引擎");
    let mut engine = ArbitrageEngineBuilder::new()
        .with_min_profit_threshold(1.0) // $1 minimum profit for demo
        .with_max_hops(4)
        .with_parallel_calculation(true)
        .build();
    
    // Initialize with precomputed paths
    engine.initialize(&market.token_graph)?;
    let stats = engine.get_statistics();
    println!("✅ 引擎初始化完成:");
    println!("   - 预计算路径数量: {}", stats.precomputed_paths_count);
    println!("   - 最大跳数: {}", stats.max_hops);
    println!("   - 最小利润阈值: ${:.2}", stats.min_profit_threshold_usd);
    
    // Step 3: Process a single market snapshot
    println!("\n📈 步骤3: 处理单个市场快照");
    let market_snapshot = create_sample_market_snapshot();
    let opportunities = engine.process_market_snapshot(&market_snapshot)?;
    
    println!("发现 {} 个套利机会:", opportunities.len());
    for (i, opportunity) in opportunities.iter().enumerate() {
        print_opportunity(i + 1, opportunity);
    }
    
    // Step 4: Demonstrate real-time processing (brief demo)
    println!("\n🔄 步骤4: 演示实时处理循环");
    demonstrate_realtime_processing(engine).await?;
    
    println!("\n✨ 示例完成！");
    Ok(())
}

/// Create a sample market with some pools for demonstration
fn create_sample_market() -> Result<Market> {
    let mut market = Market::default();
    
    // Add WMNT token (required for arbitrage paths)
    let wmnt_token = Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18));
    market.add_token(wmnt_token);
    
    // Add some other tokens
    let token1_addr = Address::repeat_byte(1);
    let token2_addr = Address::repeat_byte(2);
    let token3_addr = Address::repeat_byte(3);
    
    market.add_token(Token::new_with_data(token1_addr, Some("TOKEN1".to_string()), None, Some(18)));
    market.add_token(Token::new_with_data(token2_addr, Some("TOKEN2".to_string()), None, Some(18)));
    market.add_token(Token::new_with_data(token3_addr, Some("TOKEN3".to_string()), None, Some(18)));
    
    // Create pools to form arbitrage cycles
    // Cycle 1: WMNT -> TOKEN1 -> TOKEN2 -> WMNT (3-hop)
    let pool1 = PoolWrapper::new(Arc::new(MockPool::new(
        WMNT, token1_addr, Address::repeat_byte(10)
    )));
    let pool2 = PoolWrapper::new(Arc::new(MockPool::new(
        token1_addr, token2_addr, Address::repeat_byte(11)
    )));
    let pool3 = PoolWrapper::new(Arc::new(MockPool::new(
        token2_addr, WMNT, Address::repeat_byte(12)
    )));
    
    // Cycle 2: WMNT -> TOKEN1 -> TOKEN3 -> WMNT (3-hop)
    let pool4 = PoolWrapper::new(Arc::new(MockPool::new(
        token1_addr, token3_addr, Address::repeat_byte(13)
    )));
    let pool5 = PoolWrapper::new(Arc::new(MockPool::new(
        token3_addr, WMNT, Address::repeat_byte(14)
    )));
    
    // Add pools to market
    market.add_pool(pool1);
    market.add_pool(pool2);
    market.add_pool(pool3);
    market.add_pool(pool4);
    market.add_pool(pool5);
    
    println!("创建了包含 {} 个池的示例市场", market.pools().len());
    Ok(market)
}

/// Create a sample market snapshot with some price imbalances
fn create_sample_market_snapshot() -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(123456, 2500.0); // Block 123456, ETH at $2500
    
    // Set reserves for all pools (some with price imbalances for arbitrage)
    let pool_reserves = [
        (Address::repeat_byte(10), ("2000000000000000000000", "1800000000000000000000")), // Pool 1: slight imbalance
        (Address::repeat_byte(11), ("1000000000000000000000", "1100000000000000000000")), // Pool 2: imbalance
        (Address::repeat_byte(12), ("1500000000000000000000", "1600000000000000000000")), // Pool 3: imbalance
        (Address::repeat_byte(13), ("1200000000000000000000", "1000000000000000000000")), // Pool 4: imbalance
        (Address::repeat_byte(14), ("1800000000000000000000", "2000000000000000000000")), // Pool 5: imbalance
    ];
    
    for (pool_addr, (reserve0_str, reserve1_str)) in pool_reserves {
        let reserve0 = U256::from_str_radix(reserve0_str, 10).unwrap();
        let reserve1 = U256::from_str_radix(reserve1_str, 10).unwrap();
        snapshot.set_pool_reserves(PoolId::Address(pool_addr), reserve0, reserve1);
    }
    
    println!("创建了包含 {} 个池储备的市场快照", snapshot.pool_reserves.len());
    snapshot
}

/// Print details about an arbitrage opportunity
fn print_opportunity(index: usize, opportunity: &ArbitrageOpportunity) {
    println!("  {}. 套利机会详情:", index);
    println!("     路径长度: {} 跳", opportunity.path.len());
    println!("     最佳输入金额: {} Wei", opportunity.optimal_input_amount);
    println!("     预期输出金额: {} Wei", opportunity.expected_output_amount);
    println!("     总利润: ${:.4}", opportunity.gross_profit_usd);
    println!("     Gas成本: ${:.4}", opportunity.gas_cost_usd);
    println!("     净利润: ${:.4}", opportunity.net_profit_usd);
    println!("     利润率: {:.2}%", opportunity.profit_margin_percent);
}

/// Demonstrate real-time processing with a brief simulation
async fn demonstrate_realtime_processing(mut engine: ArbitrageEngine) -> Result<()> {
    // Create channels for market data and opportunities
    let (market_data_tx, market_data_rx) = mpsc::channel(100);
    let (opportunity_tx, mut opportunity_rx) = mpsc::channel(100);
    
    // Set up the engine with the receiver
    engine.set_market_data_receiver(market_data_rx);
    
    // Start the real-time processing in a background task
    let engine_handle = tokio::spawn(async move {
        engine.start_real_time_processing(opportunity_tx).await
    });
    
    // Simulate sending a few market data updates
    let snapshots = [
        create_sample_market_snapshot(),
        create_market_snapshot_with_different_prices(),
    ];
    
    for (i, snapshot) in snapshots.into_iter().enumerate() {
        println!("📡 发送市场快照 {} (区块 {})", i + 1, snapshot.block_number);
        market_data_tx.send(snapshot).await?;
        
        // Wait a bit for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    // Close the sender to stop the engine
    drop(market_data_tx);
    
    // Collect any opportunities that were found
    let mut total_opportunities = 0;
    while let Ok(opportunities) = opportunity_rx.try_recv() {
        total_opportunities += opportunities.len();
        println!("🎯 实时发现 {} 个套利机会", opportunities.len());
    }
    
    // Wait for the engine to finish
    let _ = engine_handle.await?;
    
    println!("✅ 实时处理演示完成，总共发现 {} 个机会", total_opportunities);
    Ok(())
}

/// Create a market snapshot with different prices for comparison
fn create_market_snapshot_with_different_prices() -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(123457, 2600.0); // Different block and ETH price
    
    // Different reserves that might create different arbitrage opportunities
    let pool_reserves = [
        (Address::repeat_byte(10), ("2200000000000000000000", "1700000000000000000000")), 
        (Address::repeat_byte(11), ("900000000000000000000", "1200000000000000000000")), 
        (Address::repeat_byte(12), ("1400000000000000000000", "1700000000000000000000")), 
        (Address::repeat_byte(13), ("1300000000000000000000", "900000000000000000000")), 
        (Address::repeat_byte(14), ("1700000000000000000000", "2100000000000000000000")), 
    ];
    
    for (pool_addr, (reserve0_str, reserve1_str)) in pool_reserves {
        let reserve0 = U256::from_str_radix(reserve0_str, 10).unwrap();
        let reserve1 = U256::from_str_radix(reserve1_str, 10).unwrap();
        snapshot.set_pool_reserves(PoolId::Address(pool_addr), reserve0, reserve1);
    }
    
    snapshot
}
