/// 历史区块套利机会分析器 - 连接真实 Mantle 链
/// 
/// 这个程序连接真实的 Mantle 网络，分析指定区块范围内的套利机会，
/// 输出最佳套利路径和金额等详细信息，但不执行交易。

use swap_path::data_sync::{DataSyncConfig, DataSyncServiceBuilder};
use swap_path::data_sync::multicall::MulticallManager;
use swap_path::logic::{ArbitrageEngine, ArbitrageOpportunity};
use swap_path::logic::types::{ArbitrageConfig, MarketSnapshot};
use swap_path::logic::pools::{MockPool, PoolId};
use swap_path::{PoolWrapper, Token};
use swap_path::data_sync::markets::{Market, MarketConfigSection};
use alloy_primitives::{Address, U256};
use eyre::Result;
use std::sync::Arc;
use tracing::{info, warn, debug};
use std::fs;
use serde::Deserialize;
use std::env;
use std::time::Duration;
use serde_json::Value;

// Mantle 主网配置
const MANTLE_MAINNET_RPC_WSS: &str = "wss://ws.mantle.xyz";
const MANTLE_MAINNET_RPC_HTTPS: &str = "https://rpc.mantle.xyz";
const MANTLE_MULTICALL3: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

// Mantle 主网代币地址 
const WMNT: &str = "0x78c1b0C915C4FAA5FFFa6CAbf0219DA63d7f4cb8";
const METH: &str = "0xcDA86A272531e8640cD7F1a92c01839911B90bb0"; // mETH 地址
const MOE: &str = "0x4515A45337F461A11Ff0FE8aBF3c606AE5dC00c9";  // MOE 代币
const PUFF: &str = "0x26a6b0dcdCfb981362aFA56D581e4A7dBA034fBf"; // PUFF 代币
const MINU: &str = "0x51CfE5b1E764dC253F4c8C1f19a081fF4C3517eD"; // MINU 代币
const LEND: &str = "0x25356aeca4210eF7553140edb9b8026089E49396"; // LEND 代币
const JOE: &str = "0x371c7ec6D8039ff7933a2AA28EB827Ffe1F52f07";  // JOE 代币

// 池子数据结构
#[derive(Debug, Deserialize)]
struct PoolData {
    #[serde(rename = "Pair Name")]
    pair_name: String,
    #[serde(rename = "Pair Address")]
    pair_address: String,
    #[serde(rename = "TokenA Reserves")]
    token_a_reserves: String,
    #[serde(rename = "TokenB Reserves")]
    token_b_reserves: String,
}

// 区块范围配置
#[derive(Debug, Clone)]
struct BlockRangeConfig {
    start_block: u64,
    end_block: u64,
    step: u64, // 采样步长，1表示每个区块都分析
}

// 套利分析结果
#[derive(Debug)]
struct ArbitrageAnalysisResult {
    block_number: u64,
    opportunities: Vec<ArbitrageOpportunity>,
    best_opportunity: Option<ArbitrageOpportunity>,
    total_potential_profit: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    if let Err(e) = dotenvy::dotenv() {
        eprintln!("注意: 无法加载 .env 文件: {}", e);
    }
    
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_line_number(true)
        .init();

    info!("🔍 启动历史区块套利机会分析器 - Mantle 主网");
    info!("🌐 使用真实链上数据 - 连接 Mantle RPC 获取历史池子储备");
    info!("💰 Gas Token: MNT (将获取实时 MNT 价格用于利润计算)");
    info!("{}", "=".repeat(60));
    
    // 解析命令行参数或使用环境变量
    let block_range = parse_block_range().await?;
    
    info!("📊 分析配置:");
    info!("  开始区块: {}", block_range.start_block);
    info!("  结束区块: {}", block_range.end_block);
    info!("  采样步长: {}", block_range.step);
    info!("  总区块数: {}", (block_range.end_block - block_range.start_block + 1) / block_range.step);
    
    // 检查环境变量和配置
    validate_environment()?;
    
    // 创建真实环境配置
    let config = create_live_config()?;
    
    // 设置真实的市场环境
    let (market, pools) = setup_live_market().await?;
    
    // 创建数据同步服务（用于获取历史数据）
    let data_service = DataSyncServiceBuilder::new()
        .with_config(config.clone())
        .with_pools(pools)
        .build()
        .await?;
    
    // 创建套利引擎
    let mut arbitrage_engine = create_live_arbitrage_engine(&market)?;
    
    // 设置详细记录器（默认启用）
    if std::env::var("DISABLE_DETAILED_LOGGING").is_err() {
        info!("📊 启用详细区块记录器...");
        let detail_logger = swap_path::utils::BlockDetailLogger::new("./logs");
        arbitrage_engine.set_detail_logger(detail_logger);
        info!("  详细记录将保存到 ./logs/ 目录");
    } else {
        info!("🔇 详细记录已禁用（设置了 DISABLE_DETAILED_LOGGING）");
    }
    
    // 分析指定区块范围的套利机会
    let analysis_results = analyze_block_range(
        &data_service,
        &mut arbitrage_engine, 
        block_range
    ).await?;
    
    // 显示分析结果
    display_analysis_summary(&analysis_results);
    
    Ok(())
}

