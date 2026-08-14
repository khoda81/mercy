const STORED_PROBABILITY_BITS: u32 = 16;
const PROBABILITY_BITS: u32 = STORED_PROBABILITY_BITS + 1;
const HALF_PROBABILITY: u32 = 1 << STORED_PROBABILITY_BITS;
const RANGE_BITS: u32 = 24;

const FULL_RANGE: u32 = 1 << RANGE_BITS;
const RANGE_TOP: u32 = 1 << (RANGE_BITS - 8);

const OUTPUT_REPEAT_TAG: usize = 1usize << (usize::BITS - 1);
const OUTPUT_COUNT_MASK: usize = !OUTPUT_REPEAT_TAG;

/// Probability of the lower branch on the canonical half-interval grid.
///
/// `raw = k` represents `(2^16 + k) / 2^17`, so every value lies in
/// `[1/2, 1)`. The lower branch is therefore always at least as probable as
/// the upper branch, while the `u16` payload provides 17-bit absolute
/// probability resolution.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FractionalU16(u16);

impl FractionalU16 {
    pub const HALF: Self = Self(0);
    pub const MAX: Self = Self(u16::MAX);

    #[inline]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }
}

#[inline(always)]
fn split(range: u32, p: FractionalU16) -> u32 {
    let probability = HALF_PROBABILITY | p.raw() as u32;
    ((range as u64 * probability as u64) >> PROBABILITY_BITS) as u32
}

/// Lazily emitted bytes.
///
/// At most one run can be arbitrarily long. When more than four bytes remain,
/// `bytes[0]` is the first byte, `bytes[1..]` are the final three bytes, and
/// the high bit of `state` selects the omitted middle byte (`0xff` when set,
/// `0x00` when clear). The remaining bits are the byte count.
#[must_use]
pub struct OutputBytes {
    bytes: [u8; 4],
    state: usize,
}

impl OutputBytes {
    const fn new() -> Self {
        Self {
            bytes: [0; 4],
            state: 0,
        }
    }

    fn push(&mut self, first: u8, repeated: u8, repeated_count: usize) {
        debug_assert!(repeated == 0 || repeated == 0xff);

        if self.state == 0 {
            let len = repeated_count + 1;
            self.bytes[0] = first;

            if len <= self.bytes.len() {
                self.bytes[1..len].fill(repeated);
                self.state = len;
            } else {
                self.bytes[1..].fill(repeated);
                self.state = len
                    | if repeated == 0xff {
                        OUTPUT_REPEAT_TAG
                    } else {
                        0
                    };
            }

            return;
        }

        debug_assert!(repeated_count <= 2);
        self.push_byte(first);

        for _ in 0..repeated_count {
            self.push_byte(repeated);
        }
    }

    fn push_byte(&mut self, byte: u8) {
        let remaining = self.state & OUTPUT_COUNT_MASK;

        if remaining < self.bytes.len() {
            self.bytes[remaining] = byte;
            self.state = remaining + 1;
            return;
        }

        let [first, a, b, c] = self.bytes;

        if remaining == self.bytes.len() {
            debug_assert!(a == 0 || a == 0xff);
            self.state = 5 | if a == 0xff { OUTPUT_REPEAT_TAG } else { 0 };
        } else {
            debug_assert!(remaining < OUTPUT_COUNT_MASK);
            self.state += 1;
        }

        self.bytes = [first, b, c, byte];
    }
}

impl Iterator for OutputBytes {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        let remaining = self.state & OUTPUT_COUNT_MASK;

        if remaining == 0 {
            return None;
        }

        let [first, a, b, c] = self.bytes;

        if remaining > self.bytes.len() {
            self.bytes[0] = if self.state & OUTPUT_REPEAT_TAG != 0 {
                0xff
            } else {
                0x00
            };
            self.state -= 1;
        } else {
            self.bytes = [a, b, c, 0];
            self.state = remaining - 1;
        }

        Some(first)
    }
}

