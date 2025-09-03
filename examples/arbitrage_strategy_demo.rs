/// 套利策略演示
/// 
/// 这个示例展示不同的套利策略和优化技术，
/// 包括路径选择、风险管理、资金分配等高级功能。

use swap_path::logic::{ArbitrageEngine, ArbitrageOpportunity};
use swap_path::logic::types::{ArbitrageConfig, MarketSnapshot};
use swap_path::logic::pools::{PoolId, MockPool};
use swap_path::{PoolWrapper, Token};
use swap_path::data_sync::markets::{Market, MarketConfigSection};
use alloy_primitives::{Address, U256};
use eyre::Result;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    info!("🎯 套利策略演示开始");
    
    // 演示不同的套利策略
    demo_basic_arbitrage_strategy().await?;
    demo_advanced_risk_management().await?;
    demo_multi_path_optimization().await?;
    demo_market_impact_analysis().await?;
    
    info!("✅ 所有策略演示完成");
    Ok(())
}

/// 基础套利策略演示
async fn demo_basic_arbitrage_strategy() -> Result<()> {
    info!("\n📈 基础套利策略演示");
    info!("{}", "=".repeat(50));
    
    // 创建测试环境
    let (market, _) = create_strategy_test_environment().await?;
    
    // 基础配置：保守的参数
    let basic_config = ArbitrageConfig {
        min_profit_threshold_usd: 5.0,  // 低门槛，捕获更多机会
        max_hops: 3,                    // 限制在3跳，降低复杂性
        gas_price_gwei: 20,
        gas_per_hop: 150_000,
        max_precomputed_paths: 500,
        enable_parallel_calculation: true,
    };
    
    let mut engine = ArbitrageEngine::new(basic_config);
    engine.initialize(&market.token_graph)?;
    
    info!("✅ 基础引擎初始化完成");
    
    // 创建不同的市场条件
    let market_conditions = create_various_market_conditions();
    
    for (condition_name, snapshot) in market_conditions {
        info!("\n📊 分析市场条件: {}", condition_name);
        
        let opportunities = engine.process_market_snapshot(&snapshot)?;
        
        if opportunities.is_empty() {
            info!("  无套利机会");
        } else {
            info!("  发现 {} 个机会:", opportunities.len());
            for (i, opp) in opportunities.iter().take(3).enumerate() {
                info!("    {}. 净利润: ${:.2}, 路径: {}-跳", 
                      i + 1, opp.net_profit_usd, opp.path.len());
            }
            
            // 分析最佳机会
            if let Some(best) = opportunities.first() {
                analyze_arbitrage_opportunity(best, "基础策略");
            }
        }
    }
    
    Ok(())
}

/// 高级风险管理演示
async fn demo_advanced_risk_management() -> Result<()> {
    info!("\n🛡️ 高级风险管理演示");
    info!("{}", "=".repeat(50));
    
    let (market, _) = create_strategy_test_environment().await?;
    
    // 高风险配置：更激进的参数
    let aggressive_config = ArbitrageConfig {
        min_profit_threshold_usd: 100.0, // 高门槛，只要大利润
        max_hops: 4,                     // 允许更复杂路径
        gas_price_gwei: 50,              // 高gas价格环境
        gas_per_hop: 200_000,
        max_precomputed_paths: 2000,
        enable_parallel_calculation: true,
    };
    
    let mut aggressive_engine = ArbitrageEngine::new(aggressive_config);
    aggressive_engine.initialize(&market.token_graph)?;
    
    // 保守配置：风险厌恶
    let conservative_config = ArbitrageConfig {
        min_profit_threshold_usd: 50.0,  // 中等门槛
        max_hops: 3,                     // 限制复杂性
        gas_price_gwei: 15,              // 低gas环境
        gas_per_hop: 120_000,
        max_precomputed_paths: 1000,
        enable_parallel_calculation: true,
    };
    
    let mut conservative_engine = ArbitrageEngine::new(conservative_config);
    conservative_engine.initialize(&market.token_graph)?;
    
    // 比较不同策略
    let test_snapshot = create_high_volatility_snapshot();
    
    let aggressive_opps = aggressive_engine.process_market_snapshot(&test_snapshot)?;
    let conservative_opps = conservative_engine.process_market_snapshot(&test_snapshot)?;
    
    info!("策略比较结果:");
    info!("  激进策略: {} 个机会", aggressive_opps.len());
    info!("  保守策略: {} 个机会", conservative_opps.len());
    
    // 风险分析
    analyze_strategy_risk(&aggressive_opps, "激进策略");
    analyze_strategy_risk(&conservative_opps, "保守策略");
    
    Ok(())
}

