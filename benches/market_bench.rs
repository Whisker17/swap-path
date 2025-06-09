use alloy_primitives::{Address, address};
use criterion::{Criterion, criterion_group, criterion_main};
use lazy_static::lazy_static;
use std::sync::Arc;
use swap_path::{Market, MockPool, PoolWrapper, Token};

lazy_static! {
    static ref WETH: Token =
        Token::new_with_data(address!("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"), Some("WETH".to_string()), None, Some(18));
    static ref USDT: Token =
        Token::new_with_data(address!("0xdac17f958d2ee523a2206206994597c13d831ec7"), Some("USDT".to_string()), None, Some(18));
}

fn create_pool(token0: Address, token1: Address) -> MockPool {
    MockPool::new(token0, token1, Address::random())
}

fn test_market_fill() -> eyre::Result<()> {
    let mut market = Market::default();
    market.add_token(WETH.clone());
    market.add_token(USDT.clone());
    let weth_usdt_pool = create_pool(WETH.get_address(), USDT.get_address());
    market.add_pool(weth_usdt_pool);
    let weth_usdt_pool = create_pool(WETH.get_address(), USDT.get_address());
    market.add_pool(weth_usdt_pool);

    for _ in 0..1000 {
        let token_address = Address::random();
        let weth_pool = PoolWrapper::new(Arc::new(create_pool(WETH.get_address(), token_address)));
        let usdt_pool = PoolWrapper::new(Arc::new(create_pool(USDT.get_address(), token_address)));

        market.add_pool(weth_pool.clone());
        market.add_pool(usdt_pool.clone());
        market.update_paths(weth_pool.clone())?;
        market.update_paths(usdt_pool.clone())?;
    }

    for _ in 0..1000 {
        let token_address = Address::random();
        let weth_pool = PoolWrapper::new(Arc::new(create_pool(WETH.get_address(), token_address)));
        let usdt_pool = PoolWrapper::new(Arc::new(create_pool(USDT.get_address(), token_address)));
        market.add_pool(weth_pool.clone());
        market.add_pool(usdt_pool.clone());
        market.update_paths(weth_pool.clone())?;
        market.update_paths(usdt_pool.clone())?;
    }

    Ok(())
}

fn benchmark_test_group_hasher(c: &mut Criterion) {
    let mut group = c.benchmark_group("market");
    group.sample_size(10);

    group.bench_function("test_market_fill", |b| b.iter(test_market_fill));
    group.finish();
}

criterion_group!(benches, benchmark_test_group_hasher);
criterion_main!(benches);
