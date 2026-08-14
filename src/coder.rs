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

fn split(range: u32, p: FractionalU16) -> u32 {
    let probability = HALF_PROBABILITY | p.raw() as u32;
    ((range as u64 * probability as u64) >> PROBABILITY_BITS) as u32
}

fn append_byte(value: u32, byte: u8) -> u32 {
    let [_, a, b, c] = value.to_be_bytes();
    u32::from_be_bytes([a, b, c, byte])
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

        // After the first emission, at most three literal bytes can still be
        // produced by the remaining renormalization steps / final flush.
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

/// The lower bound is represented as:
///
/// ```text
/// low[0] | ff ff ... ff | low[1..4]
///          ^ ff_count
/// ```
///
/// `low[0]` is the unresolved significant byte and `low[1..4]` is the
/// live 24-bit arithmetic suffix.
///
/// If the suffix overflows, normal u32 addition carries directly into
/// `low[0]`; the omitted FF run therefore becomes a run of zeroes.
pub struct RangeEncoder {
    /// low[0] | FF × (pending - 1) | low[1..4]
    low: u32,

    /// Inclusive upper coordinate: range size is denominator + 1.
    denominator: u32,

    /// 0 => no unresolved head.
    /// n > 0 => low[0] followed by n - 1 implicit FF bytes.
    pending: usize,
}

impl Default for RangeEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeEncoder {
    pub const fn new() -> Self {
        Self {
            low: 0,
            denominator: FULL_RANGE - 1, // 0xffffff => 2^24 states
            pending: 0,
        }
    }

    pub fn put(&mut self, p: FractionalU16, lower: bool) -> OutputBytes {
        let mut output = OutputBytes::new();
        let split = split(self.denominator + 1, p);

        debug_assert!(split != 0);
        debug_assert!(split <= self.denominator);

        if lower {
            self.denominator = split - 1;
        } else {
            let [old_head, ..] = self.low.to_be_bytes();

            self.low = self.low.wrapping_add(split);
            self.denominator -= split;
            let [head, ..] = self.low.to_be_bytes();

            // The 24-bit suffix carried through the implicit FF run.
            if head != old_head {
                if self.pending == 0 {
                    // There wasn't an unresolved head before; the carry
                    // simply created one.
                    self.pending = 1;
                } else {
                    // H FF FF ... + carry = (H + 1) 00 00 ...
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

    fn renormalize(&mut self, output: &mut OutputBytes) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
                let [head, next, a, b] = self.low.to_be_bytes();

                if self.pending == 0 {
                    // Establish the first unresolved byte.
                    self.low = u32::from_be_bytes([next, a, b, 0]);
                    self.pending = 1;
                } else if next == 0xff {
                    // Can't commit the head: this byte could propagate a carry.
                    self.low = u32::from_be_bytes([head, a, b, 0]);
                    self.pending += 1;
                } else {
                    // Since next < FF, no future carry can reach the head.
                    output.push(head, 0xff, self.pending - 1);

                    self.low = u32::from_be_bytes([next, a, b, 0]);
                    self.pending = 1;
                }

                self.denominator = append_byte(self.denominator, 0xff);
            }
        }
    }

    /// Emit the current lower bound.
    ///
    /// Trailing zero bytes do not need to be written because decoder input is
    /// implicitly zero-extended.
    pub fn finish(mut self) -> OutputBytes {
        let mut output = OutputBytes::new();

        // Choose the lowest state in the current interval.
        self.denominator = 0;

        // Factors all three residual bytes out of `low`.
        self.renormalize(&mut output);

        if self.pending != 0 {
            let [head, ..] = self.low.to_be_bytes();

            // No future input exists, therefore no future carry exists.
            output.push(head, 0xff, self.pending - 1);
        }

        output
    }
}

#[derive(Clone, Copy)]
pub struct RangeDecoder<'a> {
    numerator: u32,
    denominator: u32,
    input: &'a [u8],
}

impl<'a> RangeDecoder<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        let mut decoder = Self {
            numerator: 0,
            denominator: 0,
            input,
        };

        decoder.renormalize();
        decoder
    }

    pub fn test(&mut self, p: FractionalU16) -> bool {
        let split = split(self.denominator + 1, p);
        debug_assert!(split != 0);
        debug_assert!(split <= self.denominator);

        let lower = self.numerator < split;

        if lower {
            // New states are 0..split-1.
            self.denominator = split - 1;
        } else {
            // Translate S..R-1 back to 0..R-S-1.
            self.numerator -= split;
            self.denominator -= split;
        }

        self.renormalize();
        lower
    }

    fn renormalize(&mut self) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
                self.denominator = append_byte(self.denominator, 0xff);
                self.numerator = append_byte(self.numerator, self.read_byte());
            }
        }
    }

    fn read_byte(&mut self) -> u8 {
        let [byte, rest @ ..] = self.input else {
            return 0;
        };

        self.input = rest;
        *byte
    }
}

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

#[test]
fn round_trip_random() {
    let mut seed = 0x1234_5678_u64;

    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let events: Vec<_> = (0..100_000)
        .map(|_| {
            let p = FractionalU16::from_raw(random() as u16);

            let lower = random() & 1 != 0;

            (p, lower)
        })
        .collect();

    let mut encoder = RangeEncoder::new();
    let mut bytes = Vec::new();

    for &(p, lower) in &events {
        bytes.extend(encoder.put(p, lower));
    }

    bytes.extend(encoder.finish());

    let mut decoder = RangeDecoder::new(&bytes);

    for (i, &(p, expected)) in events.iter().enumerate() {
        let numerator = decoder.numerator;
        let denominator = decoder.denominator;

        let got = decoder.test(p);

        assert_eq!(
            got,
            expected,
            "event {i}, p={}, numerator={numerator:#08x}, denominator={denominator:#08x}",
            p.raw(),
        );
    }
}