/// 多路径优化演示
async fn demo_multi_path_optimization() -> Result<()> {
    info!("\n🚀 多路径优化演示");
    info!("{}", "=".repeat(50));
    
    let (market, _) = create_strategy_test_environment().await?;
    
    let config = ArbitrageConfig {
        min_profit_threshold_usd: 1.0,
        max_hops: 4,
        gas_price_gwei: 25,
        gas_per_hop: 150_000,
        max_precomputed_paths: 3000,
        enable_parallel_calculation: true,
    };
    
    let mut engine = ArbitrageEngine::new(config);
    engine.initialize(&market.token_graph)?;
    
    let snapshot = create_complex_arbitrage_snapshot();
    let opportunities = engine.process_market_snapshot(&snapshot)?;
    
    if opportunities.len() >= 2 {
        info!("发现多个套利路径，进行优化分析:");
        
        // 按不同指标排序
        let mut by_profit = opportunities.clone();
        by_profit.sort_by(|a, b| b.net_profit_usd.partial_cmp(&a.net_profit_usd).unwrap());
        
        let mut by_roi = opportunities.clone();
        by_roi.sort_by(|a, b| {
            let roi_a = calculate_roi(a);
            let roi_b = calculate_roi(b);
            roi_b.partial_cmp(&roi_a).unwrap()
        });
        
        let mut by_risk = opportunities.clone();
        by_risk.sort_by(|a, b| a.path.len().cmp(&b.path.len())); // 跳数越少风险越低
        
        info!("\n📊 最高利润路径:");
        if let Some(best_profit) = by_profit.first() {
            display_opportunity_details(best_profit, 1);
        }
        
        info!("\n💰 最高ROI路径:");
        if let Some(best_roi) = by_roi.first() {
            display_opportunity_details(best_roi, 1);
        }
        
        info!("\n🛡️ 最低风险路径:");
        if let Some(lowest_risk) = by_risk.first() {
            display_opportunity_details(lowest_risk, 1);
        }
        
        // 资金分配建议
        recommend_capital_allocation(&opportunities);
    }
    
    Ok(())
}

/// 市场影响分析演示
async fn demo_market_impact_analysis() -> Result<()> {
    info!("\n📈 市场影响分析演示");
    info!("{}", "=".repeat(50));
    
    // 分析不同交易规模的影响
    let trade_sizes = vec![
        ("小额", U256::from_str_radix("1000000000000000000", 10).unwrap()),    // 1 WMNT
        ("中额", U256::from_str_radix("10000000000000000000", 10).unwrap()),   // 10 WMNT
        ("大额", U256::from_str_radix("100000000000000000000", 10).unwrap()),  // 100 WMNT
        ("巨额", U256::from_str_radix("1000000000000000000000", 10).unwrap()), // 1000 WMNT
    ];
    
    info!("分析不同交易规模对利润的影响:");
    
    for (size_name, amount) in trade_sizes {
        let estimated_impact = analyze_trade_size_impact(amount);
        info!("  {} ({}): 滑点影响 {:.2}%, 建议最大规模: {} WMNT", 
              size_name, 
              format_wei_to_ether(amount),
              estimated_impact.slippage_percent,
              format_wei_to_ether(estimated_impact.max_recommended_size));
    }
    
    // 流动性分析
    analyze_liquidity_requirements();
    
    Ok(())
}

