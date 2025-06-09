use alloy_primitives::{Address, B256};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, strum_macros::Display)]
#[serde(untagged)]
pub enum PoolId {
    #[strum(to_string = "{0}")]
    Address(Address),
    #[strum(to_string = "{0}")]
    B256(B256),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;

    #[test]
    fn test_deserialize_address() {
        let deserialized: PoolId = serde_json::from_str("\"0x0101010101010101010101010101010101010101\"").unwrap();
        assert_eq!(PoolId::Address(Address::repeat_byte(0x01)), deserialized);
    }

    #[test]
    fn test_serialize_address() {
        let addr = Address::repeat_byte(0x01);
        let pool_id = PoolId::Address(addr);
        let serialized = serde_json::to_string(&pool_id).unwrap();
        assert_eq!(serialized, "\"0x0101010101010101010101010101010101010101\"");
    }

    #[test]
    fn test_deserialize_b256() {
        let deserialized: PoolId = serde_json::from_str("\"0x0101010101010101010101010101010101010101010101010101010101010101\"").unwrap();
        assert_eq!(PoolId::B256(B256::repeat_byte(0x01)), deserialized);
    }

    #[test]
    fn test_serialize_b256() {
        let b256 = B256::repeat_byte(0x01);
        let pool_id = PoolId::B256(b256);
        let serialized = serde_json::to_string(&pool_id).unwrap();
        assert_eq!(serialized, "\"0x0101010101010101010101010101010101010101010101010101010101010101\"");
    }

    #[test]
    fn test_display() {
        let addr = Address::repeat_byte(0x01);
        let pool_id = PoolId::Address(addr);
        assert_eq!(format!("{}", pool_id), "0x0101010101010101010101010101010101010101");

        let b256 = B256::repeat_byte(0x01);
        let pool_id = PoolId::B256(b256);
        assert_eq!(format!("{}", pool_id), "0x0101010101010101010101010101010101010101010101010101010101010101");
    }

    #[test]
    fn test_ord() {
        let addr_small = Address::repeat_byte(0x01);
        let addr_large = Address::repeat_byte(0x02);
        let b256_small = B256::repeat_byte(0x01);
        let b256_large = B256::repeat_byte(0x02);

        let pool_id_addr_small = PoolId::Address(addr_small);
        let pool_id_addr_large = PoolId::Address(addr_large);
        let pool_id_byte32_small = PoolId::B256(b256_small);
        let pool_id_byte32_large = PoolId::B256(b256_large);

        // Address small < large
        assert!(pool_id_addr_small < pool_id_addr_large);
        assert!(pool_id_addr_large > pool_id_addr_small);

        // B256 small < large
        assert!(pool_id_byte32_small < pool_id_byte32_large);
        assert!(pool_id_byte32_large > pool_id_byte32_small);

        // Address < B256
        assert!(pool_id_addr_small < pool_id_byte32_large);
        assert!(pool_id_byte32_large > pool_id_addr_small);
    }
}
