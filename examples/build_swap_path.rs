use alloy_primitives::{Address, U256, address};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use swap_path::{AbiSwapEncoder, Market, MarketWithoutLock, MockPool, Pool, PoolId, PoolWrapper, add_swap_path};
use tokio::sync::RwLock;

#[derive(Clone, Serialize, Deserialize)]
pub struct ExamplePool {
    pub token0: Address,
    pub token1: Address,
    pub address: Address,
}

impl ExamplePool {
    pub fn new(token0: Address, token1: Address, address: Address) -> Self {
        Self { token0, token1, address }
    }
}

#[typetag::serde]
impl Pool for ExamplePool {
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
        todo!()
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let market_without_lock = Arc::new(MarketWithoutLock::default());
    let market = Arc::new(RwLock::new(Market {
        market_config: Default::default(),
        market_without_lock: market_without_lock.clone(),
        token_graph: Default::default(),
    }));

    let pool_1_address = address!("0x0000000000000000000000000000000000000001");

    let pool_1 = MockPool::new(
        address!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        address!("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
        pool_1_address.clone(),
    );
    add_new_pool_guarded(market.clone(), market_without_lock.clone(), PoolWrapper::new(Arc::new(pool_1.clone()))).await?;

    let pool_2 = MockPool::new(
        address!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        address!("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
        address!("0x0000000000000000000000000000000000000002"),
    );
    add_new_pool_guarded(market.clone(), market_without_lock.clone(), PoolWrapper::new(Arc::new(pool_2.clone()))).await?;

    let pool_paths = market_without_lock.get_pool_paths(&PoolId::Address(pool_1_address));

    for (i, path) in pool_paths.iter().enumerate() {
        println!("Path{i}: {:?}", path.pools);
    }

    Ok(())
}

async fn add_new_pool_guarded(
    market: Arc<RwLock<Market>>,
    market_without_lock: Arc<MarketWithoutLock>,
    pool_wrapped: PoolWrapper,
) -> eyre::Result<()> {
    // Building the pet graph is cpu-intensive, so we use block_in_place to avoid blocking the async runtime.
    tokio::task::block_in_place(|| async {
        let mut market_write_guard = market.write().await;
        market_write_guard.add_pool(pool_wrapped.clone());
        let pool_address = pool_wrapped.get_address();
        let new_swap_paths = match market_write_guard.update_paths(pool_wrapped) {
            Ok(paths) => paths,
            Err(e) => {
                println!("Error updating paths for pool {:#20x} : {}", pool_address, e);
                return;
            }
        };
        drop(market_write_guard);
        for swap_path in new_swap_paths {
            add_swap_path(market_without_lock.clone(), swap_path);
        }
    })
    .await;

    Ok(())
}
