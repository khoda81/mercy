use std::hint::black_box;
use std::marker::PhantomData;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use mercy::{FractionalU16, RangeDecoder, RangeEncoder};

const RANGE_BITS: u32 = 24;
const FULL_RANGE: u32 = 1 << RANGE_BITS;
const RANGE_TOP: u32 = 1 << (RANGE_BITS - 8);

const TAG: usize = 1usize << (usize::BITS - 1);
const COUNT_MASK: usize = !TAG;

trait Emission: Iterator<Item = u8> + Sized {
    fn empty() -> Self;
    fn push(&mut self, first: u8, repeated: u8, repeated_count: usize);
}

#[derive(Clone, Copy)]
struct IsizeOutput {
    bytes: [u8; 4],
    remaining: isize,
}

impl IsizeOutput {
    fn push_byte(&mut self, byte: u8) {
        let remaining = self.remaining.unsigned_abs();

        if remaining < self.bytes.len() {
            self.bytes[remaining] = byte;
            self.remaining = (remaining + 1) as isize;
            return;
        }

        let [first, a, b, c] = self.bytes;

        if remaining == self.bytes.len() {
            self.remaining = if a == 0xff { 5 } else { -5 };
        } else {
            self.remaining += if self.remaining > 0 { 1 } else { -1 };
        }

        self.bytes = [first, b, c, byte];
    }
}

impl Emission for IsizeOutput {
    fn empty() -> Self {
        Self {
            bytes: [0; 4],
            remaining: 0,
        }
    }

    fn push(&mut self, first: u8, repeated: u8, repeated_count: usize) {
        if self.remaining == 0 {
            let len = repeated_count + 1;
            self.bytes[0] = first;

            if len <= self.bytes.len() {
                self.bytes[1..len].fill(repeated);
                self.remaining = len as isize;
            } else {
                self.bytes[1..].fill(repeated);
                self.remaining = if repeated == 0xff {
                    len as isize
                } else {
                    -(len as isize)
                };
            }

            return;
        }

        self.push_byte(first);
        for _ in 0..repeated_count {
            self.push_byte(repeated);
        }
    }
}

impl Iterator for IsizeOutput {
    type Item = u8;

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
    fn push_byte(&mut self, byte: u8) {
        let remaining = self.state & COUNT_MASK;

        if remaining < self.bytes.len() {
            self.bytes[remaining] = byte;
            self.state = remaining + 1;
            return;
        }

        let [first, a, b, c] = self.bytes;

        if remaining == self.bytes.len() {
            self.state = 5 | if a == 0xff { TAG } else { 0 };
        } else {
            self.state += 1;
        }

        self.bytes = [first, b, c, byte];
    }
}

impl Emission for TaggedOutput {
    fn empty() -> Self {
        Self {
            bytes: [0; 4],
            state: 0,
        }
    }

    fn push(&mut self, first: u8, repeated: u8, repeated_count: usize) {
        if self.state == 0 {
            let len = repeated_count + 1;
            self.bytes[0] = first;

            if len <= self.bytes.len() {
                self.bytes[1..len].fill(repeated);
                self.state = len;
            } else {
                self.bytes[1..].fill(repeated);
                self.state = len | if repeated == 0xff { TAG } else { 0 };
            }

            return;
        }

        self.push_byte(first);
        for _ in 0..repeated_count {
            self.push_byte(repeated);
        }
    }
}

impl Iterator for TaggedOutput {
    type Item = u8;

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

struct BenchEncoder<O> {
    low: u32,
    denominator: u32,
    pending: usize,
    output: PhantomData<fn() -> O>,
}

impl<O: Emission> BenchEncoder<O> {
    fn new() -> Self {
        Self {
            low: 0,
            denominator: FULL_RANGE - 1,
            pending: 0,
            output: PhantomData,
        }
    }

    #[inline(always)]
    fn put(&mut self, raw: u16, lower: bool) -> O {
        let mut output = O::empty();
        let split = split(self.denominator + 1, raw);

        if lower {
            self.denominator = split - 1;
        } else {
            let [old_head, ..] = self.low.to_be_bytes();

            self.low = self.low.wrapping_add(split);
            self.denominator -= split;
            let [head, ..] = self.low.to_be_bytes();

            if head != old_head {
                if self.pending == 0 {
                    self.pending = 1;
                } else {
                    output.push(head, 0x00, self.pending - 1);

                    let [_, a, b, c] = self.low.to_be_bytes();
                    self.low = u32::from_be_bytes([0, a, b, c]);
                    self.pending = 0;
                }
            }
        }

        self.renormalize(&mut output);
        output
    }

