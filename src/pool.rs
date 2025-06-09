use crate::constants::EthFactoryAddress;
use crate::db_error::FluxDBError;
use crate::pool_id::PoolId;
use alloy_evm::EvmEnv;
use alloy_primitives::{Address, Bytes, U256};
use eyre::{Result, eyre};
use revm::DatabaseRef;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;
use strum_macros::{Display, EnumIter, EnumString, VariantNames};

pub fn get_protocol_by_factory(factory_address: Address) -> PoolProtocol {
    if factory_address == EthFactoryAddress::UNISWAP_V2 {
        PoolProtocol::UniswapV2
    } else if factory_address == EthFactoryAddress::UNISWAP_V3 {
        PoolProtocol::UniswapV3
    } else if factory_address == EthFactoryAddress::PANCAKE_V3 {
        PoolProtocol::PancakeV3
    } else if factory_address == EthFactoryAddress::NOMISWAP {
        PoolProtocol::NomiswapStable
    } else if factory_address == EthFactoryAddress::SUSHISWAP_V2 {
        PoolProtocol::Sushiswap
    } else if factory_address == EthFactoryAddress::SUSHISWAP_V3 {
        PoolProtocol::SushiswapV3
    } else if factory_address == EthFactoryAddress::DOOARSWAP {
        PoolProtocol::DooarSwap
    } else if factory_address == EthFactoryAddress::SAFESWAP {
        PoolProtocol::Safeswap
    } else if factory_address == EthFactoryAddress::MINISWAP {
        PoolProtocol::Miniswap
    } else if factory_address == EthFactoryAddress::SHIBASWAP {
        PoolProtocol::Shibaswap
    } else if factory_address == EthFactoryAddress::MAVERICK {
        PoolProtocol::Maverick
    } else if factory_address == EthFactoryAddress::NFTX_V3 {
        PoolProtocol::NftxV3
    } else if factory_address == EthFactoryAddress::KYBERSWAP {
        PoolProtocol::Kyberswap
    } else if factory_address == EthFactoryAddress::SOLIDLY_V3 {
        PoolProtocol::SolidlyV3
    } else if factory_address == EthFactoryAddress::AERODROME_V3 {
        PoolProtocol::AerodromeV3
    } else {
        PoolProtocol::Unknown
    }
}

#[derive(Copy, Clone, Debug, Display, PartialEq, Hash, Eq, EnumString, VariantNames, Default, Deserialize, Serialize, EnumIter)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PoolClass {
    #[default]
    Unknown,
    UniswapV2,
    UniswapV3,
    UniswapV4,
    Curve,
    LidoStEth,
    LidoWstEth,
    RocketPool,
    SushiBar,
    Maverick,
}

#[derive(Copy, Clone, Debug, Display, PartialEq, Eq, Serialize, Deserialize)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PoolProtocol {
    Unknown,
    UniswapV2,
    UniswapV2Like,
    NomiswapStable,
    Sushiswap,
    SushiswapV3,
    DooarSwap,
    OgPepe,
    Safeswap,
    Miniswap,
    Shibaswap,
    UniswapV3,
    UniswapV3Like,
    PancakeV3,
    Integral,
    Maverick,
    Curve,
    LidoStEth,
    LidoWstEth,
    RocketEth,
    NftxV3,
    SushiBar,
    Kyberswap,
    MintySwapV3,
    SolidlyV3,
    UniswapV4,

    // Bsc
    UnchainXV3,

    // Base
    AerodromeV3,
}

#[derive(Serialize, Deserialize)]
pub struct PoolWrapper {
    pub pool: Arc<dyn Pool>,
}

impl PartialOrd for PoolWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for PoolWrapper {}

impl Ord for PoolWrapper {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_pool_id().cmp(&other.get_pool_id())
    }
}

impl Display for PoolWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(fee={})@{:?}", self.get_protocol(), self.get_fee(), self.get_pool_id())
    }
}

impl Debug for PoolWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(fee={})@{:?}", self.get_protocol(), self.get_fee(), self.get_pool_id())
    }
}

impl Hash for PoolWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_pool_id().hash(state)
    }
}

impl PartialEq for PoolWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.pool.get_pool_id() == other.pool.get_pool_id()
    }
}

impl PoolWrapper {
    pub fn new(pool: Arc<dyn Pool>) -> Self {
        PoolWrapper { pool }
    }
}

impl Clone for PoolWrapper {
    fn clone(&self) -> Self {
        Self { pool: self.pool.clone() }
    }
}

impl Deref for PoolWrapper {
    type Target = dyn Pool;

    fn deref(&self) -> &Self::Target {
        self.pool.deref()
    }
}