/// 创建策略测试环境
async fn create_strategy_test_environment() -> Result<(Market, Vec<PoolWrapper>)> {
    let wmnt = swap_path::utils::constants::WMNT;
    let usdc = Address::from_slice(&[0x12; 20]);
    let usdt = Address::from_slice(&[0x34; 20]);
    let btc = Address::from_slice(&[0x56; 20]);
    let eth = Address::from_slice(&[0x78; 20]);
    
    let mut market = Market::new(MarketConfigSection::default().with_max_hops(4));
    
    // 添加代币
    market.add_token(Token::new_with_data(usdc, Some("USDC".to_string()), None, Some(6)));
    market.add_token(Token::new_with_data(usdt, Some("USDT".to_string()), None, Some(6)));
    market.add_token(Token::new_with_data(btc, Some("WBTC".to_string()), None, Some(8)));
    market.add_token(Token::new_with_data(eth, Some("WETH".to_string()), None, Some(18)));
    
    // 创建复杂的池子网络
    let pools = vec![
        // 主要交易对
        create_pool(Address::from_slice(&[0x01; 20]), wmnt, usdc, 
                   "1000000000000000000000000", "2000000000000"), // 1M WMNT, 2M USDC
        create_pool(Address::from_slice(&[0x02; 20]), usdc, usdt,
                   "1000000000000", "1000000000000"), // 1M USDC, 1M USDT
        create_pool(Address::from_slice(&[0x03; 20]), usdt, wmnt,
                   "1000000000000", "500000000000000000000000"), // 1M USDT, 500K WMNT
        
        // BTC路径
        create_pool(Address::from_slice(&[0x04; 20]), wmnt, btc,
                   "3000000000000000000000000", "5000000000"), // 3M WMNT, 50 BTC
        create_pool(Address::from_slice(&[0x05; 20]), btc, usdc,
                   "2500000000", "150000000000000"), // 25 BTC, 150M USDC
        
        // ETH路径
        create_pool(Address::from_slice(&[0x06; 20]), wmnt, eth,
                   "2000000000000000000000000", "1000000000000000000000"), // 2M WMNT, 1000 ETH
        create_pool(Address::from_slice(&[0x07; 20]), eth, usdc,
                   "500000000000000000000", "1000000000000000"), // 500 ETH, 1M USDC
        
        // 直接路径
        create_pool(Address::from_slice(&[0x08; 20]), usdc, wmnt,
                   "1500000000000", "750000000000000000000000"), // 1.5M USDC, 750K WMNT
    ];
    
    for pool in &pools {
        market.add_pool(pool.clone());
    }
    
    Ok((market, pools))
}

/// 创建池子的辅助函数
fn create_pool(address: Address, token0: Address, token1: Address, _reserve0: &str, _reserve1: &str) -> PoolWrapper {
    let mock_pool = MockPool { address, token0, token1 };
    PoolWrapper::new(Arc::new(mock_pool))
}

/// 创建多种市场条件
fn create_various_market_conditions() -> Vec<(&'static str, MarketSnapshot)> {
    vec![
        ("正常市场", create_normal_market_snapshot()),
        ("高波动", create_high_volatility_snapshot()),
        ("低流动性", create_low_liquidity_snapshot()),
        ("价格失衡", create_price_imbalance_snapshot()),
    ]
}

/// 创建正常市场快照
fn create_normal_market_snapshot() -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(12345, 2000.0);
    
    let pools = vec![
        (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from_str_radix("1000000000000000000000000", 10).unwrap(), U256::from_str_radix("2000000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from_str_radix("1000000000000", 10).unwrap(), U256::from_str_radix("1000000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from_str_radix("1000000000000", 10).unwrap(), U256::from_str_radix("500000000000000000000000", 10).unwrap())),
    ];
    
    for (pool_id, (r0, r1)) in pools {
        snapshot.set_pool_reserves(pool_id, r0, r1);
    }
    
    snapshot
}

/// 创建高波动市场快照
fn create_high_volatility_snapshot() -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(12346, 2100.0); // ETH价格上涨
    
    // 价格剧烈变化
    let pools = vec![
        (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from_str_radix("1200000000000000000000000", 10).unwrap(), U256::from_str_radix("1800000000000", 10).unwrap())), // WMNT下跌
        (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from_str_radix("900000000000", 10).unwrap(), U256::from_str_radix("1100000000000", 10).unwrap())), // USDT溢价
        (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from_str_radix("800000000000", 10).unwrap(), U256::from_str_radix("600000000000000000000000", 10).unwrap())), // 更大价差
    ];
    
    for (pool_id, (r0, r1)) in pools {
        snapshot.set_pool_reserves(pool_id, r0, r1);
    }
    
    snapshot
}

