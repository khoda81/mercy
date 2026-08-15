use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mercy::RankedPrefix;

const SIZES: &[usize] = &[0, 1, 8, 16, 64, 256, 1_024, 4_096, 50_000];

fn patterned(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| {
            // Deterministic, branch-unfriendly-ish spread without pulling an RNG
            // into the benchmark dependency graph.
            ((i.wrapping_mul(73).wrapping_add(i >> 3).wrapping_add(19)) & 0xff) as u8
        })
        .collect()
}

fn bench_family(c: &mut Criterion, name: &str, make: impl Fn(usize) -> Vec<u8>) {
    let mut group = c.benchmark_group(name);

    for &size in SIZES {
        let input = make(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| {
                let _ = black_box(RankedPrefix::new(black_box(input)).tail_probability());
            });
        });
    }

    group.finish();
}

fn tail_probability(c: &mut Criterion) {
    bench_family(c, "tail/zero", |n| vec![0; n]);
    bench_family(c, "tail/max", |n| vec![255; n]);
    bench_family(c, "tail/half", |n| vec![128; n]);
    bench_family(c, "tail/patterned", patterned);
}

criterion_group!(benches, tail_probability);
criterion_main!(benches);
