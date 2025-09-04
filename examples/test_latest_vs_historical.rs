
use reqwest;
use serde_json;
use eyre::Result;

/// 对比 latest 和历史区块的数据，确认是否真的获取到历史数据
#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 对比 latest 和历史区块数据...");
    
    let rpc_url = "https://rpc.mantle.xyz";
    let pool_address = "0x763868612858358f62b05691dB82Ad35a9b3E110"; // MOE-WMNT (正确地址)
    
    println!("📊 测试池子: {}", pool_address);
    println!();
    
    // 1. 先测试 latest 是否有数据
    println!("1️⃣ 测试 latest 区块:");
    match get_reserves(rpc_url, pool_address, None).await {
        Ok(data) => {
            println!("✅ Latest 数据: {}", data);
        }
        Err(e) => {
            println!("❌ Latest 查询失败: {}", e);
        }
    }
    
    println!();
    
    // 2. 测试历史区块（使用更近的区块）
    println!("2️⃣ 测试历史区块:");
    let historical_blocks = vec![84421200, 84421220, 84421240]; // 更近的区块
    
    for block in historical_blocks {
        match get_reserves(rpc_url, pool_address, Some(block)).await {
            Ok(data) => {
                println!("✅ 区块 {}: {}", block, data);
            }
            Err(e) => {
                println!("❌ 区块 {} 失败: {}", block, e);
            }
        }
    }
    
    println!();
    
    // 3. 获取当前区块号
    println!("3️⃣ 获取当前区块号:");
    match get_latest_block_number(rpc_url).await {
        Ok(latest_block) => {
            println!("📍 当前区块号: {}", latest_block);
            
            // 检查我们查询的历史区块是否太旧
            let test_block = 84421200;
            if latest_block > test_block + 1000 {
                println!("⚠️  警告: 查询的区块 ({}) 距离当前区块 ({}) 太远", test_block, latest_block);
                println!("   可能 RPC 节点不保存这么久远的历史状态");
            } else {
                println!("✅ 历史区块距离当前区块合理 (相差: {})", latest_block - test_block);
            }
        }
        Err(e) => {
            println!("❌ 获取当前区块号失败: {}", e);
        }
    }
    
    println!();
    println!("💡 结论:");
    println!("   - 如果 latest 有数据但历史区块返回空，说明 RPC 不支持历史查询");
    println!("   - 如果都返回空，说明池子地址可能不正确");
    
    Ok(())
}

async fn get_reserves(
    rpc_url: &str,
    pool_address: &str,
    block_number: Option<u64>,
) -> Result<String> {
    let client = reqwest::Client::new();
    
    let block_param = match block_number {
        Some(num) => format!("0x{:x}", num),
        None => "latest".to_string(),
    };
    
    let rpc_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [
            {
                "to": pool_address,
                "data": "0x0902f1ac"  // getReserves()
            },
            block_param
        ],
        "id": 1
    });
    
    let response = client
        .post(rpc_url)
        .header("Content-Type", "application/json")
        .json(&rpc_request)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    
    let rpc_result: serde_json::Value = response.json().await?;
    
    if let Some(error) = rpc_result.get("error") {
        return Err(eyre::eyre!("RPC 错误: {}", error));
    }
    
    let result = rpc_result["result"]
        .as_str()
        .ok_or_else(|| eyre::eyre!("无效的 RPC 响应"))?;
    
    if result == "0x" || result.len() < 10 {
        return Err(eyre::eyre!("空数据"));
    }
    
    Ok(result.to_string())
}

async fn get_latest_block_number(rpc_url: &str) -> Result<u64> {
    let client = reqwest::Client::new();
    
    let rpc_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1
    });
    
    let response = client
        .post(rpc_url)
        .header("Content-Type", "application/json")
        .json(&rpc_request)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    
    let rpc_result: serde_json::Value = response.json().await?;
    
    if let Some(error) = rpc_result.get("error") {
        return Err(eyre::eyre!("RPC 错误: {}", error));
    }
    
    let result = rpc_result["result"]
        .as_str()
        .ok_or_else(|| eyre::eyre!("无效的 RPC 响应"))?;
    
    // 解析十六进制区块号
    let block_number = u64::from_str_radix(&result[2..], 16)?;
    
    Ok(block_number)
}