/// 创建低流动性市场快照
fn create_low_liquidity_snapshot() -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(12347, 2000.0);
    
    // 流动性大幅减少
    let pools = vec![
        (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from_str_radix("100000000000000000000000", 10).unwrap(), U256::from_str_radix("200000000000", 10).unwrap())), // 10倍减少
        (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from_str_radix("100000000000", 10).unwrap(), U256::from_str_radix("100000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from_str_radix("100000000000", 10).unwrap(), U256::from_str_radix("50000000000000000000000", 10).unwrap())),
    ];
    
    for (pool_id, (r0, r1)) in pools {
        snapshot.set_pool_reserves(pool_id, r0, r1);
    }
    
    snapshot
}

/// 创建价格失衡快照
fn create_price_imbalance_snapshot() -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(12348, 2000.0);
    
    // 严重的价格失衡，创造套利机会
    let pools = vec![
        (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from_str_radix("1500000000000000000000000", 10).unwrap(), U256::from_str_radix("1500000000000", 10).unwrap())), // WMNT被低估
        (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from_str_radix("1000000000000", 10).unwrap(), U256::from_str_radix("1000000000000", 10).unwrap())), // 稳定
        (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from_str_radix("700000000000", 10).unwrap(), U256::from_str_radix("700000000000000000000000", 10).unwrap())), // WMNT被高估
    ];
    
    for (pool_id, (r0, r1)) in pools {
        snapshot.set_pool_reserves(pool_id, r0, r1);
    }
    
    snapshot
}

/// 创建复杂套利快照
fn create_complex_arbitrage_snapshot() -> MarketSnapshot {
    let mut snapshot = MarketSnapshot::new(12349, 2000.0);
    
    // 所有8个池子都有数据，形成复杂的套利网络
    let pools = vec![
        (PoolId::Address(Address::from_slice(&[0x01; 20])), (U256::from_str_radix("1000000000000000000000000", 10).unwrap(), U256::from_str_radix("2100000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x02; 20])), (U256::from_str_radix("1000000000000", 10).unwrap(), U256::from_str_radix("980000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x03; 20])), (U256::from_str_radix("950000000000", 10).unwrap(), U256::from_str_radix("520000000000000000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x04; 20])), (U256::from_str_radix("3100000000000000000000000", 10).unwrap(), U256::from_str_radix("4800000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x05; 20])), (U256::from_str_radix("2400000000", 10).unwrap(), U256::from_str_radix("155000000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x06; 20])), (U256::from_str_radix("2100000000000000000000000", 10).unwrap(), U256::from_str_radix("980000000000000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x07; 20])), (U256::from_str_radix("520000000000000000000", 10).unwrap(), U256::from_str_radix("1050000000000000", 10).unwrap())),
        (PoolId::Address(Address::from_slice(&[0x08; 20])), (U256::from_str_radix("1450000000000", 10).unwrap(), U256::from_str_radix("780000000000000000000000", 10).unwrap())),
    ];
    
    for (pool_id, (r0, r1)) in pools {
        snapshot.set_pool_reserves(pool_id, r0, r1);
    }
    
    snapshot
}

/// 分析套利机会
fn analyze_arbitrage_opportunity(opportunity: &ArbitrageOpportunity, strategy_name: &str) {
    info!("\n🔍 {} 最佳机会分析:", strategy_name);
    info!("  净利润: ${:.2}", opportunity.net_profit_usd);
    info!("  投入: {} WMNT", format_wei_to_ether(opportunity.optimal_input_amount));
    info!("  产出: {} WMNT", format_wei_to_ether(opportunity.expected_output_amount));
    info!("  ROI: {:.2}%", calculate_roi(opportunity));
    info!("  路径复杂度: {}-跳", opportunity.path.len());
    info!("  Gas成本: ${:.2}", opportunity.gas_cost_usd);
    info!("  利润率: {:.2}%", opportunity.profit_margin_percent);
    
    // 风险评估
    let risk_score = assess_risk_score(opportunity);
    info!("  风险评级: {}", format_risk_score(risk_score));
}

