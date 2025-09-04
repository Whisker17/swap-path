// examples/test_final.rs
use swap_path::{
    logic::{
        types::{MarketSnapshot, ArbitrageOpportunity},
        graph::SwapPath,
        pools::{PoolId, MockPool},
    },
    utils::{BlockDetailLogger, Token},
    PoolWrapper,
};
use alloy_primitives::{Address, U256};
use eyre::Result;
use std::{sync::Arc, time::Instant, collections::HashSet};

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 最终测试：池子储备详细记录器...");

    // 创建详细记录器
    let mut detail_logger = BlockDetailLogger::new("./logs");
    println!("✅ 创建了详细记录器");

    // 创建模拟的市场快照，包含多个池子的储备数据
    let mut market_snapshot = MarketSnapshot::new(99999);
    market_snapshot.set_total_pools_count(3);
    
    // 添加池子储备数据（模拟真实的储备数值）
    let pools = vec![
        (PoolId::Address(Address::repeat_byte(1)), U256::from_str_radix("36761692011477739202857209", 10).unwrap(), U256::from_str_radix("17903556812944400602477", 10).unwrap()),
        (PoolId::Address(Address::repeat_byte(2)), U256::from_str_radix("6976510715409879421116664", 10).unwrap(), U256::from_str_radix("493732428093990721202754", 10).unwrap()),
        (PoolId::Address(Address::repeat_byte(3)), U256::from_str_radix("2712600766906464377160", 10).unwrap(), U256::from_str_radix("200789199561004867491", 10).unwrap()),
    ];

    let mut enabled_pools = HashSet::new();
    for (pool_id, reserve0, reserve1) in pools {
        market_snapshot.set_pool_reserves(pool_id, reserve0, reserve1);
        enabled_pools.insert(pool_id);
        println!("  添加池子 {}: reserve0={:.6} MNT, reserve1={:.6} MNT", 
                 pool_id, 
                 reserve0.to_string().parse::<f64>().unwrap_or(0.0) / 1e18,
                 reserve1.to_string().parse::<f64>().unwrap_or(0.0) / 1e18);
    }
    
    market_snapshot.set_enabled_pools(enabled_pools);
    println!("✅ 创建了包含 {} 个池子的模拟市场快照", market_snapshot.pool_reserves.len());

    // 创建模拟的预计算路径
    let precomputed_paths = vec![
        create_mock_swap_path(),
    ];

    // 创建模拟的计算结果和套利机会
    let calculation_results = vec![
        swap_path::logic::types::ProfitCalculationResult::success(
            create_mock_swap_path(),
            U256::from(1000000000000000000u64),
            U256::from(1100000000000000000u64),
            U256::from(100000000000000000u64),
            U256::from(10000000000000000u64),
        ),
    ];

    let opportunities = vec![
        ArbitrageOpportunity::new(
            create_mock_swap_path(),
            U256::from(1000000000000000000u64),
            U256::from(1100000000000000000u64),
            U256::from(100000000000000000u64),
            U256::from(10000000000000000u64),
        ),
    ];

    let processing_start = Instant::now();
    let calculation_duration = std::time::Duration::from_millis(200);

    // 记录详细信息（包括池子储备）
    println!("📝 开始记录详细信息...");
    detail_logger.log_block_processing(
        &market_snapshot,
        &precomputed_paths,
        &calculation_results,
        &opportunities,
        processing_start,
        calculation_duration,
    ).await?;

    println!("✅ 成功记录区块详细信息");
    println!("📄 池子储备文件: {}", detail_logger.get_pool_reserves_file_path());

    // 验证CSV文件内容
    println!("🔍 验证生成的CSV文件...");
    if let Ok(content) = std::fs::read_to_string(detail_logger.get_pool_reserves_file_path()) {
        let lines: Vec<&str> = content.lines().collect();
        println!("📊 池子储备CSV文件内容 ({} 行):", lines.len());
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                println!("  [头部] {}", line);
            } else {
                println!("  [数据{}] {}", i, line);
            }
        }
    }

    println!("🎉 池子储备记录功能测试完成！");
    Ok(())
}

fn create_mock_swap_path() -> SwapPath {
    let token1 = Arc::new(Token::new(Address::repeat_byte(1)));
    let token2 = Arc::new(Token::new(Address::repeat_byte(2)));

    let mock_pool = MockPool::new(
        Address::repeat_byte(1),
        Address::repeat_byte(2),
        Address::repeat_byte(3),
    );
    let pool_wrapper = PoolWrapper::new(Arc::new(mock_pool));

    SwapPath::new(
        vec![token1, token2],
        vec![pool_wrapper],
    )
}
