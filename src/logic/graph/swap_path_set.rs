use super::swap_path::SwapPath;
use std::collections::HashSet;

#[derive(Default)]
pub struct SwapPathSet {
    pub set: HashSet<SwapPath>,
}

/// A set of swap paths that makes sure that there are no duplicates
impl SwapPathSet {
    /// Create a new empty swap path set
    pub fn new() -> SwapPathSet {
        SwapPathSet { set: HashSet::new() }
    }

    /// Insert a swap path
    pub fn insert(&mut self, path: SwapPath) {
        self.set.insert(path);
    }

    /// Extend from a vector of swap paths
    pub fn extend(&mut self, path_vec: Vec<SwapPath>) {
        for path in path_vec {
            self.set.insert(path);
        }
    }

    /// Convert the set to a vector
    pub fn vec(self) -> Vec<SwapPath> {
        self.set.into_iter().collect()
    }

    // used in tests
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.set.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MockPool, Token};
    use alloy_primitives::Address;
    use std::sync::Arc;

    #[test]
    fn test_swap_path_set() {
        let token1 = Arc::new(Token::random());
        let token2 = Arc::new(Token::random());
        let token3 = Arc::new(Token::random());

        let pool_1_2 = MockPool::new(token1.get_address(), token2.get_address(), Address::random());
        let pool_2_3 = MockPool::new(token2.get_address(), token3.get_address(), Address::random());
        let pool_3_1 = MockPool::new(token3.get_address(), token1.get_address(), Address::random());

        let swap_path_1 = SwapPath::new(vec![token1.clone(), token2.clone()], vec![pool_1_2.clone()]);
        let swap_path_2 = SwapPath::new(vec![token2.clone(), token3.clone()], vec![pool_2_3.clone()]);
        let swap_path_3 = SwapPath::new(vec![token3.clone(), token1.clone()], vec![pool_3_1.clone()]);

        let mut swap_path_set = SwapPathSet::new();
        swap_path_set.extend(vec![swap_path_1.clone(), swap_path_2.clone(), swap_path_3.clone()]);
        // second time should not add anything
        swap_path_set.extend(vec![swap_path_1.clone(), swap_path_2.clone(), swap_path_3.clone()]);

        assert_eq!(swap_path_set.len(), 3);

        let swap_paths = swap_path_set.vec();
        assert_eq!(swap_paths.len(), 3);
        assert!(swap_paths.contains(&swap_path_1));
        assert!(swap_paths.contains(&swap_path_2));
        assert!(swap_paths.contains(&swap_path_3));
    }
}
