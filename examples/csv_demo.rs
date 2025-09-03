/// CSV记录功能演示
/// 
/// 演示如何记录套利机会到CSV文件

use eyre::Result;
use serde::{Serialize};
use std::path::Path;
use chrono;

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

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 CSV记录功能演示");
    println!("==================");

    // 创建一些模拟的套利记录
    let mock_records = create_mock_arbitrage_records();

    // 演示CSV记录功能
    record_arbitrage_records_to_csv(&mock_records).await?;

    // 显示CSV文件内容
    display_csv_contents().await?;

    println!("\n✅ CSV记录功能演示完成！");
    println!("📄 查看生成的文件: arbitrage_opportunities.csv");

    Ok(())
}

/// 创建模拟的套利记录
fn create_mock_arbitrage_records() -> Vec<ArbitrageRecord> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    
    vec![
        ArbitrageRecord {
            timestamp: timestamp.clone(),
            block_number: 84392123,
            path_description: "WMNT → mETH → WMNT".to_string(),
            input_token: "WMNT".to_string(),
            input_amount: "1.000000".to_string(),
            output_token: "WMNT".to_string(),
            output_amount: "1.005250".to_string(),
            net_profit_usd: 10.50,
            roi_percentage: 5.25,
            gas_cost_usd: 15.00,
            pool_addresses: "0xa375ea3e1f92d62e3a71b668bab09f7155267fa3,0x763868612858358f62b05691db82ad35a9b3e110".to_string(),
            hop_count: 2,
            execution_priority: "LOW".to_string(),
        },
        ArbitrageRecord {
            timestamp: timestamp.clone(),
            block_number: 84392124,
            path_description: "WMNT → MOE → mETH → WMNT".to_string(),
            input_token: "WMNT".to_string(),
            input_amount: "2.000000".to_string(),
            output_token: "WMNT".to_string(),
            output_amount: "2.010375".to_string(),
            net_profit_usd: 20.75,
            roi_percentage: 10.375,
            gas_cost_usd: 25.00,
            pool_addresses: "0x763868612858358f62b05691db82ad35a9b3e110,0xa375ea3e1f92d62e3a71b668bab09f7155267fa3".to_string(),
            hop_count: 3,
            execution_priority: "LOW".to_string(),
        },
        ArbitrageRecord {
            timestamp: timestamp.clone(),
            block_number: 84392125,
            path_description: "MOE → WMNT → MOE".to_string(),
            input_token: "MOE".to_string(),
            input_amount: "500.000000".to_string(),
            output_token: "MOE".to_string(),
            output_amount: "501.375000".to_string(),
            net_profit_usd: 2.75,
            roi_percentage: 0.275,
            gas_cost_usd: 5.50,
            pool_addresses: "0x763868612858358f62b05691db82ad35a9b3e110".to_string(),
            hop_count: 2,
            execution_priority: "LOW".to_string(),
        },
        ArbitrageRecord {
            timestamp: timestamp.clone(),
            block_number: 84392126,
            path_description: "WMNT → mETH → WMNT".to_string(),
            input_token: "WMNT".to_string(),
            input_amount: "5.000000".to_string(),
            output_token: "WMNT".to_string(),
            output_amount: "5.250000".to_string(),
            net_profit_usd: 50.00,
            roi_percentage: 25.0,
            gas_cost_usd: 20.00,
            pool_addresses: "0xa375ea3e1f92d62e3a71b668bab09f7155267fa3".to_string(),
            hop_count: 2,
            execution_priority: "MEDIUM".to_string(),
        },
        ArbitrageRecord {
            timestamp: timestamp.clone(),
            block_number: 84392127,
            path_description: "WMNT → MOE → WMNT".to_string(),
            input_token: "WMNT".to_string(),
            input_amount: "1.000000".to_string(),
            output_token: "WMNT".to_string(),
            output_amount: "1.500000".to_string(),
            net_profit_usd: 100.00,
            roi_percentage: 50.0,
            gas_cost_usd: 18.00,
            pool_addresses: "0x763868612858358f62b05691db82ad35a9b3e110".to_string(),
            hop_count: 2,
            execution_priority: "HIGH".to_string(),
        },
    ]
}

/// 将套利记录到CSV文件
async fn record_arbitrage_records_to_csv(records: &[ArbitrageRecord]) -> Result<()> {
    if records.is_empty() {
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
    
    for record in records {
        writer.serialize(&record)?;
    }
    
    writer.flush()?;
    println!("📝 已将 {} 个套利机会记录到 {}", records.len(), csv_file);
    
    Ok(())
}



/// 显示CSV文件内容
async fn display_csv_contents() -> Result<()> {
    println!("\n📊 生成的CSV文件内容:");
    println!("{}", "=".repeat(100));
    
    if let Ok(contents) = std::fs::read_to_string("arbitrage_opportunities.csv") {
        println!("{}", contents);
    } else {
        println!("❌ 无法读取CSV文件");
    }
    
    Ok(())
}
