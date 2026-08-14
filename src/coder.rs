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

/// Lazily emitted bytes.
///
/// A pathological
///
/// ```text
/// 42 ff ff ff ff ... ff
/// ```
///
/// does not allocate that run. We store its length and generate it while
/// iterating.
#[must_use]
pub struct OutputBytes {
    bytes: [u8; 4],
    remaining: isize,
}

impl OutputBytes {
    pub fn new(first: u8, repeated: u8, count: usize) -> Self {
        let mut this = Self {
            bytes: [0; 4],
            remaining: 0,
        };

        debug_assert!(this.remaining == 0);
        debug_assert!(repeated == 0 || repeated == 0xff);

        let len = count + 1;

        this.bytes[0] = first;

        if len <= 4 {
            this.bytes[1..len].fill(repeated);
            this.remaining = len as isize;
        } else {
            this.bytes[1..].fill(repeated);
            this.remaining = if repeated == 0xff {
                len as isize
            } else {
                -(len as isize)
            };
        }

        this
    }
}

impl Iterator for OutputBytes {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        let remaining = self.remaining.unsigned_abs();

        if remaining == 0 {
            return None;
        }

        let [first, a, b, c] = self.bytes;

        if remaining > 4 {
            self.bytes[0] = if self.remaining > 0 { 0xff } else { 0x00 };

            self.remaining += if self.remaining > 0 { -1 } else { 1 };
        } else {
            self.bytes = [a, b, c, 0];

            // Once we're in the literal tail, the sign has no meaning.
            self.remaining = (remaining - 1) as isize;
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

        if lower {
            debug_assert!(split != 0);
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
