use alloy_primitives::hex;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};

#[derive(Clone, Default, Eq, PartialEq, Hash)]
pub struct SwapPathHash(pub [u8; 32]);

impl Display for SwapPathHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode_prefixed(self.0))
    }
}

impl Debug for SwapPathHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SwapPathHash({})", hex::encode_prefixed(self.0))
    }
}

impl From<[u8; 32]> for SwapPathHash {
    fn from(hash: [u8; 32]) -> Self {
        SwapPathHash(hash)
    }
}

impl Serialize for SwapPathHash {
    fn serialize<S>(&self, serializer: S) -> eyre::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode_prefixed(self.0))
    }
}

impl<'de> Deserialize<'de> for SwapPathHash {
    fn deserialize<D>(deserializer: D) -> eyre::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut hash = [0; 32];
        hash.copy_from_slice(&bytes);
        Ok(SwapPathHash(hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_swap_path_hash() {
        let swap_path_hash = SwapPathHash([1; 32]);

        let serialized = serde_json::to_string(&swap_path_hash).unwrap();
        let deserialized: SwapPathHash = serde_json::from_str(&serialized).unwrap();

        assert_eq!(swap_path_hash, deserialized);
    }
}
