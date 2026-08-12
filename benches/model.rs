use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use mercy::model::{
    BalancedTree, BinaryDistribution, Chain, DenseU32Symbols, Distribution, IndexedSymbols,
    LinearSymbols, Topology,
};

const SMALL_VOCAB: usize = 4_096;
const LLM_VOCAB: usize = 100_000;

fn uniform_weights(len: usize) -> Vec<u64> {
    vec![1; len]
}

fn peaked_weights(len: usize) -> Vec<u64> {
    let mut weights = vec![1; len];
    weights[0] = 1_000_000;
    weights
}

#[inline]
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn bench_topology_paths(c: &mut Criterion) {
    let weights = uniform_weights(LLM_VOCAB);
    let chain = Chain::from_weights(&weights).unwrap();
    let balanced = BalancedTree::from_weights(&weights).unwrap();

    let mut group = c.benchmark_group("model/topology_path_100k");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    for &(name, index) in &[("head", 0usize), ("middle", LLM_VOCAB / 2), ("tail", LLM_VOCAB - 1)] {
        group.bench_with_input(BenchmarkId::new("chain_encode", name), &index, |b, &index| {
            b.iter(|| {
                black_box(chain.encode_index(black_box(index)).unwrap().count());
            })
        });
        group.bench_with_input(
            BenchmarkId::new("balanced_encode", name),
            &index,
            |b, &index| {
                b.iter(|| {
                    black_box(balanced.encode_index(black_box(index)).unwrap().count());
                })
            },
        );
    }

    group.finish();
}

fn bench_symbol_lookup(c: &mut Criterion) {
    let weights = uniform_weights(SMALL_VOCAB);
    let topology = BalancedTree::from_weights(&weights).unwrap();
    let explicit: Vec<u32> = (0..SMALL_VOCAB as u32).collect();

    let dense = BinaryDistribution::new(
        DenseU32Symbols::new(SMALL_VOCAB as u32),
        topology.clone(),
    )
    .unwrap();
    let linear = BinaryDistribution::new(
        LinearSymbols::new(explicit.clone()).unwrap(),
        topology.clone(),
    )
    .unwrap();
    let indexed = BinaryDistribution::new(IndexedSymbols::new(explicit).unwrap(), topology).unwrap();

    let target = SMALL_VOCAB as u32 - 1;
    let mut group = c.benchmark_group("model/symbol_lookup_4k");
    group.sample_size(50);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    group.bench_function("dense_u32_encode", |b| {
        b.iter(|| black_box(dense.encode(black_box(target)).unwrap().count()))
    });
    group.bench_function("linear_encode", |b| {
        b.iter(|| black_box(linear.encode(black_box(target)).unwrap().count()))
    });
    group.bench_function("hash_indexed_encode", |b| {
        b.iter(|| black_box(indexed.encode(black_box(target)).unwrap().count()))
    });

    group.finish();
}

fn bench_sampling(c: &mut Criterion) {
    let uniform = uniform_weights(SMALL_VOCAB);
    let peaked = peaked_weights(SMALL_VOCAB);

    let cases = [
        (
            "uniform",
            BinaryDistribution::new(
                DenseU32Symbols::new(SMALL_VOCAB as u32),
                Chain::from_weights(&uniform).unwrap(),
            )
            .unwrap(),
            BinaryDistribution::new(
                DenseU32Symbols::new(SMALL_VOCAB as u32),
                BalancedTree::from_weights(&uniform).unwrap(),
            )
            .unwrap(),
        ),
        (
            "peaked",
            BinaryDistribution::new(
                DenseU32Symbols::new(SMALL_VOCAB as u32),
                Chain::from_weights(&peaked).unwrap(),
            )
            .unwrap(),
            BinaryDistribution::new(
                DenseU32Symbols::new(SMALL_VOCAB as u32),
                BalancedTree::from_weights(&peaked).unwrap(),
            )
            .unwrap(),
        ),
    ];

    let mut group = c.benchmark_group("model/sample_4k");
    group.sample_size(30);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    for (shape, chain, balanced) in &cases {
        group.bench_function(BenchmarkId::new("chain", shape), |b| {
            let mut state = 0x1234_5678u32;
            b.iter(|| {
                black_box(chain.sample_with(|| xorshift32(&mut state)));
            })
        });
        group.bench_function(BenchmarkId::new("balanced", shape), |b| {
            let mut state = 0x8765_4321u32;
            b.iter(|| {
                black_box(balanced.sample_with(|| xorshift32(&mut state)));
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_topology_paths,
    bench_symbol_lookup,
    bench_sampling
);
criterion_main!(benches);
