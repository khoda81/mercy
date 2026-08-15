use std::{hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use mercy::{
    prefix::{
        fixtures::{GENERAL_FIXTURES, MODEL_SHAPED_FIXTURES},
        implementations::{owned, Candidate, PERFORMANCE_CANDIDATES},
    },
    RankedPrefix,
};

const SIZES: &[usize] = &[8, 16, 64, 256, 4_096, 50_000, 200_000];
const MODEL_SIZES: &[usize] = &[4_096, 50_000, 100_000, 200_000];

fn tune(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group.sample_size(20);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
}

fn tail_probability(c: &mut Criterion) {
    for &fixture in GENERAL_FIXTURES {
        let mut group = c.benchmark_group(format!("tail/{}", fixture.name));
        tune(&mut group);
        for &candidate in PERFORMANCE_CANDIDATES {
            for &size in SIZES {
                let input = fixture.generate(size);
                group.throughput(Throughput::Bytes(size as u64));
                group.bench_with_input(
                    BenchmarkId::new(candidate.name, size),
                    &(candidate, input),
                    |b, (candidate, input): &(Candidate, Vec<u8>)| {
                        b.iter(|| {
                            black_box(*candidate)
                                .compute(RankedPrefix::from_slice(black_box(input)))
                        });
                    },
                );
            }
        }
        group.finish();
    }

    for &fixture in MODEL_SHAPED_FIXTURES {
        let mut group = c.benchmark_group(format!("tail/{}", fixture.name));
        tune(&mut group);
        for &candidate in PERFORMANCE_CANDIDATES {
            for &size in MODEL_SIZES {
                let input = fixture.generate(size);
                group.throughput(Throughput::Bytes(size as u64));
                group.bench_with_input(
                    BenchmarkId::new(candidate.name, size),
                    &(candidate, input),
                    |b, (candidate, input): &(Candidate, Vec<u8>)| {
                        b.iter(|| {
                            black_box(*candidate)
                                .compute(RankedPrefix::from_slice(black_box(input)))
                        });
                    },
                );
            }
        }
        group.finish();
    }
}

fn owned_tail_probability(c: &mut Criterion) {
    let fixture = mercy::prefix::fixtures::PATTERNED;
    for allocation in ["outside", "inside"] {
        let mut group = c.benchmark_group(format!("tail-owned/{allocation}/patterned"));
        tune(&mut group);
        for &candidate in owned::CANDIDATES {
            for &size in SIZES {
                let input = fixture.generate(size);
                group.throughput(Throughput::Bytes(size as u64));
                group.bench_with_input(
                    BenchmarkId::new(candidate.name, size),
                    &(candidate, input),
                    |b, (candidate, input)| {
                        if allocation == "outside" {
                            b.iter_batched(
                                || RankedPrefix::from_boxed_slice(input.clone().into_boxed_slice()),
                                |owned| black_box(*candidate).compute(black_box(owned)),
                                BatchSize::SmallInput,
                            );
                        } else {
                            b.iter(|| {
                                let owned = RankedPrefix::from_boxed_slice(
                                    black_box(input).clone().into_boxed_slice(),
                                );
                                black_box(*candidate).compute(black_box(owned))
                            });
                        }
                    },
                );
            }
        }
        group.finish();
    }
}

criterion_group!(benches, tail_probability, owned_tail_probability);
criterion_main!(benches);