/// Internal implementations kept public only so Criterion can benchmark the
/// exact library code rather than maintaining benchmark-local copies.
#[doc(hidden)]
pub mod implementations;

/// Pre-optimization implementation retained as a benchmark baseline.
#[doc(hidden)]
pub mod legacy;

// Q17 with direct radix shifts is the production implementation while we benchmark candidates.
pub use implementations::q17::{RangeDecoder, RangeEncoder};

#[test]
fn canonical_probability_grid_keeps_both_branches_nonempty() {
    for range in [RANGE_TOP + 1, FULL_RANGE] {
        for p in [FractionalU16::HALF, FractionalU16::MAX] {
            let split = split(range, p);
            assert!(split > 0);
            assert!(split < range);
        }
    }

    assert_eq!(split(FULL_RANGE, FractionalU16::HALF), FULL_RANGE / 2);
}

#[test]
fn output_bytes_compresses_long_runs() {
    for repeated in [0x00, 0xff] {
        let mut output = OutputBytes::new();
        output.push(0x42, repeated, 100);
        output.push(0x17, 0xff, 0);
        output.push(0x93, 0xff, 0);

        let bytes: Vec<_> = output.collect();

        assert_eq!(bytes[0], 0x42);
        assert_eq!(&bytes[1..101], &[repeated; 100]);
        assert_eq!(&bytes[101..], &[0x17, 0x93]);
    }
}

#[cfg(test)]
fn random_events() -> Vec<(FractionalU16, bool)> {
    let mut seed = 0x1234_5678_u64;

    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    (0..100_000)
        .map(|_| {
            let p = FractionalU16::from_raw(random() as u16);
            let lower = random() & 1 != 0;
            (p, lower)
        })
        .collect()
}

#[cfg(test)]
fn encode_production(events: &[(FractionalU16, bool)]) -> Vec<u8> {
    let mut encoder = RangeEncoder::new();
    let mut bytes = Vec::new();

    for &(p, lower) in events {
        bytes.extend(encoder.put(p, lower));
    }

    bytes.extend(encoder.finish());
    bytes
}

#[test]
fn round_trip_random() {
    let events = random_events();
    let bytes = encode_production(&events);
    let mut decoder = RangeDecoder::new(&bytes);

    for (i, &(p, expected)) in events.iter().enumerate() {
        let got = decoder.test(p);
        assert_eq!(got, expected, "event {i}, p={}", p.raw());
    }
}

#[test]
fn implementation_candidates_match_production() {
    use implementations::{branchless, range_shift};

    let events = random_events();
    let expected = encode_production(&events);

    let mut legacy_encoder = legacy::RangeEncoder::new();
    let mut legacy_bytes = Vec::new();
    let mut range_encoder = range_shift::RangeEncoder::new();
    let mut range_bytes = Vec::new();
    let mut branchless_encoder = branchless::RangeEncoder::new();
    let mut branchless_bytes = Vec::new();

    for &(p, lower) in &events {
        legacy_bytes.extend(legacy_encoder.put(p, lower));
        range_bytes.extend(range_encoder.put(p, lower));
        branchless_bytes.extend(branchless_encoder.put(p, lower));
    }

    legacy_bytes.extend(legacy_encoder.finish());
    range_bytes.extend(range_encoder.finish());
    branchless_bytes.extend(branchless_encoder.finish());

    assert_eq!(legacy_bytes, expected);
    assert_eq!(range_bytes, expected);
    assert_eq!(branchless_bytes, expected);

    let mut legacy_decoder = legacy::RangeDecoder::new(&legacy_bytes);
    let mut range_decoder = range_shift::RangeDecoder::new(&range_bytes);
    let mut branchless_decoder = branchless::RangeDecoder::new(&branchless_bytes);

    for &(p, expected) in &events {
        assert_eq!(legacy_decoder.test(p), expected);
        assert_eq!(range_decoder.test(p), expected);
        assert_eq!(branchless_decoder.test(p), expected);
    }
}