/// 策略风险分析
fn analyze_strategy_risk(opportunities: &[ArbitrageOpportunity], strategy_name: &str) {
    if opportunities.is_empty() {
        info!("  {}: 无机会，风险为零", strategy_name);
        return;
    }
    
    let total_profit: f64 = opportunities.iter().map(|o| o.net_profit_usd).sum();
    let avg_profit = total_profit / opportunities.len() as f64;
    let max_profit = opportunities.iter().map(|o| o.net_profit_usd).fold(0.0, f64::max);
    let min_profit = opportunities.iter().map(|o| o.net_profit_usd).fold(f64::INFINITY, f64::min);
    
    let avg_hops: f64 = opportunities.iter().map(|o| o.path.len() as f64).sum::<f64>() / opportunities.len() as f64;
    
    info!("  {} 风险分析:", strategy_name);
    info!("    机会数量: {}", opportunities.len());
    info!("    平均利润: ${:.2}", avg_profit);
    info!("    利润范围: ${:.2} - ${:.2}", min_profit, max_profit);
    info!("    平均复杂度: {:.1}-跳", avg_hops);
    info!("    风险分散度: {}", if opportunities.len() > 5 { "高" } else { "低" });
}

/// 显示机会详情
fn display_opportunity_details(opportunity: &ArbitrageOpportunity, index: usize) {
    info!("  {}. 净利润: ${:.2}, ROI: {:.1}%, 复杂度: {}-跳", 
          index,
          opportunity.net_profit_usd,
          calculate_roi(opportunity),
          opportunity.path.len());
}

/// 推荐资金分配
fn recommend_capital_allocation(opportunities: &[ArbitrageOpportunity]) {
    info!("\n💼 资金分配建议:");
    
    let total_profit: f64 = opportunities.iter().map(|o| o.net_profit_usd).sum();
    
    for (i, opp) in opportunities.iter().take(5).enumerate() {
        let allocation_percent = (opp.net_profit_usd / total_profit) * 100.0;
        let risk_score = assess_risk_score(opp);
        
        info!("  机会 {}: {:.1}% 资金, 风险: {}", 
              i + 1, 
              allocation_percent, 
              format_risk_score(risk_score));
    }
}

/// 交易规模影响分析
#[derive(Debug)]
struct TradeImpact {
    slippage_percent: f64,
    max_recommended_size: U256,
}

fn analyze_trade_size_impact(amount: U256) -> TradeImpact {
    // 简化的滑点模型
    let amount_f64 = amount.to::<u128>() as f64;
    let slippage = if amount_f64 > 1e21 { // > 1000 WMNT
        5.0
    } else if amount_f64 > 1e20 { // > 100 WMNT
        2.0
    } else if amount_f64 > 1e19 { // > 10 WMNT
        0.5
    } else {
        0.1
    };
    
    let max_size = U256::from_str_radix("50000000000000000000", 10).unwrap(); // 50 WMNT
    
    TradeImpact {
        slippage_percent: slippage,
        max_recommended_size: max_size,
    }
}

/// 流动性需求分析
fn analyze_liquidity_requirements() {
    info!("\n💧 流动性需求分析:");
    info!("  建议最小池子流动性: 100K WMNT");
    info!("  建议交易规模上限: 池子流动性的5%");
    info!("  高频交易建议: < 1% 池子流动性");
    info!("  紧急退出需要: > 1M WMNT 总流动性");
}

/// 计算投资回报率
fn calculate_roi(opportunity: &ArbitrageOpportunity) -> f64 {
    let input_usd = opportunity.optimal_input_amount.to::<u128>() as f64 * 2.0 / 1e18; // 假设WMNT价格$2
    if input_usd > 0.0 {
        (opportunity.net_profit_usd / input_usd) * 100.0
    } else {
        0.0
    }
}

/// 评估风险分数
fn assess_risk_score(opportunity: &ArbitrageOpportunity) -> f64 {
    let mut risk = 0.0;
    
    // 路径复杂度风险
    risk += (opportunity.path.len() as f64 - 2.0) * 10.0;
    
    // Gas费用风险
    risk += opportunity.gas_cost_usd / 10.0;
    
    // 利润率风险（利润率太高可能不稳定）
    if opportunity.profit_margin_percent > 95.0 {
        risk += 20.0;
    }
    
    risk.min(100.0).max(0.0)
}

/// 格式化风险分数
fn format_risk_score(score: f64) -> &'static str {
    if score < 20.0 {
        "低风险 🟢"
    } else if score < 50.0 {
        "中风险 🟡"
    } else if score < 80.0 {
        "高风险 🟠"
    } else {
        "极高风险 🔴"
    }
}

/// 格式化Wei到Ether
fn format_wei_to_ether(wei: U256) -> String {
    let ether = wei.to::<u128>() as f64 / 1e18;
    format!("{:.4}", ether)
}