impl<T: 'static + Pool + Clone> From<T> for PoolWrapper {
    fn from(pool: T) -> Self {
        Self { pool: Arc::new(pool) }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CalculationError {
    #[error("Pool error: {0}")]
    AlloySolError(#[from] alloy_sol_types::Error),
    #[error("Pool error report: {0}")]
    Error(#[from] eyre::Report),
    #[error("Not implemented")]
    NotImplemented,
}

#[typetag::serde(tag = "type")]
pub trait Pool: Sync + Send {
    fn get_class(&self) -> PoolClass {
        PoolClass::Unknown
    }

    fn get_protocol(&self) -> PoolProtocol {
        PoolProtocol::Unknown
    }

    fn get_address(&self) -> Address;

    fn get_pool_id(&self) -> PoolId;

    fn get_fee(&self) -> U256;

    fn get_tokens(&self) -> Vec<Address>;

    fn get_swap_directions(&self) -> Vec<(Address, Address)>;

    fn can_flash_swap(&self) -> bool;

    fn can_calculate_in_amount(&self) -> bool {
        true
    }

    fn get_encoder(&self) -> &dyn AbiSwapEncoder;

    fn get_read_only_cell_vec(&self) -> Vec<U256> {
        Vec::new()
    }
}

pub trait PoolExt: Pool {
    fn calculate_out_amount(
        &self,
        state: &dyn DatabaseRef<Error = FluxDBError>,
        env: EvmEnv,
        token_address_from: &Address,
        token_address_to: &Address,
        in_amount: U256,
    ) -> Result<(U256, u64), CalculationError>;

    // returns (in_amount, gas_used)
    fn calculate_in_amount(
        &self,
        state: &dyn DatabaseRef<Error = FluxDBError>,
        env: EvmEnv,
        token_address_from: &Address,
        token_address_to: &Address,
        out_amount: U256,
    ) -> Result<(U256, u64), CalculationError>;
}

pub struct DefaultAbiSwapEncoder {}

impl AbiSwapEncoder for DefaultAbiSwapEncoder {}

#[derive(Clone, Debug, PartialEq)]
pub enum PreswapRequirement {
    Unknown,
    Transfer(Address),
    Allowance,
    Callback,
    Base,
}

pub trait AbiSwapEncoder {
    fn encode_swap_in_amount_provided(
        &self,
        _token_from_address: Address,
        _token_to_address: Address,
        _amount: U256,
        _recipient: Address,
        _payload: Bytes,
    ) -> Result<Bytes> {
        Err(eyre!("NOT_IMPLEMENTED"))
    }
    fn encode_swap_out_amount_provided(
        &self,
        _token_from_address: Address,
        _token_to_address: Address,
        _amount: U256,
        _recipient: Address,
        _payload: Bytes,
    ) -> Result<Bytes> {
        Err(eyre!("NOT_IMPLEMENTED"))
    }
    fn preswap_requirement(&self) -> PreswapRequirement {
        PreswapRequirement::Unknown
    }

    fn is_native(&self) -> bool {
        false
    }

    fn swap_in_amount_offset(&self, _token_from_address: Address, _token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_out_amount_offset(&self, _token_from_address: Address, _token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_out_amount_return_offset(&self, _token_from_address: Address, _token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_in_amount_return_offset(&self, _token_from_address: Address, _token_to_address: Address) -> Option<u32> {
        None
    }
    fn swap_out_amount_return_script(&self, _token_from_address: Address, _token_to_address: Address) -> Option<Bytes> {
        None
    }
    fn swap_in_amount_return_script(&self, _token_from_address: Address, _token_to_address: Address) -> Option<Bytes> {
        None
    }
}

#[cfg(test)]
mod test {
    use crate::{MockPool, PoolClass, PoolProtocol, PoolWrapper};
    use alloy_primitives::Address;
    use std::sync::Arc;

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", PoolClass::Unknown), "UNKNOWN");
        assert_eq!(format!("{}", PoolClass::UniswapV2), "UNISWAP_V2");

        assert_eq!(format!("{}", PoolProtocol::Unknown), "UNKNOWN");
        assert_eq!(format!("{}", PoolProtocol::UniswapV2), "UNISWAP_V2");
    }

    #[test]
    fn test_serialize_pool_wrapper() -> eyre::Result<()> {
        let pool = MockPool::new(Address::repeat_byte(0), Address::repeat_byte(1), Address::repeat_byte(2));
        let pool_wrapper = PoolWrapper::new(Arc::new(pool));
        let serialized = serde_json::to_string(&pool_wrapper)?;
        let expected = "{\"pool\":{\"type\":\"MockPool\",\"token0\":\"0x0000000000000000000000000000000000000000\",\"token1\":\"0x0101010101010101010101010101010101010101\",\"address\":\"0x0202020202020202020202020202020202020202\"}}".to_string();
        assert_eq!(serialized, expected);

        Ok(())
    }
}
