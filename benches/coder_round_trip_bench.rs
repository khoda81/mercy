use std::hint::black_box;
use std::marker::PhantomData;
use std::time::{Duration, Instant};

use criterion::measurement::{Measurement, ValueFormatter};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use mercy::{FractionalU16, RangeDecoder, RangeEncoder};

const RANGE_BITS: u32 = 24;
const FULL_RANGE: u32 = 1 << RANGE_BITS;
const RANGE_TOP: u32 = 1 << (RANGE_BITS - 8);

const TAG: usize = 1usize << (usize::BITS - 1);
const COUNT_MASK: usize = !TAG;

struct EventTime;
struct EventFormatter;

impl Measurement for EventTime {
    type Intermediate = Instant;
    type Value = Duration;

    fn start(&self) -> Self::Intermediate {
        Instant::now()
    }

    fn end(&self, started: Self::Intermediate) -> Self::Value {
        started.elapsed()
    }

    fn add(&self, a: &Self::Value, b: &Self::Value) -> Self::Value {
        *a + *b
    }

    fn zero(&self) -> Self::Value {
        Duration::ZERO
    }

    fn to_f64(&self, value: &Self::Value) -> f64 {
        value.as_nanos() as f64
    }

    fn formatter(&self) -> &dyn ValueFormatter {
        &EventFormatter
    }
}

impl EventFormatter {
    fn events_per_second(
        &self,
        events: f64,
        typical: f64,
        values: &mut [f64],
    ) -> &'static str {
        let events_per_second = events * 1e9 / typical;
        let (scale, unit) = if events_per_second < 1e3 {
            (1.0, "event/s")
        } else if events_per_second < 1e6 {
            (1e3, "Kevent/s")
        } else if events_per_second < 1e9 {
            (1e6, "Mevent/s")
        } else {
            (1e9, "Gevent/s")
        };

        for value in values {
            *value = events * 1e9 / *value / scale;
        }

        unit
    }

    fn bytes_per_second(
        &self,
        bytes: f64,
        typical: f64,
        values: &mut [f64],
    ) -> &'static str {
        let bytes_per_second = bytes * 1e9 / typical;
        let (scale, unit) = if bytes_per_second < 1024.0 {
            (1.0, "B/s")
        } else if bytes_per_second < 1024.0 * 1024.0 {
            (1024.0, "KiB/s")
        } else if bytes_per_second < 1024.0 * 1024.0 * 1024.0 {
            (1024.0 * 1024.0, "MiB/s")
        } else {
            (1024.0 * 1024.0 * 1024.0, "GiB/s")
        };

        for value in values {
            *value = bytes * 1e9 / *value / scale;
        }

        unit
    }

    fn decimal_bytes_per_second(
        &self,
        bytes: f64,
        typical: f64,
        values: &mut [f64],
    ) -> &'static str {
        let bytes_per_second = bytes * 1e9 / typical;
        let (scale, unit) = if bytes_per_second < 1e3 {
            (1.0, "B/s")
        } else if bytes_per_second < 1e6 {
            (1e3, "KB/s")
        } else if bytes_per_second < 1e9 {
            (1e6, "MB/s")
        } else {
            (1e9, "GB/s")
        };

        for value in values {
            *value = bytes * 1e9 / *value / scale;
        }

        unit
    }
}

impl ValueFormatter for EventFormatter {
    fn scale_values(&self, typical: f64, values: &mut [f64]) -> &'static str {
        let (factor, unit) = if typical < 1.0 {
            (1e3, "ps")
        } else if typical < 1e3 {
            (1.0, "ns")
        } else if typical < 1e6 {
            (1e-3, "µs")
        } else if typical < 1e9 {
            (1e-6, "ms")
        } else {
            (1e-9, "s")
        };

        for value in values {
            *value *= factor;
        }

        unit
    }

    fn scale_throughputs(
        &self,
        typical: f64,
        throughput: &Throughput,
        values: &mut [f64],
    ) -> &'static str {
        match *throughput {
            Throughput::Elements(events) => {
                self.events_per_second(events as f64, typical, values)
            }
            Throughput::ElementsAndBytes { elements, .. } => {
                self.events_per_second(elements as f64, typical, values)
            }
            Throughput::Bytes(bytes) => self.bytes_per_second(bytes as f64, typical, values),
            Throughput::BytesDecimal(bytes) => {
                self.decimal_bytes_per_second(bytes as f64, typical, values)
            }
            Throughput::Bits(bits) => {
                self.decimal_bytes_per_second(bits as f64 / 8.0, typical, values)
            }
        }
    }

    fn scale_for_machines(&self, _values: &mut [f64]) -> &'static str {
        "ns"
    }
}

trait ProbabilityGrid {
    fn split(range: u32, raw: u16) -> u32;
}

struct Q16;
struct Q17;

impl ProbabilityGrid for Q16 {
    #[inline(always)]
    fn split(range: u32, raw: u16) -> u32 {
        debug_assert!(raw >= 1 << 15);
        ((range as u64 * raw as u64) >> 16) as u32
    }
}

impl ProbabilityGrid for Q17 {
    #[inline(always)]
    fn split(range: u32, raw: u16) -> u32 {
        let probability = (1u32 << 16) | raw as u32;
        ((range as u64 * probability as u64) >> 17) as u32
    }
}

