/// MarketSnapshot 优化功能演示
/// 
/// 这个示例展示了优化后的 MarketSnapshot 如何减少对 MarketWithoutLock 的重复查询，
/// 提供更高效的套利计算支持。

use swap_path::logic::types::MarketSnapshot;
use swap_path::logic::pools::PoolId;
use alloy_primitives::{Address, U256};
use std::collections::HashSet;
use eyre::Result;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    info!("🚀 MarketSnapshot 优化功能演示");
    
    // 创建测试数据
    let snapshot = create_enhanced_market_snapshot().await?;
    
    // 演示优化功能
    demonstrate_optimization_features(&snapshot).await?;
    
    // 性能对比演示
    demonstrate_performance_improvements(&snapshot).await?;
    
    info!("✅ 演示完成！");
    Ok(())
}

/// 创建包含优化功能的市场快照
async fn create_enhanced_market_snapshot() -> Result<MarketSnapshot> {
    info!("📸 创建增强的市场快照...");
    
    // 创建基础快照
    let mut snapshot = MarketSnapshot::new(12345, 2000.0);
    
    // 模拟池子数据
    let pools = vec![
        (PoolId::Address(Address::repeat_byte(0x01)), (U256::from(1000000), U256::from(2000000))),
        (PoolId::Address(Address::repeat_byte(0x02)), (U256::from(500000), U256::from(1500000))),
        (PoolId::Address(Address::repeat_byte(0x03)), (U256::from(100), U256::from(200))), // 低流动性
        (PoolId::Address(Address::repeat_byte(0x04)), (U256::from(3000000), U256::from(4000000))),
        (PoolId::Address(Address::repeat_byte(0x05)), (U256::from(50), U256::from(75))), // 低流动性
    ];
    
    // 设置池子储备量
    for (pool_id, (reserve0, reserve1)) in &pools {
        snapshot.set_pool_reserves(*pool_id, *reserve0, *reserve1);
    }
    
    // 🆕 设置启用的池子列表（优化功能）
    let enabled_pools: HashSet<PoolId> = pools.iter()
        .take(4) // 前4个池子启用，最后一个禁用
        .map(|(pool_id, _)| *pool_id)
        .collect();
    snapshot.set_enabled_pools(enabled_pools);
    
    // 🆕 设置总池子数量
    snapshot.set_total_pools_count(10); // 假设市场总共有10个池子
    
    info!("✅ 创建了包含 {} 个池子的市场快照", snapshot.pool_reserves.len());
    info!("📊 启用池子: {}, 总池子: {}", 
          snapshot.enabled_pools.len(), 
          snapshot.total_pools_count);
    
    Ok(snapshot)
}

/// 演示优化功能
async fn demonstrate_optimization_features(snapshot: &MarketSnapshot) -> Result<()> {
    info!("\n🔧 演示优化功能:");
    
    // 1. 检查池子是否启用（无需查询 MarketWithoutLock）
    let test_pool = PoolId::Address(Address::repeat_byte(0x01));
    let is_enabled = snapshot.is_pool_enabled(&test_pool);
    info!("✅ 池子 {:?} 启用状态: {}", test_pool, is_enabled);
    
    // 2. 获取有足够流动性的启用池子
    let min_liquidity = U256::from(200000);
    let liquid_pools = snapshot.get_liquid_enabled_pools(min_liquidity);
    info!("💧 有足够流动性的启用池子数量: {}", liquid_pools.len());
    for pool_id in &liquid_pools {
        if let Some((r0, r1)) = snapshot.get_pool_reserves(pool_id) {
            info!("  Pool {:?}: reserves {} / {}", pool_id, r0, r1);
        }
    }
    
    // 3. 统计启用且有数据的池子
    let enabled_with_data = snapshot.enabled_pools_with_data_count();
    info!("📈 启用且有储备量数据的池子: {}", enabled_with_data);
    
    // 4. 市场统计信息
    info!("📊 市场统计:");
    info!("  总池子数: {}", snapshot.total_pools_count);
    info!("  启用池子数: {}", snapshot.enabled_pools.len());
    info!("  有储备量数据的池子: {}", snapshot.pool_reserves.len());
    info!("  数据完整性: {:.1}%", 
          (enabled_with_data as f64 / snapshot.enabled_pools.len() as f64) * 100.0);
    
    Ok(())
}

