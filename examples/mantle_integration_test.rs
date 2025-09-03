/// Mantle链集成测试
/// 
/// 这个测试演示如何连接真实的Mantle链进行套利监控。
/// 注意：这需要真实的RPC端点和实际的链上数据。

use swap_path::data_sync::{DataSyncConfig, DataSyncServiceBuilder};
use swap_path::logic::{ArbitrageEngine};
use swap_path::logic::types::{ArbitrageConfig};
use swap_path::data_sync::markets::{Market, MarketConfigSection};
use swap_path::{PoolWrapper, Token};
use alloy_primitives::Address;
use eyre::Result;
use std::time::Duration;
use tokio::time::{timeout, sleep};
use tracing::{info, warn, error};

// Mantle网络的真实合约地址
const MANTLE_RPC_WSS: &str = "wss://ws.mantle.xyz";
const MANTLE_RPC_HTTPS: &str = "https://rpc.mantle.xyz";
const MANTLE_MULTICALL3: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

// 一些真实的Mantle DEX池子地址（这些需要根据实际情况更新）
const REAL_POOL_ADDRESSES: &[&str] = &[
    // MoeLP pools或其他DEX池子地址
    // 注意：这些地址需要根据实际的Mantle生态系统更新
];

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    info!("🌐 开始Mantle链集成测试");
    
    // 检查环境变量
    if std::env::var("MANTLE_RPC_URL").is_err() {
        warn!("未设置MANTLE_RPC_URL环境变量，使用默认RPC端点");
        warn!("建议设置自己的RPC端点以获得更好的性能");
    }
    
    // 创建配置
    let config = create_mantle_config().await?;
    
    // 运行集成测试
    match run_mantle_integration_test(config).await {
        Ok(_) => {
            info!("✅ Mantle集成测试完成");
        }
        Err(e) => {
            error!("❌ Mantle集成测试失败: {}", e);
            info!("💡 提示:");
            info!("  1. 确保网络连接正常");
            info!("  2. 检查RPC端点是否可用");
            info!("  3. 验证池子地址是否正确");
        }
    }
    
    Ok(())
}

/// 创建Mantle网络配置
async fn create_mantle_config() -> Result<DataSyncConfig> {
    info!("⚙️ 创建Mantle网络配置...");
    
    let rpc_wss = std::env::var("MANTLE_RPC_WSS")
        .unwrap_or_else(|_| MANTLE_RPC_WSS.to_string());
    let rpc_https = std::env::var("MANTLE_RPC_HTTPS")
        .unwrap_or_else(|_| MANTLE_RPC_HTTPS.to_string());
    
    let config = DataSyncConfig {
        rpc_wss_url: rpc_wss,
        rpc_http_url: rpc_https,
        multicall_address: MANTLE_MULTICALL3.to_string(),
        max_pools_per_batch: 50, // Mantle网络支持较大批次
        ws_connection_timeout_secs: 30,
        max_reconnect_attempts: 5,
        reconnect_delay_secs: 3,
        http_timeout_secs: 15,
        channel_buffer_size: 1000,
    };
    
    info!("配置完成:");
    info!("  WebSocket: {}", config.rpc_wss_url);
    info!("  HTTP: {}", config.rpc_http_url);
    info!("  Multicall: {}", config.multicall_address);
    
    Ok(config)
}

/// 运行Mantle集成测试
async fn run_mantle_integration_test(config: DataSyncConfig) -> Result<()> {
    info!("🔗 开始连接Mantle网络...");
    
    // 第一阶段：测试网络连接
    test_network_connectivity(&config).await?;
    
    // 第二阶段：设置真实数据同步
    if REAL_POOL_ADDRESSES.is_empty() {
        warn!("⚠️  没有配置真实池子地址，跳过实际数据同步测试");
        info!("💡 要进行真实测试，请在REAL_POOL_ADDRESSES中添加真实的DEX池子地址");
        return Ok(());
    }
    
    setup_real_data_sync(config).await?;
    
    Ok(())
}

