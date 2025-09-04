use crate::logic::types::{ArbitrageOpportunity, MarketSnapshot, ProfitCalculationResult};
use crate::logic::graph::SwapPath;
use crate::logic::pools::PoolId;
use alloy_primitives::U256;
use eyre::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

/// 区块处理详细信息记录
#[derive(Debug, Clone, Serialize)]
pub struct BlockDetailRecord {
    /// 区块号
    pub block_number: u64,
    /// 处理时间戳
    pub timestamp: u64,
    /// 市场快照信息
    pub total_pools: usize,
    pub enabled_pools: usize,
    pub pools_with_data: usize,
    /// 预计算路径统计
    pub total_precomputed_paths: usize,
    /// 利润计算结果统计
    pub successful_calculations: usize,
    pub failed_calculations: usize,
    pub calculation_duration_ms: u64,
    /// 套利机会统计
    pub total_opportunities_found: usize,
    pub profitable_opportunities: usize,
    pub max_profit_mnt: f64,
    pub total_potential_profit_mnt: f64,
    /// 最佳套利机会详情（如果存在）
    pub best_opportunity_path: Option<String>,
    pub best_opportunity_input_amount: Option<String>,
    pub best_opportunity_profit: Option<f64>,
    pub best_opportunity_margin: Option<f64>,
    /// 处理性能指标
    pub processing_duration_ms: u64,
    pub paths_per_second: f64,
}

/// 套利机会详细记录
#[derive(Debug, Clone, Serialize)]
pub struct OpportunityDetailRecord {
    /// 区块号
    pub block_number: u64,
    /// 机会发现时间戳
    pub timestamp: u64,
    /// 路径详情
    pub path_description: String,
    pub path_hops: usize,
    pub path_tokens: String,
    /// 金额和利润
    pub optimal_input_amount: String,
    pub expected_output_amount: String,
    pub gross_profit_mnt: f64,
    pub gas_cost_mnt: f64,
    pub net_profit_mnt: f64,
    pub profit_margin_percent: f64,
    /// 路径分析
    pub involved_pools: String,
    pub liquidity_score: f64,
}

/// 池子储备详细记录
#[derive(Debug, Clone, Serialize)]
pub struct PoolReserveRecord {
    /// 区块号
    pub block_number: u64,
    /// 时间戳
    pub timestamp: u64,
    /// 池子地址
    pub pool_address: String,
    /// 池子ID
    pub pool_id: String,
    /// Token0 储备
    pub reserve0: String,
    /// Token1 储备
    pub reserve1: String,
    /// Token0 储备 (MNT 格式)
    pub reserve0_mnt: f64,
    /// Token1 储备 (MNT 格式)
    pub reserve1_mnt: f64,
    /// 总流动性 (MNT)
    pub total_liquidity_mnt: f64,
    /// 池子协议
    pub protocol: String,
    /// 是否启用
    pub is_enabled: bool,
    /// 储备变化类型 (新增/更新/无变化)
    pub change_type: String,
}

/// 区块详细信息CSV记录器
pub struct BlockDetailLogger {
    /// 区块总览CSV文件路径
    block_summary_file: String,
    /// 套利机会详情CSV文件路径
    opportunity_details_file: String,
    /// 池子储备详情CSV文件路径
    pool_reserves_file: String,
    /// 上一次记录的池子储备状态（用于检测变化）
    last_pool_reserves: HashMap<PoolId, (U256, U256)>,
    /// 是否已初始化文件头
    headers_written: bool,
}

impl BlockDetailLogger {
    /// 创建新的区块详细记录器
    pub fn new(output_dir: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        Self {
            block_summary_file: format!("{}/block_details_{}.csv", output_dir, timestamp),
            opportunity_details_file: format!("{}/opportunity_details_{}.csv", output_dir, timestamp),
            pool_reserves_file: format!("{}/pool_reserves_{}.csv", output_dir, timestamp),
            last_pool_reserves: HashMap::new(),
            headers_written: false,
        }
    }

