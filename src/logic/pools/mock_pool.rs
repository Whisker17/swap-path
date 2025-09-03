use super::pool_id::PoolId;
use crate::{AbiSwapEncoder, Pool, PoolClass, PoolProtocol};
use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct MockPool {
    pub token0: Address,
    pub token1: Address,
    pub address: Address,
}

impl MockPool {
    pub fn new(token0: Address, token1: Address, address: Address) -> Self {
        Self { token0, token1, address }
    }
}

#[typetag::serde]
impl Pool for MockPool {
    fn get_class(&self) -> PoolClass {
        PoolClass::UniswapV2
    }

    fn get_protocol(&self) -> PoolProtocol {
        PoolProtocol::Unknown
    }

    fn get_address(&self) -> Address {
        self.address
    }

    fn get_pool_id(&self) -> PoolId {
        PoolId::Address(self.address)
    }

    fn get_fee(&self) -> U256 {
        U256::ZERO
    }

    fn get_tokens(&self) -> Vec<Address> {
        vec![self.token0, self.token1]
    }

    fn get_swap_directions(&self) -> Vec<(Address, Address)> {
        vec![(self.token0, self.token1), (self.token1, self.token0)]
    }

    fn can_flash_swap(&self) -> bool {
        true
    }

    fn get_encoder(&self) -> &dyn AbiSwapEncoder {
        unimplemented!("get_encoder not implemented for MockPool")
    }
}
