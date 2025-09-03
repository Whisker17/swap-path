use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall};
use eyre::Result;
use serde_json::Value;
use crate::logic::pools::PoolId;

// Standard Multicall3 contract interface
sol! {
    /// Multicall3 contract interface
    contract Multicall3 {
        struct Call {
            address target;
            bytes callData;
        }
        
        function aggregate(Call[] calldata calls) public view returns (uint256 blockNumber, bytes[] memory returnData);
        function aggregate3(Call3[] calldata calls) public view returns (Result[] memory returnData);
        
        struct Call3 {
            address target;
            bool allowFailure;
            bytes callData;
        }
        
        struct Result {
            bool success;
            bytes returnData;
        }
    }
    
    /// ERC20 Pair interface for getReserves
    interface IUniswapV2Pair {
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast);
    }
}

/// Multicall manager for batch querying pool reserves
#[derive(Debug, Clone)]
pub struct MulticallManager {
    multicall_address: Address,
    http_client: reqwest::Client,
    rpc_url: String,
}

impl MulticallManager {
    pub fn new(multicall_address: Address, rpc_url: String, timeout: std::time::Duration) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            multicall_address,
            http_client,
            rpc_url,
        }
    }
    
    /// Prepare getReserves call data for a pool
    pub fn prepare_get_reserves_call(pool_address: Address) -> Multicall3::Call {
        let call_data = IUniswapV2Pair::getReservesCall {}.abi_encode();
        
        Multicall3::Call {
            target: pool_address,
            callData: call_data.into(),
        }
    }
    
    /// Batch query reserves for multiple pools
    pub async fn batch_get_reserves(&self, pool_addresses: &[PoolId], block_number: Option<u64>) -> Result<Vec<(PoolId, Option<(U256, U256)>)>> {
        if pool_addresses.is_empty() {
            return Ok(Vec::new());
        }
        
        // Prepare multicall calls
        let calls: Vec<Multicall3::Call> = pool_addresses
            .iter()
            .map(|pool_id| {
                let address = match pool_id {
                    PoolId::Address(addr) => *addr,
                    PoolId::B256(hash) => {
                        // For B256 pool IDs, we might need different handling
                        // For now, treat as address by taking first 20 bytes
                        Address::from_slice(&hash.as_slice()[0..20])
                    }
                };
                Self::prepare_get_reserves_call(address)
            })
            .collect();
        
        // Encode multicall
        let multicall_data = Multicall3::aggregateCall { calls }.abi_encode();
        
        // Make RPC call
        let response = self.call_contract(
            self.multicall_address,
            multicall_data.into(),
            block_number,
        ).await?;
        
        // Decode response
        let decoded = Multicall3::aggregateCall::abi_decode_returns(&response, true)?;
        
        // Parse results
        let mut results = Vec::new();
        for (i, return_data) in decoded.returnData.iter().enumerate() {
            let pool_id = pool_addresses[i];
            
            if return_data.is_empty() {
                results.push((pool_id, None));
                continue;
            }
            
            match IUniswapV2Pair::getReservesCall::abi_decode_returns(return_data, true) {
                Ok(reserves) => {
                    let reserve0 = U256::from(reserves.reserve0);
                    let reserve1 = U256::from(reserves.reserve1);
                    results.push((pool_id, Some((reserve0, reserve1))));
                }
                Err(e) => {
                    tracing::warn!("Failed to decode reserves for pool {:?}: {}", pool_id, e);
                    results.push((pool_id, None));
                }
            }
        }
        
        Ok(results)
    }
    
    /// Make a contract call via RPC
    async fn call_contract(
        &self,
        to: Address,
        data: Bytes,
        block_number: Option<u64>,
    ) -> Result<Bytes> {
        let block_param = match block_number {
            Some(num) => format!("0x{:x}", num),
            None => "latest".to_string(),
        };
        
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [
                {
                    "to": format!("{:#x}", to),
                    "data": format!("{:#x}", data)
                },
                block_param
            ],
            "id": 1
        });
        
        let response = self.http_client
            .post(&self.rpc_url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;
        
        let response_json: Value = response.json().await?;
        
        if let Some(error) = response_json.get("error") {
            return Err(eyre::eyre!("RPC error: {}", error));
        }
        
        let result = response_json
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or_else(|| eyre::eyre!("Missing result in RPC response"))?;
        
        let bytes = hex::decode(result.trim_start_matches("0x"))?;
        Ok(bytes.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_prepare_get_reserves_call() {
        let pool_address = Address::repeat_byte(0x42);
        let call = MulticallManager::prepare_get_reserves_call(pool_address);
        
        assert_eq!(call.target, pool_address);
        assert!(!call.callData.is_empty());
        
        // Verify the call data matches getReserves() function selector
        let expected_selector = &IUniswapV2Pair::getReservesCall {}.abi_encode()[0..4];
        assert_eq!(&call.callData[0..4], expected_selector);
    }
    
    #[test]
    fn test_multicall_manager_creation() {
        let multicall_address = Address::repeat_byte(0x11);
        let rpc_url = "https://rpc.mantle.xyz".to_string();
        let timeout = Duration::from_secs(10);
        
        let manager = MulticallManager::new(multicall_address, rpc_url, timeout);
        assert_eq!(manager.multicall_address, multicall_address);
    }
}
