use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::measurement::{Measurement, ValueFormatter};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use mercy::coder::implementations::{branchless, q16, range_shift};
use mercy::coder::{branchless_inline, legacy, legacy_inline, range_shift_inline};
use mercy::{FractionalU16, RangeDecoder, RangeEncoder};

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
    fn events_per_second(&self, events: f64, typical: f64, values: &mut [f64]) -> &'static str {
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

    fn bytes_per_second(&self, bytes: f64, typical: f64, values: &mut [f64]) -> &'static str {
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
            Throughput::Elements(events) => self.events_per_second(events as f64, typical, values),
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

fn transcode_q16(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = q16::RangeDecoder::new(source);
    let mut encoder = q16::RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = q16::Probability::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
    }

    output.extend(encoder.finish());
    output
}

fn transcode_legacy(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = legacy::RangeDecoder::new(source);
    let mut encoder = legacy::RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = FractionalU16::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
    }

    output.extend(encoder.finish());
    output
}

fn transcode_legacy_inline(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = legacy_inline::RangeDecoder::new(source);
    let mut encoder = legacy_inline::RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = FractionalU16::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
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

fn transcode_range_shift(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = range_shift::RangeDecoder::new(source);
    let mut encoder = range_shift::RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = FractionalU16::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
    }

    output.extend(encoder.finish());
    output
}

fn transcode_range_shift_inline(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = range_shift_inline::RangeDecoder::new(source);
    let mut encoder = range_shift_inline::RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = FractionalU16::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
    }

    output.extend(encoder.finish());
    output
}

fn transcode_branchless(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = branchless::RangeDecoder::new(source);
    let mut encoder = branchless::RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = FractionalU16::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
    }

    output.extend(encoder.finish());
    output
}

fn transcode_branchless_inline(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = branchless_inline::RangeDecoder::new(source);
    let mut encoder = branchless_inline::RangeEncoder::new();
    let mut output = Vec::with_capacity(probabilities.len() / 8 + 16);

    for &raw in probabilities {
        let p = FractionalU16::from_raw(raw);
        let lower = decoder.test(p);
        output.extend(encoder.put(p, lower));
    }

    output.extend(encoder.finish());
    output
}

fn decode_q16(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = q16::RangeDecoder::new(source);
    probabilities
        .iter()
        .map(|&raw| u8::from(decoder.test(q16::Probability::from_raw(raw))))
        .collect()
}

fn decode_production(source: &[u8], probabilities: &[u16]) -> Vec<u8> {
    let mut decoder = RangeDecoder::new(source);
    probabilities
        .iter()
        .map(|&raw| u8::from(decoder.test(FractionalU16::from_raw(raw))))
        .collect()
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

    let source = (0..events * 3 + 3).map(|_| random() as u8).collect();

    (source, q16, q17)
}

fn coder_round_trip(c: &mut Criterion<EventTime>) {
    const EVENTS: usize = 1 << 20;

    let (source, q16_probabilities, q17_probabilities) = workload(EVENTS);

    let q16_choices = decode_q16(&source, &q16_probabilities);
    let production_choices = decode_production(&source, &q17_probabilities);
    assert_eq!(q16_choices, production_choices);

    let q16_bytes = transcode_q16(&source, &q16_probabilities);
    let legacy_bytes = transcode_legacy(&source, &q17_probabilities);
    let legacy_inline_bytes = transcode_legacy_inline(&source, &q17_probabilities);
    let production = transcode_production(&source, &q17_probabilities);
    let range_shift_bytes = transcode_range_shift(&source, &q17_probabilities);
    let range_shift_inline_bytes = transcode_range_shift_inline(&source, &q17_probabilities);
    let branchless_bytes = transcode_branchless(&source, &q17_probabilities);
    let branchless_inline_bytes = transcode_branchless_inline(&source, &q17_probabilities);

    assert_eq!(q16_bytes, production);
    assert_eq!(legacy_bytes, production);
    assert_eq!(legacy_inline_bytes, production);
    assert_eq!(range_shift_bytes, production);
    assert_eq!(range_shift_inline_bytes, production);
    assert_eq!(branchless_bytes, production);
    assert_eq!(branchless_inline_bytes, production);
    assert_eq!(
        decode_production(&production, &q17_probabilities),
        production_choices
    );

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

    group.bench_function("q16-shift", |b| {
        b.iter(|| {
            black_box(transcode_q16(
                black_box(&source),
                black_box(&q16_probabilities),
            ))
        });
    });

    group.bench_function("q17-legacy", |b| {
        b.iter(|| {
            black_box(transcode_legacy(
                black_box(&source),
                black_box(&q17_probabilities),
            ))
        });
    });

    group.bench_function("q17-legacy-inline", |b| {
        b.iter(|| {
            black_box(transcode_legacy_inline(
                black_box(&source),
                black_box(&q17_probabilities),
            ))
        });
    });

    group.bench_function("q17-shift-production", |b| {
        b.iter(|| {
            black_box(transcode_production(
                black_box(&source),
                black_box(&q17_probabilities),
            ))
        });
    });

    group.bench_function("range-shift", |b| {
        b.iter(|| {
            black_box(transcode_range_shift(
                black_box(&source),
                black_box(&q17_probabilities),
            ))
        });
    });

    group.bench_function("range-shift-inline", |b| {
        b.iter(|| {
            black_box(transcode_range_shift_inline(
                black_box(&source),
                black_box(&q17_probabilities),
            ))
        });
    });

    group.bench_function("branchless", |b| {
        b.iter(|| {
            black_box(transcode_branchless(
                black_box(&source),
                black_box(&q17_probabilities),
            ))
        });
    });

    group.bench_function("branchless-inline", |b| {
        b.iter(|| {
            black_box(transcode_branchless_inline(
                black_box(&source),
                black_box(&q17_probabilities),
            ))
        });
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
