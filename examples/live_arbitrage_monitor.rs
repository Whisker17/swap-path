/// 实时套利监控器 - 连接真实 Mantle 链
/// 
/// 这个程序连接真实的 Mantle 网络，监控链上 DEX 池子的实时状态，
/// 发现并计算真实的套利机会，但不执行交易。

use swap_path::data_sync::{DataSyncConfig, DataSyncServiceBuilder};
use swap_path::logic::{ArbitrageEngine, ArbitrageOpportunity};
use swap_path::logic::types::{ArbitrageConfig, MarketSnapshot};
use swap_path::logic::pools::{MockPool};
use swap_path::{PoolWrapper, Token};
use swap_path::data_sync::markets::{Market, MarketConfigSection};
use swap_path::logic::graph::SwapPathHash;
use alloy_primitives::{Address, U256};
use eyre::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error, debug};
use std::fs;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::collections::HashSet;


// Mantle 主网配置
const MANTLE_MAINNET_RPC_WSS: &str = "wss://ws.mantle.xyz";
const MANTLE_MAINNET_RPC_HTTPS: &str = "https://rpc.mantle.xyz";
const MANTLE_MULTICALL3: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

// Mantle 主网代币地址 (从池子数据中推断)
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

// 套利机会CSV记录结构
#[derive(Debug, Serialize)]
struct ArbitrageRecord {
    timestamp: String,
    block_number: u64,
    path_description: String,
    input_token: String,
    input_amount: String,
    output_token: String,
    output_amount: String,
    net_profit_usd: f64,
    roi_percentage: f64,
    gas_cost_usd: f64,
    pool_addresses: String,
    hop_count: usize,
    execution_priority: String,
}

// 套利机会去重跟踪器
#[derive(Debug)]
struct ArbitrageOpportunityTracker {
    /// 已处理的套利机会路径哈希集合
    processed_opportunities: HashSet<SwapPathHash>,
    /// 可选：设置最大缓存大小以避免内存无限增长
    max_cache_size: usize,
}

impl ArbitrageOpportunityTracker {
    fn new(max_cache_size: usize) -> Self {
        Self {
            processed_opportunities: HashSet::new(),
            max_cache_size,
        }
    }

    /// 检查套利机会是否已被处理，如果没有则标记为已处理
    fn is_new_opportunity(&mut self, opportunity: &ArbitrageOpportunity) -> bool {
        let hash = opportunity.path.swap_path_hash.clone();
        
        // 如果缓存太大，清理一半（简单的LRU策略）
        if self.processed_opportunities.len() >= self.max_cache_size {
            let keys_to_remove: Vec<_> = self.processed_opportunities
                .iter()
                .take(self.max_cache_size / 2)
                .cloned()
                .collect();
            
            for key in keys_to_remove {
                self.processed_opportunities.remove(&key);
            }
            
            debug!("清理套利机会缓存，当前大小: {}", self.processed_opportunities.len());
        }

        // 检查是否是新机会
        self.processed_opportunities.insert(hash)
    }

    /// 过滤出新的套利机会
    fn filter_new_opportunities(&mut self, opportunities: &[ArbitrageOpportunity]) -> Vec<ArbitrageOpportunity> {
        opportunities.iter()
            .filter(|opp| self.is_new_opportunity(opp))
            .cloned()
            .collect()
    }

