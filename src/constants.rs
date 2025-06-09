use alloy_primitives::{Address, address};

pub const WETH: Address = address!("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");

pub const NATIVE: Address = Address::ZERO;

#[non_exhaustive]
pub struct EthFactoryAddress;

impl EthFactoryAddress {
    // Uniswap V2 compatible
    pub const UNISWAP_V2: Address = address!("5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f");
    pub const SUSHISWAP_V2: Address = address!("c0aee478e3658e2610c5f7a4a2e1777ce9e4f2ac");
    pub const NOMISWAP: Address = address!("818339b4e536e707f14980219037c5046b049dd4");
    pub const DOOARSWAP: Address = address!("1e895bfe59e3a5103e8b7da3897d1f2391476f3c");
    pub const SAFESWAP: Address = address!("7f09d4be6bbf4b0ff0c97ca5c486a166198aeaee");
    pub const MINISWAP: Address = address!("2294577031f113df4782b881cf0b140e94209a6f");
    pub const SHIBASWAP: Address = address!("115934131916c8b277dd010ee02de363c09d037c");
    pub const KYBERSWAP: Address = address!("833e4083b7ae46cea85695c4f7ed25cdad8886de");

    // Uniswap V3 compatible
    pub const UNISWAP_V3: Address = address!("1f98431c8ad98523631ae4a59f267346ea31f984");
    pub const SUSHISWAP_V3: Address = address!("baceb8ec6b9355dfc0269c18bac9d6e2bdc29c4f");
    pub const PANCAKE_V3: Address = address!("0bfbcf9fa4f9c56b0f40a671ad40e0805a091865");
    pub const NFTX_V3: Address = address!("a70e10beB02fF9a44007D9D3695d4b96003db101");
    pub const SOLIDLY_V3: Address = address!("70Fe4a44EA505cFa3A57b95cF2862D4fd5F0f687");

    // Maverick
    pub const MAVERICK: Address = address!("eb6625d65a0553c9dbc64449e56abfe519bd9c9b");

    // BASE
    pub const AERODROME_V3: Address = Address::ZERO;
}
