use crate::constants::WMNT;
use alloy_primitives::utils::Unit;
use alloy_primitives::{Address, I256, U256};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::default::Default;
use std::hash::{Hash, Hasher};
use std::ops::{Add, Mul, Neg};
use std::sync::Arc;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Token {
    address: Address,
    decimals: u8,
    name: Option<String>,
    symbol: Option<String>,
}

pub type TokenWrapper = Arc<Token>;

impl Hash for Token {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state)
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.get_address()
    }
}

impl Eq for Token {}

impl Ord for Token {
    fn cmp(&self, other: &Self) -> Ordering {
        self.address.cmp(&other.get_address())
    }
}

impl PartialOrd for Token {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Token {
    pub fn new(address: Address) -> Token {
        Token { address, decimals: 18, ..Token::default() }
    }

    pub fn new_with_data(address: Address, symbol: Option<String>, name: Option<String>, decimals: Option<u8>) -> Token {
        Token { address, symbol, name, decimals: decimals.unwrap_or(18) }
    }

    // For testing purposes
    pub fn random() -> Token {
        Token::new(Address::random())
    }

    // For testing purposes
    pub fn repeat_byte(byte: u8) -> Token {
        Token::new(Address::repeat_byte(byte))
    }

    pub fn get_symbol(&self) -> String {
        self.symbol.clone().unwrap_or(self.address.to_string())
    }

    pub fn get_name(&self) -> String {
        self.name.clone().unwrap_or(self.address.to_string())
    }

    pub fn get_decimals(&self) -> u8 {
        self.decimals
    }

    pub fn get_exp(&self) -> U256 {
        if self.decimals == 18 { Unit::ETHER.wei() } else { U256::from(10).pow(U256::from(self.decimals)) }
    }

    pub fn get_address(&self) -> Address {
        self.address
    }

    pub fn to_float(&self, value: U256) -> f64 {
        if self.decimals == 0 {
            0f64
        } else {
            let divider = self.get_exp();
            let ret = value.div_rem(divider);

            let div = u64::try_from(ret.0);
            let rem = u64::try_from(ret.1);

            if div.is_err() || rem.is_err() {
                0f64
            } else {
                div.unwrap_or_default() as f64 + ((rem.unwrap_or_default() as f64) / (10u64.pow(self.decimals as u32) as f64))
            }
        }
    }

    pub fn to_float_sign(&self, value: I256) -> f64 {
        let r: U256 = if value.is_positive() { value.into_raw() } else { value.neg().into_raw() };
        let f = self.to_float(r);
        if value.is_positive() { f } else { -f }
    }

    pub fn from_float(&self, value: f64) -> U256 {
        let multiplier = U256::from(value as i64);
        let modulus = U256::from(((value - value.round()) * (10 ^ self.decimals as i64) as f64) as u64);
        multiplier.mul(U256::from(10).pow(U256::from(self.decimals))).add(modulus)
    }

    pub fn is_wrapped(&self) -> bool {
        self.address == WMNT
    }

    pub fn is_native(&self) -> bool {
        self.address.is_zero()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_serialize() {
        let weth_token = Token::new_with_data(WMNT, Some("WMNT".to_string()), None, Some(18));

        let serialized = serde_json::to_string(&weth_token).unwrap();
        assert_eq!(
            serialized,
            "{\"address\":\"0x78c1b0c915c4faa5fffa6cabf0219da63d7f4cb8\",\"decimals\":18,\"name\":null,\"symbol\":\"WMNT\"}"
        );
    }
}
