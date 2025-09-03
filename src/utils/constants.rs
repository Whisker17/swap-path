use alloy_primitives::{Address, address};

pub const WMNT: Address = address!("0x78c1b0c915c4faa5fffa6cabf0219da63d7f4cb8");

pub const NATIVE: Address = Address::ZERO;

#[non_exhaustive]
pub struct MantleFactoryAddress;

impl MantleFactoryAddress {
    // Uniswap V2 compatible
    pub const MERCHANT_MOE_MOE_LP: Address = address!("5bEf015CA9424A7C07B68490616a4C1F094BEdEc");

    // Uniswap V3 compatible
    pub const MERCHANT_MOE_LBT: Address = address!("a6630671775c4ea2743840f9a5016dcf2a104054");
    pub const AGNI: Address = address!("25780dc8Fc3cfBD75F33bFDAB65e969b603b2035");
    pub const UNISWAP_V3: Address = address!("0d922Fb1Bc191F64970ac40376643808b4B74Df9");
}