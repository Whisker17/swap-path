use reqwest;
use serde_json;
use eyre::Result;

/// 测试是否能获取到真正的历史数据
/// 
/// 这个测试会对比不同区块的数据，验证是否真的获取到了历史状态
#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 测试历史数据获取...");
    
    let rpc_url = "https://rpc.mantle.xyz";
    
    // 测试一个知名的池子地址 (使用历史分析器中的地址)
    let pool_address = "0x76386861d9ad4bad89e5c19e52c47fb0e2dc2de9"; // MOE-WMNT
    
    // 测试不同的区块
    let test_blocks = vec![
        84288430,
        84288440, 
        84288450,
        84288460,
    ];
    
    println!("📊 对比不同区块的储备数据:");
    println!("池子: MOE-WMNT ({})", pool_address);
    println!();
    
    for block_number in test_blocks {
        match get_reserves_at_block(rpc_url, pool_address, block_number).await {
            Ok((reserve0, reserve1)) => {
                println!("区块 {}: R0={}, R1={}", 
                         block_number, 
                         reserve0, 
                         reserve1);
            }
            Err(e) => {
                println!("❌ 区块 {} 查询失败: {}", block_number, e);
            }
        }
    }
    
    println!();
    println!("💡 如果所有区块的数据都相同，说明没有获取到真正的历史数据！");
    
    Ok(())
}

async fn get_reserves_at_block(
    rpc_url: &str,
    pool_address: &str,
    block_number: u64,
) -> Result<(String, String)> {
    let client = reqwest::Client::new();
    
    // getReserves() 方法签名: 0x0902f1ac
    let call_data = "0x0902f1ac";
    
    let rpc_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [
            {
                "to": pool_address,
                "data": call_data
            },
            format!("0x{:x}", block_number)  // 历史区块号
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
    
    println!("  📋 原始返回数据: {}", result);
    println!("  📏 数据长度: {}", result.len());
    
    // 检查是否是空结果 "0x"
    if result == "0x" || result.len() < 10 {
        return Err(eyre::eyre!("返回空数据，可能该地址不是有效的池子合约"));
    }
    
    // 简化处理：只显示原始hex数据
    Ok((result.to_string(), "".to_string()))
}
