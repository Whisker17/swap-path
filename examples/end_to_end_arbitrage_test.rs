/// 端到端套利监控和推荐测试
/// 
/// 这个测试演示了完整的套利发现流程：
/// 1. 数据层：监控链上DEX pools情况
/// 2. 逻辑层：发现套利机会并给出推荐方案
/// 3. 执行建议：包括套利路径、输入数额等详细信息

use swap_path::data_sync::{DataSyncConfig, DataSyncServiceBuilder};
use swap_path::logic::{ArbitrageEngine, ArbitrageOpportunity};
use swap_path::logic::types::{ArbitrageConfig, MarketSnapshot};
use swap_path::logic::pools::{PoolId, MockPool};
use swap_path::{PoolWrapper, Token};
use swap_path::data_sync::markets::{Market, MarketConfigSection};
use alloy_primitives::{Address, U256};
use eyre::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error, debug};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    info!("🚀 开始端到端套利监控测试");
    
    // 第一阶段：设置测试环境
    let (market, initial_pools) = setup_test_market().await?;
    info!("✅ 测试市场设置完成");
    
    // 第二阶段：启动数据层
    let mut data_service = setup_data_layer(initial_pools).await?;
    info!("✅ 数据层启动完成");
    
    // 第三阶段：初始化逻辑层
    let mut arbitrage_engine = setup_arbitrage_engine(&market).await?;
    info!("✅ 套利引擎初始化完成");
    
    // 第四阶段：端到端测试
    let test_result = run_end_to_end_test(&mut data_service, &mut arbitrage_engine).await?;
    info!("✅ 端到端测试完成");
    
    // 第五阶段：展示结果
    display_test_results(test_result).await?;
    
    info!("🎉 所有测试完成！");
    Ok(())
}

/// 设置测试用的市场环境
async fn setup_test_market() -> Result<(Market, Vec<PoolWrapper>)> {
    info!("🏗️ 设置测试市场环境...");
    
    // 使用真实的WMNT地址（来自constants）
    let wmnt = swap_path::utils::constants::WMNT;
    let usdc = Address::from_slice(&[0x12; 20]); // USDC  
    let usdt = Address::from_slice(&[0x34; 20]); // USDT
    let btc = Address::from_slice(&[0x56; 20]);  // BTC
    
    info!("创建的测试代币:");
    info!("  WMNT: {:?}", wmnt);
    info!("  USDC: {:?}", usdc);
    info!("  USDT: {:?}", usdt);
    info!("  BTC:  {:?}", btc);
    
    // 创建市场配置，允许更多跳数
    let market_config = MarketConfigSection::default().with_max_hops(4);
    let mut market = Market::new(market_config);
    
    // WMNT已经默认添加到市场中，但我们需要添加其他代币
    market.add_token(Token::new_with_data(usdc, Some("USDC".to_string()), None, Some(6)));
    market.add_token(Token::new_with_data(usdt, Some("USDT".to_string()), None, Some(6)));
    market.add_token(Token::new_with_data(btc, Some("BTC".to_string()), None, Some(8)));
    
    // 创建测试池子（形成套利环路）
    let pools = vec![
        // WMNT/USDC 池子 - 第一条边
        create_test_pool(
            Address::from_slice(&[0x01; 20]),
            wmnt, usdc,
            U256::from(1000000) * U256::from(10u64.pow(18)), // 1M WMNT
            U256::from(2000000) * U256::from(10u64.pow(6)),  // 2M USDC (price: $2)
        ),
        
        // USDC/USDT 池子 - 第二条边
        create_test_pool(
            Address::from_slice(&[0x02; 20]),
            usdc, usdt,
            U256::from(1000000) * U256::from(10u64.pow(6)),  // 1M USDC
            U256::from(1000000) * U256::from(10u64.pow(6)),  // 1M USDT (1:1)
        ),
        
        // USDT/WMNT 池子 - 完成3跳循环
        create_test_pool(
            Address::from_slice(&[0x03; 20]),
            usdt, wmnt,
            U256::from(1000000) * U256::from(10u64.pow(6)),  // 1M USDT
            U256::from(500000) * U256::from(10u64.pow(18)),  // 500K WMNT (price: $2)
        ),
        
        // 添加另一个3跳循环路径: WMNT -> BTC -> USDC -> WMNT
        create_test_pool(
            Address::from_slice(&[0x04; 20]),
            wmnt, btc,
            U256::from(3000000) * U256::from(10u64.pow(18)), // 3M WMNT 
            U256::from(100) * U256::from(10u64.pow(8)),      // 100 BTC (BTC price: $60K)
        ),
        
        create_test_pool(
            Address::from_slice(&[0x05; 20]),
            btc, usdc,
            U256::from(50) * U256::from(10u64.pow(8)),       // 50 BTC
            U256::from(3000000) * U256::from(10u64.pow(6)),  // 3M USDC
        ),
        
        // 直接的 USDC -> WMNT 池子，形成另一个循环
        create_test_pool(
            Address::from_slice(&[0x06; 20]),
            usdc, wmnt,
            U256::from(1500000) * U256::from(10u64.pow(6)),  // 1.5M USDC
            U256::from(750000) * U256::from(10u64.pow(18)),  // 750K WMNT (price: $2)
        ),
    ];
    
    // 添加池子到市场，这会自动构建TokenGraph
    for pool in &pools {
        market.add_pool(pool.clone());
    }
    
    info!("创建了 {} 个测试池子，构成套利路径网络", pools.len());
    
    // 输出TokenGraph的信息进行调试
    info!("TokenGraph调试信息:");
    info!("  代币数量: {}", market.token_graph.tokens.len());
    info!("  池子数量: {}", market.token_graph.pools.len());
    info!("  图节点数量: {}", market.token_graph.graph.node_count());
    info!("  图边数量: {}", market.token_graph.graph.edge_count());
    
    // 检查WMNT是否在图中
    if let Some(wmnt_token) = market.token_graph.tokens.get(&wmnt) {
        info!("  WMNT代币已正确添加: {:?}", wmnt_token.get_address());
    } else {
        warn!("  WMNT代币未找到!");
    }
    
    Ok((market, pools))
}