/// 解析区块范围配置
async fn parse_block_range() -> Result<BlockRangeConfig> {
    // 优先从命令行参数读取
    let args: Vec<String> = env::args().collect();
    
    if args.len() >= 3 {
        let start_block = args[1].parse::<u64>()
            .map_err(|_| eyre::eyre!("无效的开始区块号: {}", args[1]))?;
        let end_block = args[2].parse::<u64>()
            .map_err(|_| eyre::eyre!("无效的结束区块号: {}", args[2]))?;
        let step = if args.len() >= 4 {
            args[3].parse::<u64>().unwrap_or(1)
        } else {
            1
        };
        
        if start_block > end_block {
            return Err(eyre::eyre!("开始区块不能大于结束区块"));
        }
        
        // 验证区块不会太老，RPC节点可能不支持过于久远的历史状态
        let current_block = get_latest_block_number().await?;
        let max_block_distance = 10000; // 最多查询1万个区块前的数据
        
        // 检查是否是未来区块
        if start_block > current_block {
            warn!("⚠️  警告: 指定的开始区块 ({}) 大于当前区块 ({})！", start_block, current_block);
            warn!("   无法查询未来区块的数据");
            
            let suggested_start = current_block.saturating_sub(1000);
            let suggested_end = current_block;
            warn!("   建议区块范围: {} - {}", suggested_start, suggested_end);
            
            return Err(eyre::eyre!("无法查询未来区块的数据"));
        }
        
        // 检查是否太旧
        let block_distance = current_block - start_block;
        if block_distance > max_block_distance {
            warn!("⚠️  警告: 指定的开始区块 ({}) 距离当前区块 ({}) 太远！", start_block, current_block);
            warn!("   距离: {} 个区块，可能超出 RPC 节点历史状态支持范围", block_distance);
            warn!("   建议使用更近的区块或不指定区块号（使用默认最近1000个区块）");
            
            // 提供替代方案：使用最近的区块
            let suggested_start = current_block.saturating_sub(1000);
            let suggested_end = current_block;
            warn!("   建议区块范围: {} - {}", suggested_start, suggested_end);
            
            return Err(eyre::eyre!("指定的区块范围可能无法获取到历史数据"));
        }
        
        info!("✅ 区块范围验证通过，距离当前区块: {} 个区块", block_distance);
        
        return Ok(BlockRangeConfig {
            start_block,
            end_block,
            step,
        });
    }
    
    // 从环境变量读取
    let start_block = env::var("START_BLOCK")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            info!("未指定 START_BLOCK，使用当前区块减1000");
            0 // 稍后获取当前区块号
        });
    
    let end_block = env::var("END_BLOCK")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0); // 稍后获取当前区块号
        
    let step = env::var("BLOCK_STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    
    // 如果没有指定具体区块，使用最近1000个区块
    if start_block == 0 || end_block == 0 {
        info!("使用默认区块范围：最近1000个区块");
        let current_block = get_latest_block_number().await?;
        return Ok(BlockRangeConfig {
            start_block: current_block.saturating_sub(1000),
            end_block: current_block,
            step,
        });
    }
    
    Ok(BlockRangeConfig {
        start_block,
        end_block,  
        step,
    })
}

/// 获取最新区块号
async fn get_latest_block_number() -> Result<u64> {
    let rpc_url = env::var("RPC_HTTP_URL")
        .or_else(|_| env::var("MANTLE_RPC_HTTPS"))
        .unwrap_or_else(|_| MANTLE_MAINNET_RPC_HTTPS.to_string());
    
    let client = reqwest::Client::new();
    let rpc_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1
    });
    
    let response = client
        .post(&rpc_url)
        .json(&rpc_request)
        .send()
        .await?;
    
    let rpc_response: Value = response.json().await?;
    
    if let Some(result) = rpc_response.get("result") {
        if let Some(block_hex) = result.as_str() {
            let block_number = u64::from_str_radix(&block_hex[2..], 16)
                .map_err(|e| eyre::eyre!("无法解析区块号: {}", e))?;
            Ok(block_number)
        } else {
            Err(eyre::eyre!("RPC响应格式错误"))
        }
    } else {
        Err(eyre::eyre!("RPC调用失败: {:?}", rpc_response))
    }
}

/// 验证环境配置
fn validate_environment() -> Result<()> {
    info!("🔍 验证环境配置...");
    
    // 检查网络连接
    if env::var("OFFLINE_MODE").is_ok() {
        return Err(eyre::eyre!("历史分析模式不支持离线模式"));
    }
    
    // 检查 RPC 配置
    let rpc_https = env::var("RPC_HTTP_URL") 
        .or_else(|_| env::var("MANTLE_RPC_HTTPS"))
        .unwrap_or_else(|_| MANTLE_MAINNET_RPC_HTTPS.to_string());
    
    info!("HTTP RPC: {}", rpc_https);
    
    if rpc_https.contains("localhost") {
        warn!("⚠️  使用本地 RPC，请确保节点正在运行且支持历史数据查询");
    }
    
    // 检查池子数据文件
    if !std::path::Path::new("data/selected/poolLists.csv").exists() {
        warn!("⚠️  未找到 poolLists.csv 文件");
        warn!("请确保 data/selected/poolLists.csv 文件存在");
        warn!("或设置环境变量 POOL_ADDRESSES (逗号分隔)");
    } else {
        info!("✅ 找到池子数据文件: data/selected/poolLists.csv");
    }
    
    info!("✅ 环境验证完成");
    Ok(())
}

/// 创建实时环境配置
fn create_live_config() -> Result<DataSyncConfig> {
    info!("⚙️ 创建环境配置...");
    
    let config = DataSyncConfig {
        rpc_wss_url: env::var("RPC_WSS_URL")
            .or_else(|_| env::var("MANTLE_RPC_WSS"))
            .unwrap_or_else(|_| MANTLE_MAINNET_RPC_WSS.to_string()),
        rpc_http_url: env::var("RPC_HTTP_URL")
            .or_else(|_| env::var("MANTLE_RPC_HTTPS"))
            .unwrap_or_else(|_| MANTLE_MAINNET_RPC_HTTPS.to_string()),
        multicall_address: MANTLE_MULTICALL3.to_string(),
        max_pools_per_batch: 20, // 历史数据查询时降低批次大小
        ws_connection_timeout_secs: 30,
        max_reconnect_attempts: 5,
        reconnect_delay_secs: 3,
        http_timeout_secs: 30, // 历史数据查询可能需要更长时间
        channel_buffer_size: 100,
    };
    
    info!("配置详情:");
    info!("  HTTP: {}", config.rpc_http_url);
    info!("  Multicall: {}", config.multicall_address);
    info!("  批次大小: {}", config.max_pools_per_batch);
    
    Ok(config)
}

/// 设置真实市场环境
async fn setup_live_market() -> Result<(Market, Vec<PoolWrapper>)> {
    info!("🏗️ 设置市场环境...");
    
    // 创建市场配置
    let market_config = MarketConfigSection::default()
        .with_max_hops(4);
    let mut market = Market::new(market_config);
    
    // 添加真实代币
    add_real_tokens(&mut market)?;
    
    // 创建池子
    let pools = create_real_pools().await?;
    
    // 添加池子到市场
    for pool in &pools {
        market.add_pool(pool.clone());
    }
    
    info!("✅ 市场设置完成:");
    info!("  代币数量: {}", market.token_graph.tokens.len());
    info!("  池子数量: {}", pools.len());
    info!("  图节点: {}", market.token_graph.graph.node_count());
    info!("  图边数: {}", market.token_graph.graph.edge_count());
    
    Ok((market, pools))
}