    /// 记录区块处理的详细信息
    pub async fn log_block_processing(
        &mut self,
        market_snapshot: &MarketSnapshot,
        precomputed_paths: &[SwapPath],
        calculation_results: &[ProfitCalculationResult],
        opportunities: &[ArbitrageOpportunity],
        processing_start: Instant,
        calculation_duration: Duration,
    ) -> Result<()> {
        let processing_duration = processing_start.elapsed();
        
        // 计算统计信息
        let successful_calculations = calculation_results.iter().filter(|r| r.calculation_successful).count();
        let failed_calculations = calculation_results.len() - successful_calculations;
        
        let total_potential_profit: f64 = opportunities.iter()
            .map(|o| self.u256_to_mnt_f64(o.net_profit_mnt_wei))
            .sum();
        
        let max_profit = opportunities.iter()
            .map(|o| self.u256_to_mnt_f64(o.net_profit_mnt_wei))
            .fold(0.0f64, f64::max);
        
        let best_opportunity = opportunities.iter()
            .max_by(|a, b| a.net_profit_mnt_wei.cmp(&b.net_profit_mnt_wei));
        
        let paths_per_second = if processing_duration.as_secs_f64() > 0.0 {
            precomputed_paths.len() as f64 / processing_duration.as_secs_f64()
        } else {
            0.0
        };

        // 创建区块记录
        let block_record = BlockDetailRecord {
            block_number: market_snapshot.block_number,
            timestamp: market_snapshot.timestamp,
            total_pools: market_snapshot.total_pools_count,
            enabled_pools: market_snapshot.enabled_pools.len(),
            pools_with_data: market_snapshot.enabled_pools_with_data_count(),
            total_precomputed_paths: precomputed_paths.len(),
            successful_calculations,
            failed_calculations,
            calculation_duration_ms: calculation_duration.as_millis() as u64,
            total_opportunities_found: opportunities.len(),
            profitable_opportunities: opportunities.iter().filter(|o| !o.net_profit_mnt_wei.is_zero()).count(),
            max_profit_mnt: max_profit,
            total_potential_profit_mnt: total_potential_profit,
            best_opportunity_path: best_opportunity.map(|o| self.format_swap_path(&o.path)),
            best_opportunity_input_amount: best_opportunity.map(|o| o.optimal_input_amount.to_string()),
            best_opportunity_profit: best_opportunity.map(|o| self.u256_to_mnt_f64(o.net_profit_mnt_wei)),
            best_opportunity_margin: best_opportunity.map(|o| o.profit_margin_percent),
            processing_duration_ms: processing_duration.as_millis() as u64,
            paths_per_second,
        };

        // 写入区块总览
        self.write_block_summary_record(&block_record).await?;

        // 写入所有套利机会详情
        for opportunity in opportunities {
            let opportunity_record = OpportunityDetailRecord {
                block_number: market_snapshot.block_number,
                timestamp: market_snapshot.timestamp,
                path_description: self.format_swap_path(&opportunity.path),
                path_hops: opportunity.path.pools.len(),
                path_tokens: self.format_path_tokens(&opportunity.path),
                optimal_input_amount: opportunity.optimal_input_amount.to_string(),
                expected_output_amount: opportunity.expected_output_amount.to_string(),
                gross_profit_mnt: self.u256_to_mnt_f64(opportunity.gross_profit_mnt_wei),
                gas_cost_mnt: self.u256_to_mnt_f64(opportunity.gas_cost_mnt_wei),
                net_profit_mnt: self.u256_to_mnt_f64(opportunity.net_profit_mnt_wei),
                profit_margin_percent: opportunity.profit_margin_percent,
                involved_pools: self.format_involved_pools(&opportunity.path),
                liquidity_score: self.calculate_liquidity_score(&opportunity.path, market_snapshot),
            };
            
            self.write_opportunity_detail_record(&opportunity_record).await?;
        }

        // 写入所有池子储备详情
        self.log_pool_reserves(market_snapshot).await?;

        debug!("已记录区块 {} 的详细信息: {} 个机会, {:.6} MNT 总利润", 
               market_snapshot.block_number, opportunities.len(), total_potential_profit);

        Ok(())
    }

    /// 记录未发现机会的区块（但仍然检查池子储备变化）
    pub async fn log_empty_block(
        &mut self,
        market_snapshot: &MarketSnapshot,
        precomputed_paths: &[SwapPath],
        calculation_results: &[ProfitCalculationResult],
        processing_start: Instant,
        calculation_duration: Duration,
    ) -> Result<()> {
        self.log_block_processing(
            market_snapshot,
            precomputed_paths,
            calculation_results,
            &[], // 空的机会列表
            processing_start,
            calculation_duration,
        ).await
    }

    /// 写入区块总览记录
    async fn write_block_summary_record(&mut self, record: &BlockDetailRecord) -> Result<()> {
        // 确保文件存在并写入头部
        self.ensure_block_summary_headers().await?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.block_summary_file)
            .await?;

