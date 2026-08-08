use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use mercy::{compress, decompress};

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

fn bench_reference_codec(c: &mut Criterion) {
    let mut group = c.benchmark_group("reference_codec");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    let input_16 = b"mercy beaucoup!!!";
    let uniform = uniform_weights();
    group.throughput(Throughput::Bytes(input_16.len() as u64));
    group.bench_function("compress_16_uniform", |b| {
        b.iter(|| compress(black_box(uniform), black_box(input_16)).unwrap())
    });

    let encoded = compress(uniform, input_16).unwrap();
    group.bench_function("decompress_16_uniform", |b| {
        b.iter(|| decompress(black_box(uniform), black_box(&encoded)).unwrap())
    });

    let input_64 = b"the byte stream and symbol stream constrain each other; mercy beaucoup!";
    let skewed = skewed_weights();
    group.throughput(Throughput::Bytes(input_64.len() as u64));
    group.bench_function("roundtrip_64_skewed", |b| {
        b.iter(|| {
            let encoded = compress(black_box(skewed), black_box(input_64)).unwrap();
            decompress(black_box(skewed), black_box(&encoded)).unwrap()
        })
    });

    group.finish();
}

criterion_group!(benches, bench_reference_codec);
criterion_main!(benches);