    /// 获取统计信息
    fn get_stats(&self) -> (usize, usize) {
        (self.processed_opportunities.len(), self.max_cache_size)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 文件
    if let Err(e) = dotenvy::dotenv() {
        // .env 文件不存在是可以的
        eprintln!("注意: 无法加载 .env 文件: {}", e);
    }
    
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_line_number(true)
        .init();

    info!("🌐 启动实时套利监控器 - Mantle 主网");
    info!("{}", "=".repeat(60));
    
    // 检查环境变量和配置
    validate_environment()?;
    
    // 创建真实环境配置
    let config = create_live_config()?;
    
    // 设置真实的市场环境
    let (market, pools) = setup_live_market().await?;
    
    // 创建数据同步服务
    let mut data_service = DataSyncServiceBuilder::new()
        .with_config(config)
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
    
    // 启动实时监控
    run_live_monitoring(&mut data_service, &mut arbitrage_engine).await?;
    
    Ok(())
}

/// 验证环境配置
fn validate_environment() -> Result<()> {
    info!("🔍 验证环境配置...");
    
    // 检查网络连接
    if std::env::var("OFFLINE_MODE").is_ok() {
        warn!("⚠️  检测到离线模式，将使用模拟数据");
        return Ok(());
    }
    
    // 检查 RPC 配置 (优先从 .env 文件读取)
    let rpc_wss = std::env::var("RPC_WSS_URL")
        .or_else(|_| std::env::var("MANTLE_RPC_WSS"))
        .unwrap_or_else(|_| MANTLE_MAINNET_RPC_WSS.to_string());
    let rpc_https = std::env::var("RPC_HTTP_URL") 
        .or_else(|_| std::env::var("MANTLE_RPC_HTTPS"))
        .unwrap_or_else(|_| MANTLE_MAINNET_RPC_HTTPS.to_string());
    
    info!("WebSocket RPC: {}", rpc_wss);
    info!("HTTP RPC: {}", rpc_https);
    
    if rpc_wss.contains("localhost") || rpc_https.contains("localhost") {
        warn!("⚠️  使用本地 RPC，请确保节点正在运行");
    }
    
    // 检查池子配置
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
    info!("⚙️ 创建实时环境配置...");
    
    let config = DataSyncConfig {
        rpc_wss_url: std::env::var("RPC_WSS_URL")
            .or_else(|_| std::env::var("MANTLE_RPC_WSS"))
            .unwrap_or_else(|_| MANTLE_MAINNET_RPC_WSS.to_string()),
        rpc_http_url: std::env::var("RPC_HTTP_URL")
            .or_else(|_| std::env::var("MANTLE_RPC_HTTPS"))
            .unwrap_or_else(|_| MANTLE_MAINNET_RPC_HTTPS.to_string()),
        multicall_address: MANTLE_MULTICALL3.to_string(),
        max_pools_per_batch: 50, // Mantle 网络优化
        ws_connection_timeout_secs: 30,
        max_reconnect_attempts: 10, // 更多重试
        reconnect_delay_secs: 5,
        http_timeout_secs: 20,
        channel_buffer_size: 1000,
    };
    
    info!("配置详情:");
    info!("  WebSocket: {}", config.rpc_wss_url);
    info!("  HTTP: {}", config.rpc_http_url);
    info!("  Multicall: {}", config.multicall_address);
    info!("  批次大小: {}", config.max_pools_per_batch);
    
    Ok(config)
}

/// 设置真实市场环境
async fn setup_live_market() -> Result<(Market, Vec<PoolWrapper>)> {
    info!("🏗️ 设置真实市场环境...");
    
    // 创建市场配置
    let market_config = MarketConfigSection::default()
        .with_max_hops(4);
    let mut market = Market::new(market_config);
    
    // 添加真实代币
    add_real_tokens(&mut market)?;
    
    // 创建池子（使用真实地址或模拟）
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

/// 创建真实池子（或模拟池子用于测试）
async fn create_real_pools() -> Result<Vec<PoolWrapper>> {
    info!("🏊 创建池子配置...");
    
    let mut pools = Vec::new();
    
    // 如果有环境变量中的池子地址，使用它们
    if let Ok(pool_addresses) = std::env::var("POOL_ADDRESSES") {
        for addr_str in pool_addresses.split(',') {
            let addr_str = addr_str.trim();
            if let Ok(address) = addr_str.parse::<Address>() {
                // 使用WMNT和mETH作为默认代币对
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
        // 尝试从 CSV 文件读取真实池子数据
        info!("尝试从 CSV 文件加载真实池子数据...");
        match load_pools_from_csv().await {
            Ok(csv_pools) => {
                if !csv_pools.is_empty() {
                    pools = csv_pools;
                    info!("✅ 从 CSV 文件成功加载了 {} 个真实池子", pools.len());
                } else {
                    warn!("CSV 文件为空，使用测试池子");
                    pools = create_test_pools_for_live_demo()?;
                }
            }
            Err(e) => {
                warn!("CSV 加载失败: {}", e);
                warn!("⚠️  回退到使用测试池子");
                pools = create_test_pools_for_live_demo()?;
            }
        }
    }
    
    if pools.is_empty() {
        error!("❌ 没有可用的池子！");
        error!("请设置环境变量 POOL_ADDRESSES 或在代码中配置 KNOWN_POOL_ADDRESSES");
        return Err(eyre::eyre!("没有可用的池子"));
    }
    
    info!("✅ 池子配置完成，总数: {}", pools.len());
    Ok(pools)
}

/// 从 CSV 文件加载池子数据
async fn load_pools_from_csv() -> Result<Vec<PoolWrapper>> {
    info!("📄 从 CSV 文件加载池子数据...");
    
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
        info!("  加载池子: {} ({}) - {}", 
              pool_data.pair_name, 
              pool_address,
              pool_data.pair_address);
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

/// 将套利机会记录到CSV文件
async fn record_arbitrage_opportunities_to_csv(
    opportunities: &[ArbitrageOpportunity],
    snapshot: &MarketSnapshot,
) -> Result<()> {
    if opportunities.is_empty() {
        return Ok(());
    }

    let csv_file = "arbitrage_opportunities.csv";
    let file_exists = Path::new(csv_file).exists();
    
    // 如果文件不存在，创建并写入表头
    if !file_exists {
        let mut writer = csv::Writer::from_path(csv_file)?;
        
        // 写入表头
        writer.write_record(&[
            "timestamp",
            "block_number", 
            "path_description",
            "input_token",
            "input_amount",
            "output_token", 
            "output_amount",
            "net_profit_usd",
            "roi_percentage",
            "gas_cost_usd",
            "pool_addresses",
            "hop_count",
            "execution_priority",
        ])?;
        writer.flush()?;
    }
    
    // 追加套利机会记录
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(csv_file)?);
    
    for opportunity in opportunities {
        let record = create_arbitrage_record(opportunity, snapshot)?;
        writer.serialize(&record)?;
    }
    
    writer.flush()?;
    info!("📝 已将 {} 个套利机会记录到 {}", opportunities.len(), csv_file);
    
    Ok(())
}

/// 创建套利记录
fn create_arbitrage_record(
    opportunity: &ArbitrageOpportunity,
    snapshot: &MarketSnapshot,
) -> Result<ArbitrageRecord> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    
    // 构建路径描述
    let path_tokens: Vec<String> = opportunity.path.tokens.iter()
        .map(|token| get_full_token_symbol(token.get_address()))
        .collect();
    let path_description = path_tokens.join(" → ");
    
    // 获取输入/输出代币符号
    let input_token = get_full_token_symbol(opportunity.path.tokens[0].get_address());
    let output_token = get_full_token_symbol(
        opportunity.path.tokens[opportunity.path.tokens.len() - 1].get_address()
    );
    
    // 格式化数量
    let input_amount = format!("{:.6}", wei_to_ether_f64(opportunity.optimal_input_amount));
    let output_amount = format!("{:.6}", wei_to_ether_f64(opportunity.expected_output_amount));
    
    // 收集池子地址
    let pool_addresses: Vec<String> = opportunity.path.pools.iter()
        .map(|pool_wrapper| {
            let pool_id = pool_wrapper.get_pool_id();
            match pool_id {
                swap_path::PoolId::Address(addr) => format!("0x{:x}", addr),
                swap_path::PoolId::B256(hash) => format!("0x{:x}", hash),
            }
        })
        .collect();
    let pool_addresses_str = pool_addresses.join(",");
    
    // 计算ROI并确定执行优先级
    let roi = calculate_roi(opportunity);
    let execution_priority = if roi > 50.0 {
        "HIGH"
    } else if roi > 20.0 {
        "MEDIUM"
    } else {
        "LOW"
    }.to_string();
    
    Ok(ArbitrageRecord {
        timestamp,
        block_number: snapshot.block_number,
        path_description,
        input_token,
        input_amount,
        output_token,
        output_amount,
        net_profit_usd: opportunity.net_profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18 * 1.1, // 转换为USD
        roi_percentage: roi,
        gas_cost_usd: opportunity.gas_cost_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18 * 1.1, // 转换为USD
        pool_addresses: pool_addresses_str,
        hop_count: opportunity.path.tokens.len() - 1,
        execution_priority,
    })
}


/// 创建测试池子用于实时演示
fn create_test_pools_for_live_demo() -> Result<Vec<PoolWrapper>> {
    info!("创建测试池子用于实时演示...");
    
    let wmnt = WMNT.parse::<Address>()?;
    let meth = METH.parse::<Address>()?;
    let moe = MOE.parse::<Address>()?;
    let puff = PUFF.parse::<Address>()?;
    
    let pools = vec![
        // WMNT/mETH (真实池子地址)
        PoolWrapper::new(Arc::new(MockPool {
            address: "0xa375ea3e1f92d62e3A71B668bAb09f7155267fa3".parse()?,
            token0: wmnt,
            token1: meth,
        })),
        
        // MOE/WMNT (真实池子地址)
        PoolWrapper::new(Arc::new(MockPool {
            address: "0x763868612858358f62b05691dB82Ad35a9b3E110".parse()?,
            token0: moe,
            token1: wmnt,
        })),
        
        // PUFF/WMNT (真实池子地址)
        PoolWrapper::new(Arc::new(MockPool {
            address: "0xaCe7A42C030759ea903e9c39AD26a0f9B4a11927".parse()?,
            token0: puff,
            token1: wmnt,
        })),
    ];
    
    info!("创建了 {} 个测试池子", pools.len());
    Ok(pools)
}

/// 创建实时套利引擎
fn create_live_arbitrage_engine(market: &Market) -> Result<ArbitrageEngine> {
    info!("🧠 创建实时套利引擎...");
    
    // 从 .env 文件读取配置，带默认值
    let min_profit_threshold = std::env::var("MIN_PROFIT_THRESHOLD_USD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10.0);
    
    let max_hops = std::env::var("MAX_HOPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    
    let gas_price_gwei = std::env::var("GAS_PRICE_GWEI")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    
    // 生产环境的配置
    let config = ArbitrageConfig {
        min_profit_threshold_mnt_wei: U256::from_str_radix(&((min_profit_threshold / 1.1 * 1e18) as u64).to_string(), 10).unwrap(), // 转换为MNT Wei
        max_hops,
        gas_price_gwei: gas_price_gwei as f64,
        gas_per_transaction: 700_000_000,    // 700M gas per transaction
        max_precomputed_paths: 1000,     // 平衡内存使用和覆盖度
        enable_parallel_calculation: true,
    };
    
    info!("✅ 套利引擎配置:");
    let min_profit_mnt = config.min_profit_threshold_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18;
    info!("  最小利润门槛: {:.6} MNT", min_profit_mnt);
    info!("  最大跳数: {}", config.max_hops);
    info!("  并行计算: {}", config.enable_parallel_calculation);
    
    let mut engine = ArbitrageEngine::new(config);
    engine.initialize(&market.token_graph)?;
    
    info!("✅ 套利引擎初始化完成");
    
    Ok(engine)
}

/// 运行实时监控
async fn run_live_monitoring(
    data_service: &mut swap_path::data_sync::DataSyncService,
    arbitrage_engine: &mut ArbitrageEngine,
) -> Result<()> {
    info!("🚀 启动实时监控...");
    info!("按 Ctrl+C 停止监控");
    
    // 启动数据服务
    let mut market_data_rx = match data_service.start().await {
        Ok(rx) => {
            info!("✅ 数据服务启动成功");
            rx
        }
        Err(e) => {
            error!("❌ 数据服务启动失败: {}", e);
            warn!("可能的原因:");
            warn!("  1. 网络连接问题");
            warn!("  2. RPC 端点不可用");
            warn!("  3. WebSocket 连接被拒绝");
            
            // 如果是网络问题，提供离线模式建议
            if e.to_string().contains("connection") || e.to_string().contains("timeout") {
                warn!("💡 建议: 设置环境变量 OFFLINE_MODE=1 进行离线测试");
                return run_offline_demo(arbitrage_engine).await;
            }
            
            return Err(e);
        }
    };
    
    info!("📡 开始监听区块数据...");
    
    // 初始化套利机会去重跟踪器
    let mut opportunity_tracker = ArbitrageOpportunityTracker::new(10000); // 最多缓存10000个已处理的机会
    info!("✅ 套利机会去重系统已启用，缓存大小: {}", opportunity_tracker.max_cache_size);
    
    // 监控统计
    let mut blocks_processed = 0u64;
    let mut total_opportunities = 0u64;
    let mut total_profit_mnt = 0.0f64;
    let mut total_unique_opportunities = 0u64; // 新增：独特机会计数
    let start_time = std::time::Instant::now();
    
    // 主监控循环
    loop {
        tokio::select! {
            // 处理新的市场数据
            market_data = market_data_rx.recv() => {
                match market_data {
                    Some(snapshot) => {
                        blocks_processed += 1;
                        
                        // 分析套利机会
                        match analyze_arbitrage_opportunities(arbitrage_engine, &snapshot).await {
                            Ok(opportunities) => {
                                if !opportunities.is_empty() {
                                    total_opportunities += opportunities.len() as u64;
                                    
                                    // 使用去重跟踪器过滤新的套利机会
                                    let new_opportunities = opportunity_tracker.filter_new_opportunities(&opportunities);
                                    
                                    if !new_opportunities.is_empty() {
                                        total_unique_opportunities += new_opportunities.len() as u64;
                                        let block_profit: f64 = new_opportunities.iter()
                                            .map(|o| calculate_profit_in_mnt(o))
                                            .sum();
                                        total_profit_mnt += block_profit;
                                        
                                        // 只显示和记录新的套利机会
                                        display_arbitrage_opportunities(&snapshot, &new_opportunities);
                                    } else {
                                        debug!("区块 {} - 发现 {} 个套利机会，但都是重复的", 
                                              snapshot.block_number, opportunities.len());
                                    }
                                }
                                
                                // 每10个区块显示统计信息
                                if blocks_processed % 10 == 0 {
                                    display_monitoring_stats_with_dedup(
                                        blocks_processed,
                                        total_opportunities,
                                        total_unique_opportunities,
                                        total_profit_mnt,
                                        start_time.elapsed(),
                                        &opportunity_tracker,
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("套利分析失败: {}", e);
                            }
                        }
                    }
                    None => {
                        warn!("数据流结束");
                        break;
                    }
                }
            }
            
            // 处理 Ctrl+C 信号
            _ = tokio::signal::ctrl_c() => {
                info!("收到停止信号，正在关闭...");
                break;
            }
        }
    }
    
    // 显示最终统计
    display_final_stats_with_dedup(
        blocks_processed, 
        total_opportunities, 
        total_unique_opportunities, 
        total_profit_mnt, 
        start_time.elapsed(),
        &opportunity_tracker
    );
    
    Ok(())
}

/// 分析套利机会
async fn analyze_arbitrage_opportunities(
    engine: &mut ArbitrageEngine,
    snapshot: &MarketSnapshot,
) -> Result<Vec<swap_path::logic::ArbitrageOpportunity>> {
    debug!("分析区块 {} 的套利机会", snapshot.block_number);
    
    let opportunities = engine.process_market_snapshot(snapshot).await?;
    
    if !opportunities.is_empty() {
        debug!("区块 {} 发现 {} 个套利机会", snapshot.block_number, opportunities.len());
    }
    
    Ok(opportunities)
}

/// 显示发现的新套利机会（已去重）
fn display_arbitrage_opportunities(
    snapshot: &MarketSnapshot,
    opportunities: &[swap_path::logic::ArbitrageOpportunity],
) {
    info!("🎯 区块 {} - 发现 {} 个新套利机会", snapshot.block_number, opportunities.len());
    
    // 记录新套利机会到CSV文件
    tokio::spawn({
        let opportunities = opportunities.to_vec();
        let snapshot = snapshot.clone();
        async move {
            if let Err(e) = record_arbitrage_opportunities_to_csv(&opportunities, &snapshot).await {
                warn!("记录套利机会到CSV失败: {}", e);
            }
        }
    });
    
    for (i, opportunity) in opportunities.iter().take(3).enumerate() {
        let profit_mnt = calculate_profit_in_mnt(opportunity);
        let input_mnt = wei_to_ether_f64(opportunity.optimal_input_amount);
        let output_mnt = wei_to_ether_f64(opportunity.expected_output_amount);
        let gas_cost_mnt = opportunity.gas_cost_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18;
        let roi_percent = if input_mnt > 0.0 { (profit_mnt / input_mnt) * 100.0 } else { 0.0 };
        
        info!("  {}. 净利润: {:.6} MNT | ROI: {:.1}% | 路径: {}-跳",
              i + 1,
              profit_mnt,
              roi_percent,
              opportunity.path.len());
        
        // 显示具体的执行建议
        info!("     推荐输入: {:.6} MNT",
              input_mnt);
        info!("     预期产出: {:.6} MNT",
              output_mnt);
        info!("     Gas成本: {:.6} MNT (${:.2})",
              gas_cost_mnt,
              gas_cost_mnt * 1.1); // 转换为 USD 显示
        
        // 显示完整的路径信息
        let path_tokens: Vec<String> = opportunity.path.tokens.iter()
            .map(|token| get_full_token_symbol(token.get_address()))
            .collect();
        info!("     路径: {}", path_tokens.join(" → "));
    }
    
    if opportunities.len() > 3 {
        info!("     ...还有 {} 个机会", opportunities.len() - 3);
    }
}

/// 显示监控统计信息
fn display_monitoring_stats(
    blocks_processed: u64,
    total_opportunities: u64,
    total_profit_usd: f64,
    elapsed: Duration,
) {
    let avg_opportunities_per_block = if blocks_processed > 0 {
        total_opportunities as f64 / blocks_processed as f64
    } else {
        0.0
    };
    
    info!("📊 监控统计 (已运行 {:?}):", elapsed);
    info!("  已处理区块: {}", blocks_processed);
    info!("  总套利机会: {}", total_opportunities);
    info!("  平均机会/区块: {:.2}", avg_opportunities_per_block);
    info!("  累计潜在利润: ${:.2}", total_profit_usd);
}

/// 显示最终统计
fn display_final_stats(
    blocks_processed: u64,
    total_opportunities: u64,
    total_profit_usd: f64,
    total_elapsed: Duration,
) {
    info!("📋 最终统计报告:");
    info!("{}", "=".repeat(50));
    info!("  总运行时间: {:?}", total_elapsed);
    info!("  处理区块数: {}", blocks_processed);
    info!("  发现套利机会: {}", total_opportunities);
    info!("  累计潜在利润: ${:.2}", total_profit_usd);
    
    if blocks_processed > 0 {
        let blocks_per_minute = blocks_processed as f64 / (total_elapsed.as_secs() as f64 / 60.0);
        let opportunities_per_hour = total_opportunities as f64 / (total_elapsed.as_secs() as f64 / 3600.0);
        
        info!("  处理速度: {:.1} 区块/分钟", blocks_per_minute);
        info!("  机会发现率: {:.1} 机会/小时", opportunities_per_hour);
    }
    
    if total_opportunities > 0 {
        let avg_profit = total_profit_usd / total_opportunities as f64;
        info!("  平均单笔利润: ${:.2}", avg_profit);
    }
}

/// 显示简化的监控统计信息
fn display_monitoring_stats_with_dedup(
    blocks_processed: u64,
    _total_opportunities: u64,
    unique_opportunities: u64,
    total_profit_mnt: f64,
    elapsed: Duration,
    _tracker: &ArbitrageOpportunityTracker,
) {
    info!("📊 监控统计 (已运行 {:?}):", elapsed);
    info!("  已处理区块: {}", blocks_processed);
    info!("  套利机会: {}", unique_opportunities);
    info!("  累计潜在利润: {:.6} MNT", total_profit_mnt);
}

/// 显示简化的最终统计
fn display_final_stats_with_dedup(
    blocks_processed: u64,
    _total_opportunities: u64,
    unique_opportunities: u64,
    total_profit_mnt: f64,
    total_elapsed: Duration,
    _tracker: &ArbitrageOpportunityTracker,
) {
    info!("📋 最终统计报告:");
    info!("{}", "=".repeat(40));
    info!("  总运行时间: {:?}", total_elapsed);
    info!("  处理区块数: {}", blocks_processed);
    info!("  套利机会: {} 个", unique_opportunities);
    info!("  累计潜在利润: {:.6} MNT", total_profit_mnt);
    
    if blocks_processed > 0 {
        let blocks_per_minute = blocks_processed as f64 / (total_elapsed.as_secs() as f64 / 60.0);
        let opportunities_per_hour = unique_opportunities as f64 / (total_elapsed.as_secs() as f64 / 3600.0);
        
        info!("  处理速度: {:.1} 区块/分钟", blocks_per_minute);
        info!("  机会发现率: {:.1} 机会/小时", opportunities_per_hour);
    }
    
    if unique_opportunities > 0 {
        let avg_profit = total_profit_mnt / unique_opportunities as f64;
        info!("  平均单笔利润: {:.6} MNT", avg_profit);
    }
}

/// 离线演示模式
async fn run_offline_demo(arbitrage_engine: &mut ArbitrageEngine) -> Result<()> {
    warn!("🔄 启动离线演示模式...");
    
    // 创建一些模拟的市场快照
    let demo_snapshots = create_demo_snapshots();
    
    for (i, snapshot) in demo_snapshots.iter().enumerate() {
        info!("📊 处理演示快照 {} (区块 {})", i + 1, snapshot.block_number);
        
        let opportunities = arbitrage_engine.process_market_snapshot(snapshot).await?;
        
        if !opportunities.is_empty() {
            display_arbitrage_opportunities(snapshot, &opportunities);
        } else {
            info!("  未发现套利机会");
        }
        
        // 模拟区块间隔
        sleep(Duration::from_secs(2)).await;
    }
    
    info!("✅ 离线演示完成");
    Ok(())
}

/// 创建演示快照
fn create_demo_snapshots() -> Vec<MarketSnapshot> {
    use swap_path::logic::pools::PoolId;
    
    let mut snapshots = Vec::new();
    
    for i in 0..5 {
        let mut snapshot = MarketSnapshot::new(12345 + i);
        
        // 添加一些模拟的池子数据
            // 使用真实池子地址的演示数据
    let pools_data = vec![
        (PoolId::Address("0xa375ea3e1f92d62e3A71B668bAb09f7155267fa3".parse().unwrap()), // WMNT-mETH
         (U256::from_str_radix("2889044166597859096884", 10).unwrap(),
          U256::from_str_radix("711282555534558198", 10).unwrap())),
        (PoolId::Address("0x763868612858358f62b05691dB82Ad35a9b3E110".parse().unwrap()), // MOE-WMNT
         (U256::from_str_radix("7347014593293302598834514", 10).unwrap(),
          U256::from_str_radix("458257516516593089166328", 10).unwrap())),
        (PoolId::Address("0xaCe7A42C030759ea903e9c39AD26a0f9B4a11927".parse().unwrap()), // PUFF-WMNT
         (U256::from_str_radix("2769903215739275171380", 10).unwrap(),
          U256::from_str_radix("196619649067200255745", 10).unwrap())),
    ];
        
        for (pool_id, (r0, r1)) in pools_data {
            snapshot.set_pool_reserves(pool_id, r0, r1);
        }
        
        snapshots.push(snapshot);
    }
    
    snapshots
}

/// 辅助函数：计算投资回报率
fn calculate_roi(opportunity: &swap_path::logic::ArbitrageOpportunity) -> f64 {
    let input_mnt = wei_to_ether_f64(opportunity.optimal_input_amount);
    if input_mnt > 0.0 {
        let net_profit_mnt = opportunity.net_profit_mnt_wei.to_string().parse::<f64>().unwrap_or(0.0) / 1e18;
        (net_profit_mnt / input_mnt) * 100.0
    } else {
        0.0
    }
}

/// 辅助函数：Wei 转 Ether (f64)
fn wei_to_ether_f64(wei: U256) -> f64 {
    wei.to::<u128>() as f64 / 1e18
}

/// 计算MNT形式的利润
fn calculate_profit_in_mnt(opportunity: &ArbitrageOpportunity) -> f64 {
    // 计算输出与输入的差值（以MNT为单位）
    let input_mnt = wei_to_ether_f64(opportunity.optimal_input_amount);
    let output_mnt = wei_to_ether_f64(opportunity.expected_output_amount);
    output_mnt - input_mnt
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

/// 程序使用说明
#[allow(dead_code)]
fn print_usage_guide() {
    println!("🔧 实时套利监控器使用指南:");
    println!();
    println!("环境变量配置:");
    println!("  MANTLE_RPC_WSS      - WebSocket RPC 端点");
    println!("  MANTLE_RPC_HTTPS    - HTTP RPC 端点");
    println!("  POOL_ADDRESSES      - 池子地址列表(逗号分隔)");
    println!("  OFFLINE_MODE        - 离线演示模式");
    println!();
    println!("示例:");
    println!("  export MANTLE_RPC_WSS='wss://your-node.com'");
    println!("  export POOL_ADDRESSES='0x123...,0x456...,0x789...'");
    println!("  cargo run --example live_arbitrage_monitor");
    println!();
    println!("离线测试:");
    println!("  OFFLINE_MODE=1 cargo run --example live_arbitrage_monitor");
}
