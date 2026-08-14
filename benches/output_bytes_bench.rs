use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

const TAG: usize = 1usize << (usize::BITS - 1);
const COUNT_MASK: usize = !TAG;

const COUNTS: &[(&str, usize)] = &[
    ("single-byte", 0),
    ("two-bytes", 1),
    ("three-bytes", 2),
    ("inline-capacity", 3),
    ("first-compressed", 4),
    ("short-run", 8),
    ("medium-run", 100),
    ("long-run", 4_096),
    ("pathological-run", 65_536),
];

const REPEATED_BYTES: &[(&str, u8)] = &[("zero", 0x00), ("ff", 0xff)];

#[derive(Clone, Copy)]
struct IsizeOutput {
    bytes: [u8; 4],
    remaining: isize,
}

impl IsizeOutput {
    fn run(first: u8, repeated: u8, count: usize) -> Self {
        let len = count + 1;
        let mut bytes = [0; 4];
        bytes[0] = first;

        if len <= bytes.len() {
            bytes[1..len].fill(repeated);
            Self {
                bytes,
                remaining: len as isize,
            }
        } else {
            bytes[1..].fill(repeated);
            Self {
                bytes,
                remaining: if repeated == 0xff {
                    len as isize
                } else {
                    -(len as isize)
                },
            }
        }
    }

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        let remaining = self.remaining.unsigned_abs();
        if remaining == 0 {
            return None;
        }

        let [first, a, b, c] = self.bytes;
        if remaining > self.bytes.len() {
            self.bytes[0] = if self.remaining > 0 { 0xff } else { 0x00 };
            self.remaining += if self.remaining > 0 { -1 } else { 1 };
        } else {
            self.bytes = [a, b, c, 0];
            self.remaining = (remaining - 1) as isize;
        }
        Some(first)
    }
}

#[derive(Clone, Copy)]
struct TaggedOutput {
    bytes: [u8; 4],
    state: usize,
}

impl TaggedOutput {
    fn run(first: u8, repeated: u8, count: usize) -> Self {
        let len = count + 1;
        let mut bytes = [0; 4];
        bytes[0] = first;

        if len <= bytes.len() {
            bytes[1..len].fill(repeated);
            Self { bytes, state: len }
        } else {
            bytes[1..].fill(repeated);
            Self {
                bytes,
                state: len | if repeated == 0xff { TAG } else { 0 },
            }
        }
    }

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        let remaining = self.state & COUNT_MASK;
        if remaining == 0 {
            return None;
        }

        let [first, a, b, c] = self.bytes;
        if remaining > self.bytes.len() {
            self.bytes[0] = if self.state & TAG != 0 { 0xff } else { 0x00 };
            self.state -= 1;
        } else {
            self.bytes = [a, b, c, 0];
            self.state = remaining - 1;
        }
        Some(first)
    }
}

#[inline]
fn consume_isize(mut output: IsizeOutput) -> u64 {
    let mut checksum = 0u64;

    while let Some(byte) = output.next() {
        checksum = checksum.wrapping_add(byte as u64);
    }

    checksum
}

#[inline]
fn consume_tagged(mut output: TaggedOutput) -> u64 {
    let mut checksum = 0u64;

    while let Some(byte) = output.next() {
        checksum = checksum.wrapping_add(byte as u64);
    }

    checksum
}

fn output_bytes(c: &mut Criterion) {
    for &(count_name, count) in COUNTS {
        for &(repeated_name, repeated) in REPEATED_BYTES {
            if count == 0 && repeated != 0 {
                continue;
            }

            let mut group = c.benchmark_group(format!("output-bytes/{count_name}/{repeated_name}"));
            group.throughput(Throughput::Bytes((count + 1) as u64));

            group.bench_function(BenchmarkId::from_parameter("isize"), |b| {
                b.iter(|| {
                    let output =
                        IsizeOutput::run(black_box(0x42), black_box(repeated), black_box(count));
                    black_box(consume_isize(output))
                });
            });

            group.bench_function(BenchmarkId::from_parameter("tagged"), |b| {
                b.iter(|| {
                    let output =
                        TaggedOutput::run(black_box(0x42), black_box(repeated), black_box(count));
                    black_box(consume_tagged(output))
                });
            });

            group.finish();
        }
    }
}

#[inline(always)]
fn split_q16(range: u32, raw: u16) -> u32 {
    ((range as u64 * raw as u64) >> 16) as u32
}

#[inline(always)]
fn split_canonical_q17(range: u32, raw: u16) -> u32 {
    let probability = (1u32 << 16) | raw as u32;
    ((range as u64 * probability as u64) >> 17) as u32
}

fn probability_split(c: &mut Criterion) {
    const N: u32 = 1024;

    let mut group = c.benchmark_group("probability-split");
    group.throughput(Throughput::Elements(N as u64));

    group.bench_function("q16-full-interval", |b| {
        b.iter(|| {
            let mut checksum = 0u32;
            for i in 0..N {
                let range = black_box(0x01_0001 + ((i * 7919) & 0xfe_fffe));
                let raw = black_box((i.wrapping_mul(40503) as u16).wrapping_add(1));
                checksum = checksum.wrapping_add(split_q16(range, raw));
            }
            black_box(checksum)
        });
    });

    group.bench_function("canonical-q17-half-interval", |b| {
        b.iter(|| {
            let mut checksum = 0u32;
            for i in 0..N {
                let range = black_box(0x01_0001 + ((i * 7919) & 0xfe_fffe));
                let raw = black_box(i.wrapping_mul(40503) as u16);
                checksum = checksum.wrapping_add(split_canonical_q17(range, raw));
            }
            black_box(checksum)
        });
    });

    group.finish();
}

criterion_group!(benches, output_bytes, probability_split);
criterion_main!(benches);
