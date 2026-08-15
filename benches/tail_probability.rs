use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mercy::RankedPrefix;

const SIZES: &[usize] = &[8, 16, 64, 256, 4_096, 50_000, 200_000];

fn patterned(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| {
            // Deterministic, branch-unfriendly-ish spread without pulling an RNG
            // into the benchmark dependency graph.
            ((i.wrapping_mul(73).wrapping_add(i >> 3).wrapping_add(19)) & 0xff) as u8
        })
        .collect()
}

fn tail_probability(c: &mut Criterion) {
    let mut group = c.benchmark_group("tail/patterned");

    for &size in SIZES {
        let input = patterned(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| {
                let _ = black_box(RankedPrefix::from_slice(black_box(input)).tail_probability());
            });
        });
    }

    group.finish();
}

criterion_group!(benches, tail_probability);
criterion_main!(benches);