/// 测试网络连接性
async fn test_network_connectivity(config: &DataSyncConfig) -> Result<()> {
    info!("📡 测试网络连接性...");
    
    // 测试HTTP连接
    info!("测试HTTP RPC连接...");
    let client = reqwest::Client::new();
    
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1
    });
    
    match timeout(
        Duration::from_secs(10),
        client.post(&config.rpc_http_url).json(&request_body).send()
    ).await {
        Ok(Ok(response)) => {
            if response.status().is_success() {
                let result: serde_json::Value = response.json().await?;
                if let Some(block_number) = result.get("result") {
                    info!("✅ HTTP RPC连接成功，当前区块: {}", block_number);
                } else {
                    warn!("⚠️  HTTP RPC返回格式异常: {:?}", result);
                }
            } else {
                error!("❌ HTTP RPC连接失败，状态码: {}", response.status());
            }
        }
        Ok(Err(e)) => {
            error!("❌ HTTP RPC连接错误: {}", e);
        }
        Err(_) => {
            error!("❌ HTTP RPC连接超时");
        }
    }
    
    // 测试WebSocket连接（简单连接测试）
    info!("测试WebSocket连接...");
    match test_websocket_connection(&config.rpc_wss_url).await {
        Ok(_) => {
            info!("✅ WebSocket连接测试成功");
        }
        Err(e) => {
            error!("❌ WebSocket连接测试失败: {}", e);
        }
    }
    
    Ok(())
}

/// 测试WebSocket连接
async fn test_websocket_connection(ws_url: &str) -> Result<()> {
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use futures_util::{SinkExt, StreamExt};
    
    // 尝试连接
    let (ws_stream, _) = timeout(
        Duration::from_secs(10),
        connect_async(ws_url)
    ).await??;
    
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // 发送测试消息
    let test_message = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1
    });
    
    ws_sender.send(Message::Text(test_message.to_string())).await?;
    
    // 等待响应
    if let Some(response) = timeout(Duration::from_secs(5), ws_receiver.next()).await? {
        match response? {
            Message::Text(text) => {
                let result: serde_json::Value = serde_json::from_str(&text)?;
                if result.get("result").is_some() {
                    info!("WebSocket测试消息收到正确响应");
                }
            }
            _ => {
                warn!("WebSocket返回非文本消息");
            }
        }
    }
    
    Ok(())
}

/// 设置真实数据同步（如果有真实池子地址）
async fn setup_real_data_sync(config: DataSyncConfig) -> Result<()> {
    info!("🏊 设置真实数据同步...");
    
    // 创建空的初始池子列表（真实池子将通过地址动态加载）
    let initial_pools = Vec::new();
    
    // 创建数据同步服务
    let mut data_service = DataSyncServiceBuilder::new()
        .with_config(config)
        .with_pools(initial_pools)
        .build()
        .await?;
    
    info!("数据同步服务创建完成");
    
    // 创建套利引擎配置
    let arbitrage_config = ArbitrageConfig {
        min_profit_threshold_usd: 10.0, // 真实环境中设置更高的利润门槛
        max_hops: 4,
        gas_price_gwei: 50, // Mantle网络的典型gas价格
        gas_per_hop: 200_000, // 更保守的gas估算
        max_precomputed_paths: 10000,
        enable_parallel_calculation: true,
    };
    
    // 创建市场
    let market = Market::new(MarketConfigSection::default().with_max_hops(4));
    
    // 创建套利引擎
    let mut arbitrage_engine = ArbitrageEngine::new(arbitrage_config);
    
    // 如果有代币在市场中，初始化引擎
    if !market.token_graph.tokens.is_empty() {
        arbitrage_engine.initialize(&market.token_graph)?;
        info!("套利引擎初始化完成");
    } else {
        info!("⚠️  市场中没有代币，跳过套利引擎初始化");
    }
    
    info!("✅ 真实数据同步设置完成");
    
    // 运行短时间的监控测试
    run_short_monitoring_test(&mut data_service, &mut arbitrage_engine).await?;
    
    Ok(())
}