/// 创建测试池子
fn create_test_pool(
    address: Address,
    token0: Address,
    token1: Address,
    reserve0: U256,
    reserve1: U256,
) -> PoolWrapper {
    let mock_pool = MockPool {
        address,
        token0,
        token1,
    };
    
    debug!("创建池子 {:?}: {} token0 <-> {} token1", 
           address, reserve0, reserve1);
    
    PoolWrapper::new(Arc::new(mock_pool))
}

/// 设置数据层
async fn setup_data_layer(initial_pools: Vec<PoolWrapper>) -> Result<swap_path::data_sync::DataSyncService> {
    info!("🔗 设置数据同步层...");
    
    // 使用测试配置（不连接真实的RPC）
    let config = DataSyncConfig {
        rpc_wss_url: "wss://test.invalid".to_string(), // 测试中不会实际连接
        rpc_http_url: "https://test.invalid".to_string(),
        multicall_address: "0xcA11bde05977b3631167028862bE2a173976CA11".to_string(),
        max_pools_per_batch: 20,
        ws_connection_timeout_secs: 5,
        max_reconnect_attempts: 1,
        reconnect_delay_secs: 1,
        http_timeout_secs: 5,
        channel_buffer_size: 100,
    };
    
    let service = DataSyncServiceBuilder::new()
        .with_config(config)
        .with_pools(initial_pools)
        .build()
        .await?;
    
    info!("数据层配置完成，包含 {} 个初始池子", service.get_monitored_pools().await.len());
    
    Ok(service)
}

/// 设置套利引擎
async fn setup_arbitrage_engine(market: &Market) -> Result<ArbitrageEngine> {
    info!("🧠 初始化套利引擎...");
    
    let config = ArbitrageConfig {
        min_profit_threshold_usd: 1.0,  // $1 最低利润门槛
        max_hops: 4,                    // 最多4跳
        gas_price_gwei: 20,
        gas_per_hop: 150_000,
        max_precomputed_paths: 1000,
        enable_parallel_calculation: true,
    };
    
    let mut engine = ArbitrageEngine::new(config);
    
    // 使用市场的token_graph初始化引擎
    engine.initialize(&market.token_graph)?;
    
    info!("套利引擎初始化完成");
    
    Ok(engine)
}