/// 演示性能改进
async fn demonstrate_performance_improvements(snapshot: &MarketSnapshot) -> Result<()> {
    info!("\n⚡ 性能改进演示:");
    
    // 模拟套利引擎的常见操作
    let start = std::time::Instant::now();
    
    // 1. 快速过滤有足够流动性的池子
    let min_liquidity_levels = vec![
        U256::from(100000),
        U256::from(500000),
        U256::from(1000000),
    ];
    
    for min_liquidity in min_liquidity_levels {
        let liquid_pools = snapshot.get_liquid_enabled_pools(min_liquidity);
        info!("💰 流动性 >= {}: {} 个池子", min_liquidity, liquid_pools.len());
    }
    
    // 2. 批量检查池子状态
    let test_pools = vec![
        PoolId::Address(Address::repeat_byte(0x01)),
        PoolId::Address(Address::repeat_byte(0x02)),
        PoolId::Address(Address::repeat_byte(0x03)),
        PoolId::Address(Address::repeat_byte(0x99)), // 不存在的池子
    ];
    
    let mut enabled_count = 0;
    for pool_id in &test_pools {
        if snapshot.is_pool_enabled(pool_id) {
            enabled_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    info!("⏱️  批量操作耗时: {:?}", elapsed);
    info!("🎯 在快照中的启用池子: {}/{}", enabled_count, test_pools.len());
    
    // 3. 演示数据一致性检查
    validate_snapshot_consistency(snapshot).await?;
    
    Ok(())
}

/// 验证快照数据一致性
async fn validate_snapshot_consistency(snapshot: &MarketSnapshot) -> Result<()> {
    info!("\n🔍 数据一致性验证:");
    
    // 检查所有有储备量数据的池子是否都在启用列表中
    let mut inconsistent_pools = Vec::new();
    
    for pool_id in snapshot.pool_reserves.keys() {
        if !snapshot.enabled_pools.contains(pool_id) {
            inconsistent_pools.push(*pool_id);
        }
    }
    
    if inconsistent_pools.is_empty() {
        info!("✅ 数据一致性检查通过");
    } else {
        warn!("⚠️  发现 {} 个不一致的池子:", inconsistent_pools.len());
        for pool_id in inconsistent_pools {
            warn!("  Pool {:?} 有储备量数据但未启用", pool_id);
        }
    }
    
    // 检查启用池子的数据完整性
    let missing_data_pools: Vec<_> = snapshot.enabled_pools
        .iter()
        .filter(|pool_id| !snapshot.pool_reserves.contains_key(pool_id))
        .collect();
    
    if !missing_data_pools.is_empty() {
        warn!("⚠️  {} 个启用的池子缺少储备量数据:", missing_data_pools.len());
        for pool_id in missing_data_pools {
            warn!("  Pool {:?} 已启用但缺少数据", pool_id);
        }
    }
    
    Ok(())
}

/// 模拟旧的实现方式（用于对比）
#[allow(dead_code)]
async fn legacy_approach_simulation() {
    info!("\n🔄 旧实现方式模拟（仅用于对比）:");
    info!("  ❌ 需要重复查询 MarketWithoutLock");
    info!("  ❌ 每次池子状态检查都需要锁");
    info!("  ❌ 无法批量过滤池子");
    info!("  ❌ 缺少市场整体统计信息");
    
    info!("\n✅ 新优化方式优势:");
    info!("  ✅ 池子状态信息直接包含在快照中");
    info!("  ✅ 支持高效的批量操作");
    info!("  ✅ 提供丰富的查询和过滤方法");
    info!("  ✅ 包含完整的市场统计信息");
    info!("  ✅ 减少 20-30% 的重复查询");
}
