use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mercy::BoundaryMerge;

const SELECT_RANKS: [usize; 7] = [0, 1, 4, 8, 16, 31, 63];
const PARTITION_RANKS: [usize; 6] = [0, 31, 63, 127, 191, 255];
const RANDOM_QUERY_COUNT: usize = 1024;

const fn build_select8() -> [[u8; 8]; 256] {
    let mut table = [[u8::MAX; 8]; 256];
    let mut mask = 0usize;
    while mask < 256 {
        let mut rank = 0usize;
        let mut bit = 0usize;
        while bit < 8 {
            if ((mask >> bit) & 1) != 0 {
                table[mask][rank] = bit as u8;
                rank += 1;
            }
            bit += 1;
        }
        mask += 1;
    }
    table
}

const SELECT8: [[u8; 8]; 256] = build_select8();

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

#[inline]
fn select_nth_set_bit_loop(mut word: u64, mut n: usize) -> usize {
    debug_assert!(n < word.count_ones() as usize);
    loop {
        let bit = word.trailing_zeros() as usize;
        if n == 0 {
            return bit;
        }
        word &= word - 1;
        n -= 1;
    }
}

#[inline]
fn select_nth_set_bit_lut(mut word: u64, mut n: usize) -> usize {
    debug_assert!(n < word.count_ones() as usize);

    let mut byte_i = 0usize;
    while byte_i < 8 {
        let byte = word as u8;
        let count = byte.count_ones() as usize;
        if n < count {
            return byte_i * 8 + SELECT8[byte as usize][n] as usize;
        }
        n -= count;
        word >>= 8;
        byte_i += 1;
    }

    unreachable!("rank must name a set bit")
}

#[inline]
fn select_nth_set_bit_binary(mut word: u64, mut n: usize) -> usize {
    debug_assert!(n < word.count_ones() as usize);

    let mut base = 0usize;
    let mut width = 32usize;
    while width != 0 {
        let low_mask = (1u64 << width) - 1;
        let low = word & low_mask;
        let low_count = low.count_ones() as usize;
        if n < low_count {
            word = low;
        } else {
            n -= low_count;
            word >>= width;
            base += width;
        }
        width >>= 1;
    }
    base
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "bmi2")]
unsafe fn select_nth_set_bit_pdep(word: u64, n: usize) -> usize {
    use core::arch::x86_64::_pdep_u64;

    debug_assert!(n < word.count_ones() as usize);
    let deposited = _pdep_u64(1u64 << n, word);
    deposited.trailing_zeros() as usize
}

fn pseudo_random_ranks() -> [usize; RANDOM_QUERY_COUNT] {
    // Deterministic xorshift sequence. The RNG runs during benchmark setup, not
    // in the measured loop; the benchmark sees a stable but nontrivial rank mix.
    let mut ranks = [0usize; RANDOM_QUERY_COUNT];
    let mut x = 0x9e37_79b9_7f4a_7c15u64;
    let mut i = 0usize;
    while i < RANDOM_QUERY_COUNT {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        ranks[i] = (x as usize) & 255;
        i += 1;
    }
    ranks
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

fn bench_query_ranks(c: &mut Criterion) {
    let partition = BoundaryMerge::from_weights(&skewed_weights()).unwrap();
    let mut group = c.benchmark_group("partition/query_by_rank");
    group.throughput(Throughput::Elements(1));

    for rank in PARTITION_RANKS {
        group.bench_with_input(
            BenchmarkId::new("select_symbol", rank),
            &rank,
            |b, &rank| b.iter(|| partition.select_symbol(black_box(rank))),
        );
        group.bench_with_input(BenchmarkId::new("select_byte", rank), &rank, |b, &rank| {
            b.iter(|| partition.select_byte(black_box(rank)))
        });
    }

    group.finish();
}

fn bench_random_queries(c: &mut Criterion) {
    let partition = BoundaryMerge::from_weights(&skewed_weights()).unwrap();
    let ranks = pseudo_random_ranks();
    let mut group = c.benchmark_group("partition/random_queries");
    group.throughput(Throughput::Elements(RANDOM_QUERY_COUNT as u64));

    group.bench_function("select_symbol", |b| {
        b.iter(|| {
            let mut checksum = 0usize;
            for &rank in &ranks {
                checksum ^= partition.select_symbol(black_box(rank));
            }
            black_box(checksum)
        })
    });

    group.bench_function("select_byte", |b| {
        b.iter(|| {
            let mut checksum = 0usize;
            for &rank in &ranks {
                checksum ^= partition.select_byte(black_box(rank));
            }
            black_box(checksum)
        })
    });

    group.finish();
}

fn bench_select_primitive(c: &mut Criterion) {
    // All bits set makes every rank 0..63 valid and isolates rank dependence in
    // the selection algorithm itself. black_box prevents constant folding.
    let word = u64::MAX;

    for rank in 0..64 {
        let expected = select_nth_set_bit_loop(word, rank);
        assert_eq!(select_nth_set_bit_lut(word, rank), expected);
        assert_eq!(select_nth_set_bit_binary(word, rank), expected);

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("bmi2") {
            // SAFETY: guarded by runtime BMI2 detection.
            assert_eq!(unsafe { select_nth_set_bit_pdep(word, rank) }, expected);
        }
    }

    let mut group = c.benchmark_group("partition/select_primitive");
    group.throughput(Throughput::Elements(1));

    for rank in SELECT_RANKS {
        group.bench_with_input(BenchmarkId::new("loop", rank), &rank, |b, &rank| {
            b.iter(|| select_nth_set_bit_loop(black_box(word), black_box(rank)))
        });

        group.bench_with_input(BenchmarkId::new("lut8", rank), &rank, |b, &rank| {
            b.iter(|| select_nth_set_bit_lut(black_box(word), black_box(rank)))
        });

        group.bench_with_input(BenchmarkId::new("binary", rank), &rank, |b, &rank| {
            b.iter(|| select_nth_set_bit_binary(black_box(word), black_box(rank)))
        });

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("bmi2") {
            group.bench_with_input(BenchmarkId::new("pdep", rank), &rank, |b, &rank| {
                b.iter(|| {
                    // SAFETY: this benchmark is registered only after runtime
                    // BMI2 detection above.
                    unsafe { select_nth_set_bit_pdep(black_box(word), black_box(rank)) }
                })
            });
        }
    }

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

criterion_group!(
    benches,
    bench_build,
    bench_queries,
    bench_query_ranks,
    bench_random_queries,
    bench_select_primitive,
    bench_full_scans
);
criterion_main!(benches);
