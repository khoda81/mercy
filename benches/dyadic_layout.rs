use std::{hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mercy::{
    dyadic::implementations::{Candidate, CANDIDATES},
    prefix::fixtures::PATTERNED,
    RankedPrefix,
};

const SIZES: &[usize] = &[8, 16, 64, 256, 4_096, 50_000, 200_000];

fn tune(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group.sample_size(20);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
}

fn values(
    size: usize,
    candidate: Candidate,
) -> (
    mercy::dyadic::implementations::Value,
    mercy::dyadic::implementations::Value,
) {
    let left = PATTERNED.generate(size);
    let mut right = PATTERNED.generate(size);
    right.rotate_left(size / 3);
    let left = RankedPrefix::from_slice(&left).tail_probability();
    let right = RankedPrefix::from_slice(&right).tail_probability();
    (candidate.prepare(&left), candidate.prepare(&right))
}

fn multiply(c: &mut Criterion) {
    let mut group = c.benchmark_group("dyadic-layout/multiply");
    tune(&mut group);
    for &candidate in CANDIDATES {
        for &size in SIZES {
            let (left, right) = values(size, candidate);
            group.throughput(Throughput::Elements(size as u64));
            group.bench_with_input(
                BenchmarkId::new(candidate.name, size),
                &(candidate, left, right),
                |b, (candidate, left, right)| {
                    b.iter(|| black_box(*candidate).multiply(black_box(left), black_box(right)));
                },
            );
        }
    }
    group.finish();
}

fn scale_floor(c: &mut Criterion) {
    let mut group = c.benchmark_group("dyadic-layout/scale-floor-u64");
    tune(&mut group);
    for &candidate in CANDIDATES {
        for &size in SIZES {
            let (value, _) = values(size, candidate);
            group.throughput(Throughput::Elements(size as u64));
            group.bench_with_input(
                BenchmarkId::new(candidate.name, size),
                &(candidate, value),
                |b, (candidate, value)| {
                    b.iter(|| {
                        black_box(*candidate)
                            .scale_floor_u64(black_box(value), black_box(u64::MAX - 4095))
                    });
                },
            );
        }
    }
    group.finish();
}

fn construct(c: &mut Criterion) {
    let mut group = c.benchmark_group("dyadic-layout/construct");
    tune(&mut group);
    for &candidate in CANDIDATES {
        for &size in SIZES {
            let input = PATTERNED.generate(size);
            let value = RankedPrefix::from_slice(&input).tail_probability();
            group.throughput(Throughput::Elements(size as u64));
            group.bench_with_input(
                BenchmarkId::new(candidate.name, size),
                &(candidate, value),
                |b, (candidate, value)| {
                    b.iter(|| black_box(*candidate).prepare(black_box(value)));
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, multiply, scale_floor, construct);
criterion_main!(benches);