        let csv_line = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6},{},{},{:.6},{:.6},{},{:.2}\n",
            record.block_number,
            record.timestamp,
            record.total_pools,
            record.enabled_pools,
            record.pools_with_data,
            record.total_precomputed_paths,
            record.successful_calculations,
            record.failed_calculations,
            record.calculation_duration_ms,
            record.total_opportunities_found,
            record.profitable_opportunities,
            record.max_profit_mnt,
            record.total_potential_profit_mnt,
            record.best_opportunity_path.as_deref().unwrap_or(""),
            record.best_opportunity_input_amount.as_deref().unwrap_or(""),
            record.best_opportunity_profit.unwrap_or(0.0),
            record.best_opportunity_margin.unwrap_or(0.0),
            record.processing_duration_ms,
            record.paths_per_second,
        );

        file.write_all(csv_line.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    /// 写入套利机会详情记录
    async fn write_opportunity_detail_record(&mut self, record: &OpportunityDetailRecord) -> Result<()> {
        // 确保文件存在并写入头部
        self.ensure_opportunity_details_headers().await?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.opportunity_details_file)
            .await?;

        let csv_line = format!(
            "{},{},\"{}\",{},\"{}\",{},{},{:.6},{:.6},{:.6},{:.6},\"{}\",{:.6}\n",
            record.block_number,
            record.timestamp,
            record.path_description,
            record.path_hops,
            record.path_tokens,
            record.optimal_input_amount,
            record.expected_output_amount,
            record.gross_profit_mnt,
            record.gas_cost_mnt,
            record.net_profit_mnt,
            record.profit_margin_percent,
            record.involved_pools,
            record.liquidity_score,
        );

        file.write_all(csv_line.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    /// 确保区块总览文件有正确的头部
    async fn ensure_block_summary_headers(&mut self) -> Result<()> {
        if Path::new(&self.block_summary_file).exists() {
            return Ok(());
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&self.block_summary_file)
            .await?;

        let header = "block_number,timestamp,total_pools,enabled_pools,pools_with_data,total_precomputed_paths,successful_calculations,failed_calculations,calculation_duration_ms,total_opportunities_found,profitable_opportunities,max_profit_mnt,total_potential_profit_mnt,best_opportunity_path,best_opportunity_input_amount,best_opportunity_profit,best_opportunity_margin,processing_duration_ms,paths_per_second\n";
        
        file.write_all(header.as_bytes()).await?;
        file.flush().await?;

        info!("创建区块详情记录文件: {}", self.block_summary_file);
        Ok(())
    }

    /// 确保套利机会详情文件有正确的头部
    async fn ensure_opportunity_details_headers(&mut self) -> Result<()> {
        if Path::new(&self.opportunity_details_file).exists() {
            return Ok(());
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&self.opportunity_details_file)
            .await?;

        let header = "block_number,timestamp,path_description,path_hops,path_tokens,optimal_input_amount,expected_output_amount,gross_profit_mnt,gas_cost_mnt,net_profit_mnt,profit_margin_percent,involved_pools,liquidity_score\n";
        
        file.write_all(header.as_bytes()).await?;
        file.flush().await?;

        info!("创建套利机会详情记录文件: {}", self.opportunity_details_file);
        Ok(())
    }

    /// 将U256转换为MNT的f64表示
    fn u256_to_mnt_f64(&self, value: U256) -> f64 {
        value.to_string().parse::<f64>().unwrap_or(0.0) / 1e18
    }

    /// 格式化交换路径为可读字符串
    fn format_swap_path(&self, path: &SwapPath) -> String {
        path.pools.iter()
            .map(|pool_wrapper| format!("Pool_{}", pool_wrapper.get_pool_id()))
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    /// 格式化路径中的代币
    fn format_path_tokens(&self, path: &SwapPath) -> String {
        path.tokens.iter()
            .map(|token| format!("Token_{}", token.get_address()))
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    /// 格式化涉及的池子
    fn format_involved_pools(&self, path: &SwapPath) -> String {
        path.pools.iter()
            .map(|pool_wrapper| pool_wrapper.get_pool_id().to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// 计算路径的流动性评分
    fn calculate_liquidity_score(&self, path: &SwapPath, market_snapshot: &MarketSnapshot) -> f64 {
        let mut total_liquidity = 0.0;
        let mut pool_count = 0;

        for pool_wrapper in &path.pools {
            let pool_id = pool_wrapper.get_pool_id();
            if let Some((reserve0, reserve1)) = market_snapshot.get_pool_reserves(&pool_id) {
                let liquidity = self.u256_to_mnt_f64(reserve0) + self.u256_to_mnt_f64(reserve1);
                total_liquidity += liquidity;
                pool_count += 1;
            }
        }

        if pool_count > 0 {
            total_liquidity / pool_count as f64
        } else {
            0.0
        }
    }

    /// 获取区块总览文件路径
    pub fn get_block_summary_file_path(&self) -> &str {
        &self.block_summary_file
    }

    /// 获取套利机会详情文件路径
    pub fn get_opportunity_details_file_path(&self) -> &str {
        &self.opportunity_details_file
    }

    /// 获取池子储备详情文件路径
    pub fn get_pool_reserves_file_path(&self) -> &str {
        &self.pool_reserves_file
    }

    /// 记录池子储备详情（只记录发生变化的储备）
    async fn log_pool_reserves(&mut self, market_snapshot: &MarketSnapshot) -> Result<()> {
        let mut changes_count = 0;
        
        for (pool_id, (reserve0, reserve1)) in &market_snapshot.pool_reserves {
            // 检查是否有变化
            let change_type = if let Some((last_reserve0, last_reserve1)) = self.last_pool_reserves.get(pool_id) {
                if last_reserve0 != reserve0 || last_reserve1 != reserve1 {
                    "Updated"
                } else {
                    continue; // 没有变化，跳过记录
                }
            } else {
                "New" // 新池子
            };

            // 只有发生变化才记录
            let pool_record = PoolReserveRecord {
                block_number: market_snapshot.block_number,
                timestamp: market_snapshot.timestamp,
                pool_address: self.format_pool_address(pool_id),
                pool_id: pool_id.to_string(),
                reserve0: reserve0.to_string(),
                reserve1: reserve1.to_string(),
                reserve0_mnt: self.u256_to_mnt_f64(*reserve0),
                reserve1_mnt: self.u256_to_mnt_f64(*reserve1),
                total_liquidity_mnt: self.u256_to_mnt_f64(*reserve0) + self.u256_to_mnt_f64(*reserve1),
                protocol: "Unknown".to_string(), // 可以后续扩展获取真实协议信息
                is_enabled: market_snapshot.enabled_pools.contains(pool_id),
                change_type: change_type.to_string(),
            };

            self.write_pool_reserve_record(&pool_record).await?;
            changes_count += 1;

            // 更新上一次记录的储备状态
            self.last_pool_reserves.insert(*pool_id, (*reserve0, *reserve1));
        }

        if changes_count > 0 {
            debug!("区块 {} 记录了 {} 个池子储备变化", market_snapshot.block_number, changes_count);
        }

        Ok(())
    }

    /// 写入池子储备记录
    async fn write_pool_reserve_record(&mut self, record: &PoolReserveRecord) -> Result<()> {
        // 确保文件存在并写入头部
        self.ensure_pool_reserves_headers().await?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.pool_reserves_file)
            .await?;

        let csv_line = format!(
            "{},{},\"{}\",\"{}\",{},{},{:.6},{:.6},{:.6},\"{}\",{},\"{}\"\n",
            record.block_number,
            record.timestamp,
            record.pool_address,
            record.pool_id,
            record.reserve0,
            record.reserve1,
            record.reserve0_mnt,
            record.reserve1_mnt,
            record.total_liquidity_mnt,
            record.protocol,
            record.is_enabled,
            record.change_type,
        );

        file.write_all(csv_line.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    /// 确保池子储备详情文件有正确的头部
    async fn ensure_pool_reserves_headers(&mut self) -> Result<()> {
        if Path::new(&self.pool_reserves_file).exists() {
            return Ok(());
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&self.pool_reserves_file)
            .await?;

        let header = "block_number,timestamp,pool_address,pool_id,reserve0,reserve1,reserve0_mnt,reserve1_mnt,total_liquidity_mnt,protocol,is_enabled,change_type\n";
        
        file.write_all(header.as_bytes()).await?;
        file.flush().await?;

        info!("创建池子储备详情记录文件: {}", self.pool_reserves_file);
        Ok(())
    }

    /// 格式化池子地址
    fn format_pool_address(&self, pool_id: &crate::logic::pools::PoolId) -> String {
        pool_id.to_string()
    }
}