/// 运行端到端测试
async fn run_end_to_end_test(
    _data_service: &mut swap_path::data_sync::DataSyncService,
    arbitrage_engine: &mut ArbitrageEngine,
) -> Result<Vec<ArbitrageOpportunity>> {
    info!("🔄 开始端到端测试流程...");
    
    // 模拟市场数据更新
    let market_snapshots = create_test_market_snapshots().await?;
    
    let mut all_opportunities = Vec::new();
    
    for (i, snapshot) in market_snapshots.iter().enumerate() {
        info!("📊 处理市场快照 {} (区块 {})", i + 1, snapshot.block_number);
        
        // 使用套利引擎分析市场快照
        match arbitrage_engine.process_market_snapshot(snapshot) {
            Ok(opportunities) => {
                info!("发现 {} 个套利机会", opportunities.len());
                
                for (j, opportunity) in opportunities.iter().enumerate() {
                    info!("  机会 {}: 净利润 ${:.2}, 利润率 {:.2}%", 
                          j + 1, 
                          opportunity.net_profit_usd,
                          opportunity.profit_margin_percent);
                }
                
                all_opportunities.extend(opportunities);
            }
            Err(e) => {
                error!("处理市场快照失败: {}", e);
            }
        }
        
        // 模拟实时处理间隔
        sleep(Duration::from_millis(100)).await;
    }
    
    Ok(all_opportunities)
}

/// 创建测试用的市场快照
async fn create_test_market_snapshots() -> Result<Vec<MarketSnapshot>> {
    info!("📸 创建测试市场快照...");
    
    let snapshots = vec![
        // 快照1: 正常市场状态
        create_market_snapshot(
            12345,
            vec![
                (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from(1000000e18 as u64), U256::from(2000000e6 as u64))),
                (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from(1000000e6 as u64), U256::from(1000000e6 as u64))),
                (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from(1000000e6 as u64), U256::from(500000e18 as u64))),
                (PoolId::Address(Address::from_slice(&[0x04; 20])), (U256::from(100e8 as u64), U256::from(3000000e18 as u64))),
                (PoolId::Address(Address::from_slice(&[0x05; 20])), (U256::from(50e8 as u64), U256::from(3000000e6 as u64))),
            ],
        ),
        
        // 快照2: 价格失衡，创造套利机会
        create_market_snapshot(
            12346,
            vec![
                (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from(1100000e18 as u64), U256::from(1900000e6 as u64))), // WMNT价格下降
                (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from(1000000e6 as u64), U256::from(1000000e6 as u64))),   // 稳定
                (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from(900000e6 as u64), U256::from(550000e18 as u64))),   // WMNT价格仍高
                (PoolId::Address(Address::from_slice(&[0x04; 20])), (U256::from(100e8 as u64), U256::from(3000000e18 as u64))),
                (PoolId::Address(Address::from_slice(&[0x05; 20])), (U256::from(50e8 as u64), U256::from(3000000e6 as u64))),
            ],
        ),
        
        // 快照3: 更大的价格失衡
        create_market_snapshot(
            12347,
            vec![
                (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from(1200000e18 as u64), U256::from(1800000e6 as u64))), // 更大价格差
                (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from(1100000e6 as u64), U256::from(900000e6 as u64))),   // USDC溢价
                (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from(800000e6 as u64), U256::from(600000e18 as u64))),   
                (PoolId::Address(Address::from_slice(&[0x04; 20])), (U256::from(98e8 as u64), U256::from(3100000e18 as u64))),      // BTC价格变化
                (PoolId::Address(Address::from_slice(&[0x05; 20])), (U256::from(52e8 as u64), U256::from(2900000e6 as u64))),
            ],
        ),
    ];
    
    info!("创建了 {} 个测试快照，模拟价格变化", snapshots.len());
    
    Ok(snapshots)
}

/// 创建单个市场快照
fn create_market_snapshot(
    block_number: u64,
    pool_reserves: Vec<(PoolId, (U256, U256))>,
) -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(block_number);
    
    let mut enabled_pools = std::collections::HashSet::new();
    
    for (pool_id, (reserve0, reserve1)) in pool_reserves {
        snapshot.set_pool_reserves(pool_id, reserve0, reserve1);
        enabled_pools.insert(pool_id);
    }
    
    snapshot.set_enabled_pools(enabled_pools);
    snapshot.set_total_pools_count(5);
    
    debug!("创建快照 - 区块: {}, 池子数: {}", block_number, snapshot.pool_reserves.len());
    
    snapshot
}

