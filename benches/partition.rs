use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use mercy::BoundaryMerge;

fn uniform_weights() -> [u64; 257] {
    [1; 257]
}

fn skewed_weights() -> [u64; 257] {
    let mut weights = [1; 257];
    weights[b' ' as usize] = 10_000;
    weights[b'e' as usize] = 5_000;
    weights[b't' as usize] = 2_500;
    weights[b'a' as usize] = 1_250;
    weights[256] = 17;
    weights
}

fn bench_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("partition/build");
    group.throughput(Throughput::Elements(1));

    let uniform = uniform_weights();
    group.bench_function("uniform", |b| {
        b.iter(|| BoundaryMerge::from_weights(black_box(&uniform)).unwrap())
    });

    let skewed = skewed_weights();
    group.bench_function("skewed", |b| {
        b.iter(|| BoundaryMerge::from_weights(black_box(&skewed)).unwrap())
    });

    group.finish();
}

fn bench_queries(c: &mut Criterion) {
    let partition = BoundaryMerge::from_weights(&skewed_weights()).unwrap();
    let mut group = c.benchmark_group("partition/query");
    group.throughput(Throughput::Elements(1));

    group.bench_function("rank_symbols", |b| {
        b.iter(|| partition.rank_symbols(black_box(257)))
    });

    group.bench_function("rank_bytes", |b| {
        b.iter(|| partition.rank_bytes(black_box(257)))
    });

    group.bench_function("select_symbol", |b| {
        b.iter(|| partition.select_symbol(black_box(127)))
    });

    group.bench_function("select_byte", |b| {
        b.iter(|| partition.select_byte(black_box(127)))
    });

    group.bench_function("boundaries_in_bucket", |b| {
        b.iter(|| partition.symbol_boundaries_in_bucket(black_box(127)))
    });

    group.finish();
}

fn bench_full_scans(c: &mut Criterion) {
    let partition = BoundaryMerge::from_weights(&skewed_weights()).unwrap();
    let mut group = c.benchmark_group("partition/full_scan");
    group.throughput(Throughput::Elements(256));

    group.bench_function("select_all_symbols", |b| {
        b.iter(|| {
            let mut checksum = 0usize;
            for k in 0..256 {
                checksum ^= partition.select_symbol(black_box(k));
            }
            black_box(checksum)
        })
    });

    group.bench_function("select_all_bytes", |b| {
        b.iter(|| {
            let mut checksum = 0usize;
            for k in 0..256 {
                checksum ^= partition.select_byte(black_box(k));
            }
            black_box(checksum)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_build, bench_queries, bench_full_scans);
criterion_main!(benches);