/// 添加真实代币到市场
fn add_real_tokens(market: &mut Market) -> Result<()> {
    info!("💰 添加真实代币...");
    
    let tokens = vec![
        (WMNT, "WMNT", 18),
        (METH, "mETH", 18),
        (MOE, "MOE", 18),
        (PUFF, "PUFF", 18),
        (MINU, "MINU", 18),
        (LEND, "LEND", 18),
        (JOE, "JOE", 18),
    ];
    
    for (address_str, symbol, decimals) in tokens {
        let address = address_str.parse::<Address>()?;
        let token = Token::new_with_data(
            address,
            Some(symbol.to_string()),
            None,
            Some(decimals),
        );
        market.add_token(token);
        info!("  添加代币: {} ({})", symbol, address);
    }
    
    Ok(())
}

/// 创建真实池子
async fn create_real_pools() -> Result<Vec<PoolWrapper>> {
    info!("🏊 创建池子配置...");
    
    let mut pools = Vec::new();
    
    // 优先从环境变量读取
    if let Ok(pool_addresses) = env::var("POOL_ADDRESSES") {
        for addr_str in pool_addresses.split(',') {
            let addr_str = addr_str.trim();
            if let Ok(address) = addr_str.parse::<Address>() {
                let wmnt = WMNT.parse::<Address>()?;
                let meth = METH.parse::<Address>()?;
                
                let mock_pool = MockPool {
                    address,
                    token0: wmnt,
                    token1: meth,
                };
                pools.push(PoolWrapper::new(Arc::new(mock_pool)));
                info!("  添加池子: {}", address);
            }
        }
    } else {
        // 从 CSV 文件读取
        info!("从 CSV 文件加载池子数据...");
        match load_pools_from_csv().await {
            Ok(csv_pools) => {
                if !csv_pools.is_empty() {
                    pools = csv_pools;
                    info!("✅ 从 CSV 文件成功加载了 {} 个池子", pools.len());
                } else {
                    warn!("CSV 文件为空，使用测试池子");
                    pools = create_test_pools()?;
                }
            }
            Err(e) => {
                warn!("CSV 加载失败: {}, 使用测试池子", e);
                pools = create_test_pools()?;
            }
        }
    }
    
    if pools.is_empty() {
        return Err(eyre::eyre!("没有可用的池子"));
    }
    
    info!("✅ 池子配置完成，总数: {}", pools.len());
    Ok(pools)
}

/// 从 CSV 文件加载池子数据
async fn load_pools_from_csv() -> Result<Vec<PoolWrapper>> {
    let csv_path = "data/selected/poolLists.csv";
    let csv_content = fs::read_to_string(csv_path)
        .map_err(|e| eyre::eyre!("无法读取 CSV 文件 {}: {}", csv_path, e))?;
    
    let mut csv_reader = csv::Reader::from_reader(csv_content.as_bytes());
    let mut pools = Vec::new();
    
    for result in csv_reader.deserialize() {
        let pool_data: PoolData = result?;
        
        let pool_address = pool_data.pair_address.parse::<Address>()?;
        let (token0, token1) = parse_token_pair(&pool_data.pair_name)?;
        
        let mock_pool = MockPool {
            address: pool_address,
            token0,
            token1,
        };
        
        pools.push(PoolWrapper::new(Arc::new(mock_pool)));
        debug!("  加载池子: {} ({})", pool_data.pair_name, pool_address);
    }
    
    if pools.is_empty() {
        return Err(eyre::eyre!("CSV 文件中没有有效的池子数据"));
    }
    
    info!("✅ 成功加载 {} 个池子", pools.len());
    Ok(pools)
}

/// 解析代币对名称获取代币地址
fn parse_token_pair(pair_name: &str) -> Result<(Address, Address)> {
    let tokens: Vec<&str> = pair_name.split('-').collect();
    if tokens.len() != 2 {
        return Err(eyre::eyre!("无效的代币对格式: {}", pair_name));
    }
    
    let token0_addr = get_token_address(tokens[0])?;
    let token1_addr = get_token_address(tokens[1])?;
    
    Ok((token0_addr, token1_addr))
}

/// 根据代币符号获取地址
fn get_token_address(symbol: &str) -> Result<Address> {
    let address_str = match symbol {
        "WMNT" => WMNT,
        "mETH" => METH,
        "MOE" => MOE,
        "PUFF" => PUFF,
        "MINU" => MINU,
        "LEND" => LEND,
        "JOE" => JOE,
        _ => return Err(eyre::eyre!("未知代币符号: {}", symbol)),
    };
    
    address_str.parse::<Address>()
        .map_err(|e| eyre::eyre!("无效地址 {} for {}: {}", address_str, symbol, e))
}

/// 创建测试池子
fn create_test_pools() -> Result<Vec<PoolWrapper>> {
    info!("创建测试池子...");
    
    let wmnt = WMNT.parse::<Address>()?;
    let meth = METH.parse::<Address>()?;
    let moe = MOE.parse::<Address>()?;
    let puff = PUFF.parse::<Address>()?;
    
    let pools = vec![
        PoolWrapper::new(Arc::new(MockPool {
            address: "0xa375ea3e1f92d62e3A71B668bAb09f7155267fa3".parse()?,
            token0: wmnt,
            token1: meth,
        })),
        PoolWrapper::new(Arc::new(MockPool {
            address: "0x763868612858358f62b05691dB82Ad35a9b3E110".parse()?,
            token0: moe,
            token1: wmnt,
        })),
        PoolWrapper::new(Arc::new(MockPool {
            address: "0xaCe7A42C030759ea903e9c39AD26a0f9B4a11927".parse()?,
            token0: puff,
            token1: wmnt,
        })),
    ];
    
    info!("创建了 {} 个测试池子", pools.len());
    Ok(pools)
}

