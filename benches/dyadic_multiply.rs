use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mercy::RankedPrefix;

const SIZES: &[usize] = &[8, 16, 64, 256, 4_096, 50_000, 200_000];

fn patterned(size: usize, salt: usize) -> Vec<u8> {
    (0..size)
        .map(|i| ((i.wrapping_mul(73).wrapping_add(i >> 3).wrapping_add(salt)) & 0xff) as u8)
        .collect()
}

fn dyadic_multiply(c: &mut Criterion) {
    let mut group = c.benchmark_group("dyadic_multiply/equal");

    for &size in SIZES {
        let lhs_bytes = patterned(size, 19);
        let rhs_bytes = patterned(size, 101);
        let lhs = RankedPrefix::from_slice(&lhs_bytes).tail_probability();
        let rhs = RankedPrefix::from_slice(&rhs_bytes).tail_probability();

        // `size` is the number of probability entries used to derive each
        // operand. It is a stable, user-facing proxy for dyadic precision.
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| {
                let _ = black_box(black_box(&lhs) * black_box(&rhs));
            });
        });
    }

    group.finish();
}

criterion_group!(benches, dyadic_multiply);
criterion_main!(benches);