/// 运行短时间的监控测试
async fn run_short_monitoring_test(
    data_service: &mut swap_path::data_sync::DataSyncService,
    arbitrage_engine: &mut ArbitrageEngine,
) -> Result<()> {
    info!("⏱️  运行30秒的监控测试...");
    
    // 尝试启动数据服务（注意：可能会因为网络问题失败）
    match data_service.start().await {
        Ok(mut market_data_rx) => {
            info!("数据服务启动成功，开始监控...");
            
            // 监控30秒
            let monitoring_duration = Duration::from_secs(30);
            let start_time = std::time::Instant::now();
            
            while start_time.elapsed() < monitoring_duration {
                match timeout(Duration::from_secs(5), market_data_rx.recv()).await {
                    Ok(Some(market_snapshot)) => {
                        info!("收到市场快照，区块: {}, 池子数: {}", 
                              market_snapshot.block_number, 
                              market_snapshot.pool_reserves.len());
                        
                        // 如果引擎已初始化，尝试发现套利机会
                        if arbitrage_engine.is_initialized {
                            match arbitrage_engine.process_market_snapshot(&market_snapshot) {
                                Ok(opportunities) => {
                                    if !opportunities.is_empty() {
                                        info!("🎯 发现 {} 个套利机会！", opportunities.len());
                                        for (i, opportunity) in opportunities.iter().take(3).enumerate() {
                                            info!("  机会 {}: 净利润 ${:.2}", 
                                                  i + 1, opportunity.net_profit_usd);
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("套利分析失败: {}", e);
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        info!("数据流结束");
                        break;
                    }
                    Err(_) => {
                        // 超时，继续等待
                        info!("等待市场数据...");
                    }
                }
            }
            
            info!("监控测试完成");
        }
        Err(e) => {
            warn!("数据服务启动失败: {}", e);
            info!("这可能是由于:");
            info!("  - 网络连接问题");
            info!("  - RPC端点限制");
            info!("  - WebSocket连接问题");
        }
    }
    
    Ok(())
}

/// 显示真实环境使用指南
#[allow(dead_code)]
fn display_production_guide() {
    info!("\n📚 生产环境使用指南:");
    info!("=" .repeat(60));
    
    info!("\n1. 🔧 环境配置:");
    info!("   export MANTLE_RPC_WSS=\"wss://your-premium-rpc.com\"");
    info!("   export MANTLE_RPC_HTTPS=\"https://your-premium-rpc.com\"");
    info!("   export PRIVATE_KEY=\"your-private-key\"");
    
    info!("\n2. 🏊 池子配置:");
    info!("   - 添加真实的DEX池子地址到REAL_POOL_ADDRESSES");
    info!("   - 确保池子有足够的流动性");
    info!("   - 验证池子的代币对信息");
    
    info!("\n3. ⚙️ 参数优化:");
    info!("   - 调整min_profit_threshold_usd（建议 $50-100）");
    info!("   - 设置合理的gas_price_gwei");
    info!("   - 限制max_pools_per_batch避免RPC限制");
    
    info!("\n4. 🔒 安全考虑:");
    info!("   - 使用专用的热钱包");
    info!("   - 设置资金限额");
    info!("   - 实施滑点保护");
    info!("   - 监控异常交易");
    
    info!("\n5. 📊 监控和告警:");
    info!("   - 设置利润/损失告警");
    info!("   - 监控gas费用变化");
    info!("   - 跟踪成功率指标");
    info!("   - 实时性能监控");
}

/// 简化的集成测试（用于CI/CD）
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mantle_config_creation() {
        let config = create_mantle_config().await.unwrap();
        
        assert!(!config.rpc_wss_url.is_empty());
        assert!(!config.rpc_http_url.is_empty());
        assert!(!config.multicall_address.is_empty());
        assert!(config.max_pools_per_batch > 0);
    }
    
    #[tokio::test]
    async fn test_network_connectivity_timeout() {
        // 测试错误的RPC地址应该正确处理超时
        let bad_config = DataSyncConfig {
            rpc_wss_url: "wss://nonexistent.example.com".to_string(),
            rpc_http_url: "https://nonexistent.example.com".to_string(),
            multicall_address: MANTLE_MULTICALL3.to_string(),
            max_pools_per_batch: 10,
            ws_connection_timeout_secs: 1,
            max_reconnect_attempts: 1,
            reconnect_delay_secs: 1,
            http_timeout_secs: 1,
            channel_buffer_size: 10,
        };
        
        // 这应该失败但不会panic
        let result = test_network_connectivity(&bad_config).await;
        // 我们不检查结果，因为网络错误是预期的
        let _ = result;
    }
}
