# 套利机会CSV记录功能

## 📋 功能概述

实现了完整的套利机会记录系统，当监控器发现套利机会时，会自动将详细信息保存到CSV文件中，便于后续分析和审查。

## 🎯 主要特性

### CSV文件结构
生成的 `arbitrage_opportunities.csv` 文件包含以下字段：

| 字段名 | 类型 | 描述 |
|--------|------|------|
| `timestamp` | String | UTC时间戳 (格式: YYYY-MM-DD HH:MM:SS UTC) |
| `block_number` | u64 | 发现套利机会的区块号 |
| `path_description` | String | 套利路径的可读描述 (如: WMNT → mETH → WMNT) |
| `input_token` | String | 输入代币符号 |
| `input_amount` | String | 推荐输入数量 (6位小数) |
| `output_token` | String | 输出代币符号 |
| `output_amount` | String | 预期输出数量 (6位小数) |
| `net_profit_usd` | f64 | 净利润 (USD) |
| `roi_percentage` | f64 | 投资回报率 (%) |
| `gas_cost_usd` | f64 | 预估Gas费用 (USD) |
| `pool_addresses` | String | 涉及的池子地址 (逗号分隔) |
| `hop_count` | usize | 交换跳数 |
| `execution_priority` | String | 执行优先级 (HIGH/MEDIUM/LOW) |

### 执行优先级分类
- **HIGH**: ROI > 50%
- **MEDIUM**: 20% < ROI ≤ 50%  
- **LOW**: ROI ≤ 20%

## 🔧 技术实现

### 核心函数

#### 1. `record_arbitrage_opportunities_to_csv()`
```rust
async fn record_arbitrage_opportunities_to_csv(
    opportunities: &[ArbitrageOpportunity],
    snapshot: &MarketSnapshot,
) -> Result<()>
```
- **功能**: 将套利机会记录到CSV文件
- **特性**: 自动创建文件头、支持追加模式
- **调用时机**: 在 `display_arbitrage_opportunities()` 中异步触发

#### 2. `create_arbitrage_record()`
```rust
fn create_arbitrage_record(
    opportunity: &ArbitrageOpportunity,
    snapshot: &MarketSnapshot,
) -> Result<ArbitrageRecord>
```
- **功能**: 转换 `ArbitrageOpportunity` 为 CSV 记录格式
- **特性**: 包含代币符号转换、ROI计算、优先级判定

#### 3. `get_token_symbol_from_address()`
```rust
fn get_token_symbol_from_address(address: Address) -> String
```
- **功能**: 将代币地址转换为可读符号
- **支持代币**: WMNT, mETH, MOE, PUFF, MINU, LEND, JOE

## 📊 示例输出

```csv
timestamp,block_number,path_description,input_token,input_amount,output_token,output_amount,net_profit_usd,roi_percentage,gas_cost_usd,pool_addresses,hop_count,execution_priority
2025-09-03 15:58:27 UTC,84392123,WMNT → mETH → WMNT,WMNT,1.000000,WMNT,1.005250,10.5,5.25,15.0,0xa375ea3e1f92d62e3a71b668bab09f7155267fa3,2,LOW
2025-09-03 15:58:27 UTC,84392126,WMNT → mETH → WMNT,WMNT,5.000000,WMNT,5.250000,50.0,25.0,20.0,0xa375ea3e1f92d62e3a71b668bab09f7155267fa3,2,MEDIUM
2025-09-03 15:58:27 UTC,84392127,WMNT → MOE → WMNT,WMNT,1.000000,WMNT,1.500000,100.0,50.0,18.0,0x763868612858358f62b05691db82ad35a9b3e110,2,HIGH
```

## 🚀 使用方法

### 在实时监控器中自动记录
当运行 `live_arbitrage_monitor` 时，系统会自动：
1. 检测套利机会
2. 创建/追加 `arbitrage_opportunities.csv` 文件
3. 异步记录详细信息
4. 在日志中显示记录状态

### 演示脚本
运行演示脚本查看CSV功能：
```bash
cargo run --example csv_demo
```

## 📁 文件组织

### 新增文件
- `examples/csv_demo.rs` - CSV功能演示脚本
- `arbitrage_opportunities.csv` - 生成的记录文件

### 修改的文件  
- `examples/live_arbitrage_monitor.rs` - 添加CSV记录功能
- `Cargo.toml` - 添加 `chrono` 依赖

## ⚡ 性能特性

- **异步记录**: 使用 `tokio::spawn` 避免阻塞主监控流程
- **错误容错**: CSV记录失败不影响主要监控功能
- **自动文件管理**: 首次运行自动创建文件头，后续追加记录
- **内存友好**: 流式写入，不缓存大量数据

## 🔍 数据分析建议

生成的CSV文件可用于：

1. **机会统计**: 分析不同时间段的套利机会数量和质量
2. **利润分析**: 计算累计利润、平均ROI等指标  
3. **路径分析**: 研究哪些交易路径最频繁或最有利可图
4. **Gas优化**: 分析不同跳数的gas消耗模式
5. **执行策略**: 根据优先级制定自动化执行策略

## 📈 扩展可能

未来可以考虑添加：
- JSON格式输出选项
- 数据库集成
- 实时dashboard
- 邮件/webhook通知
- 历史数据清理机制

---

✅ **状态**: 已完成并测试通过  
📅 **最后更新**: 2025-09-03  
🔧 **维护者**: Arbitrage Monitor System
