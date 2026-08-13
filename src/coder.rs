const PROBABILITY_BITS: u32 = 16;
const RANGE_BITS: u32 = 24;

const FULL_RANGE: u32 = 1 << RANGE_BITS;
const RANGE_TOP: u32 = 1 << (RANGE_BITS - 8);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FractionalU16(u16);

impl FractionalU16 {
    pub const ZERO: Self = Self(0);
    pub const HALF: Self = Self(1 << 15);

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
    ((range as u64 * p.raw() as u64) >> PROBABILITY_BITS) as u32
}

fn append_byte(value: u32, byte: u8) -> u32 {
    let [_, a, b, c] = value.to_be_bytes();
    u32::from_be_bytes([a, b, c, byte])
}

#[derive(Clone, Copy)]
struct Run {
    first: u8,
    repeated: u8,
    repeated_count: usize,
}

impl Run {
    const EMPTY: Self = Self {
        first: 0,
        repeated: 0,
        repeated_count: 0,
    };
}

/// Lazily emitted bytes.
///
/// A pathological
///
///     42 ff ff ff ff ... ff
///
/// does not allocate that run. We store its length and generate it while
/// iterating.
#[must_use]
pub struct OutputBytes {
    runs: [Run; 4],
    len: usize,
    run: usize,
    offset: usize,
}

impl OutputBytes {
    const fn new() -> Self {
        Self {
            runs: [Run::EMPTY; 4],
            len: 0,
            run: 0,
            offset: 0,
        }
    }

    #[inline(always)]
    fn push(&mut self, first: u8, repeated: u8, repeated_count: usize) {
        debug_assert!(self.len < self.runs.len());

        self.runs[self.len] = Run {
            first,
            repeated,
            repeated_count,
        };
        self.len += 1;
    }
}

impl Iterator for OutputBytes {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<u8> {
        if self.run == self.len {
            return None;
        }

        let run = self.runs[self.run];

        if self.offset == 0 {
            if run.repeated_count == 0 {
                self.run += 1;
            } else {
                self.offset = 1;
            }

            return Some(run.first);
        }

        let byte = run.repeated;

        if self.offset == run.repeated_count {
            self.run += 1;
            self.offset = 0;
        } else {
            self.offset += 1;
        }

        Some(byte)
    }
}

/// The lower bound is represented as:
///
///     low[0] | ff ff ... ff | low[1..4]
///              ^ ff_count
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

        let range = self.denominator as u64 + 1;
        let split = ((range * p.raw() as u64) >> PROBABILITY_BITS) as u32;

        if lower {
            debug_assert!(split != 0);
            self.denominator = split - 1;
        } else {
            let [old_head, ..] = self.low.to_be_bytes();

            self.low = self.low.wrapping_add(split);
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
            let p = FractionalU16::from_raw((random() as u16).max(1));

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

    for &(p, expected) in &events {
        assert_eq!(decoder.test(p), expected);
    }
}