    #[inline(always)]
    fn renormalize(&mut self, output: &mut O) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
                let [head, next, a, b] = self.low.to_be_bytes();

                if self.pending == 0 {
                    self.low = u32::from_be_bytes([next, a, b, 0]);
                    self.pending = 1;
                } else if next == 0xff {
                    self.low = u32::from_be_bytes([head, a, b, 0]);
                    self.pending += 1;
                } else {
                    output.push(head, 0xff, self.pending - 1);
                    self.low = u32::from_be_bytes([next, a, b, 0]);
                    self.pending = 1;
                }

                self.denominator = append_byte(self.denominator, 0xff);
            }
        }
    }

    fn finish(mut self) -> O {
        let mut output = O::empty();
        self.denominator = 0;
        self.renormalize(&mut output);

        if self.pending != 0 {
            let [head, ..] = self.low.to_be_bytes();
            output.push(head, 0xff, self.pending - 1);
        }

        output
    }
}

#[inline(always)]
fn split(range: u32, raw: u16) -> u32 {
    let probability = (1u32 << 16) | raw as u32;
    ((range as u64 * probability as u64) >> 17) as u32
}

#[inline(always)]
fn append_byte(value: u32, byte: u8) -> u32 {
    let [_, a, b, c] = value.to_be_bytes();
    u32::from_be_bytes([a, b, c, byte])
}

fn transcode<O: Emission>(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = RangeDecoder::new(source);
    let mut encoder = BenchEncoder::<O>::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let lower = decoder.test(FractionalU16::from_raw(raw));
        output.extend(encoder.put(raw, lower));
    }

    output.extend(encoder.finish());
    output
}

fn transcode_production(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = RangeDecoder::new(source);
    let mut encoder = RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = FractionalU16::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
    }

    output.extend(encoder.finish());
    output
}

fn decode_choices(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = RangeDecoder::new(source);
    probabilities
        .iter()
        .map(|&raw| u8::from(decoder.test(FractionalU16::from_raw(raw))))
        .collect()
}

fn verify(source: &[u8], probabilities: &[u16], expected: &[u8]) {
    assert_eq!(decode_choices(source, probabilities), expected);
}

fn workload(events: usize) -> (Vec<u16>, Vec<u8>) {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;

    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let probabilities = (0..events).map(|_| random() as u16).collect();

    // Initialization and every event can pull at most three bytes, so this
    // guarantees the benchmark never reaches implicit zero-extended EOF.
    let source = (0..events * 3 + 3).map(|_| random() as u8).collect();

    (probabilities, source)
}

fn coder_round_trip(c: &mut Criterion) {
    const EVENTS: usize = 1 << 20;

    let (probabilities, source) = workload(EVENTS);
    let choices = decode_choices(&source, &probabilities);

    let isize_bytes = transcode::<IsizeOutput>(&source, &probabilities);
    let tagged_bytes = transcode::<TaggedOutput>(&source, &probabilities);
    let production_bytes = transcode_production(&source, &probabilities);

    assert_eq!(isize_bytes, tagged_bytes);
    assert_eq!(tagged_bytes, production_bytes);
    verify(&production_bytes, &probabilities, &choices);

    eprintln!(
        "decoder-sampled workload: {EVENTS} events -> {} canonical bytes ({:.4} bits/event)",
        production_bytes.len(),
        production_bytes.len() as f64 * 8.0 / EVENTS as f64
    );

    let mut group = c.benchmark_group("coder-round-trip");
    group.throughput(Throughput::Elements(EVENTS as u64));

    group.bench_function("isize", |b| {
        b.iter(|| {
            black_box(transcode::<IsizeOutput>(
                black_box(&source),
                black_box(&probabilities),
            ))
        });
    });

    group.bench_function("tagged", |b| {
        b.iter(|| {
            black_box(transcode::<TaggedOutput>(
                black_box(&source),
                black_box(&probabilities),
            ))
        });
    });

    group.bench_function("production", |b| {
        b.iter(|| {
            black_box(transcode_production(
                black_box(&source),
                black_box(&probabilities),
            ))
        });
    });

    group.finish();
}

criterion_group!(benches, coder_round_trip);
criterion_main!(benches);