/// 创建套利引擎
fn create_live_arbitrage_engine(market: &Market) -> Result<ArbitrageEngine> {
    info!("🧠 创建套利引擎...");
    
    // 从环境变量读取配置，带默认值
    // 基于 MNT 成本设置最小利润门槛：3跳约0.014 MNT，4跳约0.0144 MNT
    // 以美元计算约为 0.014 * $1.1 = $0.0154
    let min_profit_threshold = env::var("MIN_PROFIT_THRESHOLD_USD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.016); // 基于 MNT 成本设置合理门槛
    
    let max_hops = env::var("MAX_HOPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4); // 默认使用 4 跳
    
    let gas_price_gwei = env::var("GAS_PRICE_GWEI")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    
    let config = ArbitrageConfig {
        min_profit_threshold_mnt_wei: U256::from_str_radix(&((min_profit_threshold * 1e18) as u64).to_string(), 10).unwrap(),
        max_hops,
        gas_price_gwei: gas_price_gwei as f64,
        gas_per_transaction: if max_hops <= 3 { 700_000_000 } else { 720_000_000 }, // 3跳约700M，4跳约720M
        max_precomputed_paths: 5000, // 增加路径数量以发现更多WMNT循环机会
        enable_parallel_calculation: true,
    };
    
    info!("✅ 套利引擎配置:");
    let min_profit_mnt = config.min_profit_threshold_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18;
    info!("  最小利润门槛: {:.6} MNT", min_profit_mnt);
    info!("  最大跳数: {}", config.max_hops);
    info!("  Gas每交易: {} (总成本 {}跳约{:.4} MNT)", 
          config.gas_per_transaction, 
          config.max_hops, 
          if config.max_hops <= 3 { 0.014 } else { 0.0144 });
    info!("  并行计算: {}", config.enable_parallel_calculation);
    info!("  Gas Token: MNT (Mantle 网络原生 token)");
    info!("  套利路径限制: 仅 WMNT 起点和终点");
    
    let mut engine = ArbitrageEngine::new(config);
    engine.initialize(&market.token_graph)?;
    
    // 打印所有预计算的套利路径
    print_all_arbitrage_paths(&engine, &market)?;
    
    info!("✅ 套利引擎初始化完成");
    
    Ok(engine)
}

/// 分析指定区块范围的套利机会
async fn analyze_block_range(
    data_service: &swap_path::data_sync::DataSyncService,
    arbitrage_engine: &mut ArbitrageEngine,
    block_range: BlockRangeConfig,
) -> Result<Vec<ArbitrageAnalysisResult>> {
    info!("🔬 开始分析区块范围套利机会...");
    
    let mut analysis_results = Vec::new();
    let total_blocks = (block_range.end_block - block_range.start_block + 1) / block_range.step;
    let mut processed_blocks = 0;
    
    // 分析每个指定的区块
    for block_number in (block_range.start_block..=block_range.end_block).step_by(block_range.step as usize) {
        processed_blocks += 1;
        info!("📊 分析区块 {} ({}/{})", block_number, processed_blocks, total_blocks);
        
        // 获取区块的池子状态
        match get_block_pool_states(data_service, block_number).await {
            Ok(snapshot) => {
                // 分析套利机会
                match arbitrage_engine.process_market_snapshot(&snapshot).await {
                    Ok(opportunities) => {
                        let best_opportunity = find_best_opportunity(&opportunities);
                        let total_potential_profit: f64 = opportunities.iter()
                            .map(|o| o.net_profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18)
                            .sum();
                        
                        let result = ArbitrageAnalysisResult {
                            block_number,
                            opportunities: opportunities.clone(),
                            best_opportunity: best_opportunity.cloned(),
                            total_potential_profit,
                        };
                        
                        if !opportunities.is_empty() {
                            info!("  ✅ 发现 {} 个套利机会，最佳利润: {:.6} MNT",
                                  opportunities.len(),
                                  best_opportunity.map(|o| o.net_profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18).unwrap_or(0.0)
                            );
                            
                            // 显示最佳机会的简要信息
                            if let Some(best) = &best_opportunity {
                                display_opportunity_summary(block_number, best);
                            }
                        } else {
                            debug!("  未发现套利机会");
                        }
                        
                        analysis_results.push(result);
                    }
                    Err(e) => {
                        warn!("区块 {} 套利分析失败: {}", block_number, e);
                    }
                }
            }
            Err(e) => {
                warn!("获取区块 {} 数据失败: {}", block_number, e);
            }
        }
    }
    
    info!("✅ 区块范围分析完成，共处理 {} 个区块", processed_blocks);
    Ok(analysis_results)
}

/// 获取指定区块的池子状态
async fn get_block_pool_states(
    _data_service: &swap_path::data_sync::DataSyncService,
    block_number: u64,
) -> Result<MarketSnapshot> {
    // 在 Mantle 网络上，MNT 是 gas token，所以需要 MNT 价格而不是 ETH 价格
    let mnt_price_usd = get_mnt_price_usd().await.unwrap_or(1.1); // 默认 $1.1
    
    info!("💰 MNT 价格: ${:.3} (用于 gas 成本和利润计算)", mnt_price_usd);
    
    // 创建市场快照，不再需要 ETH 价格，所有成本和利润计算都以 MNT Wei 为单位
    let mut snapshot = MarketSnapshot::new(block_number);
    
    // 获取真实的历史池子储备数据
    get_real_pool_reserves_for_block(&mut snapshot, block_number).await?;
    
    // 打印此区块的所有池子储备情况
    print_block_pool_reserves(&snapshot, block_number);
    
    Ok(snapshot)
}

/// 获取指定区块的真实池子储备数据 - 使用 MulticallManager 批量查询
async fn get_real_pool_reserves_for_block(snapshot: &mut MarketSnapshot, block_number: u64) -> Result<()> {
    info!("🔍 获取区块 {} 的真实池子储备数据...", block_number);
    
    let rpc_url = env::var("RPC_HTTP_URL")
        .or_else(|_| env::var("MANTLE_RPC_HTTPS"))
        .unwrap_or_else(|_| MANTLE_MAINNET_RPC_HTTPS.to_string());
    
    // 创建 MulticallManager
    let multicall_address = Address::parse_checksummed(MANTLE_MULTICALL3, None)
        .map_err(|e| eyre::eyre!("无效的 multicall 地址: {}", e))?;
    
    let multicall_manager = MulticallManager::new(
        multicall_address,
        rpc_url.clone(),
        Duration::from_secs(30), // 30秒超时
    );
    
    // 从CSV读取池子地址列表
    let pool_addresses = get_pool_addresses_from_csv().await?;
    
    info!("📊 开始批量查询 {} 个池子的储备状态...", pool_addresses.len());
    
    // 准备池子ID列表
    let pool_ids: Vec<PoolId> = pool_addresses.iter()
        .map(|(_, address)| PoolId::Address(*address))
        .collect();
    
    // 使用 MulticallManager 批量获取储备数据
    let start_time = std::time::Instant::now();
    match multicall_manager.batch_get_reserves(&pool_ids, Some(block_number)).await {
        Ok(results) => {
            let elapsed = start_time.elapsed();
            info!("✅ 批量查询完成，耗时: {:?}", elapsed);
            
            // 创建池子名称映射
            let name_map: std::collections::HashMap<Address, String> = pool_addresses.iter()
                .map(|(name, addr)| (*addr, name.clone()))
                .collect();
            
            let mut success_count = 0;
            let mut failed_count = 0;
            
            for (pool_id, reserves_opt) in results {
                if let PoolId::Address(address) = pool_id {
                    let default_name = "Unknown Pool".to_string();
                    let pool_name = name_map.get(&address).unwrap_or(&default_name);
                    
                    match reserves_opt {
                        Some((reserve0, reserve1)) => {
                            snapshot.set_pool_reserves(pool_id, reserve0, reserve1);
                            info!("✅ {} ({}): R0={:.6}, R1={:.6}", 
                                  pool_name, 
                                  format!("0x{:x}", address)[..10].to_string() + "...", 
                                  wei_to_ether_f64(reserve0),
                                  wei_to_ether_f64(reserve1));
                            success_count += 1;
                        }
                        None => {
                            warn!("❌ 获取池子 {} ({}) 储备失败", 
                                  pool_name, 
                                  format!("0x{:x}", address));
                            failed_count += 1;
                        }
                    }
                }
            }
            
            info!("📊 批量查询结果: {} 成功, {} 失败", success_count, failed_count);
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            warn!("❌ 批量查询失败，耗时: {:?}, 错误: {}", elapsed, e);
            
            // 如果批量查询失败，回退到逐个查询
            warn!("📢 回退到逐个查询模式...");
            for (pool_name, pool_address) in pool_addresses {
                match get_pool_reserves_at_block(&rpc_url, &pool_address, block_number).await {
                    Ok((reserve0, reserve1)) => {
                        let pool_id = PoolId::Address(pool_address);
                        snapshot.set_pool_reserves(pool_id, reserve0, reserve1);
                        info!("✅ {} ({}): R0={:.6}, R1={:.6}", 
                              pool_name, 
                              format!("0x{:x}", pool_address)[..10].to_string() + "...", 
                              wei_to_ether_f64(reserve0),
                              wei_to_ether_f64(reserve1));
                    }
                    Err(e) => {
                        warn!("❌ 获取池子 {} ({}) 储备失败: {}", 
                              pool_name, 
                              format!("0x{:x}", pool_address), 
                              e);
                        // 继续处理其他池子，不因单个失败而中断
                    }
                }
            }
        }
    }
    
    Ok(())
}

/// 从CSV文件读取池子地址
async fn get_pool_addresses_from_csv() -> Result<Vec<(String, Address)>> {
    let csv_path = "data/selected/poolLists.csv";
    let csv_content = fs::read_to_string(csv_path)
        .map_err(|e| eyre::eyre!("无法读取 CSV 文件 {}: {}", csv_path, e))?;
    
    let mut csv_reader = csv::Reader::from_reader(csv_content.as_bytes());
    let mut pool_addresses = Vec::new();
    
    for result in csv_reader.deserialize() {
        let pool_data: PoolData = result?;
        let pool_address = pool_data.pair_address.parse::<Address>()?;
        pool_addresses.push((pool_data.pair_name.clone(), pool_address));
    }
    
    Ok(pool_addresses)
}

/// 通过RPC获取指定区块的池子储备
async fn get_pool_reserves_at_block(
    rpc_url: &str,
    pool_address: &Address,
    block_number: u64,
) -> Result<(U256, U256)> {
    let client = reqwest::Client::new();
    
    // 构建 getReserves() 调用 (Uniswap V2 类型的池子)
    // getReserves() 方法签名: 0x0902f1ac
    let call_data = "0x0902f1ac";
    
    let rpc_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [
            {
                "to": format!("0x{:x}", pool_address),
                "data": call_data
            },
            format!("0x{:x}", block_number)
        ],
        "id": 1
    });
    
    let response = client
        .post(rpc_url)
        .json(&rpc_request)
        .send()
        .await
        .map_err(|e| eyre::eyre!("RPC 请求失败: {}", e))?;
    
    let rpc_response: Value = response.json().await
        .map_err(|e| eyre::eyre!("解析 RPC 响应失败: {}", e))?;
    
    if let Some(error) = rpc_response.get("error") {
        return Err(eyre::eyre!("RPC 错误: {:?}", error));
    }
    
    let result = rpc_response.get("result")
        .and_then(|r| r.as_str())
        .ok_or_else(|| eyre::eyre!("无效的 RPC 响应格式"))?;
    
    // 解析 getReserves() 返回值
    // 返回格式: (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)
    if result.len() < 194 { // 0x + 64*3 字符
        return Err(eyre::eyre!("getReserves 返回数据长度不足"));
    }
    
    let reserve0_hex = &result[2..66];   // 第一个 32 字节
    let reserve1_hex = &result[66..130]; // 第二个 32 字节
    
    let reserve0 = U256::from_str_radix(reserve0_hex, 16)
        .map_err(|e| eyre::eyre!("解析 reserve0 失败: {}", e))?;
    let reserve1 = U256::from_str_radix(reserve1_hex, 16)
        .map_err(|e| eyre::eyre!("解析 reserve1 失败: {}", e))?;
    
    Ok((reserve0, reserve1))
}