#[derive(Clone, Copy)]
struct OutputBytes {
    bytes: [u8; 4],
    state: usize,
}

impl OutputBytes {
    fn new() -> Self {
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

impl Iterator for OutputBytes {
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

struct BenchEncoder<G> {
    low: u32,
    denominator: u32,
    pending: usize,
    grid: PhantomData<fn() -> G>,
}

impl<G: ProbabilityGrid> BenchEncoder<G> {
    fn new() -> Self {
        Self {
            low: 0,
            denominator: FULL_RANGE - 1,
            pending: 0,
            grid: PhantomData,
        }
    }

    #[inline(always)]
    fn put(&mut self, raw: u16, lower: bool) -> OutputBytes {
        let mut output = OutputBytes::new();
        let split = G::split(self.denominator + 1, raw);

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
    fn renormalize(&mut self, output: &mut OutputBytes) {
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

    fn finish(mut self) -> OutputBytes {
        let mut output = OutputBytes::new();
        self.denominator = 0;
        self.renormalize(&mut output);

        if self.pending != 0 {
            let [head, ..] = self.low.to_be_bytes();
            output.push(head, 0xff, self.pending - 1);
        }

        output
    }
}

struct BenchDecoder<'a, G> {
    numerator: u32,
    denominator: u32,
    input: &'a [u8],
    grid: PhantomData<fn() -> G>,
}

impl<'a, G: ProbabilityGrid> BenchDecoder<'a, G> {
    fn new(input: &'a [u8]) -> Self {
        let mut decoder = Self {
            numerator: 0,
            denominator: 0,
            input,
            grid: PhantomData,
        };
        decoder.renormalize();
        decoder
    }

    #[inline(always)]
    fn test(&mut self, raw: u16) -> bool {
        let split = G::split(self.denominator + 1, raw);
        let lower = self.numerator < split;

        if lower {
            self.denominator = split - 1;
        } else {
            self.numerator -= split;
            self.denominator -= split;
        }

        self.renormalize();
        lower
    }

    #[inline(always)]
    fn renormalize(&mut self) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
                self.denominator = append_byte(self.denominator, 0xff);
                self.numerator = append_byte(self.numerator, self.read_byte());
            }
        }
    }

    #[inline(always)]
    fn read_byte(&mut self) -> u8 {
        let [byte, rest @ ..] = self.input else {
            return 0;
        };
        self.input = rest;
        *byte
    }
}

#[inline(always)]
fn append_byte(value: u32, byte: u8) -> u32 {
    let [_, a, b, c] = value.to_be_bytes();
    u32::from_be_bytes([a, b, c, byte])
}

fn transcode<G: ProbabilityGrid>(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = BenchDecoder::<G>::new(source);
    let mut encoder = BenchEncoder::<G>::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let lower = decoder.test(raw);
        output.extend(encoder.put(raw, lower));
    }

    output.extend(encoder.finish());
    output
}

fn decode_choices<G: ProbabilityGrid>(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = BenchDecoder::<G>::new(source);
    probabilities
        .iter()
        .map(|&raw| u8::from(decoder.test(raw)))
        .collect()
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

fn workload(events: usize) -> (Vec<u8>, Vec<u16>, Vec<u16>) {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;

    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let q16: Vec<u16> = (0..events)
        .map(|_| 0x8000 | (random() as u16 & 0x7fff))
        .collect();
    let q17 = q16.iter().map(|&raw| (raw - 0x8000) << 1).collect();

    // Initialization and every event can pull at most three bytes, so this
    // guarantees the benchmark never reaches implicit zero-extended EOF.
    let source = (0..events * 3 + 3).map(|_| random() as u8).collect();

    (source, q16, q17)
}

fn coder_round_trip(c: &mut Criterion<EventTime>) {
    const EVENTS: usize = 1 << 20;

    let (source, q16, q17) = workload(EVENTS);

    let q16_choices = decode_choices::<Q16>(&source, &q16);
    let q17_choices = decode_choices::<Q17>(&source, &q17);
    assert_eq!(q16_choices, q17_choices);

    let q16_bytes = transcode::<Q16>(&source, &q16);
    let q17_bytes = transcode::<Q17>(&source, &q17);
    assert_eq!(q16_bytes, q17_bytes);

    let production = transcode_production(&source, &q17);
    assert_eq!(q17_bytes, production);
    assert_eq!(decode_choices::<Q17>(&production, &q17), q17_choices);

    eprintln!(
        "matched workload: {EVENTS} events -> {} canonical bytes ({:.4} bits/event)",
        production.len(),
        production.len() as f64 * 8.0 / EVENTS as f64
    );

    let mut group = c.benchmark_group("coder-decode-encode");
    group.throughput(Throughput::ElementsAndBytes {
        elements: EVENTS as u64,
        bytes: production.len() as u64,
    });

    group.bench_function("q16", |b| {
        b.iter(|| black_box(transcode::<Q16>(black_box(&source), black_box(&q16))));
    });

    group.bench_function("q17", |b| {
        b.iter(|| black_box(transcode::<Q17>(black_box(&source), black_box(&q17))));
    });

    group.finish();
}

fn event_criterion() -> Criterion<EventTime> {
    Criterion::default().with_measurement(EventTime)
}

criterion_group! {
    name = benches;
    config = event_criterion();
    targets = coder_round_trip
}
criterion_main!(benches);