/// 展示测试结果
async fn display_test_results(opportunities: Vec<ArbitrageOpportunity>) -> Result<()> {
    info!("\n🎯 测试结果分析:");
    info!("{}", "=".repeat(80));
    
    if opportunities.is_empty() {
        warn!("未发现任何套利机会");
        info!("可能原因:");
        info!("  - 市场价格相对平衡");
        info!("  - 利润门槛设置过高");
        info!("  - Gas费用过高");
        return Ok(());
    }
    
    info!("💰 发现 {} 个套利机会:", opportunities.len());
    
    // 按利润排序
    let mut sorted_opportunities = opportunities;
    sorted_opportunities.sort_by(|a, b| b.net_profit_usd.partial_cmp(&a.net_profit_usd).unwrap());
    
    for (i, opportunity) in sorted_opportunities.iter().take(5).enumerate() {
        info!("\n📈 套利机会 {}:", i + 1);
        info!("  路径长度: {} 跳", opportunity.path.len());
        info!("  代币路径: {}", format_token_path(&opportunity.path));
        info!("  推荐输入: {} Wei", opportunity.optimal_input_amount);
        info!("  预期输出: {} Wei", opportunity.expected_output_amount);
        info!("  毛利润: ${:.4}", opportunity.gross_profit_usd);
        info!("  Gas费用: ${:.4}", opportunity.gas_cost_usd);
        info!("  净利润: ${:.4}", opportunity.net_profit_usd);
        info!("  利润率: {:.2}%", opportunity.profit_margin_percent);
        info!("  发现时间: {:?}", opportunity.discovered_at);
        
        // 显示详细的交换步骤
        display_swap_steps(&opportunity.path);
    }
    
    // 统计信息
    let total_profit: f64 = sorted_opportunities.iter().map(|o| o.net_profit_usd).sum();
    let avg_profit: f64 = total_profit / sorted_opportunities.len() as f64;
    let max_profit = sorted_opportunities.first().map(|o| o.net_profit_usd).unwrap_or(0.0);
    
    info!("\n📊 统计信息:");
    info!("  总套利机会: {}", sorted_opportunities.len());
    info!("  最大单笔利润: ${:.4}", max_profit);
    info!("  平均利润: ${:.4}", avg_profit);
    info!("  总利润潜力: ${:.4}", total_profit);
    
    // 提供执行建议
    info!("\n💡 执行建议:");
    if let Some(best_opportunity) = sorted_opportunities.first() {
        info!("  优先执行: 套利机会 1 (净利润 ${:.4})", best_opportunity.net_profit_usd);
        info!("  建议输入: {} Wei WMNT", best_opportunity.optimal_input_amount);
        info!("  预期回报: {} Wei WMNT", best_opportunity.expected_output_amount);
        
        let roi = ((best_opportunity.expected_output_amount.saturating_sub(best_opportunity.optimal_input_amount)).to::<u128>() as f64 / best_opportunity.optimal_input_amount.to::<u128>() as f64) * 100.0;
        info!("  投资回报率: {:.4}%", roi);
    }
    
    Ok(())
}

/// 格式化代币路径显示
fn format_token_path(path: &swap_path::logic::graph::SwapPath) -> String {
    path.tokens
        .iter()
        .map(|token| format!("{:?}", token.get_address()))
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// 显示详细的交换步骤
fn display_swap_steps(path: &swap_path::logic::graph::SwapPath) {
    info!("  详细交换步骤:");
    for (i, pool) in path.pools.iter().enumerate() {
        let token_in = &path.tokens[i];
        let token_out = &path.tokens[i + 1];
        info!("    步骤 {}: 在池子 {:?} 中 {} -> {}", 
              i + 1,
              pool.get_address(),
              format!("{:?}", token_in.get_address()),
              format!("{:?}", token_out.get_address()));
    }
}

/// 简化的测试运行器（用于CI/CD）
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_end_to_end_arbitrage_discovery() {
        // 简化版本的端到端测试，适合自动化测试
        let (market, initial_pools) = setup_test_market().await.unwrap();
        let mut arbitrage_engine = setup_arbitrage_engine(&market).await.unwrap();
        
        // 创建简单的测试快照
        let snapshot = create_market_snapshot(
            12345,
            vec![
                (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from(1000000u64), U256::from(2000000u64))),
                (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from(1000000u64), U256::from(1000000u64))),
            ],
        );
        
        // 测试套利发现
        let opportunities = arbitrage_engine.process_market_snapshot(&snapshot).unwrap();
        
        // 验证结果结构正确
        for opportunity in opportunities {
            assert!(opportunity.optimal_input_amount > U256::ZERO);
            assert!(opportunity.expected_output_amount > U256::ZERO);
            assert!(opportunity.path.len() >= 2);
        }
    }
}
