use alloy_primitives::Address;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use swap_path::graph::TokenGraph;
use swap_path::graph::find_all_paths;
use swap_path::{MockPool, PoolWrapper, SwapPath, Token};

fn benchmark_find_all_paths(c: &mut Criterion) {
    let token1 = Arc::new(Token::random());
    let token2 = Arc::new(Token::random());
    let token3 = Arc::new(Token::random());

    let mut token_graph = TokenGraph::new();
    token_graph.add_or_get_token_idx_by_token(token1.clone());
    token_graph.add_or_get_token_idx_by_token(token2.clone());
    token_graph.add_or_get_token_idx_by_token(token3.clone());

    let pool_1_2 = PoolWrapper::from(MockPool::new(token1.get_address(), token2.get_address(), Address::random()));
    let pool_2_3 = PoolWrapper::from(MockPool::new(token2.get_address(), token3.get_address(), Address::random()));
    let pool_3_1 = PoolWrapper::from(MockPool::new(token3.get_address(), token1.get_address(), Address::random()));

    token_graph.add_pool(pool_1_2.clone()).unwrap();
    token_graph.add_pool(pool_2_3.clone()).unwrap();
    token_graph.add_pool(pool_3_1.clone()).unwrap();

    let start_node_index = token_graph.token_index.get(&token2.get_address()).unwrap();
    let end_node_index = token_graph.token_index.get(&token1.get_address()).unwrap();

    let initial_swap_path = SwapPath::new_first(token1, token2, pool_1_2);

    c.bench_function("find_all_paths", |b| {
        b.iter(|| {
            find_all_paths(
                black_box(&token_graph),
                black_box(initial_swap_path.clone()),
                black_box(*start_node_index),
                black_box(*end_node_index),
                black_box(3),
                false,
            )
            .unwrap();
        })
    });
}

criterion_group!(benches, benchmark_find_all_paths);
criterion_main!(benches);