/// 获取 MNT 的实时 USD 价格
async fn get_mnt_price_usd() -> Result<f64> {
    // 优先从环境变量获取固定价格（用于测试）
    if let Ok(price_str) = env::var("MNT_PRICE_USD") {
        if let Ok(price) = price_str.parse::<f64>() {
            return Ok(price);
        }
    }
    
    // 尝试从 CoinGecko API 获取实时价格
    match fetch_mnt_price_from_coingecko().await {
        Ok(price) => {
            info!("📈 从 CoinGecko 获取 MNT 价格: ${:.3}", price);
            Ok(price)
        }
        Err(e) => {
            warn!("⚠️  获取 MNT 价格失败: {}", e);
            warn!("🔄 使用默认 MNT 价格: $1.1");
            Ok(1.1) // Mantle 的大致价格
        }
    }
}

/// 从 CoinGecko API 获取 MNT 价格
async fn fetch_mnt_price_from_coingecko() -> Result<f64> {
    let client = reqwest::Client::new();
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=mantle&vs_currencies=usd";
    
    let response = client
        .get(url)
        .header("User-Agent", "arbitrage-analyzer/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| eyre::eyre!("CoinGecko API 请求失败: {}", e))?;
    
    let json: Value = response.json().await
        .map_err(|e| eyre::eyre!("解析 CoinGecko 响应失败: {}", e))?;
    
    let price = json
        .get("mantle")
        .and_then(|m| m.get("usd"))
        .and_then(|p| p.as_f64())
        .ok_or_else(|| eyre::eyre!("无法解析 MNT 价格数据"))?;
    
    if price <= 0.0 || price > 100.0 { // 合理性检查
        return Err(eyre::eyre!("MNT 价格超出合理范围: ${}", price));
    }
    
    Ok(price)
}

/// 为指定区块添加模拟的池子储备数据（仅用于测试）
#[allow(dead_code)]
fn add_mock_pool_reserves_for_block(snapshot: &mut MarketSnapshot, block_number: u64) {
    // 从CSV文件中读取的所有12个池子的基础储备数据
    let pools_data = vec![
        // PUFF-mETH
        (PoolId::Address("0xae9a0d9b1c9cd31D60FdBfe270CCb8C878bb15c8".parse().unwrap()),
         create_block_varying_reserves(block_number, 34667850634686217287_u128, 602621578158786_u128)),
        // PUFF-WMNT 
        (PoolId::Address("0xaCe7A42C030759ea903e9c39AD26a0f9B4a11927".parse().unwrap()),
         create_block_varying_reserves(block_number, 2769903215739275171380_u128, 196619649067200255745_u128)),
        // MINU-mETH
        (PoolId::Address("0x05C53A5233E7105cAE6c37eE5A7bc7D43131625b".parse().unwrap()),
         create_block_varying_reserves(block_number, 145529855393445386787569_u128, 17797399449290089_u128)),
        // LEND-mETH
        (PoolId::Address("0xFb16B5CCC62dc125834c33BF6B063c87e6e6F581".parse().unwrap()),
         create_block_varying_reserves(block_number, 3668360992173709441453429_u128, 13517468309818090112_u128)),
        // LEND-MOE
        (PoolId::Address("0xB70F7b25fe962EaB2DBd634c756b6f8251764609".parse().unwrap()),
         create_block_varying_reserves(block_number, 4475077464975626981706_u128, 1067698127818181944412_u128)),
        // MOE-MINU
        (PoolId::Address("0xd27492C12826187a804b52d16EE4f74479563cC4".parse().unwrap()),
         create_block_varying_reserves(block_number, 251124640478891581537_u128, 32349250577443717297705_u128)),
        // JOE-MOE
        (PoolId::Address("0xb670D2B452D0Ecc468cccFD532482d45dDdDe2a1".parse().unwrap()),
         create_block_varying_reserves(block_number, 44217251293126494490929_u128, 102855834329834116246522_u128)),
        // MOE-WMNT
        (PoolId::Address("0x763868612858358f62b05691dB82Ad35a9b3E110".parse().unwrap()),
         create_block_varying_reserves(block_number, 7347014593293302598834514_u128, 458257516516593089166328_u128)),
        // WMNT-mETH
        (PoolId::Address("0xa375ea3e1f92d62e3A71B668bAb09f7155267fa3".parse().unwrap()),
         create_block_varying_reserves(block_number, 2889044166597859096884_u128, 711282555534558198_u128)),
        // LEND-WMNT
        (PoolId::Address("0x30ac02b4c99D140CDE2a212ca807CBdA35D4f6b5".parse().unwrap()),
         create_block_varying_reserves(block_number, 84239911918934501172540_u128, 1262725124528695223434_u128)),
        // MINU-WMNT
        (PoolId::Address("0x5126aC4145eD84eBE28cFB34bB6300Bcef492bB7".parse().unwrap()),
         create_block_varying_reserves(block_number, 36707542827073960123070119_u128, 17929811076939897622215_u128)),
        // JOE-WMNT
        (PoolId::Address("0xEFC38C1B0d60725B824EBeE8D431aBFBF12BC953".parse().unwrap()),
         create_block_varying_reserves(block_number, 72668578710121037317301_u128, 10598413415701793352088_u128)),
    ];
    
    info!("🔄 为区块 {} 设置 {} 个池子的储备数据", block_number, pools_data.len());
    
    for (pool_id, (r0, r1)) in pools_data {
        snapshot.set_pool_reserves(pool_id, r0, r1);
    }
}

/// 创建基于区块变化的储备数据
fn create_block_varying_reserves(block_number: u64, base_r0: u128, base_r1: u128) -> (U256, U256) {
    // 根据区块号创建轻微变化的储备量，模拟真实的池子状态变化
    let variation = (block_number % 100) as f64 / 1000.0; // 0-10%的变化
    let r0 = (base_r0 as f64 * (1.0 + variation)) as u128;
    let r1 = (base_r1 as f64 * (1.0 - variation * 0.5)) as u128;
    
    (U256::from(r0), U256::from(r1))
}

/// 找到最佳套利机会
fn find_best_opportunity(opportunities: &[ArbitrageOpportunity]) -> Option<&ArbitrageOpportunity> {
    opportunities.iter()
        .max_by(|a, b| a.net_profit_mnt_wei.partial_cmp(&b.net_profit_mnt_wei).unwrap())
}

/// 显示套利机会摘要信息
fn display_opportunity_summary(_block_number: u64, opportunity: &ArbitrageOpportunity) {
    let input_mnt = wei_to_ether_f64(opportunity.optimal_input_amount);
    let output_mnt = wei_to_ether_f64(opportunity.expected_output_amount);
    let profit_mnt = output_mnt - input_mnt;
    let roi_percent = if input_mnt > 0.0 { (profit_mnt / input_mnt) * 100.0 } else { 0.0 };
    
    info!("    💡 最佳机会: {:.6} MNT → {:.6} MNT (净利润: {:.6} MNT, ROI: {:.1}%)",
          input_mnt, output_mnt, profit_mnt, roi_percent);
    
    // 显示路径
    let path_tokens: Vec<String> = opportunity.path.tokens.iter()
        .map(|token| get_full_token_symbol(token.get_address()))
        .collect();
    info!("    🛤️  路径: {}", path_tokens.join(" → "));
}

/// 显示完整的分析结果汇总
fn display_analysis_summary(results: &[ArbitrageAnalysisResult]) {
    info!("\n📋 套利机会分析报告");
    info!("{}", "=".repeat(80));
    
    let total_blocks = results.len();
    let blocks_with_opportunities: Vec<_> = results.iter()
        .filter(|r| !r.opportunities.is_empty())
        .collect();
    
    let total_opportunities: usize = results.iter()
        .map(|r| r.opportunities.len())
        .sum();
    
    let total_profit: f64 = results.iter()
        .map(|r| r.total_potential_profit)
        .sum();
    
    info!("📊 总体统计:");
    info!("  分析区块数: {}", total_blocks);
    info!("  有机会区块数: {}", blocks_with_opportunities.len());
    info!("  机会覆盖率: {:.1}%", 
          (blocks_with_opportunities.len() as f64 / total_blocks as f64) * 100.0);
    info!("  总套利机会: {}", total_opportunities);
    info!("  累计潜在利润: ${:.2}", total_profit);
    
    if !blocks_with_opportunities.is_empty() {
        let avg_opportunities = total_opportunities as f64 / blocks_with_opportunities.len() as f64;
        let avg_profit = total_profit / blocks_with_opportunities.len() as f64;
        info!("  平均机会/区块: {:.2}", avg_opportunities);
        info!("  平均利润/区块: ${:.2}", avg_profit);
    }
    
    info!("\n🎯 最佳套利机会 Top 5:");
    info!("{}", "-".repeat(80));
    
    // 收集所有最佳机会并排序
    let mut best_opportunities: Vec<_> = results.iter()
        .filter_map(|r| r.best_opportunity.as_ref().map(|o| (r.block_number, o)))
        .collect();
    
    best_opportunities.sort_by(|a, b| b.1.net_profit_mnt_wei.partial_cmp(&a.1.net_profit_mnt_wei).unwrap());
    
    for (i, (block_number, opportunity)) in best_opportunities.iter().take(5).enumerate() {
        let input_mnt = wei_to_ether_f64(opportunity.optimal_input_amount);
        let output_mnt = wei_to_ether_f64(opportunity.expected_output_amount);
        let profit_mnt = output_mnt - input_mnt;
        let roi_percent = if input_mnt > 0.0 { (profit_mnt / input_mnt) * 100.0 } else { 0.0 };
        
        let net_profit_mnt = opportunity.net_profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18;
        info!("{}. 区块 {} - 净利润: {:.6} MNT ({:.6} MNT) | ROI: {:.1}%", 
              i + 1, block_number, net_profit_mnt, profit_mnt, roi_percent);
        
        info!("   输入: {:.6} MNT → 输出: {:.6} MNT", input_mnt, output_mnt);
        let gas_cost_mnt = opportunity.gas_cost_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18;
        info!("   Gas成本: {:.6} MNT | {}-跳路径", 
              gas_cost_mnt, opportunity.path.len());
        
        // 显示路径
        let path_tokens: Vec<String> = opportunity.path.tokens.iter()
            .map(|token| get_full_token_symbol(token.get_address()))
            .collect();
        info!("   路径: {}", path_tokens.join(" → "));
        
        if i < 4 { info!(""); }
    }
    
    info!("\n💡 使用建议:");
    if total_opportunities > 0 {
        let high_profit_threshold = U256::from_str_radix("45000000000000000000", 10).unwrap(); // 45 MNT ≈ $50
        let high_profit_count = results.iter()
            .flat_map(|r| &r.opportunities)
            .filter(|o| o.net_profit_mnt_wei > high_profit_threshold)
            .count();
        
        let medium_profit_threshold = U256::from_str_radix("18000000000000000000", 10).unwrap(); // 18 MNT ≈ $20
        let medium_profit_count = results.iter()
            .flat_map(|r| &r.opportunities)
            .filter(|o| o.net_profit_mnt_wei > medium_profit_threshold && o.net_profit_mnt_wei <= high_profit_threshold)
            .count();
        
        info!("  高价值机会 (>$50): {} 个", high_profit_count);
        info!("  中等价值机会 ($20-$50): {} 个", medium_profit_count);
        info!("  低价值机会 (<$20): {} 个", total_opportunities - high_profit_count - medium_profit_count);
        
        if high_profit_count > 0 {
            info!("  🚀 重点关注高价值机会，优先执行");
        }
        if blocks_with_opportunities.len() as f64 / total_blocks as f64 > 0.3 {
            info!("  📈 套利机会较多，考虑实施自动化策略");
        } else {
            info!("  ⏰ 套利机会稀少，建议增加监控频率或扩大分析范围");
        }
    } else {
        info!("  📉 未发现套利机会，建议:");
        info!("     - 增加最大跳数限制 (MAX_HOPS=4 或更高)");
        info!("     - 扩大代币和池子范围");
        info!("     - 检查池子数据的准确性");
        info!("     - 分析不同的区块范围（可能存在时间相关的机会）");
    }
}

/// 辅助函数：Wei 转 Ether (f64)
fn wei_to_ether_f64(wei: U256) -> f64 {
    wei.to::<u128>() as f64 / 1e18
}

/// 获取完整的代币符号
fn get_full_token_symbol(address: Address) -> String {
    let addr_str = format!("{:?}", address);
    if addr_str.contains(WMNT) { "WMNT".to_string() }
    else if addr_str.contains(METH) { "mETH".to_string() }
    else if addr_str.contains(MOE) { "MOE".to_string() }
    else if addr_str.contains(PUFF) { "PUFF".to_string() }
    else if addr_str.contains(MINU) { "MINU".to_string() }
    else if addr_str.contains(LEND) { "LEND".to_string() }
    else if addr_str.contains(JOE) { "JOE".to_string() }
    else { format!("{}...{}", &addr_str[2..6], &addr_str[addr_str.len()-4..]) }
}

/// 打印所有预计算的套利路径
fn print_all_arbitrage_paths(_engine: &ArbitrageEngine, market: &Market) -> Result<()> {
    info!("📋 所有预计算的套利路径 (以 WMNT 为起点和终点):");
    info!("{}", "=".repeat(60));
    
    let wmnt_address = WMNT.parse::<Address>()?;
    
    // 这里需要访问引擎内部的预计算路径
    // 由于 ArbitrageEngine 可能没有公开路径访问方法，我们手动构建可能的路径
    let mut path_count = 0;
    
    // 获取所有代币
    let tokens: Vec<_> = market.token_graph.tokens.values().collect();
    let wmnt_token = tokens.iter().find(|t| t.get_address() == wmnt_address);
    
    if let Some(_wmnt) = wmnt_token {
        // 2跳路径: WMNT -> Token -> WMNT
        for intermediate_token in tokens.iter() {
            if intermediate_token.get_address() != wmnt_address {
                path_count += 1;
                info!("  {}. WMNT → {} → WMNT", 
                      path_count, 
                      get_full_token_symbol(intermediate_token.get_address()));
            }
        }
        
        // 3跳路径: WMNT -> Token1 -> Token2 -> WMNT  
        for token1 in tokens.iter() {
            if token1.get_address() != wmnt_address {
                for token2 in tokens.iter() {
                    if token2.get_address() != wmnt_address && token2.get_address() != token1.get_address() {
                        path_count += 1;
                        info!("  {}. WMNT → {} → {} → WMNT", 
                              path_count,
                              get_full_token_symbol(token1.get_address()),
                              get_full_token_symbol(token2.get_address()));
                    }
                }
            }
        }
        
        // 4跳路径: WMNT -> Token1 -> Token2 -> Token3 -> WMNT
        for token1 in tokens.iter() {
            if token1.get_address() != wmnt_address {
                for token2 in tokens.iter() {
                    if token2.get_address() != wmnt_address && token2.get_address() != token1.get_address() {
                        for token3 in tokens.iter() {
                            if token3.get_address() != wmnt_address 
                               && token3.get_address() != token1.get_address() 
                               && token3.get_address() != token2.get_address() {
                                path_count += 1;
                                info!("  {}. WMNT → {} → {} → {} → WMNT", 
                                      path_count,
                                      get_full_token_symbol(token1.get_address()),
                                      get_full_token_symbol(token2.get_address()),
                                      get_full_token_symbol(token3.get_address()));
                            }
                        }
                    }
                }
            }
        }
    }
    
    info!("📊 总共找到 {} 条以 WMNT 为起点和终点的套利路径", path_count);
    info!("💰 成本估算:");
    info!("  3跳成本: ~0.014 MNT (700M gas * 0.02 gwei)");
    info!("  4跳成本: ~0.0144 MNT (720M gas * 0.02 gwei)");
    info!("{}", "=".repeat(60));
    
    Ok(())
}

/// 打印指定区块的所有池子储备情况
fn print_block_pool_reserves(snapshot: &MarketSnapshot, block_number: u64) {
    info!("💧 区块 {} 的真实池子储备情况 ({} 个池子):", block_number, snapshot.pool_reserves.len());
    
    // 按池子名称排序以便于阅读
    let mut pools: Vec<_> = snapshot.pool_reserves.iter().collect();
    pools.sort_by_key(|(pool_id, _)| {
        let pool_address = match pool_id {
            PoolId::Address(addr) => format!("0x{:x}", addr),
            PoolId::B256(hash) => format!("0x{:x}", hash),
        };
        get_pool_name_by_address(&pool_address)
    });
    
    for (pool_id, reserves) in pools {
        let pool_address = match pool_id {
            PoolId::Address(addr) => format!("0x{:x}", addr),
            PoolId::B256(hash) => format!("0x{:x}", hash),
        };
        
        let pool_name = get_pool_name_by_address(&pool_address);
        let reserve0_ether = wei_to_ether_f64(reserves.0);
        let reserve1_ether = wei_to_ether_f64(reserves.1);
        
        info!("  {} ({}...)", pool_name, &pool_address[..10]);
        info!("    Reserve0: {:.6} | Reserve1: {:.6}", reserve0_ether, reserve1_ether);
    }
}

/// 根据池子地址获取池子名称
fn get_pool_name_by_address(address: &str) -> String {
    match address {
        // 主要池子（与WMNT直接相关）
        addr if addr.contains("a375ea3e1f92d62e3A71B668bAb09f7155267fa3") => "WMNT-mETH".to_string(),
        addr if addr.contains("763868612858358f62b05691dB82Ad35a9b3E110") => "MOE-WMNT".to_string(),
        addr if addr.contains("aCe7A42C030759ea903e9c39AD26a0f9B4a11927") => "PUFF-WMNT".to_string(),
        addr if addr.contains("30ac02b4c99D140CDE2a212ca807CBdA35D4f6b5") => "LEND-WMNT".to_string(),
        addr if addr.contains("5126aC4145eD84eBE28cFB34bB6300Bcef492bB7") => "MINU-WMNT".to_string(),
        addr if addr.contains("EFC38C1B0d60725B824EBeE8D431aBFBF12BC953") => "JOE-WMNT".to_string(),
        
        // 其他代币对池子
        addr if addr.contains("ae9a0d9b1c9cd31D60FdBfe270CCb8C878bb15c8") => "PUFF-mETH".to_string(),
        addr if addr.contains("05C53A5233E7105cAE6c37eE5A7bc7D43131625b") => "MINU-mETH".to_string(),
        addr if addr.contains("Fb16B5CCC62dc125834c33BF6B063c87e6e6F581") => "LEND-mETH".to_string(),
        addr if addr.contains("B70F7b25fe962EaB2DBd634c756b6f8251764609") => "LEND-MOE".to_string(),
        addr if addr.contains("d27492C12826187a804b52d16EE4f74479563cC4") => "MOE-MINU".to_string(),
        addr if addr.contains("b670D2B452D0Ecc468cccFD532482d45dDdDe2a1") => "JOE-MOE".to_string(),
        
        _ => format!("Unknown Pool ({})", &address[..10]),
    }
}

/// 程序使用说明
fn print_usage_guide() {
    println!("🔧 历史区块套利机会分析器使用指南:");
    println!();
    println!("🌐 数据源: 真实 Mantle 链上数据");
    println!("  - 通过 RPC 调用获取指定区块的真实池子储备");
    println!("  - 调用池子合约的 getReserves() 方法");
    println!("  - 分析真实的历史套利机会");
    println!();
    println!("命令行参数:");
    println!("  cargo run --example historical_arbitrage_analyzer <开始区块> <结束区块> [步长]");
    println!();
    println!("环境变量配置:");
    println!("  MANTLE_RPC_HTTPS        - HTTP RPC 端点 (默认: https://rpc.mantle.xyz)");
    println!("  START_BLOCK             - 开始区块号");
    println!("  END_BLOCK               - 结束区块号"); 
    println!("  BLOCK_STEP              - 采样步长 (默认: 1)");
    println!("  MIN_PROFIT_THRESHOLD_USD - 最小利润门槛 (默认: $0.016)");
    println!("  MAX_HOPS                - 最大跳数 (默认: 4)");
    println!("  GAS_PRICE_GWEI          - Gas价格 (默认: 20)");
    println!("  MNT_PRICE_USD           - 固定 MNT 价格 (默认: 从 CoinGecko 获取)");
    println!();
    println!("套利优化:");
    println!("  - 所有套利路径以 WMNT 为起点和终点");
    println!("  - 3跳成本约 0.014 MNT，4跳成本约 0.0144 MNT");
    println!("  - 利润计算基于 MNT 价值");
    println!("  - 监控 12 个真实 DEX 池子");
    println!();
    println!("示例:");
    println!("  # 分析特定区块范围 (真实数据)");
    println!("  cargo run --example historical_arbitrage_analyzer 84288440 84288460");
    println!();
    println!("  # 分析最近100个区块，每5个区块采样一次");
    println!("  cargo run --example historical_arbitrage_analyzer 84288000 84288100 5");
    println!();
    println!("  # 使用环境变量");
    println!("  START_BLOCK=84288000 END_BLOCK=84288100 cargo run --example historical_arbitrage_analyzer");
    println!();
    println!("⚠️  注意:");
    println!("  - 需要稳定的网络连接到 Mantle RPC");
    println!("  - 历史区块查询可能较慢，建议使用较小的区块范围");
    println!("  - 确保指定的区块号存在且可访问");
}
